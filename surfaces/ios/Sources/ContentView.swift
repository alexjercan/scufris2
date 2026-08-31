import PhotosUI
import QuickLook
import QuickLookThumbnailing
import SwiftUI
import UniformTypeIdentifiers
import UIKit

struct ContentView: View {
    @StateObject private var store = ConversationStore()
    @State private var isShowingSetup = false
    @State private var isHoldingMicrophone = false
    @State private var isShowingDocumentPicker = false
    @State private var selectedPhoto: PhotosPickerItem?
    @State private var isLoadingPhoto = false
    @State private var previewAttachment: LocalAttachment?
    @State private var sharedAttachment: LocalAttachment?
    @FocusState private var composerFocused: Bool

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
        .sheet(item: $previewAttachment) { attachment in
            QuickLookSheet(url: attachment.url)
                .ignoresSafeArea()
        }
        .sheet(item: $sharedAttachment) { attachment in
            ActivitySheet(items: [attachment.url])
        }
        .fileImporter(
            isPresented: $isShowingDocumentPicker,
            allowedContentTypes: [.item],
            allowsMultipleSelection: false
        ) { result in
            switch result {
            case let .success(urls):
                if let url = urls.first { store.addDocument(url) }
            case let .failure(error):
                store.attachmentFailed(error)
            }
        }
        .onChange(of: selectedPhoto) {
            guard let item = selectedPhoto else { return }
            selectedPhoto = nil
            loadPhoto(item)
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

                if composerFocused {
                    Button {
                        composerFocused = false
                    } label: {
                        Image(systemName: "keyboard.chevron.compact.down")
                            .font(.system(size: 14, weight: .medium))
                            .foregroundStyle(ScufrisPalette.quartz)
                            .frame(width: 40, height: 40)
                    }
                    .accessibilityLabel("Dismiss keyboard")
                }

                Button {
                    isShowingSetup = true
                } label: {
                    Image(systemName: "slider.horizontal.3")
                        .font(.system(size: 14, weight: .medium))
                        .foregroundStyle(ScufrisPalette.quartz)
                        .frame(width: 40, height: 40)
                        .background(ScufrisPalette.quartz.opacity(0.08))
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
                            ConversationRow(
                                entry: entry,
                                loadAttachment: store.localCopy,
                                onPreview: preview,
                                onSave: save
                            )
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
            .scrollDismissesKeyboard(.interactively)
            .simultaneousGesture(
                TapGesture().onEnded { composerFocused = false }
            )
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

            if let notice = store.attachmentNotice {
                HStack(spacing: 8) {
                    Rectangle()
                        .fill(attachmentAccent)
                        .frame(width: 2, height: 12)
                    Text(notice.uppercased())
                        .font(.system(size: 9, weight: .bold, design: .monospaced))
                        .tracking(0.7)
                        .foregroundStyle(attachmentAccent)
                        .lineLimit(2)
                    Spacer(minLength: 0)
                    if store.isUploadingAttachment {
                        ProgressView()
                            .controlSize(.small)
                            .tint(ScufrisPalette.niagara)
                    }
                }
                .padding(.horizontal, 15)
                .padding(.top, 9)
            }

            if !store.selectedAttachments.isEmpty {
                ScrollView(.horizontal) {
                    HStack(spacing: 7) {
                        ForEach(store.selectedAttachments) { attachment in
                            SelectedAttachmentChip(attachment: attachment) {
                                store.removeAttachment(id: attachment.id)
                            }
                        }
                    }
                    .padding(.horizontal, 15)
                }
                .scrollIndicators(.hidden)
                .padding(.top, 9)
            }

            if let notice = store.dictationState.notice {
                HStack(spacing: 8) {
                    Rectangle()
                        .fill(dictationAccent)
                        .frame(width: 2, height: 12)
                    Text(notice)
                        .font(.system(size: 9, weight: .bold, design: .monospaced))
                        .tracking(0.7)
                        .foregroundStyle(dictationAccent)
                    Spacer(minLength: 0)
                    if store.dictationState == .reviewing || isDictationFailure {
                        Button("DISCARD") {
                            store.discardDictation()
                        }
                        .font(.system(size: 10, weight: .bold, design: .monospaced))
                        .foregroundStyle(ScufrisPalette.red)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 7)
                        .background(ScufrisPalette.red.opacity(0.1))
                    } else if store.dictationState.isActive {
                        Button("CANCEL") {
                            isHoldingMicrophone = false
                            store.cancelDictation()
                        }
                        .font(.system(size: 10, weight: .bold, design: .monospaced))
                        .foregroundStyle(ScufrisPalette.red)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 7)
                        .background(ScufrisPalette.red.opacity(0.1))
                    }
                }
                .padding(.horizontal, 15)
                .padding(.top, 9)
            }

