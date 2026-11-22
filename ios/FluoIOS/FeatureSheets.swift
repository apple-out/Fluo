import SwiftUI

struct OnboardingSheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            ScrollView {
                VStack(alignment: .leading, spacing: 18) {
                    VStack(alignment: .leading, spacing: 6) {
                        Text("Fluo")
                            .font(.largeTitle.bold())
                        Text(t("app.tagline"))
                            .font(.subheadline.weight(.semibold))
                            .foregroundStyle(.indigo)
                    }

                    Text(t("onb.desc"))
                        .font(.subheadline)
                        .foregroundStyle(.secondary)

                    LazyVGrid(columns: [GridItem(.flexible()), GridItem(.flexible())], spacing: 10) {
                        ForEach(AppLanguage.allCases) { lang in
                            Button {
                                store.uiLang = lang
                                store.targetLanguage = lang.defaultTargetLanguage
                                store.savePreferences()
                            } label: {
                                HStack {
                                    Text(lang.nativeName).fontWeight(.semibold)
                                    Spacer()
                                    Text(lang.shortCode)
                                        .font(.caption.bold())
                                        .foregroundStyle(.secondary)
                                }
                                .padding(14)
                                .background(
                                    RoundedRectangle(cornerRadius: 14)
                                        .fill(store.uiLang == lang ? Color.indigo.opacity(0.18) : Color(.secondarySystemBackground))
                                )
                                .overlay(
                                    RoundedRectangle(cornerRadius: 14)
                                        .stroke(store.uiLang == lang ? Color.indigo : .clear, lineWidth: 2)
                                )
                            }
                            .buttonStyle(.plain)
                        }
                    }

                    VStack(alignment: .leading, spacing: 6) {
                        Text(t("onb.targetLabel"))
                            .font(.footnote.weight(.semibold))
                        TextField(t("onb.targetLabel"), text: $store.targetLanguage)
                            .textFieldStyle(.roundedBorder)
                    }

                    Button {
                        store.onboardingCompleted = true
                        store.savePreferences()
                        dismiss()
                    } label: {
                        Text(t("onb.startBtn"))
                            .font(.headline)
                            .frame(maxWidth: .infinity)
                            .padding(.vertical, 12)
                    }
                    .buttonStyle(.borderedProminent)
                    .tint(.indigo)
                }
                .padding(20)
            }
            .navigationTitle(t("onb.title"))
            .navigationBarTitleDisplayMode(.inline)
        }
    }
}

struct TranslateSheet: View {
    @ObservedObject var store: BrowserStore
    @ObservedObject var catalog: ModelCatalog
    var openSettings: () -> Void
    @Environment(\.dismiss) private var dismiss

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            List {
                if store.provider == .hyMt2 && !catalog.isInstalled(store.selectedLocalModel) {
                    Section {
                        Text(t("tr.needModel"))
                            .font(.footnote)
                            .foregroundStyle(.secondary)
                        let rec = ModelCatalog.specs[0]
                        if let pct = catalog.downloadProgress[rec.id] {
                            ProgressView(value: pct)
                        } else {
                            Button(t("tr.downloadRec")) {
                                catalog.startDownload(rec)
                            }
                        }
                    }
                }

                if let err = store.translationError {
                    Section {
                        Text(err).font(.footnote).foregroundStyle(.red)
                    }
                }

                Section {
                    Button {
                        dismiss()
                        store.translateActiveTab(bilingual: false)
                    } label: {
                        Label(t("tr.inline"), systemImage: "sparkles")
                    }

                    Button {
                        dismiss()
                        store.translateActiveTab(bilingual: true)
                    } label: {
                        Label(t("tr.bilingual"), systemImage: "rectangle.split.2x1")
                    }

                    Button {
                        dismiss()
                        store.startRegionPicker(bilingual: false)
                    } label: {
                        Label(t("tr.regionPicker"), systemImage: "hand.tap")
                    }

                    Button {
                        dismiss()
                        store.translateSelectedText()
                    } label: {
                        Label(t("tr.selection"), systemImage: "text.quote")
                    }

                    Button {
                        dismiss()
                        store.restoreOriginal()
                    } label: {
                        Label(t("tr.original"), systemImage: "arrow.uturn.backward")
                    }
                }

                Section {
                    Picker(t("tr.style"), selection: $store.userData.style) {
                        ForEach(TranslationStyle.allCases) { st in
                            Text(t("style.\(st.rawValue)")).tag(st)
                        }
                    }
                    .onChange(of: store.userData.style) { _ in store.savePreferences() }

                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(t("tr.fontScale"))
                            Spacer()
                            Text(String(format: "%.2fx", store.userData.fontScale)).foregroundStyle(.secondary)
                        }
                        Slider(value: $store.userData.fontScale, in: 0.8...1.6, step: 0.05)
                            .onChange(of: store.userData.fontScale) { _ in store.savePreferences() }
                    }

                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(t("tr.lineHeight"))
                            Spacer()
                            Text(String(format: "%.1f", store.userData.lineHeight)).foregroundStyle(.secondary)
                        }
                        Slider(value: $store.userData.lineHeight, in: 1.2...2.2, step: 0.1)
                            .onChange(of: store.userData.lineHeight) { _ in store.savePreferences() }
                    }

                    Button {
                        openSettings()
                    } label: {
                        Label("\(store.provider.rawValue) · \(store.targetLanguage)", systemImage: "slider.horizontal.3")
                    }
                }
            }
            .navigationTitle(t("sheet.translateTitle"))
            .navigationBarTitleDisplayMode(.inline)
        }
    }
}

