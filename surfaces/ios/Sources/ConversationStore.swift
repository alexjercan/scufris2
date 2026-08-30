import Foundation
import SwiftUI
import UIKit

@MainActor
final class ConversationStore: ObservableObject {
    enum ConnectionState: Equatable {
        case unconfigured
        case connecting
        case connected
        case disconnected(String)

        var label: String {
            switch self {
            case .unconfigured:
                "Not configured"
            case .connecting:
                "Connecting"
            case .connected:
                "Connected"
            case .disconnected:
                "Disconnected"
            }
        }
    }

    @Published private(set) var connectionState: ConnectionState = .unconfigured
    @Published private(set) var conversation: [ConversationEntry] = []
    @Published private(set) var serviceDetail = ""
    @Published private(set) var settings = ConnectionSettings(backendURL: "", token: "")

    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()
    private let surfaceID: String
    private var socket: URLSessionWebSocketTask?
    private var connectionTask: Task<Void, Never>?
    private var generation = 0

    init() {
        if let stored = try? SecureStore.read("surface-id") {
            surfaceID = stored
        } else {
            let generated = "ios-\(UUID().uuidString.lowercased())"
            surfaceID = generated
            try? SecureStore.write(generated, account: "surface-id")
        }
        let backendURL = (try? SecureStore.read("backend-url")) ?? nil
        let token = (try? SecureStore.read("backend-token")) ?? nil
        if let backendURL, let token {
            settings = ConnectionSettings(backendURL: backendURL, token: token)
            if settings.isComplete {
                startConnection()
            }
        }
    }

    func configure(_ newSettings: ConnectionSettings) throws {
        guard newSettings.isComplete else {
            throw ConfigurationFailure.invalid
        }
        try SecureStore.write(newSettings.backendURL, account: "backend-url")
        try SecureStore.write(newSettings.token, account: "backend-token")
        settings = newSettings
        startConnection()
    }

    func reconnect() {
        guard settings.isComplete else {
            connectionState = .unconfigured
            return
        }
        startConnection()
    }

    func send(_ submitted: String) {
        let text = submitted.trimmingCharacters(in: .whitespacesAndNewlines)
        guard
            !text.isEmpty,
            text.utf8.count <= scufrisMaximumTextBytes,
            case .connected = connectionState,
            let socket
        else {
            return
        }
        let request = SurfaceMessageRequest(
            id: "ios-\(UUID().uuidString.lowercased())",
            text: text
        )
        Task {
            do {
                try await send(request, through: socket)
            } catch {
                failCurrentConnection(error)
            }
        }
    }

    private func startConnection() {
        generation += 1
        let currentGeneration = generation
        connectionTask?.cancel()
        socket?.cancel(with: .goingAway, reason: nil)
        socket = nil
        conversation = []
        serviceDetail = ""
        connectionTask = Task {
            while !Task.isCancelled, currentGeneration == generation {
                do {
                    try await connectAndReceive(generation: currentGeneration)
                } catch is CancellationError {
                    return
                } catch {
                    guard currentGeneration == generation else { return }
                    connectionState = .disconnected(error.localizedDescription)
                    serviceDetail = error.localizedDescription
                }
                do {
                    try await Task.sleep(for: .seconds(2))
                } catch {
                    return
                }
            }
        }
    }

    private func connectAndReceive(generation currentGeneration: Int) async throws {
        guard let url = URL(string: settings.backendURL) else {
            throw ConfigurationFailure.invalid
        }
        connectionState = .connecting
        serviceDetail = "Connecting to \(url.host ?? "backend")"
        var request = URLRequest(url: url)
        request.setValue("Bearer \(settings.token)", forHTTPHeaderField: "Authorization")
        let webSocket = URLSession.shared.webSocketTask(with: request)
        webSocket.maximumMessageSize = scufrisMaximumMessageBytes
        socket = webSocket
        webSocket.resume()

        let hello = SurfaceHello(
            surface: SurfaceRegistration(
                id: surfaceID,
                name: UIDevice.current.name,
                widgets: []
            )
        )
        try await send(hello, through: webSocket)

        while !Task.isCancelled, currentGeneration == generation {
            let frame = try await webSocket.receive()
            switch frame {
            case let .string(text):
                try receive(text)
            case .data:
                throw ProtocolFailure.unexpectedFrame
            @unknown default:
                throw ProtocolFailure.unexpectedFrame
            }
        }
    }

    private func send(_ message: some Encodable, through webSocket: URLSessionWebSocketTask) async throws {
        let data = try encoder.encode(message)
        guard data.count <= scufrisMaximumMessageBytes else {
            throw ProtocolFailure.oversized
        }
        guard let text = String(data: data, encoding: .utf8) else {
            throw ProtocolFailure.invalidMessage("request is not UTF-8")
        }
        try await webSocket.send(.string(text))
    }

    private func receive(_ text: String) throws {
        guard text.utf8.count <= scufrisMaximumMessageBytes else {
            throw ProtocolFailure.oversized
        }
        let data = Data(text.utf8)
        let envelope = try decoder.decode(IncomingEnvelope.self, from: data)
        guard envelope.v == scufrisProtocolVersion else {
            throw ProtocolFailure.unsupportedVersion(envelope.v)
        }
        switch envelope.type {
        case "surface.message":
            let message = try decoder.decode(IncomingConversationMessage.self, from: data)
            guard
                !message.text.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty,
                message.text.utf8.count <= scufrisMaximumTextBytes
            else {
                throw ProtocolFailure.invalidMessage("conversation text is outside its bounds")
            }
            conversation.append(
                ConversationEntry(
                    role: message.role,
                    surface: message.surface,
                    text: message.text,
                    details: message.details
                )
            )
        case "surface.ready":
            _ = try decoder.decode(IncomingReady.self, from: data)
            connectionState = .connected
            serviceDetail = "Ready"
        case "surface.state":
            let state = try decoder.decode(IncomingState.self, from: data)
            serviceDetail = state.detail.isEmpty ? state.state.capitalized : state.detail
        case "surface.rejected":
            let rejected = try decoder.decode(IncomingRejected.self, from: data)
            serviceDetail = "\(rejected.code): \(rejected.detail)"
        case "surface.message_ack", "surface.aborted":
            break
        default:
            throw ProtocolFailure.invalidMessage("unknown type \(envelope.type)")
        }
    }

    private func failCurrentConnection(_ error: Error) {
        socket?.cancel(with: .goingAway, reason: nil)
        connectionState = .disconnected(error.localizedDescription)
        serviceDetail = error.localizedDescription
    }
}

enum ConfigurationFailure: LocalizedError {
    case invalid

    var errorDescription: String? {
        "Enter a wss:// URL and a 32 to 256 byte pairing token."
    }
}
