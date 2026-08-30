import AVFoundation
import Foundation
import SwiftUI
import UIKit

private let maximumRecordingBytes = 2 * 1024 * 1024
private let maximumRecordingDuration: TimeInterval = 60
private let maximumTranscriptionResponseBytes = 16 * 1024

@MainActor
final class ConversationStore: NSObject, ObservableObject {
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
    @Published private(set) var serviceState = "idle"
    @Published private(set) var settings = ConnectionSettings(backendURL: "", token: "")
    @Published var draft = ""
    @Published private(set) var dictationState: DictationState = .idle
    @Published private(set) var selectedAttachments: [AttachmentDescriptor] = []
    @Published private(set) var isUploadingAttachment = false
    @Published private(set) var attachmentNotice: String?

    var visualState: SurfaceVisualState {
        switch connectionState {
        case .unconfigured:
            .setup
        case .connecting:
            .connecting
        case .connected:
            .connected(serviceState: serviceState)
        case .disconnected:
            .offline
        }
    }

    var isThinking: Bool {
        visualState.showsThinking
    }

    var routeLabel: String {
        guard let url = URL(string: settings.backendURL), let host = url.host else {
            return "private WSS / not configured"
        }
        let path = url.path == "/" || url.path.isEmpty ? "" : " \(url.path)"
        return "\(host)\(path) / private surface"
    }

    var showsStatusNotice: Bool {
        guard !serviceDetail.isEmpty, serviceDetail != "Ready" else { return false }
        switch connectionState {
        case .connected:
            return serviceState != "idle" && serviceState != "working"
        case .unconfigured:
            return false
        case .connecting, .disconnected:
            return true
        }
    }

    private let encoder = JSONEncoder()
    private let decoder = JSONDecoder()
    private let surfaceID: String
    private var socket: URLSessionWebSocketTask?
    private var connectionTask: Task<Void, Never>?
    private var generation = 0
    private var dictationGeneration = 0
    private var attachmentGeneration = 0
    private var recorder: AVAudioRecorder?
    private var recordingURL: URL?
    private var transcriptionTask: Task<Void, Never>?
    private var attachmentCopies: [String: URL] = [:]

    override init() {
        if let stored = try? SecureStore.read("surface-id") {
            surfaceID = stored
        } else {
            let generated = "ios-\(UUID().uuidString.lowercased())"
            surfaceID = generated
            try? SecureStore.write(generated, account: "surface-id")
        }
        super.init()
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
        selectedAttachments = []
        attachmentNotice = nil
        attachmentCopies = [:]
        startConnection()
    }

    func reconnect() {
        guard settings.isComplete else {
            connectionState = .unconfigured
            return
        }
        startConnection()
    }

    func beginDictation() {
        guard
            case .connected = connectionState,
            dictationState.canBegin
        else {
            return
        }
        discardRecording()
        dictationGeneration += 1
        let currentGeneration = dictationGeneration
        switch AVAudioSession.sharedInstance().recordPermission {
        case .granted:
            startRecording(generation: currentGeneration)
        case .denied:
            dictationState = .failed(DictationFailure.permissionDenied.localizedDescription)
        case .undetermined:
            dictationState = .requestingPermission
            AVAudioSession.sharedInstance().requestRecordPermission { [weak self] granted in
                Task { @MainActor [weak self] in
                    guard
                        let self,
                        currentGeneration == self.dictationGeneration,
                        self.dictationState == .requestingPermission
                    else {
                        return
                    }
                    if granted {
                        self.dictationState = .idle
                    } else {
                        self.dictationState = .failed(
                            DictationFailure.permissionDenied.localizedDescription
                        )
                    }
                }
            }
        @unknown default:
            dictationState = .failed(DictationFailure.permissionDenied.localizedDescription)
        }
    }

    func finishDictation() {
        guard dictationState == .recording, let recorder, let recordingURL else {
            if dictationState == .requestingPermission {
                cancelDictation()
            }
            return
        }
        recorder.stop()
        self.recorder = nil
        deactivateRecordingSession()
        dictationState = .transcribing
        let currentGeneration = dictationGeneration
        transcriptionTask = Task {
            do {
                let text = try await transcribe(recordingURL)
                guard currentGeneration == dictationGeneration else { return }
                draft = text
                dictationState = .reviewing
                discardRecording()
            } catch is CancellationError {
                return
            } catch {
                guard currentGeneration == dictationGeneration else { return }
                dictationState = .failed(error.localizedDescription)
                discardRecording()
            }
        }
    }

