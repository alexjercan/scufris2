import CoreGraphics

/// Whether the conversation keeps the newest message in view, and what is
/// waiting at the bottom when it does not.
///
/// This is the whole of the decision, kept apart from the view that draws it so
/// it can be reasoned about and tested without a scroll view. The view reports
/// one number - how far the bottom of the content is below the bottom of the
/// window - and this decides the rest.
///
/// The desktop conversation window holds the same contract in
/// `surfaces/desktop/ui/hud.ts`, down to the threshold: a reader is at the
/// bottom of a conversation well before they are at the last pixel of it, and a
/// surface that followed only an exact bottom would stop following the moment
/// one line wrapped further than the last one did.
struct ConversationFollow: Equatable {
    /// How near the bottom still counts as reading the newest message.
    static let nearBottom: CGFloat = 24

    /// True while the newest message is to be kept in view.
    private(set) var isFollowing = true

    /// Messages that have arrived since the reader stopped following.
    private(set) var unseen = 0

    static func isNearBottom(_ distance: CGFloat) -> Bool {
        distance < nearBottom
    }

    /// Reports where the reader is, and settles what follows from it.
    ///
    /// The position is measured rather than remembered. A drag, a momentum
    /// scroll, a keyboard appearing under the list, a thumbnail that finished
    /// loading and a details disclosure opening all move it, and the only
    /// account of it that cannot go stale is the scroll view's own geometry.
    mutating func observe(distanceFromBottom distance: CGFloat) {
        if Self.isNearBottom(distance) {
            caughtUp()
        } else {
            isFollowing = false
        }
    }

    /// Counts messages that arrived while the reader was reading something else.
    mutating func appended(_ count: Int) {
        guard !isFollowing, count > 0 else { return }
        unseen += count
    }

    /// The reader is at the newest message, however they arrived at it.
    mutating func caughtUp() {
        isFollowing = true
        unseen = 0
    }

    /// True while there is somewhere for the way-back control to go.
    var showsLatestControl: Bool {
        !isFollowing
    }

    /// True while messages the reader has not reached are waiting below.
    var hasUnseen: Bool {
        unseen > 0
    }

    /// What VoiceOver calls the way-back control, including what is waiting.
    var latestControlLabel: String {
        switch unseen {
        case 0: "Jump to the latest message"
        case 1: "Jump to the latest message, 1 new"
        default: "Jump to the latest message, \(unseen) new"
        }
    }

    /// Said once, when the conversation moves on without the reader. Repeating
    /// it for every message that follows would talk over what they are reading.
    var announcement: String {
        switch unseen {
        case 0: ""
        case 1: "1 new message below"
        default: "\(unseen) new messages below"
        }
    }
}

/// Where the conversation's content sits inside its window.
///
/// `minY` is the top of the content in the scroll view's own space: zero at the
/// top of the conversation and increasingly negative as it is scrolled down.
struct ConversationGeometry: Equatable {
    var contentHeight: CGFloat = 0
    var contentMinY: CGFloat = 0
    var viewportHeight: CGFloat = 0

    /// How far the end of the conversation is below the end of the window.
    ///
    /// Never negative: a conversation shorter than the window, and the rubber
    /// band at either end of a scroll, both put the content past its own
    /// bottom, and neither is somewhere to scroll back from.
    var distanceFromBottom: CGFloat {
        max(0, contentHeight + contentMinY - viewportHeight)
    }
}
