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
    func helloUsesTheStrictProtocolV4SurfaceShape() throws {
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
        #expect(object["v"] as? Int == 4)
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
    func conversationResponsesDecodeWithoutWidgetPresentation() throws {
        let data = Data(
            #"{"v":4,"type":"surface.message","role":"assistant","surface":"desk","text":"Done.","details":"Passed."}"#.utf8
        )
        let message = try JSONDecoder().decode(
            IncomingConversationMessage.self,
            from: data
        )
        #expect(message.v == 4)
        #expect(message.role == .assistant)
        #expect(message.text == "Done.")
        #expect(message.details == "Passed.")
    }
}