struct TabsSheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss

    @State private var showNewWorkspace = false
    @State private var wsDraftName = ""

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            VStack(spacing: 0) {
                ScrollView(.horizontal, showsIndicators: false) {
                    HStack(spacing: 8) {
                        ForEach(store.userData.workspaces) { ws in
                            Button {
                                store.switchWorkspace(ws.id)
                            } label: {
                                Text(ws.name)
                                    .font(.caption.weight(.semibold))
                                    .padding(.horizontal, 12)
                                    .padding(.vertical, 7)
                                    .background(
                                        store.userData.activeWorkspaceId == ws.id
                                            ? AnyShapeStyle(Color.indigo)
                                            : AnyShapeStyle(Color(.secondarySystemBackground)),
                                        in: Capsule()
                                    )
                                    .foregroundStyle(store.userData.activeWorkspaceId == ws.id ? .white : .primary)
                            }
                            .contextMenu {
                                if ws.id != "default" {
                                    Button(role: .destructive) {
                                        store.deleteWorkspace(ws.id)
                                    } label: {
                                        Label(t("set.delete"), systemImage: "trash")
                                    }
                                }
                            }
                        }

                        Button {
                            wsDraftName = ""
                            showNewWorkspace = true
                        } label: {
                            Image(systemName: "plus.circle")
                                .font(.title3)
                        }
                    }
                    .padding(.horizontal, 16)
                    .padding(.vertical, 10)
                }

                List {
                    ForEach(store.activeWorkspaceTabs) { tab in
                        HStack {
                            Button {
                                store.activeTabId = tab.id
                                dismiss()
                            } label: {
                                HStack(spacing: 8) {
                                    if tab.isPinned {
                                        Image(systemName: "pin.fill").foregroundStyle(.indigo).font(.caption)
                                    }
                                    if tab.isPrivate {
                                        Text(t("tabs.privateBadge"))
                                            .font(.caption2.bold())
                                            .padding(.horizontal, 6)
                                            .padding(.vertical, 2)
                                            .background(Color.purple.opacity(0.2), in: Capsule())
                                    }
                                    VStack(alignment: .leading, spacing: 2) {
                                        Text(tab.title)
                                            .font(.subheadline.weight(tab.id == store.activeTabId ? .bold : .regular))
                                            .lineLimit(1)
                                        Text(tab.urlString)
                                            .font(.caption)
                                            .foregroundStyle(.secondary)
                                            .lineLimit(1)
                                    }
                                }
                            }
                            .buttonStyle(.plain)

                            Spacer()

                            if !tab.isPinned && store.tabs.count > 1 {
                                Button(role: .destructive) {
                                    store.closeTab(tab.id)
                                } label: {
                                    Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
                                }
                                .buttonStyle(.plain)
                            }
                        }
                        .contextMenu {
                            Button {
                                store.togglePinTab(tab.id)
                            } label: {
                                Label(tab.isPinned ? t("tabs.unpin") : t("tabs.pin"), systemImage: "pin")
                            }

                            Menu(t("tabs.moveWs")) {
                                ForEach(store.userData.workspaces) { ws in
                                    Button(ws.name) { store.moveTab(tab.id, toWorkspace: ws.id) }
                                }
                            }
                        }
                    }
                }
            }
            .navigationTitle(t("tabs.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    Button {
                        store.newTab(isPrivate: true)
                        dismiss()
                    } label: {
                        Label(t("tabs.newPrivate"), systemImage: "eye.slash")
                    }
                }
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button {
                        store.newTab(isPrivate: false)
                        dismiss()
                    } label: {
                        Label(t("tabs.newTab"), systemImage: "plus")
                    }
                }
            }
            .alert(t("ws.new"), isPresented: $showNewWorkspace) {
                TextField(t("ws.namePlaceholder"), text: $wsDraftName)
                Button(t("common.cancel"), role: .cancel) {}
                Button(t("common.add")) { store.createWorkspace(name: wsDraftName) }
            }
        }
    }
}

