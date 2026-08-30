import SwiftUI

struct ContentView: View {
    @StateObject private var store = ConversationStore()
    @State private var isShowingSetup = false
    @State private var draft = ""

    var body: some View {
        ZStack {
            ScufrisPalette.background
                .ignoresSafeArea()

            VStack(spacing: 0) {
                header
                Divider().overlay(ScufrisPalette.line)
                conversation
                statusNotice
                composer
            }
        }
        .preferredColorScheme(.dark)
        .sheet(isPresented: $isShowingSetup) {
            SetupView(settings: store.settings) { settings in
                try store.configure(settings)
            }
            .preferredColorScheme(.dark)
        }
    }

    private var header: some View {
        VStack(alignment: .leading, spacing: 7) {
            HStack(alignment: .firstTextBaseline) {
                Text("SCUFRIS")
                    .font(.system(size: 11, weight: .bold, design: .monospaced))
                    .tracking(2.2)
                    .foregroundStyle(ScufrisPalette.quartz)

                Spacer()

                Text(store.visualState.label.uppercased())
                    .font(.system(size: 10, weight: .bold, design: .monospaced))
                    .tracking(1.6)
                    .foregroundStyle(store.visualState.accent)

                Button {
                    isShowingSetup = true
                } label: {
                    Image(systemName: "slider.horizontal.3")
                        .font(.system(size: 12, weight: .medium))
                        .foregroundStyle(ScufrisPalette.muted)
                        .frame(width: 28, height: 28)
                }
                .accessibilityLabel("Configure backend")
            }

            Text(store.routeLabel)
                .font(.system(size: 9, weight: .regular, design: .monospaced))
                .tracking(0.7)
                .foregroundStyle(ScufrisPalette.muted)
                .lineLimit(1)
                .truncationMode(.middle)
        }
        .padding(.leading, 17)
        .padding(.trailing, 10)
        .padding(.top, 12)
        .padding(.bottom, 11)
    }

    private var conversation: some View {
        ScrollViewReader { proxy in
            ScrollView {
                LazyVStack(spacing: 17) {
                    if store.conversation.isEmpty && !store.isThinking {
                        emptyConversation
                    } else {
                        ForEach(store.conversation) { entry in
                            ConversationRow(entry: entry)
                                .id(entry.id)
                        }
                        if store.isThinking {
                            ThinkingRow()
                                .id("thinking")
                        }
                    }
                }
                .frame(maxWidth: .infinity)
                .padding(.horizontal, 17)
                .padding(.vertical, 22)
            }
            .scrollIndicators(.hidden)
            .defaultScrollAnchor(.bottom)
            .onChange(of: store.conversation.count) {
                if let last = store.conversation.last {
                    withAnimation(.easeOut(duration: 0.18)) {
                        proxy.scrollTo(last.id, anchor: .bottom)
                    }
                }
            }
            .onChange(of: store.isThinking) {
                if store.isThinking {
                    withAnimation(.easeOut(duration: 0.18)) {
                        proxy.scrollTo("thinking", anchor: .bottom)
                    }
                }
            }
        }
    }

    private var emptyConversation: some View {
        VStack(spacing: 12) {
            Spacer(minLength: 80)

            Text(store.visualState.emptyTitle)
                .font(.system(size: 11, weight: .bold, design: .monospaced))
                .tracking(1.2)
                .foregroundStyle(store.visualState.accent)

            Text(store.visualState.emptyDetail)
                .font(.system(size: 12, design: .monospaced))
                .foregroundStyle(ScufrisPalette.foreground)
                .multilineTextAlignment(.center)
                .lineSpacing(4)

            switch store.connectionState {
            case .unconfigured:
                terminalButton("CONFIGURE") {
                    isShowingSetup = true
                }
            case .disconnected:
                terminalButton("RECONNECT") {
                    store.reconnect()
                }
            case .connecting:
                ProgressView()
                    .tint(ScufrisPalette.quartz)
            case .connected:
                EmptyView()
            }

            Spacer(minLength: 80)
        }
        .frame(maxWidth: 290)
    }

    @ViewBuilder
    private var statusNotice: some View {
        if store.showsStatusNotice {
            HStack(alignment: .firstTextBaseline, spacing: 8) {
                Rectangle()
                    .fill(store.visualState.accent)
                    .frame(width: 2)

                Text(store.serviceDetail)
                    .font(.system(size: 10, weight: .medium, design: .monospaced))
                    .tracking(0.6)
                    .foregroundStyle(store.visualState.accent)
                    .lineLimit(2)

                Spacer(minLength: 0)
            }
            .padding(.horizontal, 17)
            .padding(.vertical, 8)
            .background(store.visualState.accent.opacity(0.07))
        }
    }

