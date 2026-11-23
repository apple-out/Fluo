import SwiftUI
import UIKit
import WebKit

enum ActiveSheetKind: String, Identifiable {
    case translate
    case tabs
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
    }

    final class Coordinator: NSObject, UITextFieldDelegate {
        var parent: SelectAllURLField

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
        return isHttps ? .green : .orange
    }

    var body: some View {
        ZStack(alignment: .bottom) {
            VStack(spacing: 0) {
                if store.activeTab.isLoading {
                    ProgressView(value: store.activeTab.progress)
                        .tint(FluoThemeTokens.indigo)
                        .progressViewStyle(.linear)
                }

                if store.showFindBar {
                    findBar
                        .padding(.horizontal, 12)
                        .padding(.top, 6)
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
                    .liquidGlass(cornerRadius: 18, tint: FluoThemeTokens.indigo)
                    .padding(.horizontal, 12)
                    .padding(.top, 6)
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
                            VStack(spacing: 12) {
                                Image(systemName: "wifi.exclamationmark")
                                    .font(.largeTitle)
                                    .foregroundStyle(.orange)
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
                                    .padding(.horizontal, 16)
                                    .padding(.vertical, 8)
                                    .liquidGlassPill(tint: FluoThemeTokens.indigo, interactive: true, prominent: true)

                                    Button(t("menu.startPage")) {
                                        store.activeTab.openStartPage()
                                    }
                                    .font(.caption.bold())
                                    .padding(.horizontal, 14)
                                    .padding(.vertical, 8)
                                    .liquidGlassPill(interactive: true)
                                }
                            }
                            .padding(20)
                            .liquidGlass(cornerRadius: 24)
                            .padding(.horizontal, 24)
                        }
                    }
                }
            }

            VStack(spacing: 8) {
                if isEditingURL {
                    omniboxOverlayPanel
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
                        Button(t("tr.selection")) {
                            store.translateSelectedText()
                        }
                        .font(.caption.bold())
                        .foregroundStyle(.white)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 5)
                        .liquidGlassPill(tint: FluoThemeTokens.indigo, interactive: true, prominent: true)

                        Button(t("sel.addGlossary")) {
                            store.addGlossaryItem(source: store.liveWebSelection, target: "", caseSensitive: false)
                            store.liveWebSelection = ""
                            activeSheet = .glossary
                        }
                        .font(.caption.bold())
                        .padding(.horizontal, 10)
                        .padding(.vertical, 5)
                        .liquidGlassPill(interactive: true)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 7)
                    .liquidGlassPill()
                    .padding(.horizontal, 12)
                }

                if store.isSpeakingTTS {
                    HStack(spacing: 12) {
                        Image(systemName: "speaker.wave.2.fill")
                            .foregroundStyle(FluoThemeTokens.indigo)
                            .font(.caption.bold())
                        Text("\(store.ttsIndex + 1)/\(max(1, store.ttsTotal))")
                            .font(.caption.monospacedDigit().weight(.semibold))
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
                                .padding(.horizontal, 7)
                                .padding(.vertical, 3)
                                .liquidGlassPill(interactive: true)
                        }
                        .buttonStyle(.plain)
                        Button {
                            store.stopTTS()
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(.secondary)
                        }
                        .buttonStyle(.plain)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .liquidGlassPill(tint: FluoThemeTokens.indigo)
                }

                if !isEditingURL && store.showAutoTranslatePrompt && !store.isTranslating && store.activeTab.displayMode == .original {
                    HStack(spacing: 8) {
                        Image(systemName: "globe")
                            .font(.caption.bold())
                            .foregroundStyle(FluoThemeTokens.indigo)
                        Text("\(t("tr.autoPrompt")) (\(store.detectedForeignLanguageCode))")
                            .font(.caption.weight(.semibold))
                            .lineLimit(1)
                        Spacer(minLength: 4)
                        Button(t("bar.translate")) {
                            store.translateActiveTab(bilingual: false)
                        }
                        .font(.caption.bold())
                        .foregroundStyle(.white)
                        .padding(.horizontal, 10)
                        .padding(.vertical, 5)
                        .liquidGlassPill(tint: FluoThemeTokens.indigo, interactive: true, prominent: true)

                        Button(t("tr.alwaysHost")) {
                            store.toggleAlwaysTranslateCurrentHost()
                        }
                        .font(.caption2.bold())
                        .padding(.horizontal, 8)
                        .padding(.vertical, 5)
                        .liquidGlassPill(interactive: true)

                        Button {
                            store.showAutoTranslatePrompt = false
                        } label: {
                            Image(systemName: "xmark.circle.fill")
                                .foregroundStyle(.secondary)
                        }
                        .buttonStyle(.plain)
                    }
                    .padding(.horizontal, 12)
                    .padding(.vertical, 8)
                    .liquidGlassPill()
                    .padding(.horizontal, 12)
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
                        }
                        .buttonStyle(.plain)
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 9)
                    .liquidGlassPill(tint: FluoThemeTokens.indigo)
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
                        }
                        .buttonStyle(.plain)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 9)
                    .liquidGlass(cornerRadius: 16, tint: .red)
                    .padding(.horizontal, 14)
                }

                safariBottomChrome
            }
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
                    TranslateSheet(store: store, catalog: catalog, openSettings: {
                        switchSheet(.settings)
                    })
                    .presentationDetents([.medium, .large])
                case .tabs:
                    TabsSheet(store: store)
                        .presentationDetents([.medium, .large])
                case .menu:
                    MenuSheet(store: store, openSheet: { target in
                        if target == .bookmarkEditor {
                            openBookmarkEditor(for: store.currentBookmark())
                        } else {
                            switchSheet(target)
                        }
                    })
                    .presentationDetents([.medium, .large])
                case .bookmarks:
                    BookmarksSheet(store: store, onEdit: { item in
                        openBookmarkEditor(for: item)
                    })
                case .bookmarkEditor:
                    BookmarkEditorSheet(store: store, existing: editingBookmark)
                        .presentationDetents([.medium])
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
                                isEditingURL = false
                                UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
                                store.selectTab(tid)
                            } else {
                                isEditingURL = false
                                UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
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
                .liquidGlass(cornerRadius: 18)
                .padding(.horizontal, 12)
            }

            ScrollView(.horizontal, showsIndicators: false) {
                HStack(spacing: 6) {
                    ForEach([".com", ".kr", "/", "https://", "@namu ", "@naver ", "@yt "], id: \.self) { token in
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
                            isEditingURL = false
                            UIApplication.shared.sendAction(#selector(UIResponder.resignFirstResponder), to: nil, from: nil, for: nil)
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

    private var safariBottomChrome: some View {
        let collapsed = store.isDockCollapsed && !store.activeTab.isStartPage
        let displayHost = URL(string: store.activeTab.urlString)?.host ?? store.activeTab.title

        return VStack(spacing: collapsed ? 0 : 8) {
            // Safari-style Top Row: Address & Security Pill (morphs smoothly into compact pill when scrolled)
            HStack(spacing: 8) {
                Button {
                    activeSheet = .siteSecurity
                } label: {
                    Image(systemName: securityIconName)
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(securityIconColor)
                        .frame(width: 24, height: 24)
                        .contentShape(Circle())
                }
                .buttonStyle(.plain)

                if collapsed {
                    Text(displayHost.isEmpty ? "Fluo" : displayHost)
                        .font(.caption.weight(.semibold))
                        .lineLimit(1)
                } else {
                    SelectAllURLField(
                        text: $urlDraft,
                        isEditing: $isEditingURL,
                        placeholder: t("bar.urlPlaceholder")
                    ) {
                        store.activeTab.navigate(input: urlDraft, searchEngine: store.userData.searchEngine)
                    }
                    .frame(height: 24)
                }

                if collapsed {
                    if store.activeTab.displayMode != .original {
                        Image(systemName: "character.bubble.fill")
                            .font(.caption2.weight(.bold))
                            .foregroundStyle(FluoThemeTokens.indigo)
                    }
                } else if !store.activeTab.isStartPage {
                    Button {
                        if store.activeTab.isLoading {
                            store.activeTab.webView.stopLoading()
                        } else {
                            store.activeTab.webView.reload()
                        }
                    } label: {
                        Image(systemName: store.activeTab.isLoading ? "xmark" : "arrow.clockwise")
                            .font(.caption.weight(.bold))
                            .foregroundStyle(.secondary)
                            .frame(width: 28, height: 28)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
            }
            .padding(.horizontal, collapsed ? 14 : 12)
            .padding(.vertical, collapsed ? 8 : 7)
            .frame(maxWidth: collapsed ? 230 : .infinity)
            .liquidGlassPill(
                tint: store.activeTab.isPrivate ? FluoThemeTokens.violet : .clear
            )
            .contentShape(Capsule())
            .onTapGesture(count: 2) {
                guard collapsed else { return }
                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                store.activeTab.scrollToTop()
                store.isDockCollapsed = false
            }
            .onTapGesture(count: 1) {
                guard collapsed else { return }
                UIImpactFeedbackGenerator(style: .light).impactOccurred()
                store.isDockCollapsed = false
            }
            .gesture(dockDragGesture)

            // Safari-style Bottom Action Toolbar Row (smoothly height/opacity collapses on scroll down)
            LiquidGlassGroup(spacing: 10) {
                HStack(spacing: 0) {
                    Button {
                        store.activeTab.webView.goBack()
                    } label: {
                        Image(systemName: "chevron.left")
                            .font(.system(size: 16, weight: .semibold))
                            .frame(width: 40, height: 40)
                            .contentShape(Circle())
                    }
                    .buttonStyle(LiquidPressButtonStyle())
                    .liquidGlassCircle(interactive: true)
                    .disabled(!store.activeTab.canGoBack)
                    .opacity(store.activeTab.canGoBack ? 1.0 : 0.38)
                    .contextMenu {
                        backForwardContextMenu(
                            items: Array(store.activeTab.webView.backForwardList.backList.reversed().prefix(8))
                        )
                    }

                    Spacer(minLength: 4)

                    Button {
                        store.activeTab.goForwardFromCurrent()
                    } label: {
                        Image(systemName: "chevron.right")
                            .font(.system(size: 16, weight: .semibold))
                            .frame(width: 40, height: 40)
                            .contentShape(Circle())
                    }
                    .buttonStyle(LiquidPressButtonStyle())
                    .liquidGlassCircle(interactive: true)
                    .disabled(!store.activeTab.canGoForward)
                    .opacity(store.activeTab.canGoForward ? 1.0 : 0.38)
                    .contextMenu {
                        backForwardContextMenu(
                            items: Array(store.activeTab.webView.backForwardList.forwardList.prefix(8))
                        )
                    }

                    Spacer(minLength: 4)

                    Button {
                        activeSheet = .translate
                    } label: {
                        Image(systemName: store.activeTab.displayMode == .original ? "character.bubble" : "character.bubble.fill")
                            .font(.system(size: 16, weight: .semibold))
                            .foregroundStyle(store.activeTab.displayMode == .original ? Color.primary : Color.white)
                            .frame(width: 44, height: 40)
                            .contentShape(Circle())
                    }
                    .buttonStyle(LiquidPressButtonStyle())
                    .liquidGlassCircle(
                        tint: store.activeTab.displayMode == .original ? .clear : FluoThemeTokens.indigo,
                        interactive: true,
                        prominent: store.activeTab.displayMode != .original
                    )
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

                    Spacer(minLength: 4)

                    Button {
                        openBookmarkEditor(for: store.currentBookmark())
                    } label: {
                        Image(systemName: store.isCurrentPageBookmarked ? "star.fill" : "star")
                            .font(.system(size: 15, weight: .semibold))
                            .foregroundStyle(store.isCurrentPageBookmarked ? .white : .primary)
                            .frame(width: 40, height: 40)
                            .contentShape(Circle())
                    }
                    .buttonStyle(LiquidPressButtonStyle())
                    .liquidGlassCircle(
                        tint: store.isCurrentPageBookmarked ? FluoThemeTokens.indigo : .clear,
                        interactive: true,
                        prominent: store.isCurrentPageBookmarked
                    )
                    .disabled(store.activeTab.isStartPage)
                    .opacity(store.activeTab.isStartPage ? 0.38 : 1.0)

                    Spacer(minLength: 4)

                    Button {
                        activeSheet = .tabs
                    } label: {
                        Text("\(store.activeWorkspaceTabs.count)")
                            .font(.caption.bold())
                            .frame(width: 40, height: 40)
                            .contentShape(RoundedRectangle(cornerRadius: 13, style: .continuous))
                    }
                    .buttonStyle(LiquidPressButtonStyle())
                    .liquidGlass(cornerRadius: 13, interactive: true)

                    Spacer(minLength: 4)

                    Button {
                        activeSheet = .menu
                    } label: {
                        Image(systemName: "ellipsis")
                            .font(.system(size: 16, weight: .bold))
                            .frame(width: 40, height: 40)
                            .contentShape(Circle())
                    }
                    .buttonStyle(LiquidPressButtonStyle())
                    .liquidGlassCircle(interactive: true)
                }
            }
            .frame(height: collapsed ? 0 : 44)
            .opacity(collapsed ? 0 : 1)
            .scaleEffect(collapsed ? 0.92 : 1.0, anchor: .bottom)
            .clipped()
            .allowsHitTesting(!collapsed)
        }
        .liquidGlassDockChrome(tint: store.activeTab.isPrivate ? FluoThemeTokens.violet : .clear)
        .offset(x: dockDragOffset)
        .padding(.horizontal, 12)
        .padding(.bottom, 6)
        .animation(.interactiveSpring(response: 0.32, dampingFraction: 0.86), value: store.isDockCollapsed)
    }
}
