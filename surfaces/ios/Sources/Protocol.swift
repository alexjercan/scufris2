import Foundation

let scufrisProtocolVersion = 10
let scufrisMaximumMessageBytes = 64 * 1024
let scufrisMaximumTextBytes = 8 * 1024
let scufrisMaximumDetailsBytes = 32 * 1024

func conversationDetailsAreValid(_ details: String?) -> Bool {
    guard let details else { return true }
    return !details.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty
        && details.utf8.count <= scufrisMaximumDetailsBytes
        && !details.contains("\0")
        && !details.contains("\r")
}

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

/// One row control, pressed. `cancel` stops the job; `archive` only files it.
enum JobAction: String, Codable {
    case cancel
    case archive
}

struct SurfaceJobCommandRequest: Encodable {
    let v = scufrisProtocolVersion
    let type = "job.command"
    let id: String
    let action: JobAction
}

/// Presentation dismissal for one terminal delivered briefing. The service
/// retains the audit row, canonical response, and collection artifacts.
struct SurfaceBriefingDismissRequest: Encodable {
    let v = scufrisProtocolVersion
    let type = "briefing.dismiss"
    let id: String
}

/// Taking one offer. The words behind it never leave the extension: this
/// carries the identifier and the host submits what it stored against it.
struct SurfaceOfferTakeRequest: Encodable {
    let v = scufrisProtocolVersion
    let type = "offer.take"
    let id: String
}

struct IncomingEnvelope: Decodable {
    let v: Int
    let type: String
}

/// What one measured fact says, in the only four words a badge has.
///
/// `unknown` is not a no. A fetch that failed leaves the fact unmeasured, and
/// drawing that as a refusal would invent the one thing the receipt was
/// careful not to claim.
enum ReceiptState: String, Codable, Equatable {
    case measured
    case refuted
    case claimed
    case unknown
}

struct ReceiptBadge: Codable, Equatable, Identifiable {
    let label: String
    let value: String
    let state: ReceiptState

    var id: String { "\(label)=\(value)" }
}

struct Offer: Codable, Equatable, Identifiable {
    let id: String
    let label: String
    var taken: Bool?
}

/// Every badge one message carries about one job, led by that job's own id.
struct Citation: Codable, Equatable, Identifiable {
    let jobID: String
    var badges: [ReceiptBadge]?
    var offers: [Offer]?

    var id: String { jobID }

    enum CodingKeys: String, CodingKey {
        case badges, offers
        case jobID = "job_id"
    }
}

/// One delegated job. A row outlives its job: filing it is what clears it.
struct JobRow: Codable, Equatable, Identifiable {
    let id: String
    let project: String?
    let state: JobRowState
    /// Unix seconds the job started, which the row shows the age of.
    let since: UInt64
    let summary: String

    var isTerminal: Bool { state == .done || state == .failed }
}

enum JobRowState: String, Codable, Equatable {
    case working
    case blocked
    case done
    case failed
}

/// One generation-fenced scheduled briefing. It is quiet surface state.
struct BriefingRow: Codable, Equatable, Identifiable {
    let id: String
    let date: String
    let profile: String
    let collection: BriefingCollectionState
    let delivery: BriefingDeliveryState
    let since: UInt64
    let completed: UInt32
    let total: UInt32
    let failed: UInt32
    let summary: String

    var isActive: Bool {
        collection == .collecting || delivery == .pending || delivery == .inProgress
    }

    var requiresAttention: Bool {
        delivery == .failed
            || (delivery == .delivered
                && (collection == .failed || (collection == .collected && failed > 0)))
    }

    var canDismiss: Bool {
        delivery == .delivered && requiresAttention
    }

    var isProtocolValid: Bool {
        let identifier = #"^[A-Za-z0-9._-]{1,64}$"#
        return id.range(of: identifier, options: .regularExpression) != nil
            && profile.range(of: identifier, options: .regularExpression) != nil
            && date.range(of: #"^[0-9]{4}-[0-9]{2}-[0-9]{2}$"#, options: .regularExpression) != nil
            && completed <= total && failed <= completed
            && summary.utf8.count <= 256
            && !summary.contains("\0") && !summary.contains("\r")
    }
}

enum BriefingCollectionState: String, Codable, Equatable {
    case collecting
    case collected
    case failed
}

enum BriefingDeliveryState: String, Codable, Equatable {
    case pending
    case inProgress = "in_progress"
    case failed
    case delivered
}

/// The deterministic visible slice of the whole briefing list.
struct BriefingDrawerPresentation: Equatable {
    let rows: [BriefingRow]
    let activeCount: Int
    let attentionCount: Int
    let hiddenAttentionCount: Int

    init(rows: [BriefingRow], expanded: Bool) {
        let relevant = rows
            .filter { $0.isActive || $0.requiresAttention }
            .sorted {
                $0.since == $1.since ? $0.id < $1.id : $0.since < $1.since
            }
        let attention = relevant.filter(\.requiresAttention)
        let latestAttention = attention.last
        self.rows = expanded
            ? relevant
            : relevant.filter { $0.isActive || $0.id == latestAttention?.id }
        activeCount = relevant.filter(\.isActive).count
        attentionCount = attention.count
        hiddenAttentionCount = expanded ? 0 : relevant.count - self.rows.count
    }
}

struct IncomingConversationMessage: Decodable {
    let v: Int
    let type: String
    let role: ConversationRole
    let surface: String
    let text: String
    let details: String?
    let attachments: [AttachmentDescriptor]?
    let receipts: [Citation]?
}

struct IncomingJobs: Decodable {
    let v: Int
    let type: String
    let jobs: [JobRow]
}

struct IncomingBriefings: Decodable {
    let v: Int
    let type: String
    let briefings: [BriefingRow]
}

struct IncomingOfferTaken: Decodable {
    let v: Int
    let type: String
    let id: String
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
    var receipts: [Citation] = []
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
