import Combine
import Foundation
import UIKit
import WebKit

enum InlineDisplayMode: String {
    case original
    case inline
    case bilingual
}

final class BrowserTab: NSObject, Identifiable, ObservableObject, WKNavigationDelegate, WKUIDelegate, UIGestureRecognizerDelegate {
    let id = UUID()
    let webView: WKWebView

    @Published var workspaceId: String
    @Published var isPrivate: Bool
    @Published var isPinned: Bool = false
    @Published var isDesktopMode: Bool = false
    @Published var pageZoom: Double = 1.0
    @Published var urlString: String
    @Published var title: String
    @Published var progress: Double = 0
    @Published var isLoading: Bool = false
    @Published var canGoBack: Bool = false
    @Published var canGoForward: Bool = false
    @Published var displayMode: InlineDisplayMode = .original

    var isStartPage: Bool {
        urlString == FluoUserDataSnapshot.startPageURL
    }

    var onVisitRecorded: ((String, String) -> Void)?
    var onNavigationSyncAdBlock: ((WKWebView, String?) -> Void)?
    var onScrollDockCollapseChanged: ((Bool) -> Void)?
    var onTwoFingerTap: (() -> Void)?
    var onOpenNewTab: ((URL) -> Void)?

    private var obsProgress: NSKeyValueObservation?
    private var obsLoading: NSKeyValueObservation?
    private var obsTitle: NSKeyValueObservation?
    private var obsUrl: NSKeyValueObservation?
    private let refreshControl = UIRefreshControl()
    private var panAnchorY: CGFloat = 0

