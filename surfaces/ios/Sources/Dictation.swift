import Foundation

enum DictationState: Equatable {
    case idle
    case requestingPermission
    case recording
    case transcribing
    case reviewing
    case failed(String)

    var isActive: Bool {
        switch self {
        case .requestingPermission, .recording, .transcribing:
            true
        case .idle, .reviewing, .failed:
            false
        }
    }

    var canBegin: Bool {
        switch self {
        case .idle, .reviewing, .failed:
            true
        case .requestingPermission, .recording, .transcribing:
            false
        }
    }

    var notice: String? {
        switch self {
        case .idle:
            nil
        case .requestingPermission:
            "ALLOW MICROPHONE ACCESS, THEN HOLD AGAIN"
        case .recording:
            "RECORDING - RELEASE TO TRANSCRIBE"
        case .transcribing:
            "TRANSCRIBING ON PRIVATE HOST"
        case .reviewing:
            "TRANSCRIPT READY - EDIT, SEND, OR DISCARD"
        case let .failed(message):
            message.uppercased()
        }
    }
}

struct TranscriptionResponse: Decodable, Equatable {
    let text: String
}

struct GatewayErrorResponse: Decodable {
    struct Body: Decodable {
        let code: String
        let message: String
    }

    let error: Body
}

enum DictationFailure: LocalizedError {
    case permissionDenied
    case recordingFailed
    case recordingTooLarge
    case invalidEndpoint
    case invalidResponse
    case gateway(String)

    var errorDescription: String? {
        switch self {
        case .permissionDenied:
            "Microphone access is required for dictation."
        case .recordingFailed:
            "The recording could not be captured."
        case .recordingTooLarge:
            "The recording is too large. Hold for no more than 60 seconds."
        case .invalidEndpoint:
            "The private transcription endpoint is invalid."
        case .invalidResponse:
            "The host returned an invalid transcript."
        case let .gateway(message):
            message
        }
    }
}

enum TranscriptionEndpoint {
    static func url(for surfaceURL: URL) -> URL? {
        guard var components = URLComponents(url: surfaceURL, resolvingAgainstBaseURL: false) else {
            return nil
        }
        switch components.scheme {
        case "wss":
            components.scheme = "https"
        case "ws":
            components.scheme = "http"
        default:
            return nil
        }
        var base = components.path == "/"
            ? ""
            : components.path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        if base == "surface" || base.hasSuffix("/surface") {
            base = String(base.dropLast("surface".count))
                .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        }
        components.path = base.isEmpty
            ? "/audio/transcription"
            : "/\(base)/audio/transcription"
        components.query = nil
        components.fragment = nil
        return components.url
    }
}