    private var composer: some View {
        VStack(spacing: 0) {
            Divider().overlay(ScufrisPalette.line)

            HStack(alignment: .bottom, spacing: 10) {
                Text(">")
                    .font(.system(size: 15, design: .monospaced))
                    .foregroundStyle(ScufrisPalette.quartz)
                    .padding(.bottom, 7)

                TextField("Type a message", text: $draft, axis: .vertical)
                    .font(.system(size: 14, design: .monospaced))
                    .foregroundStyle(ScufrisPalette.foregroundStrong)
                    .tint(ScufrisPalette.yellow)
                    .lineLimit(1 ... 4)
                    .submitLabel(.send)
                    .onSubmit(submit)
                    .accessibilityLabel("Type a message")

                Button(action: submit) {
                    ZStack {
                        Circle()
                            .stroke(ScufrisPalette.quartz, lineWidth: 1)
                        Image(systemName: "arrow.up")
                            .font(.system(size: 12, weight: .medium))
                            .foregroundStyle(ScufrisPalette.quartz)
                    }
                    .frame(width: 30, height: 30)
                }
                .disabled(!canSubmit)
                .opacity(canSubmit ? 1 : 0.3)
                .accessibilityLabel("Send message")
            }
            .padding(.horizontal, 15)
            .padding(.top, 12)
            .padding(.bottom, 9)

            Rectangle()
                .fill(ScufrisPalette.line)
                .frame(height: 1)
                .padding(.horizontal, 15)
                .padding(.bottom, 10)
        }
        .background(ScufrisPalette.background)
    }

    private var canSubmit: Bool {
        guard case .connected = store.connectionState else { return false }
        let text = draft.trimmingCharacters(in: .whitespacesAndNewlines)
        return !text.isEmpty && text.utf8.count <= scufrisMaximumTextBytes
    }

    private func submit() {
        guard canSubmit else { return }
        let text = draft
        draft = ""
        store.send(text)
    }

    private func terminalButton(_ title: String, action: @escaping () -> Void) -> some View {
        Button(action: action) {
            Text(title)
                .font(.system(size: 10, weight: .bold, design: .monospaced))
                .tracking(1.4)
                .foregroundStyle(ScufrisPalette.quartz)
                .padding(.horizontal, 14)
                .padding(.vertical, 9)
                .overlay(Rectangle().stroke(ScufrisPalette.lineStrong, lineWidth: 1))
        }
        .buttonStyle(.plain)
    }
}

private struct ThinkingRow: View {
    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text("SCUFRIS")
                .font(.system(size: 9, weight: .bold, design: .monospaced))
                .tracking(1.15)
                .foregroundStyle(ScufrisPalette.niagara)
                .frame(width: 67, alignment: .trailing)

            Text("thinking...")
                .font(.system(size: 13, design: .monospaced))
                .foregroundStyle(ScufrisPalette.niagara)
                .frame(maxWidth: .infinity, alignment: .leading)
        }
        .accessibilityElement(children: .combine)
        .accessibilityLabel("Scufris is thinking")
    }
}

private struct ConversationRow: View {
    let entry: ConversationEntry

    var body: some View {
        HStack(alignment: .firstTextBaseline, spacing: 12) {
            Text(entry.role == .user ? "YOU" : "SCUFRIS")
                .font(.system(size: 9, weight: .bold, design: .monospaced))
                .tracking(1.15)
                .foregroundStyle(
                    entry.role == .user
                        ? ScufrisPalette.quartz
                        : ScufrisPalette.wisteria
                )
                .frame(width: 67, alignment: .trailing)

            VStack(alignment: .leading, spacing: 9) {
                Text(entry.text)
                    .font(.system(size: 13, design: .monospaced))
                    .foregroundStyle(
                        entry.role == .assistant
                            ? ScufrisPalette.foregroundStrong
                            : ScufrisPalette.foreground
                    )
                    .lineSpacing(4)
                    .textSelection(.enabled)
                    .frame(maxWidth: .infinity, alignment: .leading)

                if let details = entry.details, !details.isEmpty {
                    DisclosureGroup {
                        Text(details)
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(ScufrisPalette.foreground)
                            .lineSpacing(3)
                            .textSelection(.enabled)
                            .padding(.top, 4)
                    } label: {
                        Text("DETAILS")
                            .font(.system(size: 9, weight: .bold, design: .monospaced))
                            .tracking(1.1)
                            .foregroundStyle(ScufrisPalette.muted)
                    }
                    .tint(ScufrisPalette.muted)
                }
            }
        }
    }
}