    init(initialURL: URL, workspaceId: String, isPrivate: Bool = false) {
        let config = WKWebViewConfiguration()
        config.allowsInlineMediaPlayback = true
        if isPrivate {
            config.websiteDataStore = .nonPersistent()
        }
        self.webView = WKWebView(frame: .zero, configuration: config)
        self.workspaceId = workspaceId
        self.isPrivate = isPrivate
        self.urlString = initialURL.absoluteString
        self.title = initialURL.absoluteString == FluoUserDataSnapshot.startPageURL ? "Fluo" : (initialURL.host ?? "Fluo")
        super.init()

        webView.navigationDelegate = self
        webView.uiDelegate = self
        webView.allowsBackForwardNavigationGestures = true
        refreshControl.addTarget(self, action: #selector(handlePullToRefresh(_:)), for: .valueChanged)
        webView.scrollView.refreshControl = refreshControl
        webView.scrollView.panGestureRecognizer.addTarget(self, action: #selector(handleScrollPan(_:)))

        let twoFingerTap = UITapGestureRecognizer(target: self, action: #selector(handleTwoFingerTap(_:)))
        twoFingerTap.numberOfTouchesRequired = 2
        twoFingerTap.cancelsTouchesInView = false
        twoFingerTap.delegate = self
        webView.addGestureRecognizer(twoFingerTap)

        obsProgress = webView.observe(\.estimatedProgress, options: [.new]) { [weak self] wv, _ in
            Task { @MainActor in self?.progress = wv.estimatedProgress }
        }
        obsLoading = webView.observe(\.isLoading, options: [.new]) { [weak self] wv, _ in
            Task { @MainActor in
                guard let self else { return }
                self.isLoading = wv.isLoading
                if !self.isStartPage {
                    self.canGoBack = wv.canGoBack
                    self.canGoForward = wv.canGoForward
                }
                if !wv.isLoading, self.refreshControl.isRefreshing {
                    self.refreshControl.endRefreshing()
                }
            }
        }
        obsTitle = webView.observe(\.title, options: [.new]) { [weak self] wv, _ in
            Task { @MainActor in
                guard let self, !self.isStartPage else { return }
                if let t = wv.title, !t.isEmpty { self.title = t }
            }
        }
        obsUrl = webView.observe(\.url, options: [.new]) { [weak self] wv, _ in
            Task { @MainActor in
                guard let self, !self.isStartPage else { return }
                if let u = wv.url?.absoluteString, !u.isEmpty, u != "about:blank" {
                    self.urlString = u
                }
                self.canGoBack = wv.canGoBack
                self.canGoForward = wv.canGoForward
            }
        }
        if initialURL.absoluteString != FluoUserDataSnapshot.startPageURL {
            webView.load(URLRequest(url: initialURL))
        }
    }

    func navigate(input: String, searchEngine: SearchEngine) {
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        if trimmed == FluoUserDataSnapshot.startPageURL || trimmed == "tb://start" {
            openStartPage()
            return
        }
        let url: URL?
        if trimmed.hasPrefix("http://") || trimmed.hasPrefix("https://") {
            url = URL(string: trimmed)
        } else if trimmed.contains(".") && !trimmed.contains(" ") {
            url = URL(string: "https://\(trimmed)")
        } else {
            url = searchEngine.searchURL(query: trimmed)
        }
        if let target = url {
            displayMode = .original
            urlString = target.absoluteString
            canGoBack = webView.canGoBack || webView.url != nil
            webView.load(URLRequest(url: target))
        }
    }

    func openStartPage() {
        webView.stopLoading()
        displayMode = .original
        urlString = FluoUserDataSnapshot.startPageURL
        title = "Fluo"
        progress = 0
        isLoading = false
        canGoBack = false
        canGoForward = webView.url != nil
    }

    func goForwardFromCurrent() {
        if isStartPage, let wvUrl = webView.url {
            urlString = wvUrl.absoluteString
            title = webView.title ?? (wvUrl.host ?? "Fluo")
            canGoBack = true
            canGoForward = webView.canGoForward
        } else {
            webView.goForward()
        }
    }

    nonisolated func webView(
        _ webView: WKWebView,
        createWebViewWith configuration: WKWebViewConfiguration,
        for navigationAction: WKNavigationAction,
        windowFeatures: WKWindowFeatures
    ) -> WKWebView? {
        if let url = navigationAction.request.url {
            let scheme = url.scheme?.lowercased() ?? ""
            if scheme == "http" || scheme == "https" {
                if navigationAction.targetFrame == nil {
                    Task { @MainActor in
                        self.onOpenNewTab?(url)
                    }
                } else {
                    webView.load(navigationAction.request)
                }
            }
        }
        return nil
    }

    func applyDesktopMode(_ desktop: Bool) {
        isDesktopMode = desktop
        webView.configuration.defaultWebpagePreferences.preferredContentMode = desktop ? .desktop : .mobile
        webView.customUserAgent = desktop
            ? "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/605.1.15 (KHTML, like Gecko) Version/17.0 Safari/605.1.15"
            : nil
        webView.reload()
    }

    func applyPageZoom(_ zoom: Double) {
        let clamped = min(2.0, max(0.5, zoom))
        pageZoom = clamped
        let pct = Int((clamped * 100).rounded())
        _ = webView.evaluateJavaScript("document.documentElement.style.zoom = '\(pct)%';", completionHandler: nil)
    }

    func scrollToTop() {
        let top = -webView.scrollView.adjustedContentInset.top
        webView.scrollView.setContentOffset(CGPoint(x: 0, y: top), animated: true)
    }

    @objc private func handlePullToRefresh(_ sender: UIRefreshControl) {
        UIImpactFeedbackGenerator(style: .light).impactOccurred()
        webView.reload()
    }

    @objc private func handleScrollPan(_ pan: UIPanGestureRecognizer) {
        let ty = pan.translation(in: webView).y
        switch pan.state {
        case .began:
            panAnchorY = ty
        case .changed:
            let dy = ty - panAnchorY
            let offsetY = webView.scrollView.contentOffset.y
            if dy < -28 && offsetY > 44 {
                panAnchorY = ty
                onScrollDockCollapseChanged?(true)
            } else if (dy > 18 && offsetY > 10) || offsetY <= 10 {
                panAnchorY = ty
                onScrollDockCollapseChanged?(false)
            }
        default:
            break
        }
    }

    @objc private func handleTwoFingerTap(_ tap: UITapGestureRecognizer) {
        guard tap.state == .ended else { return }
        onTwoFingerTap?()
    }

    func gestureRecognizer(
        _ gestureRecognizer: UIGestureRecognizer,
        shouldRecognizeSimultaneouslyWith otherGestureRecognizer: UIGestureRecognizer
    ) -> Bool {
        true
    }

    nonisolated func webView(_ webView: WKWebView, decidePolicyFor navigationAction: WKNavigationAction) async -> WKNavigationActionPolicy {
        let host = navigationAction.request.url?.host
        await MainActor.run {
            self.onNavigationSyncAdBlock?(webView, host)
        }
        return .allow
    }

    nonisolated func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        Task { @MainActor in
            let url = webView.url?.absoluteString ?? ""
            let pageTitle = webView.title ?? url
            if !self.isPrivate && !url.isEmpty {
                self.onVisitRecorded?(url, pageTitle)
            }
            if abs(self.pageZoom - 1.0) > 0.01 {
                self.applyPageZoom(self.pageZoom)
            }
        }
    }
}

@MainActor
final class BrowserStore: ObservableObject {
    @Published var tabs: [BrowserTab] = []
    @Published var activeTabId: UUID
    @Published var userData: FluoUserDataSnapshot

    @Published var uiLang: AppLanguage
    @Published var targetLanguage: String
    @Published var onboardingCompleted: Bool
    @Published var provider: AiProviderKind
    @Published var selectedLocalModel: String
    @Published var geminiModel: String
    @Published var openAiBaseUrl: String
    @Published var openAiModel: String

    @Published var isTranslating = false
    @Published var translationDone = 0
    @Published var translationTotal = 0
    @Published var translationStatusText = ""
    @Published var translationError: String?

    @Published var closedTabs: [(url: String, title: String)] = []

    @Published var isPickingRegion = false
    @Published var isDockCollapsed = false
    @Published var showFindBar = false
    @Published var findQuery = ""
    @Published var findCurrent = 0
    @Published var findTotal = 0

    @Published var selectionSourceText = ""
    @Published var selectionTranslatedText = ""
    @Published var showSelectionResultSheet = false

    private var translationTask: Task<Void, Never>?
    private var regionPollTask: Task<Void, Never>?
    private var tabCancellables: [UUID: AnyCancellable] = [:]

    init() {
        let loaded = FluoUserDataSnapshot.load()
        self.userData = loaded

        let savedUi = UserDefaults.standard.string(forKey: "fluo.uiLang").flatMap(AppLanguage.init(rawValue:)) ?? AppLanguage.detectSystem()
        self.uiLang = savedUi
        self.targetLanguage = UserDefaults.standard.string(forKey: "fluo.targetLang") ?? savedUi.defaultTargetLanguage
        self.onboardingCompleted = UserDefaults.standard.bool(forKey: "fluo.onboardingDone")
        let savedProv = UserDefaults.standard.string(forKey: "fluo.provider").flatMap(AiProviderKind.init(rawValue:)) ?? .hyMt2
        self.provider = savedProv
        self.selectedLocalModel = UserDefaults.standard.string(forKey: "fluo.selectedModel") ?? ModelCatalog.specs[0].id
        self.geminiModel = UserDefaults.standard.string(forKey: "fluo.geminiModel") ?? "gemini-2.5-flash"
        self.openAiBaseUrl = UserDefaults.standard.string(forKey: "fluo.openAiBaseUrl") ?? "https://api.openai.com/v1"
        self.openAiModel = UserDefaults.standard.string(forKey: "fluo.openAiModel") ?? "gpt-4.1-mini"

        let initialUrl = URL(string: FluoUserDataSnapshot.startPageURL)!
        let first = BrowserTab(initialURL: initialUrl, workspaceId: loaded.activeWorkspaceId, isPrivate: false)
        self.tabs = [first]
        self.activeTabId = first.id
        wireTabCallbacks(first)
    }

    private func wireTabCallbacks(_ tab: BrowserTab) {
        tabCancellables[tab.id] = tab.objectWillChange.sink { [weak self] _ in
            self?.objectWillChange.send()
        }
        tab.onOpenNewTab = { [weak self, weak tab] url in
            guard let self, let tab else { return }
            self.newTab(urlString: url.absoluteString, isPrivate: tab.isPrivate)
        }
        tab.onVisitRecorded = { [weak self] url, title in
            self?.recordHistory(url: url, title: title)
        }
        tab.onNavigationSyncAdBlock = { [weak self] webView, host in
            guard let self else { return }
            AdBlockManager.shared.apply(
                to: webView,
                enabled: self.userData.adBlockEnabled,
                host: host,
                allowlist: self.userData.adBlockAllowlist
            )
        }
        tab.onScrollDockCollapseChanged = { [weak self, weak tab] collapsed in
            Task { @MainActor in
                guard let self, let tab, self.activeTabId == tab.id else { return }
                guard !self.showFindBar && !self.isPickingRegion else { return }
                if self.isDockCollapsed != collapsed {
                    self.isDockCollapsed = collapsed
                }
            }
        }
        tab.onTwoFingerTap = { [weak self, weak tab] in
            Task { @MainActor in
                guard let self, let tab, self.activeTabId == tab.id, !self.isPickingRegion else { return }
                UIImpactFeedbackGenerator(style: .medium).impactOccurred()
                self.quickToggleTranslation()
            }
        }
        AdBlockManager.shared.apply(
            to: tab.webView,
            enabled: userData.adBlockEnabled,
            host: URL(string: tab.urlString)?.host,
            allowlist: userData.adBlockAllowlist
        )
    }

    var activeTab: BrowserTab {
        tabs.first(where: { $0.id == activeTabId }) ?? tabs[0]
    }

    var activeWorkspaceTabs: [BrowserTab] {
        let filtered = tabs.filter { $0.workspaceId == userData.activeWorkspaceId }
        return filtered.isEmpty ? tabs : filtered
    }

    var currentHost: String {
        URL(string: activeTab.urlString)?.host?.lowercased() ?? ""
    }

    var isCurrentHostAdBlockEnabled: Bool {
        guard userData.adBlockEnabled else { return false }
        let host = currentHost
        if host.isEmpty { return userData.adBlockEnabled }
        return !userData.adBlockAllowlist.contains(where: { $0.lowercased() == host })
    }

    var isCurrentPageBookmarked: Bool {
        let url = activeTab.urlString
        return userData.bookmarks.contains { $0.url == url }
    }

    func savePreferences() {
        UserDefaults.standard.set(uiLang.rawValue, forKey: "fluo.uiLang")
        UserDefaults.standard.set(targetLanguage, forKey: "fluo.targetLang")
        UserDefaults.standard.set(onboardingCompleted, forKey: "fluo.onboardingDone")
        UserDefaults.standard.set(provider.rawValue, forKey: "fluo.provider")
        UserDefaults.standard.set(selectedLocalModel, forKey: "fluo.selectedModel")
        UserDefaults.standard.set(geminiModel, forKey: "fluo.geminiModel")
        UserDefaults.standard.set(openAiBaseUrl, forKey: "fluo.openAiBaseUrl")
        UserDefaults.standard.set(openAiModel, forKey: "fluo.openAiModel")
        userData.save()
    }

    // MARK: - Tabs & Workspaces

    func newTab(urlString: String? = nil, isPrivate: Bool = false) {
        let targetStr = urlString ?? FluoUserDataSnapshot.startPageURL
        let url = URL(string: targetStr) ?? URL(string: FluoUserDataSnapshot.startPageURL)!
        let tab = BrowserTab(initialURL: url, workspaceId: userData.activeWorkspaceId, isPrivate: isPrivate)
        wireTabCallbacks(tab)
        tabs.append(tab)
        activeTabId = tab.id
    }

    func closeTab(_ id: UUID) {
        guard let idx = tabs.firstIndex(where: { $0.id == id }) else { return }
        let removed = tabs[idx]
        if removed.isPinned { return }
        if !removed.isPrivate && !removed.isStartPage {
            closedTabs.append((url: removed.urlString, title: removed.title))
            if closedTabs.count > 20 { closedTabs.removeFirst() }
        }
        if tabs.count == 1 {
            removed.openStartPage()
            return
        }
        tabCancellables.removeValue(forKey: id)
        tabs.remove(at: idx)
        if !tabs.contains(where: { $0.id == activeTabId }) {
            activeTabId = activeWorkspaceTabs.first?.id ?? tabs[0].id
        }
    }

    func reopenClosedTab() {
        guard let last = closedTabs.popLast() else { return }
        newTab(urlString: last.url, isPrivate: false)
    }

    func closeAllTabsInCurrentWorkspace() {
        let wsId = userData.activeWorkspaceId
        for tab in tabs where tab.workspaceId == wsId && !tab.isPinned {
            if !tab.isPrivate && !tab.isStartPage {
                closedTabs.append((url: tab.urlString, title: tab.title))
            }
            tabCancellables.removeValue(forKey: tab.id)
        }
        tabs.removeAll { $0.workspaceId == wsId && !$0.isPinned }
        if activeWorkspaceTabs.isEmpty {
            newTab()
        } else if !tabs.contains(where: { $0.id == activeTabId }) {
            activeTabId = activeWorkspaceTabs[0].id
        }
    }

    func togglePinTab(_ id: UUID) {
        if let tab = tabs.first(where: { $0.id == id }) {
            tab.isPinned.toggle()
            objectWillChange.send()
        }
    }

    func createWorkspace(name: String) {
        let clean = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty else { return }
        let ws = WorkspaceItem(id: UUID().uuidString, name: clean)
        userData.workspaces.append(ws)
        userData.activeWorkspaceId = ws.id
        newTab()
        userData.save()
    }

    func renameWorkspace(_ id: String, name: String) {
        let clean = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty,
              let idx = userData.workspaces.firstIndex(where: { $0.id == id }) else { return }
        userData.workspaces[idx].name = clean
        objectWillChange.send()
        userData.save()
    }

    func switchWorkspace(_ id: String) {
        guard userData.workspaces.contains(where: { $0.id == id }) else { return }
        userData.activeWorkspaceId = id
        if let existing = tabs.first(where: { $0.workspaceId == id }) {
            activeTabId = existing.id
        } else {
            newTab()
        }
        userData.save()
    }

    func deleteWorkspace(_ id: String) {
        guard userData.workspaces.count > 1, id != "default" else { return }
        for tab in tabs where tab.workspaceId == id {
            tab.workspaceId = "default"
        }
        userData.workspaces.removeAll { $0.id == id }
        if userData.activeWorkspaceId == id {
            userData.activeWorkspaceId = userData.workspaces[0].id
        }
        userData.save()
    }

    func moveTab(_ tabId: UUID, toWorkspace wsId: String) {
        if let tab = tabs.first(where: { $0.id == tabId }) {
            tab.workspaceId = wsId
            objectWillChange.send()
        }
    }

    @discardableResult
    func switchAdjacentTab(delta: Int, createIfPastEnd: Bool = true) -> Bool {
        let list = activeWorkspaceTabs
        guard let currentIndex = list.firstIndex(where: { $0.id == activeTabId }) else { return false }
        let nextIndex = currentIndex + delta
        if nextIndex >= list.count {
            if createIfPastEnd {
                newTab(isPrivate: activeTab.isPrivate)
                isDockCollapsed = false
                return true
            }
            return false
        }
        guard nextIndex >= 0 else { return false }
        activeTabId = list[nextIndex].id
        isDockCollapsed = false
        return true
    }

    func quickToggleTranslation() {
        if isTranslating {
            cancelTranslation()
        } else if activeTab.displayMode == .original {
            translateActiveTab(bilingual: false)
        } else {
            restoreOriginal()
        }
    }

    // MARK: - Bookmarks, History, Glossary

    func currentBookmark() -> BookmarkItem? {
        let url = activeTab.urlString
        return userData.bookmarks.first { $0.url == url }
    }

    func upsertBookmark(id: String?, title: String, url: String, folder: String) {
        let cleanUrl = url.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanUrl.isEmpty else { return }
        let cleanTitle = title.trimmingCharacters(in: .whitespacesAndNewlines)
        let cleanFolder = folder.trimmingCharacters(in: .whitespacesAndNewlines)
        let finalFolder = cleanFolder.isEmpty ? "Default" : cleanFolder
        let finalTitle = cleanTitle.isEmpty ? cleanUrl : cleanTitle

        if let id, let idx = userData.bookmarks.firstIndex(where: { $0.id == id }) {
            userData.bookmarks[idx].title = finalTitle
            userData.bookmarks[idx].url = cleanUrl
            userData.bookmarks[idx].folder = finalFolder
        } else if let idx = userData.bookmarks.firstIndex(where: { $0.url == cleanUrl }) {
            userData.bookmarks[idx].title = finalTitle
            userData.bookmarks[idx].folder = finalFolder
        } else {
            userData.bookmarks.insert(
                BookmarkItem(
                    id: UUID().uuidString,
                    url: cleanUrl,
                    title: finalTitle,
                    folder: finalFolder,
                    createdAt: Date()
                ),
                at: 0
            )
        }
        userData.save()
    }

    func bookmarkAllTabs() {
        var existing = Set(userData.bookmarks.map(\.url))
        for tab in activeWorkspaceTabs where !tab.isStartPage && !tab.urlString.isEmpty {
            if existing.contains(tab.urlString) { continue }
            existing.insert(tab.urlString)
            userData.bookmarks.insert(
                BookmarkItem(
                    id: UUID().uuidString,
                    url: tab.urlString,
                    title: tab.title.isEmpty ? tab.urlString : tab.title,
                    folder: "Default",
                    createdAt: Date()
                ),
                at: 0
            )
        }
        userData.save()
    }

    func toggleCurrentBookmark() {
        let url = activeTab.urlString
        if isCurrentPageBookmarked {
            userData.bookmarks.removeAll { $0.url == url }
            userData.save()
        } else {
            upsertBookmark(id: nil, title: activeTab.title, url: url, folder: "Default")
        }
    }

    func deleteBookmark(_ id: String) {
        userData.bookmarks.removeAll { $0.id == id }
        userData.save()
    }

    func recordHistory(url: String, title: String) {
        if url == FluoUserDataSnapshot.startPageURL { return }
        if userData.history.first?.url == url { return }
        let entry = HistoryEntry(id: UUID().uuidString, url: url, title: title, visitedAt: Date())
        userData.history.insert(entry, at: 0)
        if userData.history.count > 300 {
            userData.history.removeLast(userData.history.count - 300)
        }
        userData.save()
    }

    func clearHistory() {
        userData.history.removeAll()
        userData.save()
    }

    func addGlossaryItem(source: String, target: String, caseSensitive: Bool, folder: String = "Default") {
        let cleanSrc = source.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanSrc.isEmpty else { return }
        let cleanFolder = folder.trimmingCharacters(in: .whitespacesAndNewlines)
        let finalFolder = cleanFolder.isEmpty ? "Default" : cleanFolder
        if !userData.glossaryFolders.contains(finalFolder) {
            userData.glossaryFolders.append(finalFolder)
        }
        userData.glossary.removeAll {
            $0.folder == finalFolder && $0.source.caseInsensitiveCompare(cleanSrc) == .orderedSame
        }
        userData.glossary.insert(
            GlossaryItem(
                id: UUID().uuidString,
                source: cleanSrc,
                target: target.trimmingCharacters(in: .whitespacesAndNewlines),
                caseSensitive: caseSensitive,
                folder: finalFolder,
                enabled: true
            ),
            at: 0
        )
        userData.save()
    }

    func toggleGlossaryEnabled(_ id: String) {
        if let idx = userData.glossary.firstIndex(where: { $0.id == id }) {
            userData.glossary[idx].enabled.toggle()
            userData.save()
        }
    }

    func deleteGlossaryItem(_ id: String) {
        userData.glossary.removeAll { $0.id == id }
        userData.save()
    }

    func addGlossaryFolder(_ name: String) {
        let clean = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty, !userData.glossaryFolders.contains(clean) else { return }
        userData.glossaryFolders.append(clean)
        userData.save()
    }

    func renameGlossaryFolder(old: String, new: String) {
        let cleanNew = new.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanNew.isEmpty, old != "Default", old != cleanNew else { return }
        if let idx = userData.glossaryFolders.firstIndex(of: old) {
            userData.glossaryFolders[idx] = cleanNew
        } else if !userData.glossaryFolders.contains(cleanNew) {
            userData.glossaryFolders.append(cleanNew)
        }
        for i in userData.glossary.indices where userData.glossary[i].folder == old {
            userData.glossary[i].folder = cleanNew
        }
        userData.save()
    }

    func deleteGlossaryFolder(_ name: String) {
        guard name != "Default" else { return }
        userData.glossaryFolders.removeAll { $0 == name }
        for i in userData.glossary.indices where userData.glossary[i].folder == name {
            userData.glossary[i].folder = "Default"
        }
        userData.save()
    }

    // MARK: - AdBlock & Clear Data

    func toggleCurrentHostAdBlock() {
        let host = currentHost
        guard !host.isEmpty else { return }
        if let idx = userData.adBlockAllowlist.firstIndex(where: { $0.lowercased() == host }) {
            userData.adBlockAllowlist.remove(at: idx)
        } else {
            userData.adBlockAllowlist.append(host)
        }
        userData.save()
        AdBlockManager.shared.apply(
            to: activeTab.webView,
            enabled: userData.adBlockEnabled,
            host: host,
            allowlist: userData.adBlockAllowlist
        )
        activeTab.webView.reload()
    }

    func clearSiteData() async {
        let store = WKWebsiteDataStore.default()
        let types = WKWebsiteDataStore.allWebsiteDataTypes()
        await store.removeData(ofTypes: types, modifiedSince: .distantPast)
    }

    // MARK: - Script Helpers (Find in Page & Inline Translation)

    private func loadResourceScript(_ name: String) -> String? {
        let url = Bundle.main.url(forResource: name, withExtension: "js")
            ?? Bundle.main.url(forResource: name, withExtension: "js", subdirectory: "Scripts")
            ?? Bundle.main.url(forResource: name, withExtension: "js", subdirectory: "Resources")
        guard let url else { return nil }
        return try? String(contentsOf: url, encoding: .utf8)
    }

    func runFindInPage(action: String) {
        if activeTab.isStartPage || findQuery.isEmpty || action == "clear" {
            findTotal = 0
            findCurrent = 0
        }
        guard !activeTab.isStartPage, let js = loadResourceScript("find_in_page") else { return }
        let qJson = (try? JSONEncoder().encode(findQuery)).flatMap { String(data: $0, encoding: .utf8) } ?? "\"\""
        let aJson = (try? JSONEncoder().encode(action)).flatMap { String(data: $0, encoding: .utf8) } ?? "\"find\""
        let call = "(\(js))(\(qJson), \(aJson));"
        activeTab.webView.evaluateJavaScript(call) { [weak self] result, _ in
            guard let self, let str = result as? String,
                  let data = str.data(using: .utf8),
                  let obj = try? JSONSerialization.jsonObject(with: data) as? [String: Int] else { return }
            Task { @MainActor in
                self.findTotal = obj["total"] ?? 0
                self.findCurrent = obj["current"] ?? 0
            }
        }
    }

    func closeFindBar() {
        findQuery = ""
        runFindInPage(action: "clear")
        showFindBar = false
    }

    // MARK: - Region Picker

    func startRegionPicker(bilingual: Bool) {
        guard let js = loadResourceScript("region_picker") else { return }
        isPickingRegion = true
        let setLangJs = "window.__tbUiLang = '\(uiLang.rawValue)';"
        activeTab.webView.evaluateJavaScript(setLangJs + js, completionHandler: nil)

        regionPollTask?.cancel()
        regionPollTask = Task { @MainActor in
            while !Task.isCancelled && isPickingRegion {
                try? await Task.sleep(nanoseconds: 250_000_000)
                let res = try? await activeTab.webView.evaluateJavaScript("window.__tbRegionPick && window.__tbRegionPick.status")
                if let status = res as? String {
                    if status == "picked" {
                        isPickingRegion = false
                        translateActiveTab(bilingual: bilingual)
                        return
                    } else if status == "cancelled" {
                        isPickingRegion = false
                        return
                    }
                }
            }
        }
    }

    func cancelRegionPicker() {
        regionPollTask?.cancel()
        isPickingRegion = false
        _ = activeTab.webView.evaluateJavaScript("window.__tbRegionPickerCleanup && window.__tbRegionPickerCleanup();", completionHandler: nil)
    }

    // MARK: - Selection Quick Translate

    func translateSelectedText() {
        translationError = nil
        activeTab.webView.evaluateJavaScript("window.getSelection ? window.getSelection().toString() : ''") { [weak self] res, _ in
            guard let self else { return }
            let picked = (res as? String)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            Task { @MainActor in
                if picked.isEmpty {
                    self.translationError = L10n.tr(self.uiLang, "tr.noSelection")
                    return
                }
                self.selectionSourceText = picked
                self.selectionTranslatedText = ""
                self.showSelectionResultSheet = true
                do {
                    let tr = try await TranslationEngine.shared.translateText(
                        picked,
                        provider: self.provider,
                        modelId: self.activeModelId,
                        baseUrl: self.openAiBaseUrl,
                        targetLanguage: self.targetLanguage,
                        style: self.userData.style.rawValue,
                        glossary: self.userData.glossary.filter(\.enabled)
                    )
                    self.selectionTranslatedText = tr
                } catch {
                    self.selectionTranslatedText = error.localizedDescription
                }
            }
        }
    }

    private var activeModelId: String {
        switch provider {
        case .hyMt2: return selectedLocalModel
        case .googleGemini: return geminiModel
        case .openAiCompatible: return openAiModel
        }
    }

    // MARK: - Inline Page Translation & Cancel

    func cancelTranslation() {
        translationTask?.cancel()
        isTranslating = false
    }

    func restoreOriginal() {
        cancelTranslation()
        let tab = activeTab
        guard let js = loadResourceScript("inline_restore") else { return }
        tab.webView.evaluateJavaScript(js) { _, _ in
            Task { @MainActor in
                tab.displayMode = .original
            }
        }
    }

    func translateActiveTab(bilingual: Bool) {
        translationTask?.cancel()
        translationError = nil
        let tab = activeTab
        if tab.isStartPage { return }
        guard let extractJs = loadResourceScript("content_extract"),
              let applyJs = loadResourceScript("inline_translate") else {
            translationError = "Bundled translation scripts not found."
            return
        }

        let currentProvider = provider
        let currentModel = activeModelId
        let currentBase = openAiBaseUrl
        let currentTarget = targetLanguage
        let currentStyle = userData.style.rawValue
        let currentGlossary = userData.glossary.filter(\.enabled)
        let currentFontScale = userData.fontScale
        let currentLineHeight = userData.lineHeight
        let currentUi = uiLang.rawValue

        translationTask = Task { @MainActor in
            isTranslating = true
            translationDone = 0
            translationTotal = 0
            translationStatusText = L10n.tr(uiLang, "tr.preparing")
            defer {
                isTranslating = false
                translationStatusText = ""
            }

            do {
                let rawResult = try await tab.webView.evaluateJavaScript(extractJs)
                guard let dict = rawResult as? [String: Any],
                      let blocks = dict["blocks"] as? [[String: Any]] else {
                    if let d = rawResult as? [String: Any], let jsErr = d["error"] as? String {
                        translationError = jsErr
                    } else {
                        translationError = L10n.tr(uiLang, "tr.noBlocks")
                    }
                    return
                }
                let segments: [(key: String, text: String)] = blocks.compactMap { b in
                    guard let k = (b["inlineKey"] as? String) ?? (b["key"] as? String),
                          let t = b["text"] as? String, !t.isEmpty else { return nil }
                    return (k, t)
                }
                translationTotal = segments.count
                translationDone = 0
                guard !segments.isEmpty else {
                    translationError = L10n.tr(uiLang, "tr.noBlocks")
                    return
                }

                let uiJson = (try? JSONEncoder().encode(currentUi)).flatMap { String(data: $0, encoding: .utf8) } ?? "\"ko\""

                for seg in segments {
                    try Task.checkCancellation()
                    let tr = try await TranslationEngine.shared.translateText(
                        seg.text,
                        provider: currentProvider,
                        modelId: currentModel,
                        baseUrl: currentBase,
                        targetLanguage: currentTarget,
                        style: currentStyle,
                        glossary: currentGlossary
                    )
                    translationDone += 1

                    let payload: [[String: Any]] = [[
                        "key": seg.key,
                        "translation": tr,
                        "uncertain": false,
                        "notes": [String](),
                    ]]
                    let jsonData = try JSONSerialization.data(withJSONObject: payload)
                    let jsonStr = String(data: jsonData, encoding: .utf8) ?? "[]"
                    let invokeJs = applyJs
                        .replacingOccurrences(of: "__PAYLOAD__", with: jsonStr)
                        .replacingOccurrences(of: "__FONT_SCALE__", with: String(format: "%.2f", currentFontScale))
                        .replacingOccurrences(of: "__LINE_HEIGHT__", with: String(format: "%.2f", currentLineHeight))
                        .replacingOccurrences(of: "__BILINGUAL__", with: bilingual ? "true" : "false")
                        .replacingOccurrences(of: "__UI_LANG__", with: uiJson)
                    _ = try? await tab.webView.evaluateJavaScript(invokeJs)
                }
                tab.displayMode = bilingual ? .bilingual : .inline
                if !tab.isPrivate {
                    userData.translationHistory.insert(
                        TranslationHistoryEntry(
                            id: UUID().uuidString,
                            url: tab.urlString,
                            title: tab.title,
                            targetLanguage: currentTarget,
                            provider: currentProvider.rawValue,
                            translatedAt: Date()
                        ),
                        at: 0
                    )
                    if userData.translationHistory.count > 100 {
                        userData.translationHistory.removeLast(userData.translationHistory.count - 100)
                    }
                    userData.save()
                }
            } catch is CancellationError {
                // Cancelled cleanly
            } catch {
                translationError = error.localizedDescription
            }
        }
    }
}