struct MenuSheet: View {
    @ObservedObject var store: BrowserStore
    var openSheet: (ActiveSheetKind) -> Void
    @Environment(\.dismiss) private var dismiss

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    Button {
                        store.toggleCurrentBookmark()
                        dismiss()
                    } label: {
                        Label(
                            store.isCurrentPageBookmarked ? t("menu.bookmarkRemove") : t("menu.bookmarkAdd"),
                            systemImage: store.isCurrentPageBookmarked ? "star.fill" : "star"
                        )
                    }

                    Button {
                        dismiss()
                        store.showFindBar = true
                    } label: {
                        Label(t("menu.find"), systemImage: "magnifyingglass")
                    }

                    Button {
                        store.activeTab.applyDesktopMode(!store.activeTab.isDesktopMode)
                        dismiss()
                    } label: {
                        HStack {
                            Label(t("menu.desktop"), systemImage: "desktopcomputer")
                            Spacer()
                            if store.activeTab.isDesktopMode {
                                Image(systemName: "checkmark").foregroundStyle(.indigo)
                            }
                        }
                    }

                    if !store.currentHost.isEmpty {
                        Button {
                            store.toggleCurrentHostAdBlock()
                            dismiss()
                        } label: {
                            HStack {
                                Label("\(t("menu.adblockSite")) (\(store.currentHost))", systemImage: "shield.lefthalf.filled")
                                Spacer()
                                Image(systemName: store.isCurrentHostAdBlockEnabled ? "checkmark.circle.fill" : "circle")
                                    .foregroundStyle(store.isCurrentHostAdBlockEnabled ? .green : .secondary)
                            }
                        }
                    }
                }

                Section {
                    Button { openSheet(.bookmarks) } label: { Label(t("menu.bookmarks"), systemImage: "book") }
                    Button { openSheet(.history) } label: { Label(t("menu.history"), systemImage: "clock") }
                    Button { openSheet(.translationHistory) } label: { Label(t("menu.trHistory"), systemImage: "clock.arrow.2.circlepath") }
                    Button { openSheet(.glossary) } label: { Label(t("menu.glossary"), systemImage: "character.book.closed") }
                }

                Section {
                    Button { openSheet(.feedback) } label: { Label(t("menu.feedback"), systemImage: "ladybug") }
                    Button { openSheet(.settings) } label: { Label(t("menu.settings"), systemImage: "gearshape") }
                }
            }
            .navigationTitle(t("menu.title"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.close")) { dismiss() }
                }
            }
        }
    }
}

struct BookmarksSheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss
    @State private var search = ""

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var filtered: [BookmarkItem] {
        let q = search.trimmingCharacters(in: .whitespacesAndNewlines)
        if q.isEmpty { return store.userData.bookmarks }
        return store.userData.bookmarks.filter {
            $0.title.localizedCaseInsensitiveContains(q) || $0.url.localizedCaseInsensitiveContains(q)
        }
    }

    var body: some View {
        NavigationStack {
            List {
                if filtered.isEmpty {
                    Text(t("bm.empty")).foregroundStyle(.secondary)
                } else {
                    ForEach(filtered) { item in
                        Button {
                            store.activeTab.navigate(input: item.url, searchEngine: store.userData.searchEngine)
                            dismiss()
                        } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(item.title).font(.subheadline.weight(.medium)).lineLimit(1)
                                Text(item.url).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                            }
                        }
                        .swipeActions {
                            Button(role: .destructive) { store.deleteBookmark(item.id) } label: { Text(t("set.delete")) }
                        }
                    }
                }
            }
            .searchable(text: $search, prompt: t("bm.search"))
            .navigationTitle(t("bm.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.done")) { dismiss() }
                }
            }
        }
    }
}