    func cancelDictation() {
        dictationGeneration += 1
        transcriptionTask?.cancel()
        transcriptionTask = nil
        recorder?.stop()
        recorder = nil
        deactivateRecordingSession()
        discardRecording()
        dictationState = .idle
    }

    func discardDictation() {
        draft = ""
        cancelDictation()
    }

    @discardableResult
    func send(_ submitted: String) -> Bool {
        let text = submitted.trimmingCharacters(in: .whitespacesAndNewlines)
        guard
            !text.isEmpty,
            text.utf8.count <= scufrisMaximumTextBytes,
            selectedAttachments.count <= scufrisMaximumAttachments,
            case .connected = connectionState,
            let socket
        else {
            return false
        }
        dictationState = .idle
        let request = SurfaceMessageRequest(
            id: "ios-\(UUID().uuidString.lowercased())",
            text: text,
            attachments: selectedAttachments.map(\.id)
        )
        selectedAttachments = []
        attachmentNotice = nil
        Task {
            do {
                try await send(request, through: socket)
            } catch {
                failCurrentConnection(error)
            }
        }
        return true
    }

    func addDocument(_ url: URL) {
        guard canAddAttachment else {
            attachmentNotice = AttachmentFailure.tooMany.localizedDescription
            return
        }
        let access = url.startAccessingSecurityScopedResource()
        attachmentGeneration += 1
        let currentAttachmentGeneration = attachmentGeneration
        isUploadingAttachment = true
        attachmentNotice = "UPLOADING \(url.lastPathComponent)"
        Task {
            defer {
                if access { url.stopAccessingSecurityScopedResource() }
                if currentAttachmentGeneration == attachmentGeneration {
                    isUploadingAttachment = false
                }
            }
            do {
                let values = try url.resourceValues(forKeys: [
                    .isRegularFileKey, .fileSizeKey, .nameKey,
                ])
                guard values.isRegularFile == true,
                      let byteCount = values.fileSize,
                      byteCount > 0
                else {
                    throw AttachmentFailure.invalidSelection
                }
                guard byteCount <= scufrisMaximumAttachmentBytes else {
                    throw AttachmentFailure.tooLarge
                }
                let descriptor = try await upload(
                    file: url,
                    name: values.name ?? url.lastPathComponent,
                    mediaType: AttachmentTransfer.mediaType(for: url)
                )
                guard currentAttachmentGeneration == attachmentGeneration,
                      connectionState == .connected
                else { return }
                acceptAttachment(descriptor)
            } catch {
                guard currentAttachmentGeneration == attachmentGeneration else { return }
                attachmentNotice = error.localizedDescription
            }
        }
    }

    func addPhoto(_ data: Data, name: String, mediaType: String) {
        guard canAddAttachment else {
            attachmentNotice = AttachmentFailure.tooMany.localizedDescription
            return
        }
        guard !data.isEmpty else {
            attachmentNotice = AttachmentFailure.invalidSelection.localizedDescription
            return
        }
        guard data.count <= scufrisMaximumAttachmentBytes else {
            attachmentNotice = AttachmentFailure.tooLarge.localizedDescription
            return
        }
        attachmentGeneration += 1
        let currentAttachmentGeneration = attachmentGeneration
        isUploadingAttachment = true
        attachmentNotice = "UPLOADING \(name)"
        Task {
            defer {
                if currentAttachmentGeneration == attachmentGeneration {
                    isUploadingAttachment = false
                }
            }
            do {
                let descriptor = try await upload(data: data, name: name, mediaType: mediaType)
                guard currentAttachmentGeneration == attachmentGeneration,
                      connectionState == .connected
                else { return }
                acceptAttachment(descriptor)
            } catch {
                guard currentAttachmentGeneration == attachmentGeneration else { return }
                attachmentNotice = error.localizedDescription
            }
        }
    }

    func removeAttachment(id: String) {
        selectedAttachments.removeAll { $0.id == id }
        attachmentNotice = nil
    }