private struct SetupView: View {
    @Environment(\.dismiss) private var dismiss
    @State private var backendURL: String
    @State private var accessToken: String
    @State private var errorMessage: String?

    let onSave: (ConnectionSettings) throws -> Void

    init(
        settings: ConnectionSettings,
        onSave: @escaping (ConnectionSettings) throws -> Void
    ) {
        _backendURL = State(initialValue: settings.backendURL)
        _accessToken = State(initialValue: settings.token)
        self.onSave = onSave
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField("wss://nixos.example.ts.net", text: $backendURL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                    SecureField("Pairing token", text: $accessToken)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                } header: {
                    sectionLabel("SCUFRIS BACKEND")
                } footer: {
                    Text("The URL and token stay in this device's Keychain.")
                        .font(.system(size: 10, design: .monospaced))
                        .foregroundStyle(ScufrisPalette.muted)
                }

                if let errorMessage {
                    Section {
                        Text(errorMessage)
                            .font(.system(size: 11, design: .monospaced))
                            .foregroundStyle(ScufrisPalette.red)
                    }
                }
            }
            .font(.system(size: 13, design: .monospaced))
            .scrollContentBackground(.hidden)
            .background(ScufrisPalette.background)
            .navigationTitle("CONNECTION")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("CANCEL") {
                        dismiss()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("SAVE") {
                        save()
                    }
                }
            }
            .tint(ScufrisPalette.quartz)
        }
    }

    private func sectionLabel(_ text: String) -> some View {
        Text(text)
            .font(.system(size: 9, weight: .bold, design: .monospaced))
            .tracking(1.3)
            .foregroundStyle(ScufrisPalette.quartz)
    }

    private func save() {
        do {
            try onSave(
                ConnectionSettings(
                    backendURL: backendURL.trimmingCharacters(in: .whitespacesAndNewlines),
                    token: accessToken.trimmingCharacters(in: .whitespacesAndNewlines)
                )
            )
            dismiss()
        } catch {
            errorMessage = error.localizedDescription
        }
    }
}

enum SurfaceVisualState: String, Equatable {
    case setup
    case connecting
    case idle
    case working
    case attention
    case error
    case offline

    static func connected(serviceState: String) -> SurfaceVisualState {
        switch serviceState {
        case "working": .working
        case "blocked": .attention
        case "failed": .error
        case "starting": .connecting
        default: .idle
        }
    }

    var label: String {
        switch self {
        case .offline: "offline"
        default: rawValue
        }
    }

    var emptyTitle: String {
        switch self {
        case .setup: "NOT CONFIGURED"
        case .connecting: "CONNECTING"
        case .offline: "OFFLINE"
        case .error: "CONNECTION ERROR"
        default: "ONE CONVERSATION"
        }
    }

    var emptyDetail: String {
        switch self {
        case .setup: "Add the private WSS route and pairing token."
        case .connecting: "Opening the private surface route."
        case .offline, .error: "Scufris will keep trying in the background."
        default: "Type below to start."
        }
    }

    var showsThinking: Bool {
        self == .working
    }

    var accent: Color {
        switch self {
        case .setup, .idle: ScufrisPalette.quartz
        case .connecting: ScufrisPalette.brown
        case .working: ScufrisPalette.niagara
        case .attention: ScufrisPalette.wisteria
        case .error: ScufrisPalette.red
        case .offline: ScufrisPalette.lineStrong
        }
    }
}

private enum ScufrisPalette {
    static let background = Color(red: 16 / 255, green: 16 / 255, blue: 16 / 255)
    static let line = Color(red: 51 / 255, green: 48 / 255, blue: 46 / 255)
    static let lineStrong = Color(red: 82 / 255, green: 73 / 255, blue: 78 / 255)
    static let foreground = Color(red: 228 / 255, green: 228 / 255, blue: 239 / 255)
    static let foregroundStrong = Color(red: 244 / 255, green: 244 / 255, blue: 255 / 255)
    static let muted = Color(red: 108 / 255, green: 119 / 255, blue: 114 / 255)
    static let quartz = Color(red: 149 / 255, green: 169 / 255, blue: 159 / 255)
    static let yellow = Color(red: 255 / 255, green: 221 / 255, blue: 51 / 255)
    static let brown = Color(red: 204 / 255, green: 140 / 255, blue: 60 / 255)
    static let niagara = Color(red: 150 / 255, green: 166 / 255, blue: 200 / 255)
    static let wisteria = Color(red: 158 / 255, green: 149 / 255, blue: 199 / 255)
    static let red = Color(red: 244 / 255, green: 56 / 255, blue: 65 / 255)
}

#Preview {
    ContentView()
}