struct HistorySheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            List {
                if store.userData.history.isEmpty {
                    Text(t("hist.empty")).foregroundStyle(.secondary)
                } else {
                    ForEach(store.userData.history) { entry in
                        Button {
                            store.activeTab.navigate(input: entry.url, searchEngine: store.userData.searchEngine)
                            dismiss()
                        } label: {
                            VStack(alignment: .leading, spacing: 2) {
                                Text(entry.title).font(.subheadline).lineLimit(1)
                                Text(entry.url).font(.caption).foregroundStyle(.secondary).lineLimit(1)
                            }
                        }
                    }
                }
            }
            .navigationTitle(t("hist.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarLeading) {
                    if !store.userData.history.isEmpty {
                        Button(t("hist.clear"), role: .destructive) { store.clearHistory() }
                    }
                }
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.done")) { dismiss() }
                }
            }
        }
    }
}

struct TranslationHistorySheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            List {
                if store.userData.translationHistory.isEmpty {
                    Text(t("trhist.empty")).foregroundStyle(.secondary)
                } else {
                    ForEach(store.userData.translationHistory) { item in
                        Button {
                            store.activeTab.navigate(input: item.url, searchEngine: store.userData.searchEngine)
                            dismiss()
                        } label: {
                            VStack(alignment: .leading, spacing: 3) {
                                Text(item.title).font(.subheadline.weight(.medium)).lineLimit(1)
                                Text("\(item.targetLanguage) · \(item.provider)")
                                    .font(.caption).foregroundStyle(.indigo)
                                Text(item.url).font(.caption2).foregroundStyle(.secondary).lineLimit(1)
                            }
                        }
                    }
                }
            }
            .navigationTitle(t("trhist.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.done")) { dismiss() }
                }
            }
        }
    }
}

struct GlossarySheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss

    @State private var src = ""
    @State private var dst = ""
    @State private var caseSensitive = false

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    TextField(t("gl.source"), text: $src)
                    TextField(t("gl.target"), text: $dst)
                    Toggle(t("gl.case"), isOn: $caseSensitive)
                    Button(t("gl.add")) {
                        store.addGlossaryItem(source: src, target: dst, caseSensitive: caseSensitive)
                        src = ""
                        dst = ""
                    }
                    .disabled(src.trimmingCharacters(in: .whitespacesAndNewlines).isEmpty)
                }

                Section {
                    if store.userData.glossary.isEmpty {
                        Text(t("gl.empty")).foregroundStyle(.secondary)
                    } else {
                        ForEach(store.userData.glossary) { item in
                            HStack {
                                VStack(alignment: .leading) {
                                    Text(item.source).font(.subheadline.weight(.semibold))
                                    Text(item.target.isEmpty ? "(keep original)" : item.target)
                                        .font(.caption).foregroundStyle(.secondary)
                                }
                                Spacer()
                                Button(role: .destructive) {
                                    store.deleteGlossaryItem(item.id)
                                } label: {
                                    Image(systemName: "trash")
                                }
                            }
                        }
                    }
                }
            }
            .navigationTitle(t("gl.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.done")) { dismiss() }
                }
            }
        }
    }
}

struct SelectionResultSheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            List {
                Section {
                    Text(store.selectionSourceText).font(.subheadline).foregroundStyle(.secondary)
                }
                Section {
                    if store.selectionTranslatedText.isEmpty {
                        HStack(spacing: 8) {
                            ProgressView()
                            Text(t("tr.progress"))
                        }
                    } else {
                        Text(store.selectionTranslatedText).font(.body)
                        Button(t("sel.copy")) {
                            UIPasteboard.general.string = store.selectionTranslatedText
                        }
                        Button(t("sel.addGlossary")) {
                            store.addGlossaryItem(source: store.selectionSourceText, target: store.selectionTranslatedText, caseSensitive: false)
                            dismiss()
                        }
                    }
                }
            }
            .navigationTitle(t("sel.title"))
            .navigationBarTitleDisplayMode(.inline)
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.done")) { dismiss() }
                }
            }
        }
    }
}

