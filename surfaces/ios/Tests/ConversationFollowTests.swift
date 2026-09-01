import Foundation
import Testing
@testable import Scufris

struct ConversationFollowTests {
    @Test
    func aNewConversationFollowsAndOffersNoWayBack() {
        let follow = ConversationFollow()
        #expect(follow.isFollowing)
        #expect(!follow.showsLatestControl)
        #expect(!follow.hasUnseen)
        #expect(follow.announcement.isEmpty)
    }

    @Test
    func readingAwayFromTheBottomStopsFollowingAndOffersTheWayBack() {
        var follow = ConversationFollow()
        follow.observe(distanceFromBottom: 400)
        #expect(!follow.isFollowing)
        #expect(follow.showsLatestControl)

        // Arriving at the bottom is arriving, however it was arrived at.
        follow.observe(distanceFromBottom: 0)
        #expect(follow.isFollowing)
        #expect(!follow.showsLatestControl)
    }

    @Test
    func theThresholdIsTheSameContractTheDesktopWindowHolds() {
        #expect(ConversationFollow.nearBottom == 24)
        #expect(ConversationFollow.isNearBottom(23.9))
        // One line wrapping further than the last is not the reader moving.
        #expect(!ConversationFollow.isNearBottom(ConversationFollow.nearBottom))
        #expect(!ConversationFollow.isNearBottom(25))
    }

    @Test
    func messagesAreCountedOnlyWhileTheReaderIsElsewhere() {
        var follow = ConversationFollow()
        follow.appended(3)
        #expect(follow.unseen == 0, "a follower is never behind")

        follow.observe(distanceFromBottom: 300)
        follow.appended(1)
        follow.appended(2)
        #expect(follow.unseen == 3)
        #expect(follow.hasUnseen)

        // A count of nothing is nothing, including a conversation that shrank.
        follow.appended(0)
        follow.appended(-2)
        #expect(follow.unseen == 3)
    }

    @Test
    func reachingTheNewestMessageClearsWhatWasWaiting() {
        var follow = ConversationFollow()
        follow.observe(distanceFromBottom: 300)
        follow.appended(2)

        follow.caughtUp()
        #expect(follow.isFollowing)
        #expect(follow.unseen == 0)
        #expect(!follow.showsLatestControl)
        #expect(follow.announcement.isEmpty)
    }

    @Test
    func theWayBackSaysWhatIsWaitingAtTheEndOfIt() {
        var follow = ConversationFollow()
        follow.observe(distanceFromBottom: 300)
        #expect(follow.latestControlLabel == "Jump to the latest message")

        follow.appended(1)
        #expect(follow.latestControlLabel == "Jump to the latest message, 1 new")
        #expect(follow.announcement == "1 new message below")

        follow.appended(4)
        #expect(follow.latestControlLabel == "Jump to the latest message, 5 new")
        #expect(follow.announcement == "5 new messages below")
    }

    @Test
    func aConversationShorterThanItsWindowIsAlreadyAtItsEnd() {
        let geometry = ConversationGeometry(
            contentHeight: 120,
            contentMinY: 0,
            viewportHeight: 600
        )
        #expect(geometry.distanceFromBottom == 0)

        var follow = ConversationFollow()
        follow.observe(distanceFromBottom: geometry.distanceFromBottom)
        #expect(follow.isFollowing)
        #expect(!follow.showsLatestControl)
    }

    @Test
    func theDistanceIsMeasuredFromTheEndOfTheContentToTheEndOfTheWindow() {
        // Parked at the bottom: the content's top is one window above its end.
        let bottom = ConversationGeometry(
            contentHeight: 2000,
            contentMinY: -1400,
            viewportHeight: 600
        )
        #expect(bottom.distanceFromBottom == 0)

        // Scrolled up by 500.
        let read = ConversationGeometry(
            contentHeight: 2000,
            contentMinY: -900,
            viewportHeight: 600
        )
        #expect(read.distanceFromBottom == 500)

        // Rubber-banded past the end is still the end, not somewhere to
        // scroll back from.
        let stretched = ConversationGeometry(
            contentHeight: 2000,
            contentMinY: -1480,
            viewportHeight: 600
        )
        #expect(stretched.distanceFromBottom == 0)
    }

    @Test
    func contentGrowingUnderAReaderDoesNotMoveThem() {
        var follow = ConversationFollow()
        var geometry = ConversationGeometry(
            contentHeight: 2000,
            contentMinY: -900,
            viewportHeight: 600
        )
        follow.observe(distanceFromBottom: geometry.distanceFromBottom)
        #expect(!follow.isFollowing)

        // A thumbnail finished loading below them. They are further from the
        // bottom than before, and still exactly where they were reading.
        geometry.contentHeight = 2400
        follow.observe(distanceFromBottom: geometry.distanceFromBottom)
        #expect(!follow.isFollowing)
        #expect(geometry.distanceFromBottom == 900)
    }
}
