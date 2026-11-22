import SwiftUI
import UIKit
import WebKit

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
    @State private var dockDragOffset: CGFloat = 0

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
                        .liquidGlassPill(tint: FluoThemeTokens.indigo, interactive: true)
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
                .padding(.bottom, store.isDockCollapsed ? 58 : 84)
            }

            Group {
                if store.isDockCollapsed {
                    compactDockPill
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                } else {
                    bottomDock
                        .transition(.move(edge: .bottom).combined(with: .opacity))
                }
            }
            .animation(.spring(response: 0.30, dampingFraction: 0.84), value: store.isDockCollapsed)
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
            store.isDockCollapsed = false
        }
        .onChange(of: store.showFindBar) { show in
            if show { store.isDockCollapsed = false }
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
                withAnimation(.spring(response: 0.28, dampingFraction: 0.78)) {
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

    private var compactDockPill: some View {
        let displayHost = URL(string: store.activeTab.urlString)?.host ?? store.activeTab.title
        return HStack(spacing: 8) {
            Image(systemName: store.activeTab.isPrivate ? "eye.slash.fill" : (store.isCurrentHostAdBlockEnabled ? "shield.checkered" : "globe"))
                .font(.caption.weight(.semibold))
                .foregroundStyle(store.activeTab.isPrivate ? FluoThemeTokens.violet : FluoThemeTokens.indigo)

            Text(displayHost.isEmpty ? "Fluo" : displayHost)
                .font(.caption.weight(.semibold))
                .lineLimit(1)

            if store.activeTab.displayMode != .original {
                Image(systemName: "character.bubble.fill")
                    .font(.caption2.weight(.bold))
                    .foregroundStyle(FluoThemeTokens.indigo)
            }
        }
        .padding(.horizontal, 16)
        .padding(.vertical, 9)
        .liquidGlassPill(
            tint: store.activeTab.isPrivate ? FluoThemeTokens.violet : .clear
        )
        .contentShape(Capsule())
        .onTapGesture(count: 2) {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            store.activeTab.scrollToTop()
            withAnimation(.spring(response: 0.30, dampingFraction: 0.84)) {
                store.isDockCollapsed = false
            }
        }
        .onTapGesture(count: 1) {
            UIImpactFeedbackGenerator(style: .light).impactOccurred()
            withAnimation(.spring(response: 0.30, dampingFraction: 0.84)) {
                store.isDockCollapsed = false
            }
        }
        .offset(x: dockDragOffset)
        .gesture(dockDragGesture)
        .padding(.bottom, 6)
    }

    private var bottomDock: some View {
        LiquidGlassGroup(spacing: 6) {
            HStack(spacing: 6) {
                Button {
                    store.activeTab.webView.goBack()
                } label: {
                    Image(systemName: "chevron.left")
                        .font(.system(size: 14, weight: .semibold))
                        .frame(width: 36, height: 36)
                        .contentShape(Circle())
                }
                .buttonStyle(LiquidPressButtonStyle())
                .liquidGlassCircle(interactive: true)
                .disabled(!store.activeTab.canGoBack)
                .opacity(store.activeTab.canGoBack ? 1.0 : 0.4)
                .contextMenu {
                    backForwardContextMenu(
                        items: Array(store.activeTab.webView.backForwardList.backList.reversed().prefix(8))
                    )
                }

                Button {
                    store.activeTab.webView.goForward()
                } label: {
                    Image(systemName: "chevron.right")
                        .font(.system(size: 14, weight: .semibold))
                        .frame(width: 36, height: 36)
                        .contentShape(Circle())
                }
                .buttonStyle(LiquidPressButtonStyle())
                .liquidGlassCircle(interactive: true)
                .disabled(!store.activeTab.canGoForward)
                .opacity(store.activeTab.canGoForward ? 1.0 : 0.4)
                .contextMenu {
                    backForwardContextMenu(
                        items: Array(store.activeTab.webView.backForwardList.forwardList.prefix(8))
                    )
                }

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
                            .frame(width: 28, height: 28)
                            .contentShape(Rectangle())
                    }
                    .buttonStyle(.plain)
                }
                .padding(.horizontal, 11)
                .padding(.vertical, 6)
                .liquidGlassPill(
                    tint: store.activeTab.isPrivate ? FluoThemeTokens.violet : .clear
                )
                .gesture(dockDragGesture)

                Button {
                    activeSheet = .translate
                } label: {
                    Image(systemName: store.activeTab.displayMode == .original ? "character.bubble" : "character.bubble.fill")
                        .font(.system(size: 15, weight: .semibold))
                        .foregroundStyle(store.activeTab.displayMode == .original ? Color.primary : Color.white)
                        .frame(width: 36, height: 36)
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

                Button {
                    activeSheet = .tabs
                } label: {
                    Text("\(store.activeWorkspaceTabs.count)")
                        .font(.caption.bold())
                        .frame(width: 34, height: 34)
                        .contentShape(RoundedRectangle(cornerRadius: 11, style: .continuous))
                }
                .buttonStyle(LiquidPressButtonStyle())
                .liquidGlass(cornerRadius: 11, interactive: true)

                Button {
                    activeSheet = .menu
                } label: {
                    Image(systemName: "ellipsis")
                        .font(.system(size: 15, weight: .bold))
                        .frame(width: 36, height: 36)
                        .contentShape(Circle())
                }
                .buttonStyle(LiquidPressButtonStyle())
                .liquidGlassCircle(interactive: true)
            }
        }
        .liquidGlassDockChrome(tint: store.activeTab.isPrivate ? FluoThemeTokens.violet : .clear)
        .offset(x: dockDragOffset)
        .padding(.horizontal, 12)
        .padding(.bottom, 6)
    }
}