struct FeedbackSheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss
    @Environment(\.openURL) private var openURL

    @State private var category = "bug"
    @State private var summary = ""
    @State private var details = ""
    @State private var includeDiag = true

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    private var markdownReport: String {
        var lines = [
            "## Summary",
            summary.trimmingCharacters(in: .whitespacesAndNewlines),
            "",
            "## Details",
            details.trimmingCharacters(in: .whitespacesAndNewlines),
        ]
        if includeDiag {
            lines.append("")
            lines.append("## iOS Diagnostics (Sanitized)")
            lines.append("- Platform: Fluo iOS \(UIDevice.current.systemVersion)")
            lines.append("- UI / Target: \(store.uiLang.rawValue) -> \(store.targetLanguage)")
            lines.append("- Provider: \(store.provider.rawValue)")
            lines.append("- Style: \(store.userData.style.rawValue)")
        }
        return lines.joined(separator: "\n")
    }

    var body: some View {
        NavigationStack {
            Form {
                Section {
                    Picker(t("fb.category"), selection: $category) {
                        Text(t("fb.bug")).tag("bug")
                        Text(t("fb.ux")).tag("enhancement")
                        Text(t("fb.mistranslation")).tag("translation-error")
                    }
                    TextField(t("fb.summary"), text: $summary)
                    TextEditor(text: $details).frame(minHeight: 110)
                    Toggle(t("fb.includeDiag"), isOn: $includeDiag)
                }

                Section {
                    Button(t("fb.openGithub")) {
                        let title = summary.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? "Fluo%20iOS%20Feedback"
                        let body = markdownReport.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? ""
                        let label = category.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? "bug"
                        if let url = URL(string: "https://github.com/apple-out/transratebrowser/issues/new?title=\(title)&labels=\(label)&body=\(body)") {
                            openURL(url)
                        }
                    }
                    Button(t("fb.copyMd")) {
                        UIPasteboard.general.string = markdownReport
                    }
                }
            }
            .navigationTitle(t("fb.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.close")) { dismiss() }
                }
            }
        }
    }
}

struct SettingsSheet: View {
    @ObservedObject var store: BrowserStore
    @ObservedObject var catalog: ModelCatalog
    @Environment(\.dismiss) private var dismiss

