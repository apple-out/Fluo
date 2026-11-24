import SwiftUI
import UIKit
import WebKit

enum ActiveSheetKind: String, Identifiable {
    case translate
    case tabs
    case workspaces
    case menu
    case bookmarks
    case bookmarkEditor
    case siteSecurity
    case history
    case translationHistory
    case glossary
    case downloads
    case feedback
    case settings

    var id: String { rawValue }
}

private struct SelectAllURLField: UIViewRepresentable {
    @Binding var text: String
    @Binding var isEditing: Bool
    var placeholder: String
    var autoFocus: Bool = false
    var onSubmit: () -> Void

    func makeCoordinator() -> Coordinator {
        Coordinator(self)
    }

    func makeUIView(context: Context) -> UITextField {
        let tf = UITextField(frame: .zero)
        tf.delegate = context.coordinator
        tf.placeholder = placeholder
        tf.font = UIFont.preferredFont(forTextStyle: .subheadline)
        tf.autocapitalizationType = .none
        tf.autocorrectionType = .no
        tf.spellCheckingType = .no
        tf.keyboardType = .URL
        tf.returnKeyType = .go
        tf.clearButtonMode = .whileEditing
        tf.setContentCompressionResistancePriority(.defaultLow, for: .horizontal)
        tf.setContentHuggingPriority(.defaultLow, for: .horizontal)
        tf.addTarget(context.coordinator, action: #selector(Coordinator.textChanged(_:)), for: .editingChanged)
        return tf
    }

    func updateUIView(_ uiView: UITextField, context: Context) {
        context.coordinator.parent = self
        if uiView.placeholder != placeholder {
            uiView.placeholder = placeholder
        }
        if uiView.markedTextRange == nil && uiView.text != text {
            uiView.text = text
        }
        if autoFocus && !context.coordinator.didAutoFocus {
            context.coordinator.didAutoFocus = true
            DispatchQueue.main.async {
                uiView.becomeFirstResponder()
            }
        }
    }

    final class Coordinator: NSObject, UITextFieldDelegate {
        var parent: SelectAllURLField
        var didAutoFocus = false

        init(_ parent: SelectAllURLField) {
            self.parent = parent
        }

        @objc func textChanged(_ sender: UITextField) {
            parent.text = sender.text ?? ""
        }

        func textFieldDidBeginEditing(_ textField: UITextField) {
            parent.isEditing = true
            DispatchQueue.main.async {
                textField.selectAll(nil)
            }
        }

        func textFieldDidEndEditing(_ textField: UITextField) {
            parent.isEditing = false
        }

        func textFieldShouldReturn(_ textField: UITextField) -> Bool {
            parent.isEditing = false
            textField.resignFirstResponder()
            parent.onSubmit()
            return true
        }
    }
}

struct ContentView: View {
    @ObservedObject var store: BrowserStore
    @ObservedObject private var catalog = ModelCatalog.shared

    @State private var urlDraft = ""
    @State private var isEditingURL = false
    @State private var activeSheet: ActiveSheetKind?
    @State private var editingBookmark: BookmarkItem?
    @State private var showOnboarding = false
    @State private var dockDragOffset: CGFloat = 0
    @FocusState private var findFieldFocused: Bool

    private func t(_ key: String) -> String {
        L10n.tr(store.uiLang, key)
    }

    private func switchSheet(_ target: ActiveSheetKind) {
        activeSheet = nil
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.16) {
            activeSheet = target
        }
    }

    private func openBookmarkEditor(for existing: BookmarkItem? = nil) {
        editingBookmark = existing ?? store.currentBookmark()
        if activeSheet != nil {
            switchSheet(.bookmarkEditor)
        } else {
            activeSheet = .bookmarkEditor
        }
    }

    private var resolvedColorScheme: ColorScheme? {
        switch store.userData.appTheme {
        case .system: return nil
        case .light: return .light
        case .dark: return .dark
        }
    }

    private var isHttps: Bool {
        store.activeTab.urlString.lowercased().hasPrefix("https://")
    }

    private var securityIconName: String {
        if store.activeTab.isPrivate { return "eye.slash.fill" }
        if store.activeTab.isStartPage { return "sparkles" }
        return isHttps ? "lock.fill" : "exclamationmark.triangle.fill"
    }

    private var securityIconColor: Color {
        if store.activeTab.isPrivate { return FluoThemeTokens.violet }
        if store.activeTab.isStartPage { return FluoThemeTokens.indigo }
        return isHttps ? FluoThemeTokens.emerald : FluoThemeTokens.amber
    }

    private var displayDomain: String {
        guard let host = URL(string: store.activeTab.urlString)?.host, !host.isEmpty else {
            return store.activeTab.title.isEmpty ? "Fluo" : store.activeTab.title
        }
        return host.hasPrefix("www.") ? String(host.dropFirst(4)) : host
    }

