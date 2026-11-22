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
        DispatchQueue.main.asyncAfter(deadline: .now() + 0.16) {
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
                        .liquidGlassPill(tint: FluoThemeTokens.indigo)
                    }
                    .padding(.horizontal, 14)
                    .padding(.vertical, 10)
                    .liquidGlass(cornerRadius: 18, tint: FluoThemeTokens.indigo)
                    .padding(.horizontal, 12)
                    .padding(.top, 6)
                }

                WebViewContainer(webView: store.activeTab.webView)
                    .id(store.activeTab.id)
                    .ignoresSafeArea(edges: .bottom)
            }

            if store.isTranslating && store.translationTotal > 0 {
                HStack(spacing: 10) {
                    ProgressView()
                        .tint(FluoThemeTokens.indigo)
                    Text("\(t("tr.progress")) \(store.translationDone)/\(store.translationTotal)")
                        .font(.caption.weight(.semibold))
                    Button {
                        store.cancelTranslation()
                    } label: {
                        Image(systemName: "xmark.circle.fill")
                            .foregroundStyle(.secondary)
                    }
                }
                .padding(.horizontal, 16)
                .padding(.vertical, 9)
                .liquidGlassPill(tint: FluoThemeTokens.indigo)
                .padding(.bottom, 84)
            }

            bottomDock
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
            .liquidGlassSheet()
        }
        .sheet(isPresented: $store.showSelectionResultSheet) {
            SelectionResultSheet(store: store)
                .presentationDetents([.medium])
                .liquidGlassSheet()
        }
    }

    private var findBar: some View {
        HStack(spacing: 8) {
            Image(systemName: "magnifyingglass")
                .foregroundStyle(.secondary)
                .font(.footnote.weight(.semibold))
            TextField(t("find.placeholder"), text: $store.findQuery)
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
            if store.findTotal > 0 {
                Text("\(store.findCurrent)/\(store.findTotal)")
                    .font(.caption.monospacedDigit().weight(.semibold))
                    .foregroundStyle(.secondary)
            }
            Button { store.runFindInPage(action: "prev") } label: { Image(systemName: "chevron.up") }
            Button { store.runFindInPage(action: "next") } label: { Image(systemName: "chevron.down") }
            Button { store.closeFindBar() } label: { Image(systemName: "xmark.circle.fill").foregroundStyle(.secondary) }
        }
        .padding(.horizontal, 14)
        .padding(.vertical, 9)
        .liquidGlassPill()
    }

    private var bottomDock: some View {
        HStack(spacing: 8) {
            Button {
                store.activeTab.webView.goBack()
            } label: {
                Image(systemName: "chevron.left")
                    .font(.system(size: 14, weight: .semibold))
                    .frame(width: 32, height: 32)
                    .background(.white.opacity(0.08), in: Circle())
            }
            .buttonStyle(LiquidPressButtonStyle())
            .disabled(!store.activeTab.canGoBack)
            .opacity(store.activeTab.canGoBack ? 1.0 : 0.4)

            Button {
                store.activeTab.webView.goForward()
            } label: {
                Image(systemName: "chevron.right")
                    .font(.system(size: 14, weight: .semibold))
                    .frame(width: 32, height: 32)
                    .background(.white.opacity(0.08), in: Circle())
            }
            .buttonStyle(LiquidPressButtonStyle())
            .disabled(!store.activeTab.canGoForward)
            .opacity(store.activeTab.canGoForward ? 1.0 : 0.4)

            HStack(spacing: 6) {
                Image(systemName: store.activeTab.isPrivate ? "eye.slash.fill" : (store.isCurrentHostAdBlockEnabled ? "shield.checkered" : "globe"))
                    .font(.caption2.weight(.semibold))
                    .foregroundStyle(store.activeTab.isPrivate ? FluoThemeTokens.violet : FluoThemeTokens.indigo)

                TextField(t("bar.urlPlaceholder"), text: $urlDraft)
                    .textInputAutocapitalization(.never)
                    .autocorrectionDisabled()
                    .keyboardType(.URL)
                    .submitLabel(.go)
                    .font(.subheadline)
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
                        .font(.caption.weight(.bold))
                        .foregroundStyle(.secondary)
                        .frame(width: 22, height: 22)
                }
            }
            .padding(.horizontal, 11)
            .padding(.vertical, 8)
            .background {
                Capsule().fill(.thinMaterial)
            }
            .overlay(
                Capsule().strokeBorder(.white.opacity(0.24), lineWidth: 0.7)
            )

            Button {
                activeSheet = .translate
            } label: {
                Image(systemName: store.activeTab.displayMode == .original ? "character.bubble" : "character.bubble.fill")
                    .font(.system(size: 15, weight: .semibold))
                    .foregroundStyle(store.activeTab.displayMode == .original ? Color.primary : Color.white)
                    .frame(width: 34, height: 34)
                    .background {
                        if store.activeTab.displayMode == .original {
                            Circle().fill(.white.opacity(0.10))
                        } else {
                            Circle().fill(FluoThemeTokens.liquidGradient)
                        }
                    }
                    .overlay(Circle().strokeBorder(.white.opacity(0.32), lineWidth: 0.8))
            }
            .buttonStyle(LiquidPressButtonStyle())

            Button {
                activeSheet = .tabs
            } label: {
                Text("\(store.activeWorkspaceTabs.count)")
                    .font(.caption.bold())
                    .frame(width: 30, height: 30)
                    .background(.white.opacity(0.10), in: RoundedRectangle(cornerRadius: 10, style: .continuous))
                    .overlay(RoundedRectangle(cornerRadius: 10, style: .continuous).strokeBorder(.white.opacity(0.34), lineWidth: 1))
            }
            .buttonStyle(LiquidPressButtonStyle())

            Button {
                activeSheet = .menu
            } label: {
                Image(systemName: "ellipsis")
                    .font(.system(size: 15, weight: .bold))
                    .frame(width: 32, height: 32)
                    .background(.white.opacity(0.10), in: Circle())
            }
            .buttonStyle(LiquidPressButtonStyle())
        }
        .padding(.horizontal, 12)
        .padding(.vertical, 10)
        .liquidGlass(cornerRadius: 30, tint: store.activeTab.isPrivate ? FluoThemeTokens.violet : .clear)
        .padding(.horizontal, 10)
        .padding(.bottom, 6)
    }
}