    @State private var geminiKeyDraft = KeychainStore.load(account: "gemini_api_key") ?? ""
    @State private var openAiKeyDraft = KeychainStore.load(account: "openai_api_key") ?? ""
    @State private var showClearedAlert = false

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            Form {
                Section(t("set.langSection")) {
                    Picker(t("set.uiLang"), selection: $store.uiLang) {
                        ForEach(AppLanguage.allCases) { lang in
                            Text(lang.nativeName).tag(lang)
                        }
                    }
                    .onChange(of: store.uiLang) { newLang in
                        store.targetLanguage = newLang.defaultTargetLanguage
                        store.savePreferences()
                    }

                    TextField(t("set.targetLang"), text: $store.targetLanguage)
                        .onChange(of: store.targetLanguage) { _ in store.savePreferences() }
                }

                Section(t("set.browserSection")) {
                    Picker(t("set.searchEngine"), selection: $store.userData.searchEngine) {
                        ForEach(SearchEngine.allCases) { engine in
                            Text(engine.displayName).tag(engine)
                        }
                    }
                    .onChange(of: store.userData.searchEngine) { _ in store.savePreferences() }

                    Picker(t("set.theme"), selection: $store.userData.appTheme) {
                        Text(t("theme.system")).tag(AppTheme.system)
                        Text(t("theme.light")).tag(AppTheme.light)
                        Text(t("theme.dark")).tag(AppTheme.dark)
                    }
                    .onChange(of: store.userData.appTheme) { _ in store.savePreferences() }

                    Toggle(t("set.adblock"), isOn: $store.userData.adBlockEnabled)
                        .onChange(of: store.userData.adBlockEnabled) { enabled in
                            store.savePreferences()
                            AdBlockManager.shared.apply(
                                to: store.activeTab.webView,
                                enabled: enabled,
                                host: store.currentHost,
                                allowlist: store.userData.adBlockAllowlist
                            )
                        }

                    VStack(alignment: .leading, spacing: 4) {
                        HStack {
                            Text(t("set.zoom"))
                            Spacer()
                            Text("\(Int((store.activeTab.pageZoom * 100).rounded()))%").foregroundStyle(.secondary)
                        }
                        Slider(value: Binding(
                            get: { store.activeTab.pageZoom },
                            set: { store.activeTab.applyPageZoom($0) }
                        ), in: 0.7...1.8, step: 0.1)
                    }

                    Button(t("set.clearSiteData"), role: .destructive) {
                        Task {
                            await store.clearSiteData()
                            showClearedAlert = true
                        }
                    }
                }

                Section(t("set.aiSection")) {
                    Picker(t("set.provider"), selection: $store.provider) {
                        Text(t("set.provider.local")).tag(AiProviderKind.hyMt2)
                        Text(t("set.provider.gemini")).tag(AiProviderKind.googleGemini)
                        Text(t("set.provider.openai")).tag(AiProviderKind.openAiCompatible)
                    }
                    .onChange(of: store.provider) { _ in store.savePreferences() }
                }

                if store.provider == .hyMt2 {
                    Section {
                        Text(t("set.modelsHint"))
                            .font(.caption)
                            .foregroundStyle(.secondary)

                        ForEach(ModelCatalog.specs) { spec in
                            VStack(alignment: .leading, spacing: 6) {
                                Text(spec.displayName).font(.subheadline.weight(.semibold))
                                Text(spec.subtitle).font(.caption).foregroundStyle(.secondary)

                                if let pct = catalog.downloadProgress[spec.id] {
                                    HStack {
                                        ProgressView(value: pct)
                                        Button(t("set.cancel")) { catalog.cancelDownload(spec.id) }
                                            .font(.caption)
                                    }
                                } else if catalog.isInstalled(spec.id) {
                                    HStack {
                                        Button {
                                            store.selectedLocalModel = spec.id
                                            store.savePreferences()
                                        } label: {
                                            Label(
                                                t("set.installed"),
                                                systemImage: store.selectedLocalModel == spec.id ? "checkmark.circle.fill" : "circle"
                                            )
                                        }
                                        Spacer()
                                        Button(t("set.delete"), role: .destructive) {
                                            catalog.deleteModel(spec.id)
                                        }
                                        .font(.caption)
                                    }
                                } else {
                                    Button(t("set.download")) {
                                        catalog.startDownload(spec)
                                    }
                                    .buttonStyle(.bordered)
                                }
                            }
                            .padding(.vertical, 4)
                        }

                        if TranslationEngine.shared.activeLocalModelPath != nil {
                            Button(t("set.unload"), role: .destructive) {
                                TranslationEngine.shared.unloadOnDeviceModel()
                            }
                        }

                        if let err = catalog.statusError {
                            Text(err).font(.caption).foregroundStyle(.red)
                        }
                    } header: {
                        Text(t("set.modelsSection"))
                    }
                } else if store.provider == .googleGemini {
                    Section(t("set.provider.gemini")) {
                        SecureField(t("set.apiKey"), text: $geminiKeyDraft)
                        TextField(t("set.modelId"), text: $store.geminiModel)
                            .onChange(of: store.geminiModel) { _ in store.savePreferences() }
                        HStack {
                            Button(t("set.saveKey")) {
                                KeychainStore.save(account: "gemini_api_key", value: geminiKeyDraft)
                            }
                            Spacer()
                            Button(t("set.clearKey"), role: .destructive) {
                                geminiKeyDraft = ""
                                KeychainStore.delete(account: "gemini_api_key")
                            }
                        }
                    }
                } else {
                    Section(t("set.provider.openai")) {
                        TextField(t("set.baseUrl"), text: $store.openAiBaseUrl)
                            .textInputAutocapitalization(.never)
                            .autocorrectionDisabled()
                            .onChange(of: store.openAiBaseUrl) { _ in store.savePreferences() }
                        SecureField(t("set.apiKey"), text: $openAiKeyDraft)
                        TextField(t("set.modelId"), text: $store.openAiModel)
                            .textInputAutocapitalization(.never)
                            .onChange(of: store.openAiModel) { _ in store.savePreferences() }
                        HStack {
                            Button(t("set.saveKey")) {
                                KeychainStore.save(account: "openai_api_key", value: openAiKeyDraft)
                            }
                            Spacer()
                            Button(t("set.clearKey"), role: .destructive) {
                                openAiKeyDraft = ""
                                KeychainStore.delete(account: "openai_api_key")
                            }
                        }
                    }
                }
            }
            .navigationTitle(t("set.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button(t("common.done")) {
                        store.savePreferences()
                        dismiss()
                    }
                }
            }
            .alert(t("set.cleared"), isPresented: $showClearedAlert) {
                Button(t("common.done"), role: .cancel) {}
            }
        }
    }
}