    var body: some View {
        ZStack(alignment: .bottom) {
            VStack(spacing: 0) {
                if store.activeTab.isLoading {
                    GeometryReader { geo in
                        ZStack(alignment: .leading) {
                            Color.clear
                            Capsule()
                                .fill(FluoThemeTokens.liquidGradient)
                                .frame(
                                    width: max(28, geo.size.width * min(max(store.activeTab.progress, 0.08), 1.0)),
                                    height: 2.5
                                )
                                .shadow(color: FluoThemeTokens.indigo.opacity(0.65), radius: 4, y: 1)
                                .animation(.easeOut(duration: 0.2), value: store.activeTab.progress)
                        }
                    }
                    .frame(height: 2.5)
                }

                if store.showFindBar {
                    findBar
                        .padding(.horizontal, 12)
                        .padding(.top, 6)
                        .transition(.move(edge: .top).combined(with: .opacity))
                }

                if store.isPickingRegion {
                    HStack(spacing: 10) {
                        Image(systemName: "hand.tap.fill")
                            .foregroundStyle(FluoThemeTokens.indigo)
                        Text(t("tr.pickingBanner"))
                            .font(.caption.weight(.semibold))
                        Spacer()
                        Button(t("common.cancel")) {
                            store.cancelRegionPicker()
                        }
                        .font(.caption.bold())
                        .padding(.horizontal, 10)
                        .padding(.vertical, 5)
                        .liquidGlassPill(tint: FluoThemeTokens.indigo, interactive: true)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .liquidGlass(cornerRadius: 20, tint: FluoThemeTokens.indigo)
                    .padding(.horizontal, 12)
                    .padding(.top, 6)
                    .transition(.move(edge: .top).combined(with: .opacity))
                }

                if store.activeTab.isStartPage {
                    StartPageView(
                        store: store,
                        catalog: catalog,
                        openSettings: { activeSheet = .settings },
                        openBookmarks: { activeSheet = .bookmarks }
                    )
                } else {
                    ZStack {
                        WebViewContainer(webView: store.activeTab.webView)
                            .id(store.activeTab.id)
                            .ignoresSafeArea(edges: .bottom)

                        if let pageErr = store.activeTab.pageErrorText {
                            VStack(spacing: 14) {
                                Image(systemName: "wifi.exclamationmark")
                                    .font(.largeTitle)
                                    .foregroundStyle(FluoThemeTokens.amber)
                                Text(pageErr)
                                    .font(.footnote)
                                    .multilineTextAlignment(.center)
                                    .foregroundStyle(.secondary)
                                HStack(spacing: 10) {
                                    Button(t("err.retry")) {
                                        store.activeTab.pageErrorText = nil
                                        if let url = URL(string: store.activeTab.urlString) {
                                            store.activeTab.webView.load(URLRequest(url: url))
                                        } else {
                                            store.activeTab.webView.reload()
                                        }
                                    }
                                    .font(.caption.bold())
                                    .foregroundStyle(.white)
                                    .padding(.horizontal, 16)
                                    .padding(.vertical, 9)
                                    .liquidGlassPill(tint: FluoThemeTokens.indigo, interactive: true, prominent: true)

                                    Button(t("menu.startPage")) {
                                        store.activeTab.openStartPage()
                                    }
                                    .font(.caption.bold())
                                    .padding(.horizontal, 14)
                                    .padding(.vertical, 9)
                                    .liquidGlassPill(interactive: true)
                                }
                            }
                            .padding(22)
                            .liquidGlass(cornerRadius: 26)
                            .padding(.horizontal, 24)
                        }
                    }
                }
            }

            if isEditingURL {
                Color.black.opacity(0.24)
                    .ignoresSafeArea()
                    .onTapGesture {
                        dismissOmniboxEditing()
                    }
                    .transition(.opacity)
            }

            VStack(spacing: 8) {
                if isEditingURL {
                    omniboxOverlayPanel
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }

                if !isEditingURL && !store.liveWebSelection.isEmpty {
                    HStack(spacing: 8) {
                        Image(systemName: "text.quote")
                            .font(.caption.bold())
                            .foregroundStyle(FluoThemeTokens.indigo)
                        Text(store.liveWebSelection)
                            .font(.caption)
                            .lineLimit(1)
                            .frame(maxWidth: 140, alignment: .leading)
                        Button {
                            UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                            store.translateSelectedText()
                        } label: {
                            Text(t("tr.selection"))
                                .font(.caption.bold())
                                .foregroundStyle(.white)
                                .padding(.horizontal, 10)
                                .padding(.vertical, 6)
                                .background(FluoThemeTokens.liquidGradient, in: Capsule())
                                .contentShape(Capsule())
                        }
                        .buttonStyle(.plain)

                        Button {
                            UIImpactFeedbackGenerator(style: .light).impactOccurred()
                            store.addGlossaryItem(source: store.liveWebSelection, target: "", caseSensitive: false)
                            store.liveWebSelection = ""
                            activeSheet = .glossary
                        } label: {
                            Text(t("sel.addGlossary"))
                                .font(.caption.bold())
                                .padding(.horizontal, 10)
                                .padding(.vertical, 6)
                                .background(Color.primary.opacity(0.10), in: Capsule())
                                .contentShape(Capsule())
                        }
                        .buttonStyle(.plain)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 7)
                    .liquidGlassPill()
                    .padding(.horizontal, 12)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                }

                if store.isSpeakingTTS {
                    HStack(spacing: 12) {
                        Image(systemName: "waveform")
                            .foregroundStyle(FluoThemeTokens.cyan)
                            .font(.caption.bold())
                        Text("\(store.ttsIndex + 1)/\(max(1, store.ttsTotal))")
                            .font(.caption.monospacedDigit().weight(.bold))
                        Button {
                            store.stepTTS(delta: -1)
                        } label: {
                            Image(systemName: "backward.fill").font(.caption)
                        }
                        .buttonStyle(.plain)
                        Button {
                            store.startOrToggleTTS()
                        } label: {
                            Image(systemName: store.isPausedTTS ? "play.fill" : "pause.fill")
                                .font(.subheadline.bold())
                        }
                        .buttonStyle(.plain)
                        Button {
                            store.stepTTS(delta: 1)
                        } label: {
                            Image(systemName: "forward.fill").font(.caption)
                        }
                        .buttonStyle(.plain)
                        Button {
                            let nextRate: Float = store.ttsRate < 1.2 ? 1.25 : (store.ttsRate < 1.45 ? 1.5 : 1.0)
                            store.setTTSRate(nextRate)
                        } label: {
                            Text(String(format: "%.2gx", store.ttsRate))
                                .font(.caption2.bold())
                                .padding(.horizontal, 8)
                                .padding(.vertical, 4)
                                .background(Color.primary.opacity(0.10), in: Capsule())
                                .contentShape(Capsule())
                        }
                        .buttonStyle(.plain)
                        Button {
                            store.stopTTS()
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(.secondary)
                                .frame(width: 28, height: 28)
                                .contentShape(Circle())
                        }
                        .buttonStyle(.plain)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .liquidGlassPill(tint: FluoThemeTokens.cyan)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                }

                if !isEditingURL && store.showAutoTranslatePrompt && !store.isTranslating && store.activeTab.displayMode == .original {
                    VStack(alignment: .leading, spacing: 10) {
                        HStack(spacing: 8) {
                            Image(systemName: "sparkles")
                                .font(.caption.bold())
                                .foregroundStyle(FluoThemeTokens.indigo)
                            Text("\(t("tr.autoPrompt")) (\(store.detectedForeignLanguageCode))")
                                .font(.caption.weight(.bold))
                                .lineLimit(1)
                            Spacer(minLength: 8)
                            Button {
                                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                                store.dismissAutoTranslatePrompt()
                            } label: {
                                Image(systemName: "xmark.circle.fill")
                                    .font(.title3)
                                    .foregroundStyle(.secondary)
                                    .frame(width: 32, height: 32)
                                    .contentShape(Circle())
                            }
                            .buttonStyle(.plain)
                            .accessibilityLabel(t("common.close"))
                        }

                        HStack(spacing: 8) {
                            Button {
                                UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                                store.showAutoTranslatePrompt = false
                                store.translateActiveTab(bilingual: false)
                            } label: {
                                Label(t("bar.translate"), systemImage: "sparkles")
                                    .font(.caption.bold())
                                    .foregroundStyle(.white)
                                    .frame(maxWidth: .infinity)
                                    .padding(.vertical, 9)
                                    .background(FluoThemeTokens.liquidGradient, in: Capsule())
                                    .contentShape(Capsule())
                            }
                            .buttonStyle(.plain)

                            Button {
                                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                                store.toggleAlwaysTranslateCurrentHost()
                            } label: {
                                Text(t("tr.alwaysHost"))
                                    .font(.caption.weight(.semibold))
                                    .lineLimit(1)
                                    .padding(.horizontal, 12)
                                    .padding(.vertical, 9)
                                    .background(Color.primary.opacity(0.10), in: Capsule())
                                    .contentShape(Capsule())
                            }
                            .buttonStyle(.plain)
                        }
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 12)
                    .liquidGlass(cornerRadius: 22)
                    .padding(.horizontal, 12)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                }

                if store.isTranslating {
                    HStack(spacing: 10) {
                        ProgressView()
                            .tint(FluoThemeTokens.indigo)
                        if store.translationTotal > 0 {
                            Text("\(t("tr.progress")) \(store.translationDone)/\(store.translationTotal)")
                                .font(.caption.weight(.semibold).monospacedDigit())
                        } else {
                            Text(store.translationStatusText.isEmpty ? t("tr.preparing") : store.translationStatusText)
                                .font(.caption.weight(.semibold))
                                .lineLimit(1)
                        }
                        Button {
                            store.cancelTranslation()
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(.secondary)
                                .frame(width: 28, height: 28)
                                .contentShape(Circle())
                        }
                        .buttonStyle(.plain)
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 9)
                    .liquidGlassPill(tint: FluoThemeTokens.indigo)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                }

                if let err = store.translationError {
                    HStack(spacing: 10) {
                        Image(systemName: "exclamationmark.triangle.fill")
                            .foregroundStyle(.red)
                            .font(.caption)
                        Text(err)
                            .font(.caption)
                            .lineLimit(2)
                        Spacer(minLength: 4)
                        Button(t("tr.openSettings")) {
                            store.translationError = nil
                            activeSheet = .settings
                        }
                        .font(.caption2.bold())
                        .foregroundStyle(FluoThemeTokens.indigo)
                        Button {
                            store.translationError = nil
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(.secondary)
                                .frame(width: 28, height: 28)
                                .contentShape(Circle())
                        }
                        .buttonStyle(.plain)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .liquidGlass(cornerRadius: 18, tint: .red)
                    .padding(.horizontal, 14)
                    .transition(.move(edge: .bottom).combined(with: .opacity))
                }

                spatialBottomDock
            }
            .animation(.interactiveSpring(response: 0.24, dampingFraction: 0.86), value: isEditingURL)
            .animation(.interactiveSpring(response: 0.22, dampingFraction: 0.88), value: store.isDockCollapsed)
        }
        .preferredColorScheme(resolvedColorScheme)
        .onAppear {
            syncDraftFromActiveTab()
            if !store.onboardingCompleted {
                showOnboarding = true
            }
        }
        .onChange(of: store.activeTab.urlString) { _ in
            syncDraftFromActiveTab()
        }
        .onChange(of: store.activeTabId) { _ in
            syncDraftFromActiveTab()
            store.isDockCollapsed = false
        }
        .onChange(of: store.showFindBar) { show in
            if show {
                store.isDockCollapsed = false
                DispatchQueue.main.asyncAfter(deadline: .now() + 0.08) {
                    findFieldFocused = true
                }
            }
        }
        .onChange(of: store.isPickingRegion) { picking in
            if picking { store.isDockCollapsed = false }
        }
        .sheet(isPresented: $showOnboarding) {
            OnboardingSheet(store: store)
                .interactiveDismissDisabled(true)
                .liquidGlassSheet()
        }
        .sheet(item: $activeSheet) { kind in
            Group {
                switch kind {
                case .translate:
                    TranslateSheet(
                        store: store,
                        catalog: catalog,
                        openSettings: {
                            switchSheet(.settings)
                        },
                        onClose: {
                            activeSheet = nil
                        }
                    )
                    .presentationDetents([.medium, .large])
                case .tabs:
                    TabsSheet(
                        store: store,
                        openWorkspaces: { switchSheet(.workspaces) },
                        onClose: { activeSheet = nil }
                    )
                    .presentationDetents([.medium, .large])
                case .workspaces:
                    WorkspacesSheet(store: store, onClose: { activeSheet = nil })
                        .presentationDetents([.medium, .large])
                case .menu:
                    MenuSheet(
                        store: store,
                        openSheet: { target in
                            if target == .bookmarkEditor {
                                openBookmarkEditor(for: store.currentBookmark())
                            } else {
                                switchSheet(target)
                            }
                        },
                        onClose: {
                            activeSheet = nil
                        }
                    )
                    .presentationDetents([.medium, .large])
                case .bookmarks:
                    BookmarksSheet(
                        store: store,
                        onEdit: { item in
                            openBookmarkEditor(for: item)
                        },
                        onAddCurrent: store.activeTab.isStartPage ? nil : {
                            openBookmarkEditor(for: store.currentBookmark())
                        }
                    )
                case .bookmarkEditor:
                    BookmarkEditorSheet(store: store, existing: editingBookmark)
                        .presentationDetents([.medium, .large])
                case .siteSecurity:
                    SiteSecuritySheet(store: store)
                        .presentationDetents([.medium])
                case .history:
                    HistorySheet(store: store)
                case .translationHistory:
                    TranslationHistorySheet(store: store)
                case .glossary:
                    GlossarySheet(store: store)
                case .downloads:
                    DownloadsSheet(store: store)
                case .feedback:
                    FeedbackSheet(store: store)
                case .settings:
                    SettingsSheet(store: store, catalog: catalog)
                }
            }
            .liquidGlassSheet()
        }
        .sheet(isPresented: $store.showSelectionResultSheet) {
            SelectionResultSheet(store: store)
                .presentationDetents([.medium])
                .liquidGlassSheet()
        }
    }

    private func syncDraftFromActiveTab() {
        urlDraft = store.activeTab.isStartPage ? "" : store.activeTab.urlString
    }

    private func dismissOmniboxEditing() {
        isEditingURL = false
        UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
    }

    private var omniboxOverlayPanel: some View {
        let suggestions = store.omniboxSuggestions(for: urlDraft)
        return VStack(spacing: 6) {
            if !suggestions.isEmpty {
                VStack(spacing: 0) {
                    ForEach(Array(suggestions.prefix(5).enumerated()), id: \.element.id) { idx, item in
                        Button {
                            if item.kind == .calculation {
                                UIPasteboard.general.string = item.targetURLOrQuery
                                urlDraft = item.targetURLOrQuery
                            } else if item.kind == .switchTab, let tid = item.tabId {
                                dismissOmniboxEditing()
                                store.selectTab(tid)
                            } else {
                                dismissOmniboxEditing()
                                store.activeTab.navigate(input: item.targetURLOrQuery, searchEngine: store.userData.searchEngine)
                            }
                        } label: {
                            HStack(spacing: 10) {
                                Image(systemName: {
                                    switch item.kind {
                                    case .calculation: return "equal.circle.fill"
                                    case .switchTab: return "arrow.right.square.fill"
                                    case .bookmark: return "star.fill"
                                    case .history: return "clock"
                                    case .search: return "magnifyingglass"
                                    }
                                }())
                                .font(.caption.bold())
                                .foregroundStyle(FluoThemeTokens.indigo)

                                VStack(alignment: .leading, spacing: 2) {
                                    Text(item.title)
                                        .font(.caption.weight(.semibold))
                                        .lineLimit(1)
                                    Text(item.subtitle)
                                        .font(.caption2)
                                        .foregroundStyle(.secondary)
                                        .lineLimit(1)
                                }
                                Spacer()
                                if item.kind == .switchTab {
                                    Text(t("omni.switchTab"))
                                        .font(.caption2.bold())
                                        .padding(.horizontal, 7)
                                        .padding(.vertical, 3)
                                        .background(FluoThemeTokens.indigo.opacity(0.2), in: Capsule())
                                }
                            }
                            .padding(.horizontal, 14)
                            .padding(.vertical, 9)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)
                        if idx < min(suggestions.count, 5) - 1 {
                            Divider().opacity(0.2)
                        }
                    }
                }
                .liquidGlass(cornerRadius: 20)
                .padding(.horizontal, 12)
            }

            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach([".com", ".org", "/", "https://", "@wiki ", "@yt ", "@g "], id: \.self) { token in
                        Button {
                            if token.hasPrefix("@") {
                                urlDraft = token + urlDraft
                            } else {
                                urlDraft += token
                            }
                        } label: {
                            Text(token)
                                .font(.caption.weight(.semibold))
                                .padding(.horizontal, 10)
                                .padding(.vertical, 6)
                        }
                        .buttonStyle(.plain)
                        .liquidGlassPill(interactive: true)
                    }
                    if let clip = UIPasteboard.general.string?.trimmingCharacters(in: .whitespacesAndNewlines),
                       !clip.isEmpty {
                        Button {
                            dismissOmniboxEditing()
                            store.activeTab.navigate(input: clip, searchEngine: store.userData.searchEngine)
                        } label: {
                            Label("Paste & Go", systemImage: "doc.on.clipboard")
                                .font(.caption.weight(.semibold))
                                .padding(.horizontal, 10)
                                .padding(.vertical, 6)
                        }
                        .buttonStyle(.plain)
                        .liquidGlassPill(tint: FluoThemeTokens.indigo, interactive: true)
                    }
                }
                .padding(.horizontal, 12)
            }
        }
    }

    private var findBar: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
                .font(.footnote.weight(.semibold))
            TextField(t("find.placeholder"), text: $store.findQuery)
                .focused($findFieldFocused)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.search)
                .font(.subheadline)
                .onChange(of: store.findQuery) { _ in
                    store.runFindInPage(action: "find")
                }
                .onSubmit {
                    store.runFindInPage(action: "next")
                }
            if !store.findQuery.trimmingCharacters(in: .whitespaces).isEmpty {
                Text("\(store.findCurrent)/\(store.findTotal)")
                    .font(.caption.monospacedDigit().weight(.semibold))
                    .foregroundStyle(store.findTotal > 0 ? Color.primary : Color.secondary)
            }
            Button {
                store.runFindInPage(action: "prev")
            } label: {
                Image(systemName: "chevron.up")
                    .frame(width: 28, height: 28)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(store.findTotal == 0)

            Button {
                store.runFindInPage(action: "next")
            } label: {
                Image(systemName: "chevron.down")
                    .frame(width: 28, height: 28)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
            .disabled(store.findTotal == 0)

            Button {
                findFieldFocused = false
                store.closeFindBar()
            } label: {
                Image(systemName: "xmark.circle.fill")
                    .foregroundStyle(.secondary)
                    .frame(width: 28, height: 28)
                    .contentShape(Rectangle())
            }
            .buttonStyle(.plain)
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 8)
        .liquidGlassPill()
    }

