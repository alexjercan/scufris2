import SwiftUI

struct ContentView: View {
    @State private var isShowingSetup = false

    var body: some View {
        NavigationStack {
            VStack(spacing: 24) {
                Image(systemName: "message.badge.waveform.fill")
                    .font(.system(size: 64))
                    .foregroundStyle(.tint)
                    .accessibilityHidden(true)

                VStack(spacing: 8) {
                    Text("Scufris")
                        .font(.largeTitle.bold())
                    Text("Not connected")
                        .font(.headline)
                        .foregroundStyle(.secondary)
                }

                Text("Connect this iPhone to your Scufris backend to continue a conversation by text or voice.")
                    .multilineTextAlignment(.center)
                    .foregroundStyle(.secondary)

                Button("Configure Backend") {
                    isShowingSetup = true
                }
                .buttonStyle(.borderedProminent)
            }
            .padding(32)
            .navigationTitle("Conversation")
            .sheet(isPresented: $isShowingSetup) {
                SetupView()
            }
        }
    }
}

private struct SetupView: View {
    @Environment(\.dismiss) private var dismiss
    @State private var backendURL = ""
    @State private var accessToken = ""

    var body: some View {
        NavigationStack {
            Form {
                Section("Scufris Backend") {
                    TextField("https://scufris.example.com", text: $backendURL)
                        .textInputAutocapitalization(.never)
                        .keyboardType(.URL)
                    SecureField("Pairing token", text: $accessToken)
                        .textInputAutocapitalization(.never)
                }

                Section {
                    Text("This bootstrap build does not connect or save credentials yet.")
                        .foregroundStyle(.secondary)
                }
            }
            .navigationTitle("Connection")
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .confirmationAction) {
                    Button("Done") {
                        dismiss()
                    }
                }
            }
        }
    }
}

#Preview {
    ContentView()
}
