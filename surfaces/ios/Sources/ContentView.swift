import SwiftUI

struct ContentView: View {
    @StateObject private var store = ConversationStore()
    @State private var isShowingSetup = false
    @State private var draft = ""

    var body: some View {
        NavigationStack {
            Group {
                if store.conversation.isEmpty {
                    emptyConversation
                } else {
                    conversationList
                }
            }
            .navigationTitle("Conversation")
            .toolbar {
                ToolbarItem(placement: .topBarTrailing) {
                    Button {
                        isShowingSetup = true
                    } label: {
                        Image(systemName: "gearshape")
                    }
                    .accessibilityLabel("Configure backend")
                }
            }
            .safeAreaInset(edge: .bottom) {
                composer
            }
            .sheet(isPresented: $isShowingSetup) {
                SetupView(settings: store.settings) { settings in
                    try store.configure(settings)
                }
            }
        }
    }

    private var emptyConversation: some View {
        VStack(spacing: 20) {
            Image(systemName: "message.badge.waveform.fill")
                .font(.system(size: 60))
                .foregroundStyle(.tint)
                .accessibilityHidden(true)

            Text(store.connectionState.label)
                .font(.headline)

            if !store.serviceDetail.isEmpty {
                Text(store.serviceDetail)
                    .multilineTextAlignment(.center)
                    .foregroundStyle(.secondary)
            }

            switch store.connectionState {
            case .unconfigured:
                Button("Configure Backend") {
                    isShowingSetup = true
                }
                .buttonStyle(.borderedProminent)
            case .disconnected:
                Button("Reconnect") {
                    store.reconnect()
                }
                .buttonStyle(.borderedProminent)
            case .connecting:
                ProgressView()
            case .connected:
                Text("Send a message to start the conversation.")
                    .foregroundStyle(.secondary)
            }
        }
        .padding(32)
    }

    private var conversationList: some View {
        ScrollViewReader { proxy in
            List(store.conversation) { entry in
                ConversationRow(entry: entry)
                    .id(entry.id)
                    .listRowSeparator(.hidden)
            }
            .listStyle(.plain)
            .onChange(of: store.conversation.count) {
                if let last = store.conversation.last {
                    proxy.scrollTo(last.id, anchor: .bottom)
                }
            }
        }
    }

    private var composer: some View {
        HStack(alignment: .bottom, spacing: 12) {
            TextField("Message Scufris", text: $draft, axis: .vertical)
                .lineLimit(1 ... 5)
                .textFieldStyle(.roundedBorder)
                .onSubmit(submit)

            Button(action: submit) {
                Image(systemName: "arrow.up.circle.fill")
                    .font(.title)
            }
            .disabled(!canSubmit)
            .accessibilityLabel("Send message")
        }
        .padding()
        .background(.bar)
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
}

private struct ConversationRow: View {
    let entry: ConversationEntry

    var body: some View {
        HStack {
            if entry.role == .assistant {
                bubble
                Spacer(minLength: 40)
            } else {
                Spacer(minLength: 40)
                bubble
            }
        }
        .padding(.vertical, 4)
    }

    private var bubble: some View {
        VStack(alignment: .leading, spacing: 6) {
            Text(entry.text)
                .textSelection(.enabled)
            if let details = entry.details {
                DisclosureGroup("Details") {
                    Text(details)
                        .font(.footnote)
                        .textSelection(.enabled)
                }
            }
        }
        .padding(12)
        .background(entry.role == .assistant ? Color.secondary.opacity(0.15) : Color.accentColor)
        .foregroundStyle(entry.role == .assistant ? Color.primary : Color.white)
        .clipShape(RoundedRectangle(cornerRadius: 16))
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
                Section("Scufris Backend") {
                    TextField("wss://nixos.example.ts.net", text: $backendURL)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                        .keyboardType(.URL)
                    SecureField("Pairing token", text: $accessToken)
                        .textInputAutocapitalization(.never)
                        .autocorrectionDisabled()
                }

                Section {
                    Text("The URL and token are stored only in this device's Keychain.")
                        .foregroundStyle(.secondary)
                }

                if let errorMessage {
                    Section {
                        Text(errorMessage)
                            .foregroundStyle(.red)
                    }
                }
            }
            .navigationTitle("Connection")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .cancellationAction) {
                    Button("Cancel") {
                        dismiss()
                    }
                }
                ToolbarItem(placement: .confirmationAction) {
                    Button("Save") {
                        save()
                    }
                }
            }
        }
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

#Preview {
    ContentView()
}