    private var dockDragGesture: some Gesture {
        DragGesture(minimumDistance: 24)
            .onChanged { value in
                let dx = value.translation.width
                let dy = value.translation.height
                if abs(dx) > abs(dy) {
                    dockDragOffset = max(-60, min(60, dx * 0.45))
                }
            }
            .onEnded { value in
                let dx = value.translation.width
                let dy = value.translation.height
                withAnimation(.interactiveSpring(response: 0.30, dampingFraction: 0.84)) {
                    dockDragOffset = 0
                }
                if dy < -42 && abs(dy) > abs(dx) {
                    UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                    activeSheet = .tabs
                } else if dx < -48 && abs(dx) > abs(dy) {
                    if store.switchAdjacentTab(delta: 1, createIfPastEnd: true) {
                        UIImpactFeedbackGenerator(style: .light).impactOccurred()
                    }
                } else if dx > 48 && abs(dx) > abs(dy) {
                    if store.switchAdjacentTab(delta: -1, createIfPastEnd: false) {
                        UIImpactFeedbackGenerator(style: .light).impactOccurred()
                    }
                }
            }
    }

    @ViewBuilder
    private func backForwardContextMenu(items: [WKBackForwardListItem]) -> some View {
        ForEach(Array(items.enumerated()), id: \.offset) { _, item in
            Button {
                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                store.activeTab.webView.go(to: item)
            } label: {
                Text(item.title?.isEmpty == false ? (item.title ?? "") : item.url.absoluteString)
            }
        }
    }