    func localCopy(of descriptor: AttachmentDescriptor) async throws -> URL {
        if let existing = attachmentCopies[descriptor.id],
           FileManager.default.fileExists(atPath: existing.path)
        {
            return existing
        }
        guard let surfaceURL = URL(string: settings.backendURL) else {
            throw AttachmentFailure.invalidEndpoint
        }
        let request = try AttachmentTransfer.downloadRequest(
            surfaceURL: surfaceURL,
            token: settings.token,
            descriptor: descriptor
        )
        let (data, response) = try await URLSession.shared.data(for: request)
        let bytes = try AttachmentTransfer.downloadedData(
            data,
            response: response,
            descriptor: descriptor
        )
        let directory = FileManager.default.temporaryDirectory
            .appendingPathComponent("scufris-attachment-\(UUID().uuidString)", isDirectory: true)
        do {
            try FileManager.default.createDirectory(at: directory, withIntermediateDirectories: false)
            let destination = directory.appendingPathComponent(descriptor.name, isDirectory: false)
            try bytes.write(to: destination, options: [.atomic, .completeFileProtectionUntilFirstUserAuthentication])
            attachmentCopies[descriptor.id] = destination
            return destination
        } catch {
            try? FileManager.default.removeItem(at: directory)
            throw AttachmentFailure.saveFailed
        }
    }

    func attachmentFailed(_ error: Error) {
        attachmentNotice = error.localizedDescription
    }

    private var canAddAttachment: Bool {
        !isUploadingAttachment
            && selectedAttachments.count < scufrisMaximumAttachments
            && connectionState == .connected
    }

    private func acceptAttachment(_ descriptor: AttachmentDescriptor) {
        guard descriptor.isProtocolValid,
              !selectedAttachments.contains(where: { $0.id == descriptor.id }),
              selectedAttachments.count < scufrisMaximumAttachments
        else {
            attachmentNotice = AttachmentFailure.invalidResponse.localizedDescription
            return
        }
        selectedAttachments.append(descriptor)
        attachmentNotice = nil
    }

    private func upload(file: URL, name: String, mediaType: String) async throws -> AttachmentDescriptor {
        let request = try attachmentUploadRequest(name: name, mediaType: mediaType)
        let (data, response) = try await URLSession.shared.upload(for: request, fromFile: file)
        return try AttachmentTransfer.descriptor(from: data, response: response)
    }

    private func upload(data: Data, name: String, mediaType: String) async throws -> AttachmentDescriptor {
        let request = try attachmentUploadRequest(name: name, mediaType: mediaType)
        let (responseData, response) = try await URLSession.shared.upload(for: request, from: data)
        return try AttachmentTransfer.descriptor(from: responseData, response: response)
    }

    private func attachmentUploadRequest(name: String, mediaType: String) throws -> URLRequest {
        guard let surfaceURL = URL(string: settings.backendURL) else {
            throw AttachmentFailure.invalidEndpoint
        }
        return try AttachmentTransfer.uploadRequest(
            surfaceURL: surfaceURL,
            token: settings.token,
            name: name,
            mediaType: mediaType
        )
    }

    private func startRecording(generation currentGeneration: Int) {
        let session = AVAudioSession.sharedInstance()
        do {
            try session.setCategory(.record, mode: .measurement)
            try session.setActive(true)
            let url = FileManager.default.temporaryDirectory
                .appendingPathComponent("scufris-dictation-\(UUID().uuidString).wav")
            let settings: [String: Any] = [
                AVFormatIDKey: kAudioFormatLinearPCM,
                AVSampleRateKey: 16_000,
                AVNumberOfChannelsKey: 1,
                AVLinearPCMBitDepthKey: 16,
                AVLinearPCMIsBigEndianKey: false,
                AVLinearPCMIsFloatKey: false,
            ]
            let recorder = try AVAudioRecorder(url: url, settings: settings)
            guard recorder.prepareToRecord(), recorder.record(forDuration: maximumRecordingDuration) else {
                throw DictationFailure.recordingFailed
            }
            guard currentGeneration == dictationGeneration else {
                recorder.stop()
                try? FileManager.default.removeItem(at: url)
                return
            }
            self.recorder = recorder
            recordingURL = url
            dictationState = .recording
        } catch {
            deactivateRecordingSession()
            discardRecording()
            dictationState = .failed(error.localizedDescription)
        }
    }