            HStack(alignment: .bottom, spacing: 8) {
                Button {
                    isShowingDocumentPicker = true
                } label: {
                    ComposerIcon(name: "paperclip")
                }
                .disabled(!canAttach)
                .opacity(canAttach ? 1 : 0.35)
                .accessibilityLabel("Attach a document")

                PhotosPicker(selection: $selectedPhoto, matching: .images) {
                    ComposerIcon(name: "photo")
                }
                .disabled(!canAttach)
                .opacity(canAttach ? 1 : 0.35)
                .accessibilityLabel("Attach a photo")

                TextField("Message", text: $store.draft, axis: .vertical)
                    .font(.system(size: 14, design: .monospaced))
                    .foregroundStyle(ScufrisPalette.foregroundStrong)
                    .tint(ScufrisPalette.yellow)
                    .lineLimit(1 ... 4)
                    .submitLabel(.send)
                    .onSubmit(submit)
                    .focused($composerFocused)
                    .padding(.horizontal, 11)
                    .padding(.vertical, 10)
                    .frame(minHeight: 44, alignment: .center)
                    .background(ScufrisPalette.line.opacity(0.22))
                    .overlay(
                        Rectangle().stroke(
                            composerFocused ? ScufrisPalette.quartz : ScufrisPalette.lineStrong,
                            lineWidth: 1
                        )
                    )
                    .accessibilityLabel("Type a message")

                microphoneButton

                Button(action: submit) {
                    Image(systemName: "arrow.up")
                        .font(.system(size: 15, weight: .bold))
                        .foregroundStyle(
                            canSubmit ? ScufrisPalette.background : ScufrisPalette.muted
                        )
                        .frame(width: 44, height: 44)
                        .background(
                            canSubmit ? ScufrisPalette.quartz : ScufrisPalette.line.opacity(0.2)
                        )
                        .overlay(
                            Rectangle().stroke(
                                canSubmit ? ScufrisPalette.quartz : ScufrisPalette.lineStrong,
                                lineWidth: 1
                            )
                        )
                }
                .disabled(!canSubmit)
                .accessibilityLabel("Send message")
            }
            .padding(.horizontal, 12)
            .padding(.top, 12)
            .padding(.bottom, 11)