    @ViewBuilder
    private var backButtonView: some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            store.activeTab.goBackFromCurrent()
        } label: {
            Image(systemName: "chevron.left")
                .font(.system(size: 15, weight: .bold))
                .frame(width: 38, height: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(LiquidPressButtonStyle())
        .contextMenu {
            backForwardContextMenu(
                items: Array(store.activeTab.webView.backForwardList.backList.reversed().prefix(8))
            )
            if store.activeTab.cameFromStartPage && !store.activeTab.isStartPage {
                Button {
                    store.activeTab.openStartPage()
                } label: {
                    Label(t("start.title"), systemImage: "house")
                }
            }
        }
    }

    @ViewBuilder
    private var forwardButtonView: some View {
        Button {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            store.activeTab.goForwardFromCurrent()
        } label: {
            Image(systemName: "chevron.right")
                .font(.system(size: 15, weight: .bold))
                .frame(width: 38, height: 44)
                .contentShape(Rectangle())
        }
        .buttonStyle(LiquidPressButtonStyle())
        .contextMenu {
            backForwardContextMenu(
                items: Array(store.activeTab.webView.backForwardList.forwardList.prefix(8))
            )
        }
    }

    @ViewBuilder
    private var spatialBottomDock: some View {
        if isEditingURL {
            // Mode A: Full-width Command Omnibox
            HStack(spacing: 10) {
                Image(systemName: securityIconName)
                    .font(.caption.weight(.bold))
                    .foregroundStyle(securityIconColor)
                    .frame(width: 24, height: 24)

                SelectAllURLField(
                    text: $urlDraft,
                    isEditing: $isEditingURL,
                    placeholder: t("bar.urlPlaceholder"),
                    autoFocus: true
                ) {
                    store.activeTab.navigate(input: urlDraft, searchEngine: store.userData.searchEngine)
                }
                .frame(height: 26)

                Button {
                    dismissOmniboxEditing()
                } label: {
                    Image(systemName: "xmark.circle.fill")
                        .font(.system(size: 18))
                        .foregroundStyle(.secondary)
                        .frame(width: 28, height: 28)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)
            }
            .padding(.horizontal, 14)
            .padding(.vertical, 10)
            .liquidGlassPill(tint: store.activeTab.isPrivate ? FluoThemeTokens.violet : FluoThemeTokens.indigo)
            .padding(.horizontal, 12)
            .padding(.bottom, 6)
        } else if store.activeTab.isStartPage {
            // Mode B: Start Page Asymmetric Floating Action Pill Bar (no duplicate bottom URL bar)
            LiquidGlassGroup(spacing: 10) {
                HStack(spacing: 10) {
                    if store.activeTab.canGoForward {
                        forwardButtonView
                            .frame(width: 44, height: 44)
                            .liquidGlassCircle(interactive: true)
                    }

                    Button {
                        UIImpactFeedbackGenerator(style: .light).impactOccurred()
                        activeSheet = .bookmarks
                    } label: {
                        HStack(spacing: 7) {
                            Image(systemName: "books.vertical.fill")
                                .font(.system(size: 14, weight: .bold))
                                .foregroundStyle(FluoThemeTokens.indigo)
                            Text(t("bm.title"))
                                .font(.subheadline.weight(.semibold))
                        }
                        .padding(.horizontal, 16)
                        .frame(height: 44)
                        .contentShape(Capsule())
                    }
                    .buttonStyle(LiquidPressButtonStyle())
                    .liquidGlassPill(interactive: true)

                    Button {
                        UIImpactFeedbackGenerator(style: .light).impactOccurred()
                        activeSheet = .tabs
                    } label: {
                        HStack(spacing: 5) {
                            Image(systemName: "square.on.square")
                                .font(.system(size: 13, weight: .bold))
                            Text("\(store.activeWorkspaceTabs.count)")
                                .font(.caption.monospacedDigit().weight(.black))
                        }
                        .padding(.horizontal, 14)
                        .frame(height: 44)
                        .contentShape(Capsule())
                    }
                    .buttonStyle(LiquidPressButtonStyle())
                    .liquidGlassPill(interactive: true)
                    .contextMenu {
                        Button {
                            store.newTab(isPrivate: false)
                        } label: {
                            Label(t("tabs.newTab"), systemImage: "plus")
                        }
                        Button {
                            store.newTab(isPrivate: true)
                        } label: {
                            Label(t("tabs.newPrivate"), systemImage: "eye.slash")
                        }
                    }

                    Button {
                        UIImpactFeedbackGenerator(style: .light).impactOccurred()
                        activeSheet = .menu
                    } label: {
                        Image(systemName: "square.grid.2x2")
                            .font(.system(size: 16, weight: .semibold))
                            .frame(width: 44, height: 44)
                            .contentShape(Circle())
                    }
                    .buttonStyle(LiquidPressButtonStyle())
                    .liquidGlassCircle(interactive: true)
                    .contextMenu {
                        Button {
                            activeSheet = .settings
                        } label: {
                            Label(t("menu.settings"), systemImage: "gearshape")
                        }
                        Button {
                            activeSheet = .glossary
                        } label: {
                            Label(t("menu.glossary"), systemImage: "character.book.closed")
                        }
                        Button {
                            activeSheet = .history
                        } label: {
                            Label(t("menu.history"), systemImage: "clock")
                        }
                    }
                }
            }
            .padding(.bottom, 8)
        } else if store.isDockCollapsed {
            // Mode C-1: Collapsed Floating Reading Island (single-tap expands in 0ms)
            HStack(spacing: 8) {
                Circle()
                    .fill(securityIconColor)
                    .frame(width: 7, height: 7)

                Text(displayDomain)
                    .font(.caption.weight(.bold))
                    .lineLimit(1)

                if store.activeTab.displayMode != .original {
                    Image(systemName: "sparkles")
                        .font(.caption2.bold())
                        .foregroundStyle(FluoThemeTokens.indigo)
                }
            }
            .padding(.horizontal, 16)
            .padding(.vertical, 9)
            .frame(maxWidth: 184)
            .liquidGlassPill(
                tint: store.activeTab.isPrivate
                    ? FluoThemeTokens.violet
                    : (store.activeTab.displayMode != .original ? FluoThemeTokens.indigo : .clear),
                interactive: true
            )
            .offset(x: dockDragOffset)
            .contentShape(Capsule())
            .onTapGesture {
                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                store.isDockCollapsed = false
            }
            .gesture(dockDragGesture)
            .padding(.bottom, 6)
        } else {
            // Mode C-2: Expanded Asymmetric Floating Single-Row Spatial Dock
            LiquidGlassGroup(spacing: 8) {
                HStack(spacing: 8) {
                    // Dynamic Back / Forward cluster (only consumes space when active)
                    if store.activeTab.canGoBack && store.activeTab.canGoForward {
                        HStack(spacing: 0) {
                            backButtonView
                            Capsule()
                                .fill(Color.primary.opacity(0.14))
                                .frame(width: 1, height: 18)
                            forwardButtonView
                        }
                        .frame(height: 44)
                        .liquidGlassPill(interactive: true)
                    } else if store.activeTab.canGoBack {
                        backButtonView
                            .frame(width: 42, height: 44)
                            .liquidGlassCircle(interactive: true)
                    } else if store.activeTab.canGoForward {
                        forwardButtonView
                            .frame(width: 42, height: 44)
                            .liquidGlassCircle(interactive: true)
                    }

                    // Hero Center Reading Capsule
                    HStack(spacing: 6) {
                        Button {
                            activeSheet = .siteSecurity
                        } label: {
                            Image(systemName: securityIconName)
                                .font(.caption.weight(.bold))
                                .foregroundStyle(securityIconColor)
                                .frame(width: 28, height: 34)
                                .contentShape(Circle())
                        }
                        .buttonStyle(.plain)
                        .accessibilityLabel(t("sec.title"))

                        Button {
                            syncDraftFromActiveTab()
                            isEditingURL = true
                        } label: {
                            HStack(spacing: 4) {
                                Text(displayDomain)
                                    .font(.subheadline.weight(.semibold))
                                    .lineLimit(1)
                                Spacer(minLength: 0)
                            }
                            .frame(maxWidth: .infinity, minHeight: 34)
                            .contentShape(Rectangle())
                        }
                        .buttonStyle(.plain)

                        Button {
                            UIImpactFeedbackGenerator(style: .light).impactOccurred()
                            if store.activeTab.isLoading {
                                store.activeTab.webView.stopLoading()
                            } else {
                                store.activeTab.webView.reload()
                            }
                        } label: {
                            Image(systemName: store.activeTab.isLoading ? "xmark" : "arrow.clockwise")
                                .font(.caption.weight(.bold))
                                .foregroundStyle(.secondary)
                                .frame(width: 26, height: 30)
                                .contentShape(Circle())
                        }
                        .buttonStyle(.plain)

                        // Inline Translate Sparkle Orb
                        Button {
                            UIImpactFeedbackGenerator(style: .light).impactOccurred()
                            activeSheet = .translate
                        } label: {
                            Image(systemName: store.activeTab.displayMode == .original ? "character.bubble" : "character.bubble.fill")
                                .font(.system(size: 13, weight: .bold))
                                .foregroundStyle(store.activeTab.displayMode == .original ? Color.primary : Color.white)
                                .frame(width: 32, height: 32)
                                .background {
                                    if store.activeTab.displayMode == .original {
                                        Circle().fill(Color.primary.opacity(0.08))
                                    } else {
                                        Circle().fill(FluoThemeTokens.liquidGradient)
                                    }
                                }
                                .contentShape(Circle())
                        }
                        .buttonStyle(LiquidPressButtonStyle())
                        .contextMenu {
                            Button {
                                UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                                store.translateActiveTab(bilingual: false)
                            } label: {
                                Label(t("tr.inline"), systemImage: "sparkles")
                            }
                            Button {
                                UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                                store.translateActiveTab(bilingual: true)
                            } label: {
                                Label(t("tr.bilingual"), systemImage: "rectangle.split.2x1")
                            }
                            if store.activeTab.displayMode != .original {
                                Button {
                                    UIImpactFeedbackGenerator(style: .light).impactOccurred()
                                    store.restoreOriginal()
                                } label: {
                                    Label(t("tr.original"), systemImage: "arrow.uturn.backward")
                                }
                            }
                        }
                    }
                    .padding(.leading, 8)
                    .padding(.trailing, 6)
                    .frame(maxWidth: .infinity)
                    .frame(height: 44)
                    .liquidGlassPill(
                        tint: store.activeTab.isPrivate ? FluoThemeTokens.violet : .clear
                    )
                    .offset(x: dockDragOffset)
                    .simultaneousGesture(dockDragGesture)

                    // Tab Deck Counter Squircle
                    Button {
                        UIImpactFeedbackGenerator(style: .light).impactOccurred()
                        activeSheet = .tabs
                    } label: {
                        Text("\(store.activeWorkspaceTabs.count)")
                            .font(.caption.monospacedDigit().weight(.black))
                            .frame(width: 42, height: 44)
                            .contentShape(RoundedRectangle(cornerRadius: 15, style: .continuous))
                    }
                    .buttonStyle(LiquidPressButtonStyle())
                    .liquidGlass(cornerRadius: 15, interactive: true)
                    .accessibilityLabel(t("tabs.title"))
                    .contextMenu {
                        Button {
                            store.newTab(isPrivate: false)
                        } label: {
                            Label(t("tabs.newTab"), systemImage: "plus")
                        }
                        Button {
                            store.newTab(isPrivate: true)
                        } label: {
                            Label(t("tabs.newPrivate"), systemImage: "eye.slash")
                        }
                    }

                    // Spatial Bento Menu Orb (long-press gives instant Bookmarks / Reading List access)
                    Button {
                        UIImpactFeedbackGenerator(style: .light).impactOccurred()
                        activeSheet = .menu
                    } label: {
                        Image(systemName: store.isCurrentPageBookmarked ? "bookmark.fill" : "square.grid.2x2")
                            .font(.system(size: 15, weight: .bold))
                            .foregroundStyle(store.isCurrentPageBookmarked ? FluoThemeTokens.indigo : .primary)
                            .frame(width: 42, height: 44)
                            .contentShape(Circle())
                    }
                    .buttonStyle(LiquidPressButtonStyle())
                    .liquidGlassCircle(interactive: true)
                    .contextMenu {
                        Button {
                            activeSheet = .bookmarks
                        } label: {
                            Label(t("bm.title"), systemImage: "books.vertical")
                        }
                        Button {
                            openBookmarkEditor(for: store.currentBookmark())
                        } label: {
                            Label(
                                store.isCurrentPageBookmarked ? t("bm.editTitle") : t("menu.bookmarkAdd"),
                                systemImage: "star"
                            )
                        }
                        Button {
                            store.addCurrentPageToReadingList()
                        } label: {
                            Label(t("menu.readingListAdd"), systemImage: "eyeglasses")
                        }
                        Button {
                            activeSheet = .settings
                        } label: {
                            Label(t("menu.settings"), systemImage: "gearshape")
                        }
                    }
                }
                .padding(.horizontal, 12)
            }
            .padding(.bottom, 6)
        }
    }
}
