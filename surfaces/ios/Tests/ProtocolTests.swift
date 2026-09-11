import Foundation
import Testing
@testable import Scufris

struct ProtocolTests {
    @Test
    func connectionSettingsRequireSecureWebSocketsAndBoundedTokens() {
        #expect(
            ConnectionSettings(
                backendURL: "wss://nixos.example.ts.net",
                token: String(repeating: "a", count: 32)
            ).isComplete
        )
        #expect(
            !ConnectionSettings(
                backendURL: "ws://nixos.example.ts.net",
                token: String(repeating: "a", count: 32)
            ).isComplete
        )
        #expect(
            !ConnectionSettings(
                backendURL: "wss://nixos.example.ts.net",
                token: "short"
            ).isComplete
        )
    }

    @Test
    func helloUsesTheStrictProtocolSurfaceShape() throws {
        let hello = SurfaceHello(
            surface: SurfaceRegistration(
                id: "ios-test",
                name: "Test iPhone",
                widgets: []
            )
        )
        let object = try #require(
            JSONSerialization.jsonObject(with: JSONEncoder().encode(hello))
                as? [String: Any]
        )
        #expect(object["v"] as? Int == 11)
        #expect(object["type"] as? String == "surface.hello")
        let surface = try #require(object["surface"] as? [String: Any])
        #expect(surface["id"] as? String == "ios-test")
        #expect(surface["name"] as? String == "Test iPhone")
        #expect((surface["widgets"] as? [Any])?.isEmpty == true)
    }

    @Test
    func backendStateMapsToTheTextSurfaceGrammar() {
        #expect(SurfaceVisualState.connected(serviceState: "idle") == .idle)
        #expect(SurfaceVisualState.connected(serviceState: "working") == .working)
        #expect(SurfaceVisualState.working.showsThinking)
        #expect(!SurfaceVisualState.idle.showsThinking)
        #expect(SurfaceVisualState.connected(serviceState: "blocked") == .attention)
        #expect(SurfaceVisualState.connected(serviceState: "failed") == .error)
        #expect(SurfaceVisualState.connected(serviceState: "starting") == .connecting)
        #expect(SurfaceVisualState.connected(serviceState: "unknown") == .idle)
    }

    @Test
    func stateSaysWhichProcessIsTheAgent() throws {
        let handed = try JSONDecoder().decode(
            IncomingState.self,
            from: Data(
                #"{"v":11,"type":"surface.state","state":"starting","detail":"A terminal holds the agent.","holder":"terminal"}"#.utf8
            )
        )
        #expect(handed.v == scufrisProtocolVersion)
        #expect(handed.holder == .terminal)
        // The header says where the conversation is being answered when that
        // is not the usual place, and says nothing when it is.
        #expect(handed.holder?.label == "in a terminal")
        #expect(AgentHolder.managed.label == "")

        // A host that never hands the agent over sends no holder at all, so an
        // older payload still decodes and means the managed child.
        let managed = try JSONDecoder().decode(
            IncomingState.self,
            from: Data(#"{"v":11,"type":"surface.state","state":"idle","detail":""}"#.utf8)
        )
        #expect(managed.holder == nil)
    }

    @Test
    func attachmentsUseTheAuthenticatedHTTPSSiblingOfTheSurface() throws {
        let production = URL(string: "wss://nixos.example.ts.net/")!
        #expect(
            AttachmentEndpoint.collection(for: production, name: "diagram 1.png")?.absoluteString
                == "https://nixos.example.ts.net/attachments?name=diagram%201.png"
        )
        #expect(
            AttachmentEndpoint.object(for: production, id: "att_one")?.absoluteString
                == "https://nixos.example.ts.net/attachments/att_one"
        )

        let staging = URL(string: "wss://nixos.example.ts.net/scufris-staging")!
        #expect(
            AttachmentEndpoint.collection(for: staging, name: "diagram.png")?.absoluteString
                == "https://nixos.example.ts.net/scufris-staging/attachments?name=diagram.png"
        )
        #expect(
            AttachmentEndpoint.object(for: staging, id: "att_one")?.absoluteString
                == "https://nixos.example.ts.net/scufris-staging/attachments/att_one"
        )

        let request = try AttachmentTransfer.uploadRequest(
            surfaceURL: staging,
            token: String(repeating: "a", count: 32),
            name: "diagram.png",
            mediaType: "image/png"
        )
        #expect(request.httpMethod == "POST")
        #expect(request.value(forHTTPHeaderField: "Authorization") == "Bearer \(String(repeating: "a", count: 32))")
        #expect(request.value(forHTTPHeaderField: "Content-Type") == "image/png")
    }

    @Test
    func attachmentIDsArePresentInSurfaceSubmissions() throws {
        let request = SurfaceMessageRequest(
            id: "ios-message",
            text: "Inspect these.",
            attachments: ["att_one", "att_two"]
        )
        let object = try #require(
            JSONSerialization.jsonObject(with: JSONEncoder().encode(request))
                as? [String: Any]
        )
        #expect(object["v"] as? Int == 11)
        #expect(object["type"] as? String == "surface.message")
        #expect(object["attachments"] as? [String] == ["att_one", "att_two"])
    }

    @Test
    func dictationUsesTheAuthenticatedHTTPSSiblingOfTheSurface() throws {
        let production = try #require(
            TranscriptionEndpoint.url(
                for: URL(string: "wss://nixos.example.ts.net/")!
            )
        )
        #expect(production.absoluteString == "https://nixos.example.ts.net/audio/transcription")

        let staging = try #require(
            TranscriptionEndpoint.url(
                for: URL(string: "wss://nixos.example.ts.net/scufris-staging")!
            )
        )
        #expect(
            staging.absoluteString
                == "https://nixos.example.ts.net/scufris-staging/audio/transcription"
        )
        #expect(
            TranscriptionEndpoint.url(
                for: URL(string: "wss://nixos.example.ts.net/surface")!
            )?.absoluteString == "https://nixos.example.ts.net/audio/transcription"
        )
        #expect(
            TranscriptionEndpoint.url(
                for: URL(string: "https://nixos.example.ts.net/")!
            ) == nil
        )
    }

    @Test
    func dictationStateKeepsReviewExplicitAndFailuresInactive() {
        #expect(DictationState.recording.isActive)
        #expect(DictationState.transcribing.isActive)
        #expect(!DictationState.reviewing.isActive)
        #expect(DictationState.reviewing.canBegin)
        #expect(!DictationState.recording.canBegin)
        #expect(DictationState.reviewing.notice?.contains("EDIT") == true)
        #expect(!DictationState.failed("no microphone").isActive)
    }

    @Test
    func attachmentDescriptorsHaveTheCrossLanguageBounds() {
        let descriptor = AttachmentDescriptor(
            id: "att_0123456789",
            name: "diagram.png",
            mediaType: "image/png",
            size: 184_223
        )
        #expect(descriptor.isProtocolValid)
        #expect(
            !AttachmentDescriptor(
                id: descriptor.id,
                name: "../secret",
                mediaType: descriptor.mediaType,
                size: descriptor.size
            ).isProtocolValid
        )
        #expect(
            !AttachmentDescriptor(
                id: descriptor.id,
                name: descriptor.name,
                mediaType: "image png",
                size: descriptor.size
            ).isProtocolValid
        )
    }

    @Test
    func conversationResponsesDecodeWithoutWidgetPresentation() throws {
        let data = Data(
            #"{"v":11,"type":"surface.message","role":"assistant","surface":"desk","text":"Done.","details":"Passed.","attachments":[]}"#.utf8
        )
        let message = try JSONDecoder().decode(
            IncomingConversationMessage.self,
            from: data
        )
        #expect(message.v == 11)
        #expect(message.role == .assistant)
        #expect(message.text == "Done.")
        #expect(message.details == "Passed.")
        #expect(message.attachments?.isEmpty == true)

        let omitted = Data(
            #"{"v":11,"type":"surface.message","role":"user","surface":"ios","text":"Hello."}"#.utf8
        )
        let textOnly = try JSONDecoder().decode(IncomingConversationMessage.self, from: omitted)
        #expect(textOnly.details == nil)
        #expect(textOnly.attachments == nil)
        #expect(textOnly.receipts == nil)
    }

    @Test
    func badgesAreGroupedByTheJobTheyAreAbout() throws {
        let data = Data(
            #"{"v":11,"type":"surface.message","role":"assistant","surface":"desk","text":"Done.","receipts":[{"job_id":"750a4de8a80d","badges":[{"label":"landed","value":"yes","state":"measured"},{"label":"pushed","value":"no","state":"refuted"}],"offers":[{"id":"offer-a1","label":"push master"}]}]}"#.utf8
        )
        let message = try JSONDecoder().decode(
            IncomingConversationMessage.self,
            from: data
        )
        let citation = try #require(message.receipts?.first)
        // The identifier does the binding: nothing reads the prose above it.
        #expect(citation.jobID == "750a4de8a80d")
        #expect(citation.badges?.map(\.state) == [.measured, .refuted])
        #expect(citation.offers?.first?.label == "push master")
        // Spent is the service's to say, and it has not said it yet.
        #expect(citation.offers?.first?.taken == nil)
    }

    @Test
    func jobRowsCarryEverythingOneRowDraws() throws {
        let data = Data(
            #"{"v":11,"type":"surface.jobs","jobs":[{"id":"01ccbac98b97","project":"personal/scufris2","state":"done","since":1788901200,"summary":"reviewed 9 commits"},{"id":"3f81c204b1e9","project":null,"state":"working","since":1788904800,"summary":""}]}"#.utf8
        )
        let listed = try JSONDecoder().decode(IncomingJobs.self, from: data)
        #expect(listed.jobs.count == 2)
        // A finished row is filed, a live one is stopped. The control a row
        // has is what says which it is.
        #expect(listed.jobs[0].isTerminal)
        #expect(!listed.jobs[1].isTerminal)
        // A project is optional, and a job that has said nothing yet is a row
        // with no summary. Both are still rows.
        #expect(listed.jobs[1].project == nil)
        #expect(listed.jobs[1].summary.isEmpty)
    }

    @Test
    func aRestartedLogicalJobCanReturnAfterItsPriorSnapshotWasCleared() throws {
        let cleared = try JSONDecoder().decode(
            IncomingJobs.self,
            from: Data(#"{"v":11,"type":"surface.jobs","jobs":[]}"#.utf8)
        )
        let restarted = try JSONDecoder().decode(
            IncomingJobs.self,
            from: Data(
                #"{"v":11,"type":"surface.jobs","jobs":[{"id":"01ccbac98b97","project":"personal/scufris2","state":"working","since":1788901200,"summary":"foreground guidance submitted"}]}"#.utf8
            )
        )

        // Each frame is a complete snapshot. A later execution keeps the
        // logical ID, and the iPhone accepts it again as an active row.
        #expect(cleared.jobs.isEmpty)
        #expect(restarted.jobs.map(\.id) == ["01ccbac98b97"])
        #expect(restarted.jobs.first?.state == .working)
        #expect(restarted.jobs.first?.isTerminal == false)
    }

    @Test
    func briefingRowsKeepCollectionAndDeliveryIndependent() throws {
        let data = Data(
            #"{"v":11,"type":"surface.briefings","briefings":[{"id":"generation-a","date":"2026-09-10","profile":"nightly","collection":"collected","delivery":"in_progress","since":1788901200,"completed":3,"total":3,"failed":1,"summary":"2 of 3 sources answered; 1 failed"}]}"#.utf8
        )
        let listed = try JSONDecoder().decode(IncomingBriefings.self, from: data)
        let row = try #require(listed.briefings.first)
        #expect(row.isProtocolValid)
        #expect(row.collection == .collected)
        #expect(row.delivery == .inProgress)
        #expect(row.failed == 1)
        #expect(row.isActive)
        #expect(!row.requiresAttention)

        let deliveredPartial = BriefingRow(
            id: row.id,
            date: row.date,
            profile: row.profile,
            collection: .collected,
            delivery: .delivered,
            since: row.since,
            completed: row.completed,
            total: row.total,
            failed: row.failed,
            summary: row.summary
        )
        #expect(!deliveredPartial.isActive)
        #expect(deliveredPartial.requiresAttention)
        #expect(deliveredPartial.canDismiss)
        let deliveredSuccess = BriefingRow(
            id: row.id,
            date: row.date,
            profile: row.profile,
            collection: .collected,
            delivery: .delivered,
            since: row.since,
            completed: row.completed,
            total: row.total,
            failed: 0,
            summary: row.summary
        )
        #expect(!deliveredSuccess.requiresAttention)
        let stopped = BriefingRow(
            id: row.id,
            date: row.date,
            profile: row.profile,
            collection: .collected,
            delivery: .failed,
            since: row.since,
            completed: row.completed,
            total: row.total,
            failed: 0,
            summary: "delivery stopped; restart the Scufris service"
        )
        #expect(!stopped.isActive)
        #expect(stopped.requiresAttention)
        #expect(!stopped.canDismiss)
    }

    @Test
    func briefingDrawerTransitionsKeepActiveAndLatestAttentionRows() {
        func row(
            _ id: String,
            since: UInt64,
            collection: BriefingCollectionState,
            delivery: BriefingDeliveryState,
            failed: UInt32
        ) -> BriefingRow {
            BriefingRow(
                id: id,
                date: "2026-09-10",
                profile: "nightly",
                collection: collection,
                delivery: delivery,
                since: since,
                completed: collection == .collecting ? 1 : 3,
                total: 3,
                failed: failed,
                summary: "measured result"
            )
        }
        let listed = [
            row("stopped", since: 50, collection: .collected, delivery: .failed, failed: 0),
            row("writing", since: 40, collection: .collected, delivery: .inProgress, failed: 0),
            row("success", since: 5, collection: .collected, delivery: .delivered, failed: 0),
            row("partial", since: 30, collection: .collected, delivery: .delivered, failed: 1),
            row("active", since: 20, collection: .collecting, delivery: .pending, failed: 0),
            row("failed", since: 10, collection: .failed, delivery: .delivered, failed: 3),
        ]

        let collapsed = BriefingDrawerPresentation(rows: listed, expanded: false)
        #expect(collapsed.rows.map(\.id) == ["active", "writing", "stopped"])
        #expect(collapsed.activeCount == 2)
        #expect(collapsed.attentionCount == 3)
        #expect(collapsed.hiddenAttentionCount == 2)

        let expanded = BriefingDrawerPresentation(rows: listed, expanded: true)
        #expect(expanded.rows.map(\.id) == ["failed", "active", "partial", "writing", "stopped"])
        #expect(expanded.hiddenAttentionCount == 0)
    }

    @Test
    func aRowControlAndAnOfferCarryOnlyTheirIdentifier() throws {
        let command = try #require(
            JSONSerialization.jsonObject(
                with: JSONEncoder().encode(
                    SurfaceJobCommandRequest(id: "3f81c204b1e9", action: .cancel)
                )
            ) as? [String: Any]
        )
        #expect(command["v"] as? Int == 11)
        #expect(command["type"] as? String == "job.command")
        #expect(command["action"] as? String == "cancel")

        let dismiss = try #require(
            JSONSerialization.jsonObject(
                with: JSONEncoder().encode(
                    SurfaceBriefingDismissRequest(id: "generation-a")
                )
            ) as? [String: Any]
        )
        #expect(dismiss["type"] as? String == "briefing.dismiss")
        #expect(dismiss.keys.sorted() == ["id", "type", "v"])

        let take = try #require(
            JSONSerialization.jsonObject(
                with: JSONEncoder().encode(SurfaceOfferTakeRequest(id: "offer-a1"))
            ) as? [String: Any]
        )
        #expect(take["type"] as? String == "offer.take")
        // The words behind an offer never leave the extension.
        #expect(take.keys.sorted() == ["id", "type", "v"])
    }
}
