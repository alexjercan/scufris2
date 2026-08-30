import Foundation

let scufrisProtocolVersion = 5
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

struct AttachmentDescriptor: Codable, Equatable, Identifiable {
    let id: String
    let name: String
    let mediaType: String
    let size: UInt64

    enum CodingKeys: String, CodingKey {
        case id, name, size
        case mediaType = "media_type"
    }

    var isProtocolValid: Bool {
        let identifier = id.range(
            of: #"^[A-Za-z0-9._-]{1,64}$"#,
            options: .regularExpression
        ) != nil
        let validName = !name.isEmpty
            && name.utf8.count <= 255
            && !name.contains("/")
            && !name.contains("\\")
            && name.unicodeScalars.allSatisfy { !CharacterSet.controlCharacters.contains($0) }
        let validMediaType = mediaType.utf8.count <= 127
            && mediaType.range(
                of: #"^[A-Za-z0-9!#$&^_.+-]+/[A-Za-z0-9!#$&^_.+-]+$"#,
                options: .regularExpression
            ) != nil
        return identifier && validName && validMediaType
            && size > 0 && size <= 16 * 1024 * 1024
    }
}

struct SurfaceMessageRequest: Encodable {
    let v = scufrisProtocolVersion
    let type = "surface.message"
    let id: String
    let text: String
    let attachments: [String]
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
    let attachments: [AttachmentDescriptor]?
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
    let attachments: [AttachmentDescriptor]
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
