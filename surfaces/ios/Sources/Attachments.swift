import Foundation
import UniformTypeIdentifiers

let scufrisMaximumAttachments = 8
let scufrisMaximumAttachmentBytes = 16 * 1024 * 1024
private let maximumAttachmentResponseBytes = 16 * 1024

enum AttachmentEndpoint {
    static func collection(for surfaceURL: URL, name: String) -> URL? {
        guard var components = base(for: surfaceURL) else { return nil }
        let prefix = deploymentPrefix(components.path)
        components.path = prefix.isEmpty ? "/attachments" : "/\(prefix)/attachments"
        components.queryItems = [URLQueryItem(name: "name", value: name)]
        components.fragment = nil
        return components.url
    }

    static func object(for surfaceURL: URL, id: String) -> URL? {
        guard var components = base(for: surfaceURL) else { return nil }
        let prefix = deploymentPrefix(components.path)
        components.path = prefix.isEmpty
            ? "/attachments/\(id)"
            : "/\(prefix)/attachments/\(id)"
        components.query = nil
        components.fragment = nil
        return components.url
    }

    private static func base(for surfaceURL: URL) -> URLComponents? {
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
        return components
    }

    private static func deploymentPrefix(_ path: String) -> String {
        var prefix = path == "/"
            ? ""
            : path.trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        if prefix == "surface" || prefix.hasSuffix("/surface") {
            prefix = String(prefix.dropLast("surface".count))
                .trimmingCharacters(in: CharacterSet(charactersIn: "/"))
        }
        return prefix
    }
}

enum AttachmentTransfer {
    static func uploadRequest(
        surfaceURL: URL,
        token: String,
        name: String,
        mediaType: String
    ) throws -> URLRequest {
        guard
            validDisplayName(name),
            validMediaType(mediaType),
            let endpoint = AttachmentEndpoint.collection(for: surfaceURL, name: name)
        else {
            throw AttachmentFailure.invalidSelection
        }
        var request = URLRequest(url: endpoint)
        request.httpMethod = "POST"
        request.timeoutInterval = 135
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        request.setValue(mediaType, forHTTPHeaderField: "Content-Type")
        return request
    }

    static func downloadRequest(
        surfaceURL: URL,
        token: String,
        descriptor: AttachmentDescriptor
    ) throws -> URLRequest {
        guard
            descriptor.isProtocolValid,
            let endpoint = AttachmentEndpoint.object(for: surfaceURL, id: descriptor.id)
        else {
            throw AttachmentFailure.invalidEndpoint
        }
        var request = URLRequest(url: endpoint)
        request.httpMethod = "GET"
        request.timeoutInterval = 135
        request.setValue("Bearer \(token)", forHTTPHeaderField: "Authorization")
        return request
    }

    static func descriptor(from data: Data, response: URLResponse) throws -> AttachmentDescriptor {
        guard data.count <= maximumAttachmentResponseBytes,
              let response = response as? HTTPURLResponse
        else {
            throw AttachmentFailure.invalidResponse
        }
        guard response.statusCode == 200 else {
            if let failure = try? JSONDecoder().decode(GatewayErrorResponse.self, from: data) {
                throw AttachmentFailure.gateway(failure.error.message)
            }
            throw AttachmentFailure.gateway("The host rejected the attachment.")
        }
        let descriptor = try JSONDecoder().decode(AttachmentDescriptor.self, from: data)
        guard descriptor.isProtocolValid else {
            throw AttachmentFailure.invalidResponse
        }
        return descriptor
    }

    static func downloadedData(
        _ data: Data,
        response: URLResponse,
        descriptor: AttachmentDescriptor
    ) throws -> Data {
        guard data.count <= scufrisMaximumAttachmentBytes,
              data.count == Int(descriptor.size),
              let response = response as? HTTPURLResponse,
              response.statusCode == 200,
              response.value(forHTTPHeaderField: "Content-Type") == descriptor.mediaType
        else {
            throw AttachmentFailure.invalidResponse
        }
        return data
    }

    static func mediaType(for url: URL) -> String {
        UTType(filenameExtension: url.pathExtension)?.preferredMIMEType
            ?? "application/octet-stream"
    }

    static func validDisplayName(_ name: String) -> Bool {
        !name.isEmpty
            && name.utf8.count <= 255
            && !name.contains("/")
            && !name.contains("\\")
            && name.unicodeScalars.allSatisfy { !CharacterSet.controlCharacters.contains($0) }
    }

    static func validMediaType(_ mediaType: String) -> Bool {
        mediaType.utf8.count <= 127
            && mediaType.range(
                of: #"^[A-Za-z0-9!#$&^_.+-]+/[A-Za-z0-9!#$&^_.+-]+$"#,
                options: .regularExpression
            ) != nil
    }
}

enum AttachmentFailure: LocalizedError {
    case tooMany
    case invalidSelection
    case tooLarge
    case invalidEndpoint
    case invalidResponse
    case gateway(String)
    case saveFailed

    var errorDescription: String? {
        switch self {
        case .tooMany:
            "A message can contain at most 8 attachments."
        case .invalidSelection:
            "Choose a regular file with a valid name."
        case .tooLarge:
            "The attachment is larger than 16 MiB."
        case .invalidEndpoint:
            "The private attachment endpoint is invalid."
        case .invalidResponse:
            "The host returned an invalid attachment."
        case let .gateway(message):
            message
        case .saveFailed:
            "The attachment could not be prepared."
        }
    }
}