    private func transcribe(_ recordingURL: URL) async throws -> String {
        let audio = try Data(contentsOf: recordingURL, options: .mappedIfSafe)
        guard !audio.isEmpty else { throw DictationFailure.recordingFailed }
        guard audio.count <= maximumRecordingBytes else {
            throw DictationFailure.recordingTooLarge
        }
        guard
            let surfaceURL = URL(string: settings.backendURL),
            let endpoint = TranscriptionEndpoint.url(for: surfaceURL)
        else {
            throw DictationFailure.invalidEndpoint
        }
        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.timeoutInterval = 135
        request.setValue("Bearer \(settings.token)", forHTTPHeaderField: "Authorization")
        request.setValue("audio/wav", forHTTPHeaderField: "Content-Type")
        request.httpBody = audio
        let (data, response) = try await URLSession.shared.data(for: request)
        guard data.count <= maximumTranscriptionResponseBytes else {
            throw DictationFailure.invalidResponse
        }
        guard let response = response as? HTTPURLResponse else {
            throw DictationFailure.invalidResponse
        }
        guard response.statusCode == 200 else {
            if let failure = try? decoder.decode(GatewayErrorResponse.self, from: data) {
                throw DictationFailure.gateway(failure.error.message)
            }
            throw DictationFailure.gateway("Host transcription failed.")
        }
        let result = try decoder.decode(TranscriptionResponse.self, from: data)
        let text = result.text.trimmingCharacters(in: .whitespacesAndNewlines)
        guard
            !text.isEmpty,
            text.utf8.count <= scufrisMaximumTextBytes,
            !text.unicodeScalars.contains(where: {
                CharacterSet.controlCharacters.contains($0) && $0.value != 10 && $0.value != 9
            })
        else {
            throw DictationFailure.invalidResponse
        }
        return text
    }

    private func discardRecording() {
        if let recordingURL {
            try? FileManager.default.removeItem(at: recordingURL)
        }
        recordingURL = nil
    }

    private func deactivateRecordingSession() {
        try? AVAudioSession.sharedInstance().setActive(false, options: .notifyOthersOnDeactivation)
    }

    private func startConnection() {
        generation += 1
        attachmentGeneration += 1
        isUploadingAttachment = false
        attachmentNotice = nil
        let currentGeneration = generation
        connectionTask?.cancel()
        socket?.cancel(with: .goingAway, reason: nil)
        socket = nil
        cancelDictation()
        conversation = []
        serviceDetail = ""
        serviceState = "starting"
        connectionTask = Task {
            while !Task.isCancelled, currentGeneration == generation {
                do {
                    try await connectAndReceive(generation: currentGeneration)
                } catch is CancellationError {
                    return
                } catch {
                    guard currentGeneration == generation else { return }
                    connectionState = .disconnected(error.localizedDescription)
                    serviceState = "failed"
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
        serviceState = "starting"
        conversation = []
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
            let attachments = message.attachments ?? []
            guard attachments.count <= 8,
                  attachments.allSatisfy(\.isProtocolValid),
                  Set(attachments.map(\.id)).count == attachments.count
            else {
                throw ProtocolFailure.invalidMessage("attachments are outside their bounds")
            }
            conversation.append(
                ConversationEntry(
                    role: message.role,
                    surface: message.surface,
                    text: message.text,
                    details: message.details,
                    attachments: attachments
                )
            )
            if message.role == .assistant {
                serviceState = "idle"
            }
        case "surface.ready":
            _ = try decoder.decode(IncomingReady.self, from: data)
            connectionState = .connected
            serviceState = "idle"
            serviceDetail = "Ready"
        case "surface.state":
            let state = try decoder.decode(IncomingState.self, from: data)
            serviceState = state.state
            serviceDetail = state.detail.isEmpty ? state.state.capitalized : state.detail
        case "surface.rejected":
            let rejected = try decoder.decode(IncomingRejected.self, from: data)
            serviceState = "failed"
            serviceDetail = "\(rejected.code): \(rejected.detail)"
        case "surface.message_ack", "surface.aborted":
            break
        default:
            throw ProtocolFailure.invalidMessage("unknown type \(envelope.type)")
        }
    }

    private func failCurrentConnection(_ error: Error) {
        attachmentGeneration += 1
        isUploadingAttachment = false
        socket?.cancel(with: .goingAway, reason: nil)
        connectionState = .disconnected(error.localizedDescription)
        serviceState = "failed"
        serviceDetail = error.localizedDescription
    }
}

enum ConfigurationFailure: LocalizedError {
    case invalid

    var errorDescription: String? {
        "Enter a wss:// URL and a 32 to 256 byte pairing token."
    }
}
