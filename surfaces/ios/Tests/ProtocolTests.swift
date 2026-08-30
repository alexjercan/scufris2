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
    func helloUsesTheStrictProtocolV5SurfaceShape() throws {
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
        #expect(object["v"] as? Int == 5)
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
        #expect(object["v"] as? Int == 5)
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
            #"{"v":5,"type":"surface.message","role":"assistant","surface":"desk","text":"Done.","details":"Passed.","attachments":[]}"#.utf8
        )
        let message = try JSONDecoder().decode(
            IncomingConversationMessage.self,
            from: data
        )
        #expect(message.v == 5)
        #expect(message.role == .assistant)
        #expect(message.text == "Done.")
        #expect(message.details == "Passed.")
        #expect(message.attachments?.isEmpty == true)

        let omitted = Data(
            #"{"v":5,"type":"surface.message","role":"user","surface":"ios","text":"Hello."}"#.utf8
        )
        let textOnly = try JSONDecoder().decode(IncomingConversationMessage.self, from: omitted)
        #expect(textOnly.attachments == nil)
    }
}
