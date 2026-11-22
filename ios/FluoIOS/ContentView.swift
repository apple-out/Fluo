import SwiftUI

enum ActiveSheetKind: String, Identifiable {
    case translate
    case tabs
    case menu
    case bookmarks
    case history
    case translationHistory
    case glossary
    case feedback
    case settings

    var id: String { rawValue }
}

struct ContentView: View {
    @ObservedObject var store: BrowserStore
    @ObservedObject private var catalog = ModelCatalog.shared

    @State private var urlDraft = ""
    @State private var activeSheet: ActiveSheetKind?
    @State private var showOnboarding = false

    private func t(_ key: String) -> String {
        L10n.tr(store.uiLang, key)
    }

    private func switchSheet(_ target: ActiveSheetKind) {
        activeSheet = nil
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.15) {
            activeSheet = target
        }
    }

    private var resolvedColorScheme: ColorScheme? {
        switch store.userData.appTheme {
        case .system: return nil
        case .light: return .light
        case .dark: return .dark
        }
    }

    var body: some View {
        ZStack(alignment: .bottom) {
            VStack(spacing: 0) {
                if store.activeTab.isLoading {
                    ProgressView(value: store.activeTab.progress)
                        .tint(.indigo)
                        .progressViewStyle(.linear)
                }

                if store.showFindBar {
                    findBar
                }

                if store.isPickingRegion {
                    HStack {
                        Image(systemName: "hand.tap.fill").foregroundStyle(.indigo)
                        Text(t("tr.pickingBanner")).font(.caption.weight(.semibold))
                        Spacer()
                        Button(t("common.cancel")) {
                            store.cancelRegionPicker()
                        }
                        .font(.caption.bold())
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 8)
                    .background(.ultraThinMaterial)
                }

                WebViewContainer(webView: store.activeTab.webView)
                    .ignoresSafeArea(edges: .bottom)
            }

            if store.isTranslating && store.translationTotal > 0 {
                HStack(spacing: 10) {
                    ProgressView()
                    Text("\(t("tr.progress")) \(store.translationDone)/\(store.translationTotal)")
                        .font(.caption.weight(.semibold))
                    Button {
                        store.cancelTranslation()
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(.horizontal, 14)
                .padding(.vertical, 8)
                .background(.ultraThinMaterial, in: Capsule())
                .padding(.bottom, 74)
            }

            bottomBar
        }
        .preferredColorScheme(resolvedColorScheme)
        .onAppear {
            urlDraft = store.activeTab.urlString
            if !store.onboardingCompleted {
                showOnboarding = true
            }
        }
        .onChange(of: store.activeTab.urlString) { nextUrl in
            urlDraft = nextUrl
        }
        .onChange(of: store.activeTabId) { _ in
            urlDraft = store.activeTab.urlString
        }
        .sheet(isPresented: $showOnboarding) {
            OnboardingSheet(store: store)
                .interactiveDismissDisabled(true)
        }
        .sheet(item: $activeSheet) { kind in
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
                    switchSheet(target)
                })
                .presentationDetents([.medium, .large])
            case .bookmarks:
                BookmarksSheet(store: store)
            case .history:
                HistorySheet(store: store)
            case .translationHistory:
                TranslationHistorySheet(store: store)
            case .glossary:
                GlossarySheet(store: store)
            case .feedback:
                FeedbackSheet(store: store)
            case .settings:
                SettingsSheet(store: store, catalog: catalog)
            }
        }
        .sheet(isPresented: $store.showSelectionResultSheet) {
            SelectionResultSheet(store: store)
                .presentationDetents([.medium])
        }
    }

    private var findBar: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass").foregroundStyle(.secondary)
            TextField(t("find.placeholder"), text: $store.findQuery)
                .textInputAutocapitalization(.never)
                .autocorrectionDisabled()
                .submitLabel(.search)
                .onChange(of: store.findQuery) { _ in
                    store.runFindInPage(action: "find")
                }
                .onSubmit {
                    store.runFindInPage(action: "next")
                }
            if store.findTotal > 0 {
                Text("\(store.findCurrent)/\(store.findTotal)")
                    .font(.caption.monospacedDigit())
                    .foregroundStyle(.secondary)
            }
            Button { store.runFindInPage(action: "prev") } label: { Image(systemName: "chevron.up") }
            Button { store.runFindInPage(action: "next") } label: { Image(systemName: "chevron.down") }
            Button { store.closeFindBar() } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary) }
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 8)
        .background(.ultraThinMaterial)
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

            HStack(spacing: 4) {
                TextField(t("bar.urlPlaceholder"), text: $urlDraft)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.URL)
                    .submitLabel(.go)
                    .onSubmit {
                        store.activeTab.navigate(input: urlDraft, searchEngine: store.userData.searchEngine)
                    }

                Button {
                    if store.activeTab.isLoading {
                        store.activeTab.webView.stopLoading()
                    } else {
                        store.activeTab.webView.reload()
                    }
                } label: {
                    Image(systemName: store.activeTab.isLoading ? "xmark" : "arrow.clockwise")
                        .font(.caption.weight(.semibold))
                        .foregroundStyle(.secondary)
                }
            }
            .padding(.horizontal, 10)
            .padding(.vertical, 7)
            .background(.thinMaterial, in: Capsule())

            Button {
                activeSheet = .translate
            } label: {
                Image(systemName: store.activeTab.displayMode == .original ? "character.bubble" : "character.bubble.fill")
                    .foregroundStyle(store.activeTab.displayMode == .original ? .primary : Color.indigo)
            }

            Button {
                activeSheet = .tabs
            } label: {
                Text("\(store.activeWorkspaceTabs.count)")
                    .font(.caption.bold())
                    .frame(width: 24, height: 24)
                    .overlay(RoundedRectangle(cornerRadius: 6).stroke(.primary, lineWidth: 1.5))
            }

            Button {
                activeSheet = .menu
            } label: {
                Image(systemName: "ellipsis.circle")
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
