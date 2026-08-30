import Foundation

let scufrisProtocolVersion = 4
let scufrisMaximumMessageBytes = 64 * 1024
let scufrisMaximumTextBytes = 8 * 1024

struct SurfaceRegistration: Encodable {
    let id: String
    let name: String
    let widgets: [EmptyWidget]
}

struct EmptyWidget: Encodable {}

struct SurfaceHello: Encodable {
    let v = scufrisProtocolVersion
    let type = "surface.hello"
    let surface: SurfaceRegistration
}

struct SurfaceMessageRequest: Encodable {
    let v = scufrisProtocolVersion
    let type = "surface.message"
    let id: String
    let text: String
}

struct IncomingEnvelope: Decodable {
    let v: Int
    let type: String
}

struct IncomingConversationMessage: Decodable {
    let v: Int
    let type: String
    let role: ConversationRole
    let surface: String
    let text: String
    let details: String?
}

enum ConversationRole: String, Decodable {
    case user
    case assistant
}

struct IncomingReady: Decodable {
    let v: Int
    let type: String
    let surface: String
}

struct IncomingState: Decodable {
    let v: Int
    let type: String
    let state: String
    let detail: String
}

struct IncomingRejected: Decodable {
    let v: Int
    let type: String
    let code: String
    let detail: String
}

struct ConversationEntry: Identifiable, Equatable {
    let id = UUID()
    let role: ConversationRole
    let surface: String
    let text: String
    let details: String?
}

enum ProtocolFailure: LocalizedError {
    case oversized
    case unsupportedVersion(Int)
    case unexpectedFrame
    case invalidMessage(String)

    var errorDescription: String? {
        switch self {
        case .oversized:
            "The service sent an oversized message."
        case let .unsupportedVersion(version):
            "The service uses unsupported protocol version \(version)."
        case .unexpectedFrame:
            "The service sent a non-text WebSocket frame."
        case let .invalidMessage(detail):
            "The service sent an invalid message: \(detail)"
        }
    }
}
