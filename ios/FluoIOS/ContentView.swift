import SwiftUI

struct ContentView: View {
    @ObservedObject var store: BrowserStore
    @ObservedObject private var catalog = ModelCatalog.shared

    @State private var urlDraft = ""
    @State private var showTranslateSheet = false
    @State private var showTabsSheet = false
    @State private var showSettingsSheet = false
    @State private var showOnboarding = false

    private func t(_ key: String) -> String {
        L10n.tr(store.uiLang, key)
    }

    var body: some View {
        ZStack(alignment: .bottom) {
            VStack(spacing: 0) {
                if store.activeTab.isLoading {
                    ProgressView(value: store.activeTab.progress)
                        .tint(.indigo)
                        .progressViewStyle(.linear)
                }

                WebViewContainer(webView: store.activeTab.webView)
                    .ignoresSafeArea(edges: .bottom)
            }

            if store.isTranslating && store.translationTotal > 0 {
                HStack(spacing: 8) {
                    ProgressView()
                    Text("\(t("tr.progress")) \(store.translationDone)/\(store.translationTotal)")
                        .font(.caption.weight(.semibold))
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 8)
                .background(.ultraThinMaterial, in: Capsule())
                .padding(.bottom, 74)
            }

            bottomBar
        }
        .onAppear {
            urlDraft = store.activeTab.urlString
            if !store.onboardingCompleted {
                showOnboarding = true
            }
        }
        .onChange(of: store.activeTab.urlString) { nextUrl in
            urlDraft = nextUrl
        }
        .sheet(isPresented: $showOnboarding) {
            OnboardingSheet(store: store)
                .interactiveDismissDisabled(true)
        }
        .sheet(isPresented: $showTranslateSheet) {
            TranslateSheet(store: store, catalog: catalog, openSettings: {
                showTranslateSheet = false
                showSettingsSheet = true
            })
            .presentationDetents([.medium])
        }
        .sheet(isPresented: $showTabsSheet) {
            TabsSheet(store: store)
                .presentationDetents([.medium, .large])
        }
        .sheet(isPresented: $showSettingsSheet) {
            SettingsSheet(store: store, catalog: catalog)
        }
    }

    private var bottomBar: some View {
        HStack(spacing: 8) {
            Button {
                store.activeTab.webView.goBack()
            } label: {
                Image(systemName: "chevron.left")
            }
            .disabled(!store.activeTab.canGoBack)

            Button {
                store.activeTab.webView.goForward()
            } label: {
                Image(systemName: "chevron.right")
            }
            .disabled(!store.activeTab.canGoForward)

            TextField(t("bar.urlPlaceholder"), text: $urlDraft)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .keyboardType(.URL)
                .submitLabel(.go)
                .padding(.horizontal, 10)
                .padding(.vertical, 7)
                .background(.thinMaterial, in: Capsule())
                .onSubmit {
                    store.activeTab.navigate(input: urlDraft)
                }

            Button {
                showTranslateSheet = true
            } label: {
                Image(systemName: store.activeTab.displayMode == .original ? "character.bubble" : "character.bubble.fill")
                    .foregroundStyle(store.activeTab.displayMode == .original ? .primary : Color.indigo)
            }

            Button {
                showTabsSheet = true
            } label: {
                Text("\(store.tabs.count)")
                    .font(.caption.bold())
                    .frame(width: 24, height: 24)
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(.primary, lineWidth: 1.5))
            }

            Button {
                showSettingsSheet = true
            } label: {
                Image(systemName: "gearshape")
            }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .background(.ultraThinMaterial, in: RoundedRectangle(cornerRadius: 24, style: .continuous))
        .overlay(
            RoundedRectangle(cornerRadius: 24, style: .continuous)
                .stroke(.white.opacity(0.18), lineWidth: 1)
        )
        .shadow(color: .black.opacity(0.25), radius: 16, y: 6)
        .padding(.horizontal, 12)
        .padding(.bottom, 6)
    }
}

private struct OnboardingSheet: View {
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

private struct TranslateSheet: View {
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
                        store.restoreOriginal()
                    } label: {
                        Label(t("tr.original"), systemImage: "arrow.uturn.backward")
                    }
                }

                Section {
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

private struct TabsSheet: View {
    @ObservedObject var store: BrowserStore
    @Environment(\.dismiss) private var dismiss

    private func t(_ key: String) -> String { L10n.tr(store.uiLang, key) }

    var body: some View {
        NavigationStack {
            List {
                ForEach(store.tabs) { tab in
                    HStack {
                        Button {
                            store.activeTabId = tab.id
                            dismiss()
                        } label: {
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
                        .buttonStyle(.plain)

                        Spacer()

                        if store.tabs.count > 1 {
                            Button(role: .destructive) {
                                store.closeTab(tab.id)
                            } label: {
                                Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary)
                            }
                            .buttonStyle(.plain)
                        }
                    }
                }
            }
            .navigationTitle(t("tabs.title"))
            .toolbar {
                ToolbarItem(placement: .navigationBarTrailing) {
                    Button {
                        store.newTab()
                        dismiss()
                    } label: {
                        Label(t("tabs.newTab"), systemImage: "plus")
                    }
                }
            }
        }
    }
}

private struct SettingsSheet: View {
    @ObservedObject var store: BrowserStore
    @ObservedObject var catalog: ModelCatalog
    @Environment(\.dismiss) private var dismiss

    @State private var geminiKeyDraft = KeychainStore.load(account: "gemini_api_key") ?? ""
    @State private var openAiKeyDraft = KeychainStore.load(account: "openai_api_key") ?? ""

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
        }
    }
}