            Rectangle()
                .fill(ScufrisPalette.line)
                .frame(height: 1)
                .padding(.horizontal, 15)
                .padding(.bottom, 10)
        }
        .background(ScufrisPalette.background)
    }

    private var canSubmit: Bool {
        guard case .connected = store.connectionState, !store.dictationState.isActive else {
            return false
        }
        let text = store.draft.trimmingCharacters(in: .whitespacesAndNewlines)
        return !text.isEmpty && text.utf8.count <= scufrisMaximumTextBytes
    }

    private func submit() {
        guard canSubmit else { return }
        let text = store.draft
        if store.send(text) {
            store.draft = ""
        }
    }

    private var canAttach: Bool {
        guard case .connected = store.connectionState else { return false }
        return !store.isUploadingAttachment
            && !isLoadingPhoto
            && store.selectedAttachments.count < scufrisMaximumAttachments
            && !store.dictationState.isActive
    }

    private var attachmentAccent: Color {
        store.isUploadingAttachment ? ScufrisPalette.niagara : ScufrisPalette.red
    }

    private func loadPhoto(_ item: PhotosPickerItem) {
        isLoadingPhoto = true
        Task {
            defer { isLoadingPhoto = false }
            do {
                guard let data = try await item.loadTransferable(type: Data.self) else {
                    throw AttachmentFailure.invalidSelection
                }
                let contentType = item.supportedContentTypes.first(where: { $0.conforms(to: .image) })
                    ?? .jpeg
                let suffix = contentType.preferredFilenameExtension ?? "jpg"
                let mediaType = contentType.preferredMIMEType ?? "image/jpeg"
                store.addPhoto(
                    data,
                    name: "photo-\(UUID().uuidString.lowercased()).\(suffix)",
                    mediaType: mediaType
                )
            } catch {
                store.attachmentFailed(error)
            }
        }
    }

    private func preview(_ descriptor: AttachmentDescriptor) {
        Task {
            do {
                previewAttachment = LocalAttachment(
                    id: descriptor.id,
                    url: try await store.localCopy(of: descriptor)
                )
            } catch {
                store.attachmentFailed(error)
            }
        }
    }

    private func save(_ descriptor: AttachmentDescriptor) {
        Task {
            do {
                sharedAttachment = LocalAttachment(
                    id: descriptor.id,
                    url: try await store.localCopy(of: descriptor)
                )
            } catch {
                store.attachmentFailed(error)
            }
        }
    }

    private var canDictate: Bool {
        guard case .connected = store.connectionState else { return false }
        return store.dictationState.canBegin
    }

    private var isDictationFailure: Bool {
        if case .failed = store.dictationState { return true }
        return false
    }

    private var dictationAccent: Color {
        switch store.dictationState {
        case .recording, .failed:
            ScufrisPalette.red
        case .transcribing:
            ScufrisPalette.niagara
        default:
            ScufrisPalette.quartz
        }
    }

    private var microphoneButton: some View {
        ZStack {
            Rectangle()
                .fill(
                    store.dictationState == .recording
                        ? ScufrisPalette.red.opacity(0.18)
                        : ScufrisPalette.quartz.opacity(0.08)
                )
            Rectangle()
                .stroke(dictationAccent, lineWidth: 1)
            if store.dictationState == .transcribing {
                ProgressView()
                    .controlSize(.small)
                    .tint(ScufrisPalette.niagara)
            } else {
                Image(systemName: store.dictationState == .recording ? "waveform" : "mic")
                    .font(.system(size: 12, weight: .medium))
                    .foregroundStyle(dictationAccent)
            }
        }
        .frame(width: 44, height: 44)
        .contentShape(Rectangle())
        .opacity(canDictate || store.dictationState.isActive ? 1 : 0.3)
        .gesture(
            DragGesture(minimumDistance: 0)
                .onChanged { _ in
                    guard !isHoldingMicrophone, canDictate else { return }
                    isHoldingMicrophone = true
                    store.beginDictation()
                }
                .onEnded { _ in
                    guard isHoldingMicrophone else { return }
                    isHoldingMicrophone = false
                    store.finishDictation()
                }
        )
        .accessibilityLabel("Hold to dictate")
        .accessibilityValue(store.dictationState.notice ?? "Ready")
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

private struct ComposerIcon: View {
    let name: String

    var body: some View {
        Image(systemName: name)
            .font(.system(size: 15, weight: .semibold))
            .foregroundStyle(ScufrisPalette.quartz)
            .frame(width: 44, height: 44)
            .background(ScufrisPalette.quartz.opacity(0.08))
            .overlay(Rectangle().stroke(ScufrisPalette.lineStrong, lineWidth: 1))
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
    let loadAttachment: (AttachmentDescriptor) async throws -> URL
    let onPreview: (AttachmentDescriptor) -> Void
    let onSave: (AttachmentDescriptor) -> Void

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

                if !entry.attachments.isEmpty {
                    VStack(alignment: .leading, spacing: 7) {
                        ForEach(entry.attachments) { attachment in
                            AttachmentCard(
                                attachment: attachment,
                                loadAttachment: { try await loadAttachment(attachment) },
                                onPreview: { onPreview(attachment) },
                                onSave: { onSave(attachment) }
                            )
                        }
                    }
                }

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

private struct SelectedAttachmentChip: View {
    let attachment: AttachmentDescriptor
    let onRemove: () -> Void

    var body: some View {
        HStack(spacing: 6) {
            Text("\(attachment.name) - \(attachment.displaySize)")
                .lineLimit(1)
            Button(action: onRemove) {
                Image(systemName: "xmark")
                    .font(.system(size: 11, weight: .bold))
                    .foregroundStyle(ScufrisPalette.red)
                    .frame(width: 34, height: 34)
                    .background(ScufrisPalette.red.opacity(0.12))
            }
            .accessibilityLabel("Remove \(attachment.name)")
        }
        .font(.system(size: 10, weight: .medium, design: .monospaced))
        .foregroundStyle(ScufrisPalette.foreground)
        .padding(.leading, 9)
        .padding(.trailing, 5)
        .padding(.vertical, 5)
        .overlay(Rectangle().stroke(ScufrisPalette.lineStrong, lineWidth: 1))
    }
}

private struct AttachmentCard: View {
    let attachment: AttachmentDescriptor
    let loadAttachment: () async throws -> URL
    let onPreview: () -> Void
    let onSave: () -> Void

    var body: some View {
        VStack(alignment: .leading, spacing: 9) {
            if attachment.hasThumbnail {
                Button(action: onPreview) {
                    AttachmentThumbnail(attachment: attachment, load: loadAttachment)
                }
                .buttonStyle(.plain)
                .accessibilityLabel("Preview \(attachment.name)")
            } else {
                HStack(spacing: 9) {
                    Image(systemName: attachment.mediaType == "application/pdf" ? "doc.richtext" : "doc")
                        .font(.system(size: 20, weight: .medium))
                        .foregroundStyle(ScufrisPalette.quartz)
                    attachmentIdentity
                }
            }

            if attachment.hasThumbnail {
                attachmentIdentity
            }

            Button(action: onSave) {
                Label("SAVE", systemImage: "square.and.arrow.down")
                    .font(.system(size: 10, weight: .bold, design: .monospaced))
                    .foregroundStyle(ScufrisPalette.background)
                    .frame(maxWidth: .infinity, minHeight: 44)
                    .background(ScufrisPalette.quartz)
            }
            .accessibilityLabel("Save \(attachment.name)")
        }
        .padding(10)
        .frame(maxWidth: .infinity, alignment: .leading)
        .background(ScufrisPalette.line.opacity(0.18))
        .overlay(alignment: .leading) {
            Rectangle()
                .fill(ScufrisPalette.quartz)
                .frame(width: 2)
        }
        .accessibilityElement(children: .contain)
        .accessibilityLabel("Attachment \(attachment.name), \(attachment.displaySize)")
    }

    private var attachmentIdentity: some View {
        VStack(alignment: .leading, spacing: 3) {
            Text(attachment.name)
                .font(.system(size: 11, weight: .bold, design: .monospaced))
                .foregroundStyle(ScufrisPalette.foregroundStrong)
                .lineLimit(1)
                .truncationMode(.middle)
            Text("\(attachment.mediaType) - \(attachment.displaySize)")
                .font(.system(size: 9, design: .monospaced))
                .foregroundStyle(ScufrisPalette.muted)
        }
    }
}

private struct AttachmentThumbnail: View {
    enum Phase {
        case loading
        case loaded(UIImage)
        case unavailable
    }

    let attachment: AttachmentDescriptor
    let load: () async throws -> URL
    @State private var phase: Phase = .loading

    var body: some View {
        Group {
            switch phase {
            case .loading:
                ProgressView()
                    .tint(ScufrisPalette.quartz)
            case let .loaded(image):
                ZStack {
                    Image(uiImage: image)
                        .resizable()
                        .scaledToFit()
                    if attachment.hasVideoThumbnail {
                        Image(systemName: "play.fill")
                            .font(.system(size: 22, weight: .bold))
                            .foregroundStyle(ScufrisPalette.background)
                            .frame(width: 52, height: 52)
                            .background(ScufrisPalette.quartz.opacity(0.9))
                            .clipShape(Circle())
                    }
                }
            case .unavailable:
                Image(systemName: "photo.badge.exclamationmark")
                    .font(.system(size: 24, weight: .medium))
                    .foregroundStyle(ScufrisPalette.muted)
            }
        }
        .frame(maxWidth: .infinity, minHeight: 120, maxHeight: 240)
        .background(ScufrisPalette.background)
        .clipped()
        .task(id: attachment.id) {
            do {
                let url = try await load()
                let request = QLThumbnailGenerator.Request(
                    fileAt: url,
                    size: CGSize(width: 720, height: 480),
                    scale: UIScreen.main.scale,
                    representationTypes: .thumbnail
                )
                let thumbnail = try await QLThumbnailGenerator.shared
                    .generateBestRepresentation(for: request)
                phase = .loaded(thumbnail.uiImage)
            } catch {
                phase = .unavailable
            }
        }
        .accessibilityLabel("Preview attachment \(attachment.name)")
    }
}

private struct LocalAttachment: Identifiable {
    let id: String
    let url: URL
}

private struct QuickLookSheet: UIViewControllerRepresentable {
    let url: URL

    func makeCoordinator() -> Coordinator {
        Coordinator(url: url)
    }

    func makeUIViewController(context: Context) -> QLPreviewController {
        let controller = QLPreviewController()
        controller.dataSource = context.coordinator
        return controller
    }

    func updateUIViewController(_ controller: QLPreviewController, context: Context) {}

    final class Coordinator: NSObject, QLPreviewControllerDataSource {
        let url: URL

        init(url: URL) {
            self.url = url
        }

        func numberOfPreviewItems(in controller: QLPreviewController) -> Int { 1 }

        func previewController(
            _ controller: QLPreviewController,
            previewItemAt index: Int
        ) -> QLPreviewItem {
            url as NSURL
        }
    }
}

private struct ActivitySheet: UIViewControllerRepresentable {
    let items: [Any]

    func makeUIViewController(context: Context) -> UIActivityViewController {
        UIActivityViewController(activityItems: items, applicationActivities: nil)
    }

    func updateUIViewController(_ controller: UIActivityViewController, context: Context) {}
}

private extension AttachmentDescriptor {
    var hasThumbnail: Bool {
        hasRasterImageThumbnail || hasVideoThumbnail
    }

    var hasRasterImageThumbnail: Bool {
        mediaType.hasPrefix("image/") && mediaType != "image/svg+xml"
    }

    var hasVideoThumbnail: Bool {
        mediaType.hasPrefix("video/")
    }

    var displaySize: String {
        if size < 1024 { return "\(size) B" }
        if size < 1024 * 1024 { return "\((size + 1023) / 1024) KiB" }
        return String(format: "%.1f MiB", Double(size) / Double(1024 * 1024))
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
