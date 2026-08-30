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
