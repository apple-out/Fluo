import AVFoundation
import Combine
import Foundation
import UIKit
import WebKit

enum InlineDisplayMode: String {
    case original
    case inline
    case bilingual
}

struct OmniboxSuggestion: Identifiable, Hashable {
    enum Kind: String, Hashable {
        case calculation
        case switchTab
        case bookmark
        case history
        case search
    }
    var id: String
    var kind: Kind
    var title: String
    var subtitle: String
    var targetURLOrQuery: String
    var tabId: UUID?
}

private final class FluoScriptMessageProxy: NSObject, WKScriptMessageHandler {
    weak var tab: BrowserTab?
    init(tab: BrowserTab) {
        self.tab = tab
    }
    func userContentController(_ userContentController: WKUserContentController, didReceive message: WKScriptMessage) {
        guard let body = message.body as? [String: Any] else { return }
        Task { @MainActor in
            self.tab?.onScriptAction?(body)
        }
    }
}

final class BrowserTab: NSObject, Identifiable, ObservableObject, WKNavigationDelegate, WKUIDelegate, UIGestureRecognizerDelegate {
    let id: UUID
    let webView: WKWebView

    @Published var workspaceId: String
    @Published var isPrivate: Bool
    @Published var isPinned: Bool = false
    @Published var isSleeping: Bool = false
    @Published var isDesktopMode: Bool = false
    @Published var pageZoom: Double = 1.0
    @Published var urlString: String
    @Published var title: String
    @Published var progress: Double = 0
    @Published var isLoading: Bool = false
    @Published var canGoBack: Bool = false
    @Published var canGoForward: Bool = false
    @Published var displayMode: InlineDisplayMode = .original
    @Published var pageErrorText: String? = nil
    @Published var detectedLanguage: String = "unknown"

    var lastActiveAt: Date = Date()
    var cameFromStartPage: Bool = false

    var isStartPage: Bool {
        urlString == FluoUserDataSnapshot.startPageURL
    }

    var onVisitRecorded: ((String, String) -> Void)?
    var onNavigationSyncAdBlock: ((WKWebView, String?) -> Void)?
    var onScrollDockCollapseChanged: ((Bool) -> Void)?
    var onTwoFingerTap: (() -> Void)?
    var onOpenNewTab: ((URL) -> Void)?
    var onScriptAction: (([String: Any]) -> Void)?
    var onDownloadRequested: ((URL, String?) -> Void)?
    var onPageFinishedLoad: ((BrowserTab) -> Void)?

    private var obsProgress: NSKeyValueObservation?
    private var obsLoading: NSKeyValueObservation?
    private var obsTitle: NSKeyValueObservation?
    private var obsUrl: NSKeyValueObservation?
    private let refreshControl = UIRefreshControl()
    private var panAnchorY: CGFloat = 0

    private static let helperUserScriptSource = """
    (() => {
      if (window.__fluoMobileHelperInstalled) return;
      window.__fluoMobileHelperInstalled = true;
      const post = (payload) => {
        try {
          window.webkit?.messageHandlers?.fluoAction?.postMessage(payload);
        } catch {}
      };
      let selTimer = 0;
      document.addEventListener("selectionchange", () => {
        clearTimeout(selTimer);
        selTimer = setTimeout(() => {
          const text = (window.getSelection ? window.getSelection().toString() : "").trim();
          post({ type: "selection-changed", text: text.slice(0, 600) });
        }, 160);
      });

      window.__fluoDetectLanguage = () => {
        const sample = ((document.body && document.body.innerText) || "").slice(0, 2200);
        let hangul = 0, kana = 0, cjk = 0, latin = 0;
        for (let i = 0; i < sample.length; i++) {
          const c = sample.charCodeAt(i);
          if ((c >= 0xac00 && c <= 0xd7a3) || (c >= 0x3131 && c <= 0x318e)) hangul++;
          else if (c >= 0x3040 && c <= 0x30ff) kana++;
          else if (c >= 0x4e00 && c <= 0x9fff) cjk++;
          else if ((c >= 0x41 && c <= 0x5a) || (c >= 0x61 && c <= 0x7a)) latin++;
        }
        const total = hangul + kana + cjk + latin;
        if (total < 24) {
          const hl = (document.documentElement?.lang || "").toLowerCase();
          if (hl.startsWith("ko")) return "ko";
          if (hl.startsWith("ja")) return "ja";
          if (hl.startsWith("zh")) return "zh";
          if (hl.startsWith("en")) return "en";
          return "unknown";
        }
        if (hangul / total > 0.18) return "ko";
        if (kana > 8 || ((kana + cjk) / total > 0.22 && kana > 0)) return "ja";
        if (cjk / total > 0.22 && kana === 0) return "zh";
        if (latin / total > 0.45) return "en";
        return "other";
      };

      setTimeout(() => {
        if (location.protocol === "http:" || location.protocol === "https:") {
          post({ type: "page-language-detected", lang: window.__fluoDetectLanguage(), url: location.href });
        }
      }, 600);

      let dynObs = null;
      let dynTimer = 0;
      window.__fluoSetDynamicObserver = (enabled) => {
        window.__fluoDynamicOn = Boolean(enabled);
        if (!enabled) {
          if (dynObs) { dynObs.disconnect(); dynObs = null; }
          if (dynTimer) { clearTimeout(dynTimer); dynTimer = 0; }
          return;
        }
        if (dynObs || !document.body || typeof MutationObserver === "undefined") return;
        dynObs = new MutationObserver((mutations) => {
          if (!window.__fluoDynamicOn) return;
          let added = false;
          for (const m of mutations) {
            for (const n of m.addedNodes || []) {
              if (n.nodeType !== 1) continue;
              if (n.id?.startsWith("rb-") || n.hasAttribute?.("data-rb-inline-translation-for") || n.closest?.("[data-rb-inline-translation-for]")) continue;
              if (((n.innerText || n.textContent || "").trim()).length >= 30) { added = true; break; }
            }
            if (added) break;
          }
          if (added && !dynTimer) {
            dynTimer = setTimeout(() => {
              dynTimer = 0;
              if (window.__fluoDynamicOn) post({ type: "dynamic-blocks-added", url: location.href });
            }, 950);
          }
        });
        dynObs.observe(document.body, { childList: true, subtree: true });
      };
    })();
    """

    init(
        id: UUID = UUID(),
        initialURL: URL,
        workspaceId: String,
        isPrivate: Bool = false,
        deferLoad: Bool = false
    ) {
        self.id = id
        let config = WKWebViewConfiguration()
        config.allowsInlineMediaPlayback = true
        if isPrivate {
            config.websiteDataStore = .nonPersistent()
        }
        let userScript = WKUserScript(
            source: Self.helperUserScriptSource,
            injectionTime: .atDocumentEnd,
            forMainFrameOnly: true
        )
        config.userContentController.addUserScript(userScript)

        self.webView = WKWebView(frame: .zero, configuration: config)
        self.workspaceId = workspaceId
        self.isPrivate = isPrivate
        self.urlString = initialURL.absoluteString
        self.title = initialURL.absoluteString == FluoUserDataSnapshot.startPageURL ? "Fluo" : (initialURL.host ?? "Fluo")
        self.cameFromStartPage = (initialURL.absoluteString == FluoUserDataSnapshot.startPageURL)
        self.isSleeping = deferLoad && initialURL.absoluteString != FluoUserDataSnapshot.startPageURL
        super.init()

        let proxy = FluoScriptMessageProxy(tab: self)
        webView.configuration.userContentController.add(proxy, name: "fluoAction")
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
                if !self.isStartPage && !self.isSleeping {
                    self.canGoBack = wv.canGoBack || self.cameFromStartPage
                    self.canGoForward = wv.canGoForward
                }
                if !wv.isLoading, self.refreshControl.isRefreshing {
                    self.refreshControl.endRefreshing()
                }
            }
        }
        obsTitle = webView.observe(\.title, options: [.new]) { [weak self] wv, _ in
            Task { @MainActor in
                guard let self, !self.isStartPage, !self.isSleeping else { return }
                if let t = wv.title, !t.isEmpty { self.title = t }
            }
        }
        obsUrl = webView.observe(\.url, options: [.new]) { [weak self] wv, _ in
            Task { @MainActor in
                guard let self, !self.isStartPage, !self.isSleeping else { return }
                if let u = wv.url?.absoluteString, !u.isEmpty, u != "about:blank" {
                    self.urlString = u
                }
                self.canGoBack = wv.canGoBack || self.cameFromStartPage
                self.canGoForward = wv.canGoForward
            }
        }
        if !deferLoad && initialURL.absoluteString != FluoUserDataSnapshot.startPageURL {
            webView.load(URLRequest(url: initialURL))
        }
    }

    func wakeIfNeeded() {
        lastActiveAt = Date()
        guard isSleeping else { return }
        isSleeping = false
        pageErrorText = nil
        if !isStartPage, let url = URL(string: urlString) {
            webView.load(URLRequest(url: url))
        }
    }

    func putToSleep() {
        guard !isStartPage, !isSleeping else { return }
        isSleeping = true
        displayMode = .original
        webView.stopLoading()
        webView.loadHTMLString("", baseURL: nil)
    }

    func tearDown() {
        webView.stopLoading()
        _ = webView.evaluateJavaScript("document.querySelectorAll('video,audio').forEach(e=>{try{e.pause();e.src='';}catch(_){}});", completionHandler: nil)
        webView.configuration.userContentController.removeScriptMessageHandler(forName: "fluoAction")
        webView.navigationDelegate = nil
        webView.uiDelegate = nil
    }

    static func resolveInputToURL(_ rawInput: String, searchEngine: SearchEngine) -> URL? {
        let trimmed = rawInput.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return nil }

        // Keyword search shortcuts (@namu, @naver, @yt, @w, @g, @ddg, @bing)
        let parts = trimmed.split(separator: " ", maxSplits: 1, omittingEmptySubsequences: true)
        if parts.count == 2, parts[0].hasPrefix("@") {
            let prefix = parts[0].lowercased()
            let query = String(parts[1])
            let enc = query.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? query
            switch prefix {
            case "@n", "@naver":
                return URL(string: "https://search.naver.com/search.naver?query=\(enc)")
            case "@namu":
                return URL(string: "https://namu.wiki/Go?q=\(enc)")
            case "@yt", "@youtube":
                return URL(string: "https://www.youtube.com/results?search_query=\(enc)")
            case "@w", "@wiki":
                return URL(string: "https://en.wikipedia.org/w/index.php?search=\(enc)")
            case "@g", "@google":
                return URL(string: "https://www.google.com/search?q=\(enc)")
            case "@ddg":
                return URL(string: "https://duckduckgo.com/?q=\(enc)")
            case "@b", "@bing":
                return URL(string: "https://www.bing.com/search?q=\(enc)")
            default:
                break
            }
        }

        if trimmed.hasPrefix("http://") || trimmed.hasPrefix("https://") {
            return URL(string: trimmed)
        } else if trimmed.contains(".") && !trimmed.contains(" ") {
            return URL(string: "https://\(trimmed)")
        } else {
            return searchEngine.searchURL(query: trimmed)
        }
    }

    func navigate(input: String, searchEngine: SearchEngine) {
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        isSleeping = false
        pageErrorText = nil
        lastActiveAt = Date()
        if trimmed == FluoUserDataSnapshot.startPageURL || trimmed == "tb://start" {
            openStartPage()
            return
        }
        if let target = Self.resolveInputToURL(trimmed, searchEngine: searchEngine) {
            displayMode = .original
            urlString = target.absoluteString
            canGoBack = webView.canGoBack || webView.url != nil
            webView.load(URLRequest(url: target))
        }
    }

    func openStartPage() {
        webView.stopLoading()
        isSleeping = false
        pageErrorText = nil
        cameFromStartPage = true
        displayMode = .original
        urlString = FluoUserDataSnapshot.startPageURL
        title = "Fluo"
        progress = 0
        isLoading = false
        canGoBack = false
        canGoForward = webView.url != nil && webView.url?.absoluteString != "about:blank"
    }

    func goBackFromCurrent() {
        pageErrorText = nil
        if webView.canGoBack {
            webView.goBack()
        } else if cameFromStartPage && !isStartPage {
            openStartPage()
        }
    }

    func goForwardFromCurrent() {
        pageErrorText = nil
        if isStartPage, let wvUrl = webView.url, wvUrl.absoluteString != "about:blank" {
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
            } else {
                Task { @MainActor in
                    UIApplication.shared.open(url)
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
        pageErrorText = nil
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
        if let url = navigationAction.request.url,
           let scheme = url.scheme?.lowercased(),
           !["http", "https", "about", "data", "blob", "file", "fluo"].contains(scheme) {
            await MainActor.run {
                UIApplication.shared.open(url)
            }
            return .cancel
        }
        let host = navigationAction.request.url?.host
        await MainActor.run {
            self.pageErrorText = nil
            self.onNavigationSyncAdBlock?(webView, host)
        }
        return .allow
    }

    nonisolated func webView(_ webView: WKWebView, decidePolicyFor navigationResponse: WKNavigationResponse) async -> WKNavigationResponsePolicy {
        let isAttachment: Bool = {
            if let http = navigationResponse.response as? HTTPURLResponse,
               let disp = http.value(forHTTPHeaderField: "Content-Disposition")?.lowercased(),
               disp.contains("attachment") {
                return true
            }
            return !navigationResponse.canShowMIMEType
        }()
        if isAttachment, let url = navigationResponse.response.url {
            let suggested = navigationResponse.response.suggestedFilename
            await MainActor.run {
                self.onDownloadRequested?(url, suggested)
            }
            return .cancel
        }
        return .allow
    }

    nonisolated func webView(_ webView: WKWebView, didFinish navigation: WKNavigation!) {
        Task { @MainActor in
            self.pageErrorText = nil
            let url = webView.url?.absoluteString ?? ""
            let pageTitle = webView.title ?? url
            if !self.isPrivate && !url.isEmpty && url != "about:blank" {
                self.onVisitRecorded?(url, pageTitle)
            }
            if abs(self.pageZoom - 1.0) > 0.01 {
                self.applyPageZoom(self.pageZoom)
            }
            self.onPageFinishedLoad?(self)
        }
    }

    nonisolated func webView(_ webView: WKWebView, didFailProvisionalNavigation navigation: WKNavigation!, withError error: Error) {
        let nsErr = error as NSError
        if nsErr.domain == NSURLErrorDomain && nsErr.code == NSURLErrorCancelled { return }
        Task { @MainActor in
            self.isLoading = false
            self.pageErrorText = error.localizedDescription
        }
    }

    nonisolated func webView(_ webView: WKWebView, didFail navigation: WKNavigation!, withError error: Error) {
        let nsErr = error as NSError
        if nsErr.domain == NSURLErrorDomain && nsErr.code == NSURLErrorCancelled { return }
        Task { @MainActor in
            self.isLoading = false
            self.pageErrorText = error.localizedDescription
        }
    }
}

private final class FluoSpeechDelegate: NSObject, AVSpeechSynthesizerDelegate {
    var onUtteranceFinished: (() -> Void)?
    func speechSynthesizer(_ synthesizer: AVSpeechSynthesizer, didFinish utterance: AVSpeechUtterance) {
        Task { @MainActor in
            self.onUtteranceFinished?()
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

    @Published var liveWebSelection: String = ""
    @Published var selectionSourceText = ""
    @Published var selectionTranslatedText = ""
    @Published var showSelectionResultSheet = false

    @Published var showAutoTranslatePrompt = false
    @Published var detectedForeignLanguageCode = ""

    // Edge-style Read Aloud (TTS) state
    @Published var isSpeakingTTS = false
    @Published var isPausedTTS = false
    @Published var ttsRate: Float = 1.0
    @Published var ttsIndex: Int = 0
    @Published var ttsTotal: Int = 0
    private var ttsSegments: [String] = []
    private let speechSynthesizer = AVSpeechSynthesizer()
    private let speechDelegate = FluoSpeechDelegate()

    // File download & share feedback
    @Published var latestDownloadedFileURL: URL? = nil
    @Published var isDownloadingFile: Bool = false

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

        let fallbackId = UUID()
        self.activeTabId = fallbackId

        // Restore saved tab session if available
        if !loaded.savedTabs.isEmpty {
            let targetActiveStr = loaded.activeSavedTabId
            var restoredTabs: [BrowserTab] = []
            for saved in loaded.savedTabs.prefix(24) {
                let tabUUID = UUID(uuidString: saved.id) ?? UUID()
                let url = URL(string: saved.urlString) ?? URL(string: FluoUserDataSnapshot.startPageURL)!
                let isTargetActive = (saved.id == targetActiveStr)
                let wsValid = loaded.workspaces.contains(where: { $0.id == saved.workspaceId })
                let tab = BrowserTab(
                    id: tabUUID,
                    initialURL: url,
                    workspaceId: wsValid ? saved.workspaceId : loaded.activeWorkspaceId,
                    isPrivate: false,
                    deferLoad: !isTargetActive
                )
                tab.isPinned = saved.isPinned
                if !saved.title.isEmpty {
                    tab.title = saved.title
                }
                restoredTabs.append(tab)
            }
            if !restoredTabs.isEmpty {
                self.tabs = restoredTabs
                let chosen = restoredTabs.first(where: { $0.id.uuidString == targetActiveStr })
                    ?? restoredTabs.first(where: { $0.workspaceId == loaded.activeWorkspaceId })
                    ?? restoredTabs[0]
                chosen.wakeIfNeeded()
                self.activeTabId = chosen.id
                for t in restoredTabs {
                    wireTabCallbacks(t)
                }
            }
        }

        if self.tabs.isEmpty {
            let initialUrl = URL(string: FluoUserDataSnapshot.startPageURL)!
            let first = BrowserTab(initialURL: initialUrl, workspaceId: loaded.activeWorkspaceId, isPrivate: false)
            self.tabs = [first]
            self.activeTabId = first.id
            wireTabCallbacks(first)
        }

        speechSynthesizer.delegate = speechDelegate
        speechDelegate.onUtteranceFinished = { [weak self] in
            self?.advanceTTS()
        }
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
            self?.persistUserData()
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
        tab.onScriptAction = { [weak self, weak tab] payload in
            guard let self, let tab, self.activeTabId == tab.id else { return }
            self.handleScriptAction(payload, for: tab)
        }
        tab.onDownloadRequested = { [weak self] url, suggestedName in
            self?.startFileDownload(from: url, suggestedFilename: suggestedName)
        }
        tab.onPageFinishedLoad = { [weak self] loadedTab in
            guard let self else { return }
            self.persistUserData()
            if loadedTab.id == self.activeTabId {
                self.liveWebSelection = ""
            }
        }
        AdBlockManager.shared.apply(
            to: tab.webView,
            enabled: userData.adBlockEnabled,
            host: URL(string: tab.urlString)?.host,
            allowlist: userData.adBlockAllowlist
        )
    }

    private func handleScriptAction(_ payload: [String: Any], for tab: BrowserTab) {
        guard let type = payload["type"] as? String else { return }
        switch type {
        case "selection-changed":
            let text = (payload["text"] as? String)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            liveWebSelection = text
        case "region-picked":
            if isPickingRegion {
                regionPollTask?.cancel()
                isPickingRegion = false
                translateActiveTab(bilingual: false)
            }
        case "retranslate":
            guard let key = payload["key"] as? String else { return }
            let source = (payload["source"] as? String) ?? ""
            retranslateSingleParagraph(key: key, source: source, on: tab)
        case "edit":
            guard let translation = payload["translation"] as? String else { return }
            if !tab.isPrivate, let source = payload["source"] as? String, !source.isEmpty {
                TranslationEngine.shared.storeCachedTranslation(
                    translation,
                    for: source,
                    targetLanguage: targetLanguage,
                    style: userData.style.rawValue
                )
            }
        case "report-translation":
            let source = (payload["source"] as? String) ?? ""
            let tr = (payload["translation"] as? String) ?? ""
            UIPasteboard.general.string = "[Fluo Translation Report]\nURL: \(tab.urlString)\nSource: \(source)\nTranslation: \(tr)"
            translationStatusText = "Copied report to clipboard"
            Task { @MainActor in
                try? await Task.sleep(nanoseconds: 1_800_000_000)
                if self.translationStatusText == "Copied report to clipboard" {
                    self.translationStatusText = ""
                }
            }
        case "page-language-detected":
            let lang = (payload["lang"] as? String) ?? "unknown"
            tab.detectedLanguage = lang
            evaluateAutoTranslatePolicy(for: tab, detectedLang: lang)
        case "dynamic-blocks-added":
            if tab.displayMode != .original && !isTranslating {
                translateActiveTab(bilingual: tab.displayMode == .bilingual)
            }
        default:
            break
        }
    }

    private func evaluateAutoTranslatePolicy(for tab: BrowserTab, detectedLang: String) {
        guard !tab.isStartPage, tab.displayMode == .original, !isTranslating else { return }
        guard detectedLang != "unknown" else { return }
        let targetCode: String = {
            let low = targetLanguage.lowercased()
            if low.contains("한국") || low.contains("korean") || low == "ko" { return "ko" }
            if low.contains("日本") || low.contains("japanese") || low == "ja" { return "ja" }
            if low.contains("中文") || low.contains("chinese") || low.hasPrefix("zh") { return "zh" }
            return "en"
        }()
        if detectedLang == targetCode {
            showAutoTranslatePrompt = false
            return
        }
        let host = URL(string: tab.urlString)?.host?.lowercased() ?? ""
        if !host.isEmpty && userData.neverTranslateHosts.contains(where: { $0.lowercased() == host }) {
            showAutoTranslatePrompt = false
            return
        }
        if !host.isEmpty && userData.autoTranslateHosts.contains(where: { $0.lowercased() == host }) {
            showAutoTranslatePrompt = false
            translateActiveTab(bilingual: false)
            return
        }
        if userData.autoTranslateEnabled {
            detectedForeignLanguageCode = detectedLang.uppercased()
            showAutoTranslatePrompt = true
        }
    }

    func toggleAlwaysTranslateCurrentHost() {
        let host = currentHost
        guard !host.isEmpty else { return }
        userData.neverTranslateHosts.removeAll { $0.lowercased() == host }
        if let idx = userData.autoTranslateHosts.firstIndex(where: { $0.lowercased() == host }) {
            userData.autoTranslateHosts.remove(at: idx)
        } else {
            userData.autoTranslateHosts.append(host)
            showAutoTranslatePrompt = false
            if activeTab.displayMode == .original {
                translateActiveTab(bilingual: false)
            }
        }
        userData.save()
    }

    func toggleNeverTranslateCurrentHost() {
        let host = currentHost
        guard !host.isEmpty else { return }
        userData.autoTranslateHosts.removeAll { $0.lowercased() == host }
        if let idx = userData.neverTranslateHosts.firstIndex(where: { $0.lowercased() == host }) {
            userData.neverTranslateHosts.remove(at: idx)
        } else {
            userData.neverTranslateHosts.append(host)
            showAutoTranslatePrompt = false
        }
        userData.save()
    }

    var isCurrentHostAlwaysTranslated: Bool {
        let host = currentHost
        guard !host.isEmpty else { return false }
        return userData.autoTranslateHosts.contains { $0.lowercased() == host }
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

    func persistUserData() {
        userData.savedTabs = tabs.filter { !$0.isPrivate }.map { t in
            SavedTabItem(
                id: t.id.uuidString,
                urlString: t.urlString,
                title: t.title,
                workspaceId: t.workspaceId,
                isPinned: t.isPinned
            )
        }
        userData.activeSavedTabId = activeTabId.uuidString
        userData.save()
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
        persistUserData()
        applyLiveTypography()
    }

    func applyLiveTypography() {
        let s = String(format: "%.2f", userData.fontScale)
        let l = String(format: "%.2f", userData.lineHeight)
        _ = activeTab.webView.evaluateJavaScript("window.__tbUpdateInlineStyle && window.__tbUpdateInlineStyle(\(s), \(l));", completionHandler: nil)
    }

    // MARK: - Omnibox Suggestions & Calculator

    static func evaluateMathExpression(_ raw: String) -> String? {
        let trimmed = raw.trimmingCharacters(in: .whitespacesAndNewlines)
        let expr = trimmed.hasPrefix("=") ? String(trimmed.dropFirst()).trimmingCharacters(in: .whitespaces) : trimmed
        guard expr.count >= 3, expr.count <= 48 else { return nil }
        let allowed = CharacterSet(charactersIn: "0123456789.+-*/() ")
        guard expr.unicodeScalars.allSatisfy({ allowed.contains($0) }) else { return nil }
        guard expr.contains(where: { "+-*/".contains($0) }),
              expr.contains(where: { $0.isNumber }) else { return nil }

        let chars = Array(expr.replacingOccurrences(of: " ", with: ""))
        var pos = 0

        func parseExpression() -> Double? {
            guard var lhs = parseTerm() else { return nil }
            while pos < chars.count && (chars[pos] == "+" || chars[pos] == "-") {
                let op = chars[pos]
                pos += 1
                guard let rhs = parseTerm() else { return nil }
                lhs = (op == "+") ? (lhs + rhs) : (lhs - rhs)
            }
            return lhs
        }

        func parseTerm() -> Double? {
            guard var lhs = parseFactor() else { return nil }
            while pos < chars.count && (chars[pos] == "*" || chars[pos] == "/") {
                let op = chars[pos]
                pos += 1
                guard let rhs = parseFactor() else { return nil }
                if op == "/" {
                    guard abs(rhs) > 1e-12 else { return nil }
                    lhs /= rhs
                } else {
                    lhs *= rhs
                }
            }
            return lhs
        }

        func parseFactor() -> Double? {
            guard pos < chars.count else { return nil }
            if chars[pos] == "+" {
                pos += 1
                return parseFactor()
            }
            if chars[pos] == "-" {
                pos += 1
                guard let v = parseFactor() else { return nil }
                return -v
            }
            if chars[pos] == "(" {
                pos += 1
                guard let val = parseExpression(), pos < chars.count, chars[pos] == ")" else { return nil }
                pos += 1
                return val
            }
            let start = pos
            var dotCount = 0
            while pos < chars.count && (chars[pos].isNumber || chars[pos] == ".") {
                if chars[pos] == "." { dotCount += 1 }
                pos += 1
            }
            guard pos > start, dotCount <= 1 else { return nil }
            return Double(String(chars[start..<pos]))
        }

        guard let val = parseExpression(), pos == chars.count, val.isFinite else { return nil }
        if abs(val.rounded() - val) < 1e-9 {
            return String(format: "%.0f", val)
        }
        return String(format: "%.4f", val).replacingOccurrences(of: "\\.?0+$", with: "", options: .regularExpression)
    }

    func omniboxSuggestions(for query: String) -> [OmniboxSuggestion] {
        let q = query.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !q.isEmpty, q != FluoUserDataSnapshot.startPageURL else { return [] }
        var items: [OmniboxSuggestion] = []

        if let mathRes = Self.evaluateMathExpression(q) {
            items.append(
                OmniboxSuggestion(
                    id: "calc-\(q)",
                    kind: .calculation,
                    title: "= \(mathRes)",
                    subtitle: q,
                    targetURLOrQuery: mathRes,
                    tabId: nil
                )
            )
        }

        // Open tabs match
        for tab in tabs where tab.id != activeTabId && !tab.isStartPage {
            if tab.title.localizedCaseInsensitiveContains(q) || tab.urlString.localizedCaseInsensitiveContains(q) {
                items.append(
                    OmniboxSuggestion(
                        id: "tab-\(tab.id.uuidString)",
                        kind: .switchTab,
                        title: tab.title.isEmpty ? tab.urlString : tab.title,
                        subtitle: tab.urlString,
                        targetURLOrQuery: tab.urlString,
                        tabId: tab.id
                    )
                )
            }
            if items.count >= 3 { break }
        }

        // Bookmarks match
        for bm in userData.bookmarks {
            if bm.title.localizedCaseInsensitiveContains(q) || bm.url.localizedCaseInsensitiveContains(q) {
                items.append(
                    OmniboxSuggestion(
                        id: "bm-\(bm.id)",
                        kind: .bookmark,
                        title: bm.title,
                        subtitle: bm.url,
                        targetURLOrQuery: bm.url,
                        tabId: nil
                    )
                )
            }
            if items.count >= 6 { break }
        }

        // History match
        var seenURLs = Set(items.map(\.targetURLOrQuery))
        for h in userData.history {
            if seenURLs.contains(h.url) { continue }
            if h.title.localizedCaseInsensitiveContains(q) || h.url.localizedCaseInsensitiveContains(q) {
                seenURLs.insert(h.url)
                items.append(
                    OmniboxSuggestion(
                        id: "hist-\(h.id)",
                        kind: .history,
                        title: h.title.isEmpty ? h.url : h.title,
                        subtitle: h.url,
                        targetURLOrQuery: h.url,
                        tabId: nil
                    )
                )
            }
            if items.count >= 8 { break }
        }

        return items
    }

    // MARK: - Tabs & Workspaces

    func selectTab(_ id: UUID) {
        guard let target = tabs.first(where: { $0.id == id }) else { return }
        stopTTS()
        showAutoTranslatePrompt = false
        liveWebSelection = ""
        target.wakeIfNeeded()
        activeTabId = target.id
        persistUserData()
    }

    func newTab(urlString: String? = nil, isPrivate: Bool = false) {
        stopTTS()
        showAutoTranslatePrompt = false
        liveWebSelection = ""
        let targetStr = urlString ?? FluoUserDataSnapshot.startPageURL
        let url = URL(string: targetStr) ?? URL(string: FluoUserDataSnapshot.startPageURL)!
        let tab = BrowserTab(initialURL: url, workspaceId: userData.activeWorkspaceId, isPrivate: isPrivate)
        wireTabCallbacks(tab)
        tabs.append(tab)
        activeTabId = tab.id
        persistUserData()
    }

    func duplicateTab(_ id: UUID) {
        guard let source = tabs.first(where: { $0.id == id }) else { return }
        newTab(urlString: source.urlString, isPrivate: source.isPrivate)
    }

    func moveTabsInCurrentWorkspace(fromOffsets: IndexSet, toOffset: Int) {
        var wsTabs = activeWorkspaceTabs
        wsTabs.move(fromOffsets: fromOffsets, toOffset: toOffset)
        let otherTabs = tabs.filter { $0.workspaceId != userData.activeWorkspaceId }
        tabs = wsTabs + otherTabs
        persistUserData()
    }

    func sleepInactiveTabs(keepCount: Int = 3) {
        let nonActive = tabs
            .filter { $0.id != activeTabId && !$0.isPinned && !$0.isStartPage && !$0.isSleeping }
            .sorted { $0.lastActiveAt > $1.lastActiveAt }
        for tab in nonActive.dropFirst(max(0, keepCount - 1)) {
            tab.putToSleep()
        }
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
            persistUserData()
            return
        }
        tabCancellables.removeValue(forKey: id)
        removed.tearDown()
        tabs.remove(at: idx)
        if !tabs.contains(where: { $0.id == activeTabId }) {
            let next = activeWorkspaceTabs.first ?? tabs[0]
            next.wakeIfNeeded()
            activeTabId = next.id
        }
        persistUserData()
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
            tab.tearDown()
        }
        tabs.removeAll { $0.workspaceId == wsId && !$0.isPinned }
        if activeWorkspaceTabs.isEmpty {
            newTab()
        } else if !tabs.contains(where: { $0.id == activeTabId }) {
            let next = activeWorkspaceTabs[0]
            next.wakeIfNeeded()
            activeTabId = next.id
        }
        persistUserData()
    }

    func togglePinTab(_ id: UUID) {
        if let tab = tabs.first(where: { $0.id == id }) {
            tab.isPinned.toggle()
            objectWillChange.send()
            persistUserData()
        }
    }

    func createWorkspace(name: String) {
        let clean = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty else { return }
        let ws = WorkspaceItem(id: UUID().uuidString, name: clean)
        userData.workspaces.append(ws)
        userData.activeWorkspaceId = ws.id
        newTab()
        persistUserData()
    }

    func renameWorkspace(_ id: String, name: String) {
        let clean = name.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !clean.isEmpty,
              let idx = userData.workspaces.firstIndex(where: { $0.id == id }) else { return }
        userData.workspaces[idx].name = clean
        objectWillChange.send()
        persistUserData()
    }

    func switchWorkspace(_ id: String) {
        guard userData.workspaces.contains(where: { $0.id == id }) else { return }
        userData.activeWorkspaceId = id
        if let existing = tabs.first(where: { $0.workspaceId == id }) {
            selectTab(existing.id)
        } else {
            newTab()
        }
        persistUserData()
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
        persistUserData()
    }

    func moveTab(_ tabId: UUID, toWorkspace wsId: String) {
        if let tab = tabs.first(where: { $0.id == tabId }) {
            tab.workspaceId = wsId
            objectWillChange.send()
            persistUserData()
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
        selectTab(list[nextIndex].id)
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

    // MARK: - Custom Start Page Shortcuts

    func addCustomShortcut(title: String, urlString: String) {
        let cleanTitle = title.trimmingCharacters(in: .whitespacesAndNewlines)
        var cleanUrl = urlString.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanUrl.isEmpty else { return }
        if !cleanUrl.hasPrefix("http://") && !cleanUrl.hasPrefix("https://") {
            cleanUrl = "https://\(cleanUrl)"
        }
        let finalTitle = cleanTitle.isEmpty ? (URL(string: cleanUrl)?.host ?? cleanUrl) : cleanTitle
        userData.customShortcuts.append(
            QuickShortcutItem(
                id: UUID().uuidString,
                title: finalTitle,
                urlString: cleanUrl,
                icon: "globe",
                tintHex: "#684CFF"
            )
        )
        userData.save()
    }

    func removeCustomShortcut(_ id: String) {
        userData.customShortcuts.removeAll { $0.id == id }
        userData.save()
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

    func deleteHistoryItem(_ id: String) {
        userData.history.removeAll { $0.id == id }
        userData.save()
    }

    func deleteTranslationHistoryItem(_ id: String) {
        userData.translationHistory.removeAll { $0.id == id }
        userData.save()
    }

    func clearTranslationHistory() {
        userData.translationHistory.removeAll()
        userData.save()
    }

    func upsertGlossaryItem(
        id: String? = nil,
        source: String,
        target: String,
        caseSensitive: Bool,
        folder: String = "Default",
        exactMatch: Bool = false,
        useRegex: Bool = false,
        doNotTranslate: Bool = false,
        note: String = ""
    ) {
        let cleanSrc = source.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !cleanSrc.isEmpty else { return }
        let cleanFolder = folder.trimmingCharacters(in: .whitespacesAndNewlines)
        let finalFolder = cleanFolder.isEmpty ? "Default" : cleanFolder
        if !userData.glossaryFolders.contains(finalFolder) {
            userData.glossaryFolders.append(finalFolder)
        }
        let cleanTarget = target.trimmingCharacters(in: .whitespacesAndNewlines)
        if let id, let idx = userData.glossary.firstIndex(where: { $0.id == id }) {
            userData.glossary[idx].source = cleanSrc
            userData.glossary[idx].target = cleanTarget
            userData.glossary[idx].caseSensitive = caseSensitive
            userData.glossary[idx].folder = finalFolder
            userData.glossary[idx].exactMatch = exactMatch
            userData.glossary[idx].useRegex = useRegex
            userData.glossary[idx].doNotTranslate = doNotTranslate
            userData.glossary[idx].note = note
        } else {
            userData.glossary.removeAll {
                $0.folder == finalFolder && $0.source.caseInsensitiveCompare(cleanSrc) == .orderedSame
            }
            userData.glossary.insert(
                GlossaryItem(
                    id: UUID().uuidString,
                    source: cleanSrc,
                    target: cleanTarget,
                    caseSensitive: caseSensitive,
                    folder: finalFolder,
                    enabled: true,
                    exactMatch: exactMatch,
                    useRegex: useRegex,
                    doNotTranslate: doNotTranslate,
                    note: note
                ),
                at: 0
            )
        }
        userData.save()
    }

    func addGlossaryItem(source: String, target: String, caseSensitive: Bool, folder: String = "Default") {
        upsertGlossaryItem(id: nil, source: source, target: target, caseSensitive: caseSensitive, folder: folder)
    }

    func exportGlossaryCSV() -> String {
        var lines = ["source,target,folder,case_sensitive,exact_match,use_regex,do_not_translate,note"]
        for item in userData.glossary {
            let cols = [
                item.source,
                item.target,
                item.folder,
                item.caseSensitive ? "1" : "0",
                item.exactMatch ? "1" : "0",
                item.useRegex ? "1" : "0",
                item.doNotTranslate ? "1" : "0",
                item.note,
            ].map { col -> String in
                let escaped = col.replacingOccurrences(of: "\"", with: "\"\"")
                return "\"\(escaped)\""
            }
            lines.append(cols.joined(separator: ","))
        }
        return lines.joined(separator: "\n")
    }

    @discardableResult
    func importGlossaryCSV(_ csv: String, defaultFolder: String = "Default") -> Int {
        let rows = csv.components(separatedBy: .newlines)
        var imported = 0
        for (idx, rawLine) in rows.enumerated() {
            let line = rawLine.trimmingCharacters(in: .whitespacesAndNewlines)
            if line.isEmpty { continue }
            let cols = line.split(separator: ",", omittingEmptySubsequences: false).map {
                $0.trimmingCharacters(in: CharacterSet(charactersIn: " \"'"))
            }
            if idx == 0 && cols.first?.lowercased() == "source" { continue }
            guard cols.count >= 2, !cols[0].isEmpty else { continue }
            let folder = (cols.count >= 3 && !cols[2].isEmpty) ? cols[2] : defaultFolder
            let cs = cols.count >= 4 && (cols[3] == "1" || cols[3].lowercased() == "true")
            let em = cols.count >= 5 && (cols[4] == "1" || cols[4].lowercased() == "true")
            let rx = cols.count >= 6 && (cols[5] == "1" || cols[5].lowercased() == "true")
            let dnt = cols.count >= 7 && (cols[6] == "1" || cols[6].lowercased() == "true")
            let note = cols.count >= 8 ? cols[7] : ""
            upsertGlossaryItem(
                source: cols[0],
                target: cols[1],
                caseSensitive: cs,
                folder: folder,
                exactMatch: em,
                useRegex: rx,
                doNotTranslate: dnt,
                note: note
            )
            imported += 1
        }
        return imported
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

    func clearCurrentHostSiteData() async {
        let host = currentHost
        guard !host.isEmpty else { return }
        let store = WKWebsiteDataStore.default()
        let types = WKWebsiteDataStore.allWebsiteDataTypes()
        let records = await store.dataRecords(ofTypes: types)
        let matched = records.filter { rec in
            let d = rec.displayName.lowercased()
            return host == d || host.hasSuffix(".\(d)") || d.hasSuffix(host)
        }
        if !matched.isEmpty {
            await store.removeData(ofTypes: types, for: matched)
        }
        activeTab.webView.reload()
    }

    func clearSiteData() async {
        let store = WKWebsiteDataStore.default()
        let types = WKWebsiteDataStore.allWebsiteDataTypes()
        await store.removeData(ofTypes: types, modifiedSince: .distantPast)
        TranslationEngine.shared.clearTranslationCache()
    }

    // MARK: - File Downloads & Markdown Export

    func startFileDownload(from url: URL, suggestedFilename: String?) {
        isDownloadingFile = true
        Task {
            defer {
                Task { @MainActor in self.isDownloadingFile = false }
            }
            do {
                let (tempURL, response) = try await URLSession.shared.download(from: url)
                let docs = FileManager.default.urls(for: .documentDirectory, in: .userDomainMask).first!
                let dir = docs.appendingPathComponent("Downloads", isDirectory: true)
                try? FileManager.default.createDirectory(at: dir, withIntermediateDirectories: true)
                let rawName = suggestedFilename ?? response.suggestedFilename ?? (url.lastPathComponent.isEmpty ? "download.bin" : url.lastPathComponent)
                let dest = dir.appendingPathComponent("\(Int(Date().timeIntervalSince1970))_\(rawName)")
                try? FileManager.default.removeItem(at: dest)
                try FileManager.default.moveItem(at: tempURL, to: dest)
                let size = (try? FileManager.default.attributesOfItem(atPath: dest.path)[.size] as? Int64) ?? 0
                await MainActor.run {
                    let record = DownloadItemRecord(
                        id: UUID().uuidString,
                        fileName: rawName,
                        sourceURL: url.absoluteString,
                        localPath: dest.path,
                        byteSize: size,
                        createdAt: Date()
                    )
                    self.userData.downloads.insert(record, at: 0)
                    self.userData.save()
                    self.latestDownloadedFileURL = dest
                }
            } catch {
                await MainActor.run {
                    self.translationError = "Download failed: \(error.localizedDescription)"
                }
            }
        }
    }

    func exportCurrentPageMarkdown() async -> String {
        let tab = activeTab
        if tab.isStartPage { return "# Fluo Start Page" }
        let js = """
        (() => {
          const nodes = [...document.querySelectorAll("[data-rb-inline-translation-for], [data-rb-inline-key]:not([hidden])")];
          if (nodes.length > 0) {
            return nodes.map(n => (n.innerText || n.textContent || "").trim()).filter(Boolean).join("\\n\\n");
          }
          return (document.body && document.body.innerText) ? document.body.innerText.trim() : "";
        })()
        """
        let body = (try? await tab.webView.evaluateJavaScript(js) as? String) ?? ""
        return "# \(tab.title)\n\nSource: \(tab.urlString)\n\n---\n\n\(body)"
    }

    func togglePictureInPicture() {
        let js = """
        (() => {
          if (document.pictureInPictureElement && typeof document.exitPictureInPicture === "function") {
            document.exitPictureInPicture().catch(() => {});
            return;
          }
          const videos = [...document.querySelectorAll("video")];
          const v = videos.find(el => !el.paused && !el.ended && el.readyState >= 2)
            || videos.sort((a, b) => (b.clientWidth * b.clientHeight) - (a.clientWidth * a.clientHeight))[0];
          if (!v) return;
          if (typeof v.webkitSetPresentationMode === "function") {
            v.webkitSetPresentationMode(v.webkitPresentationMode === "picture-in-picture" ? "inline" : "picture-in-picture");
          } else if (typeof v.requestPictureInPicture === "function") {
            v.requestPictureInPicture().catch(() => {});
          }
        })()
        """
        _ = activeTab.webView.evaluateJavaScript(js, completionHandler: nil)
    }

    // MARK: - Edge-Style Read Aloud (TTS)

    func startOrToggleTTS() {
        if isSpeakingTTS {
            if isPausedTTS {
                speechSynthesizer.continueSpeaking()
                isPausedTTS = false
            } else {
                speechSynthesizer.pauseSpeaking(at: .word)
                isPausedTTS = true
            }
            return
        }
        let js = """
        (() => {
          const tr = [...document.querySelectorAll("[data-rb-inline-translation-for]")].filter(e => (e.innerText || "").trim().length > 1);
          const list = tr.length > 0 ? tr : [...document.querySelectorAll("article p, main p, p, h1, h2, h3, li")].filter(e => (e.innerText || "").trim().length >= 12 && e.offsetParent !== null);
          window.__fluoTtsNodes = list;
          return list.map(e => (e.innerText || e.textContent || "").trim());
        })()
        """
        activeTab.webView.evaluateJavaScript(js) { [weak self] res, _ in
            guard let self, let arr = res as? [String], !arr.isEmpty else { return }
            Task { @MainActor in
                self.ttsSegments = arr
                self.ttsTotal = arr.count
                self.ttsIndex = 0
                self.isSpeakingTTS = true
                self.isPausedTTS = false
                self.speakCurrentTTSSegment()
            }
        }
    }

    func stepTTS(delta: Int) {
        guard isSpeakingTTS, !ttsSegments.isEmpty else { return }
        speechSynthesizer.stopSpeaking(at: .immediate)
        ttsIndex = max(0, min(ttsSegments.count - 1, ttsIndex + delta))
        isPausedTTS = false
        speakCurrentTTSSegment()
    }

    func setTTSRate(_ rate: Float) {
        ttsRate = max(0.6, min(2.0, rate))
        if isSpeakingTTS && !isPausedTTS {
            speechSynthesizer.stopSpeaking(at: .immediate)
            speakCurrentTTSSegment()
        }
    }

    func stopTTS() {
        if speechSynthesizer.isSpeaking || speechSynthesizer.isPaused {
            speechSynthesizer.stopSpeaking(at: .immediate)
        }
        isSpeakingTTS = false
        isPausedTTS = false
        ttsIndex = 0
        ttsTotal = 0
        ttsSegments = []
        _ = activeTab.webView.evaluateJavaScript(
            "document.querySelectorAll('[data-rb-tts-active]').forEach(e => e.removeAttribute('data-rb-tts-active'));",
            completionHandler: nil
        )
    }

    private func advanceTTS() {
        guard isSpeakingTTS, !isPausedTTS else { return }
        if ttsIndex + 1 < ttsSegments.count {
            ttsIndex += 1
            speakCurrentTTSSegment()
        } else {
            stopTTS()
        }
    }

    private func speakCurrentTTSSegment() {
        guard isSpeakingTTS, ttsIndex >= 0, ttsIndex < ttsSegments.count else {
            stopTTS()
            return
        }
        let text = ttsSegments[ttsIndex]
        let highlightJs = """
        (() => {
          document.querySelectorAll('[data-rb-tts-active]').forEach(e => e.removeAttribute('data-rb-tts-active'));
          const nodes = window.__fluoTtsNodes || [];
          const el = nodes[\(ttsIndex)];
          if (el) {
            el.setAttribute('data-rb-tts-active', '1');
            el.scrollIntoView({ behavior: 'smooth', block: 'center' });
          }
        })()
        """
        _ = activeTab.webView.evaluateJavaScript(highlightJs, completionHandler: nil)

        let utterance = AVSpeechUtterance(string: text)
        let mappedRate = AVSpeechUtteranceDefaultSpeechRate * ttsRate
        utterance.rate = min(AVSpeechUtteranceMaximumSpeechRate, max(AVSpeechUtteranceMinimumSpeechRate, mappedRate))
        if text.range(of: "[가-힣]", options: .regularExpression) != nil {
            utterance.voice = AVSpeechSynthesisVoice(language: "ko-KR")
        } else if text.range(of: "[ぁ-んァ-ン]", options: .regularExpression) != nil {
            utterance.voice = AVSpeechSynthesisVoice(language: "ja-JP")
        } else if text.range(of: "[\\u4e00-\\u9fff]", options: .regularExpression) != nil {
            utterance.voice = AVSpeechSynthesisVoice(language: "zh-CN")
        } else {
            utterance.voice = AVSpeechSynthesisVoice(language: "en-US")
        }
        speechSynthesizer.speak(utterance)
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
        let runWithText = { [weak self] (picked: String) in
            guard let self else { return }
            if picked.isEmpty {
                self.translationError = L10n.tr(self.uiLang, "tr.noSelection")
                return
            }
            self.selectionSourceText = picked
            self.selectionTranslatedText = ""
            self.showSelectionResultSheet = true
            Task { @MainActor in
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

        if !liveWebSelection.isEmpty {
            runWithText(liveWebSelection)
            return
        }
        activeTab.webView.evaluateJavaScript("window.getSelection ? window.getSelection().toString() : ''") { res, _ in
            let picked = (res as? String)?.trimmingCharacters(in: .whitespacesAndNewlines) ?? ""
            Task { @MainActor in
                runWithText(picked)
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

    // MARK: - Inline Page Translation, Single-Paragraph Retranslate & Cancel

    func cancelTranslation() {
        translationTask?.cancel()
        TranslationEngine.shared.cancelActiveInference()
        isTranslating = false
        translationStatusText = ""
    }

    func restoreOriginal() {
        cancelTranslation()
        let tab = activeTab
        _ = tab.webView.evaluateJavaScript("window.__fluoSetDynamicObserver && window.__fluoSetDynamicObserver(false);", completionHandler: nil)
        guard let js = loadResourceScript("inline_restore") else { return }
        tab.webView.evaluateJavaScript(js) { _, _ in
            Task { @MainActor in
                tab.displayMode = .original
            }
        }
    }

    private func applyTranslatedSegments(
        _ pairs: [(key: String, translation: String)],
        on tab: BrowserTab,
        applyJs: String,
        fontScale: Double,
        lineHeight: Double,
        bilingual: Bool,
        uiJson: String
    ) async {
        guard !pairs.isEmpty else { return }
        let payload: [[String: Any]] = pairs.map { item in
            [
                "key": item.key,
                "translation": item.translation,
                "uncertain": false,
                "notes": [String](),
            ]
        }
        guard let jsonData = try? JSONSerialization.data(withJSONObject: payload),
              let jsonStr = String(data: jsonData, encoding: .utf8) else { return }
        let invokeJs = applyJs
            .replacingOccurrences(of: "__PAYLOAD__", with: jsonStr)
            .replacingOccurrences(of: "__FONT_SCALE__", with: String(format: "%.2f", fontScale))
            .replacingOccurrences(of: "__LINE_HEIGHT__", with: String(format: "%.2f", lineHeight))
            .replacingOccurrences(of: "__BILINGUAL__", with: bilingual ? "true" : "false")
            .replacingOccurrences(of: "__UI_LANG__", with: uiJson)
        _ = try? await tab.webView.evaluateJavaScript(invokeJs)
    }

    private func retranslateSingleParagraph(key: String, source: String, on tab: BrowserTab) {
        guard !source.isEmpty, let applyJs = loadResourceScript("inline_translate") else { return }
        let currentProvider = provider
        let currentModel = activeModelId
        let currentBase = openAiBaseUrl
        let currentTarget = targetLanguage
        let currentStyle = userData.style.rawValue
        let currentGlossary = userData.glossary.filter(\.enabled)
        let currentFontScale = userData.fontScale
        let currentLineHeight = userData.lineHeight
        let bilingual = (tab.displayMode == .bilingual)
        let uiJson = (try? JSONEncoder().encode(uiLang.rawValue)).flatMap { String(data: $0, encoding: .utf8) } ?? "\"ko\""

        Task { @MainActor in
            do {
                let tr = try await TranslationEngine.shared.translateText(
                    source,
                    provider: currentProvider,
                    modelId: currentModel,
                    baseUrl: currentBase,
                    targetLanguage: currentTarget,
                    style: currentStyle,
                    glossary: currentGlossary
                )
                if !tab.isPrivate {
                    TranslationEngine.shared.storeCachedTranslation(tr, for: source, targetLanguage: currentTarget, style: currentStyle)
                }
                await applyTranslatedSegments(
                    [(key: key, translation: tr)],
                    on: tab,
                    applyJs: applyJs,
                    fontScale: currentFontScale,
                    lineHeight: currentLineHeight,
                    bilingual: bilingual,
                    uiJson: uiJson
                )
            } catch {
                translationError = error.localizedDescription
            }
        }
    }

    func translateActiveTab(bilingual: Bool, forceRetranslate: Bool = false) {
        translationTask?.cancel()
        TranslationEngine.shared.cancelActiveInference()
        translationError = nil
        showAutoTranslatePrompt = false
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
                var segments: [(key: String, text: String)] = blocks.compactMap { b in
                    guard let k = (b["inlineKey"] as? String) ?? (b["key"] as? String),
                          let t = b["text"] as? String, !t.isEmpty else { return nil }
                    return (k, t)
                }
                guard !segments.isEmpty else {
                    translationError = L10n.tr(uiLang, "tr.noBlocks")
                    return
                }

                // Reorder segments by viewport proximity so visible screen paragraphs translate first!
                if let vpOrder = try? await tab.webView.evaluateJavaScript(
                    "window.__tbGetViewportOrder ? (window.__tbGetViewportOrder().keys || []) : []"
                ) as? [String], !vpOrder.isEmpty {
                    var rank: [String: Int] = [:]
                    for (idx, k) in vpOrder.enumerated() {
                        rank[k] = idx
                    }
                    segments.sort { a, b in
                        let ra = rank[a.key] ?? Int.max
                        let rb = rank[b.key] ?? Int.max
                        return ra < rb
                    }
                }

                translationTotal = segments.count
                translationDone = 0
                let uiJson = (try? JSONEncoder().encode(currentUi)).flatMap { String(data: $0, encoding: .utf8) } ?? "\"ko\""

                // Pass 1: Immediately restore any cached segment translations in 0ms
                var uncached: [(key: String, text: String)] = []
                var cachedBatch: [(key: String, translation: String)] = []
                for seg in segments {
                    if !forceRetranslate,
                       let hit = TranslationEngine.shared.cachedTranslation(
                        for: seg.text,
                        targetLanguage: currentTarget,
                        style: currentStyle
                       ) {
                        cachedBatch.append((key: seg.key, translation: hit))
                    } else {
                        uncached.append(seg)
                    }
                }
                if !cachedBatch.isEmpty {
                    await applyTranslatedSegments(
                        cachedBatch,
                        on: tab,
                        applyJs: applyJs,
                        fontScale: currentFontScale,
                        lineHeight: currentLineHeight,
                        bilingual: bilingual,
                        uiJson: uiJson
                    )
                    translationDone += cachedBatch.count
                }

                // Pass 2: Translate uncached segments (Batched for Cloud, Viewport-streamed for On-Device Hy-MT2)
                if currentProvider != .hyMt2 && uncached.count > 1 {
                    let batchSize = 6
                    var idx = 0
                    while idx < uncached.count {
                        try Task.checkCancellation()
                        let end = min(idx + batchSize, uncached.count)
                        let slice = Array(uncached[idx..<end])
                        let sources = slice.map(\.text)
                        let translatedList = try await TranslationEngine.shared.translateBatchCloud(
                            sources,
                            provider: currentProvider,
                            modelId: currentModel,
                            baseUrl: currentBase,
                            targetLanguage: currentTarget,
                            style: currentStyle,
                            glossary: currentGlossary
                        )
                        var pairs: [(key: String, translation: String)] = []
                        for (offset, tr) in translatedList.enumerated() where offset < slice.count {
                            let item = slice[offset]
                            if !tab.isPrivate {
                                TranslationEngine.shared.storeCachedTranslation(
                                    tr,
                                    for: item.text,
                                    targetLanguage: currentTarget,
                                    style: currentStyle
                                )
                            }
                            pairs.append((key: item.key, translation: tr))
                        }
                        await applyTranslatedSegments(
                            pairs,
                            on: tab,
                            applyJs: applyJs,
                            fontScale: currentFontScale,
                            lineHeight: currentLineHeight,
                            bilingual: bilingual,
                            uiJson: uiJson
                        )
                        translationDone += slice.count
                        idx = end
                    }
                } else {
                    for seg in uncached {
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
                        if !tab.isPrivate {
                            TranslationEngine.shared.storeCachedTranslation(
                                tr,
                                for: seg.text,
                                targetLanguage: currentTarget,
                                style: currentStyle
                            )
                        }
                        translationDone += 1
                        await applyTranslatedSegments(
                            [(key: seg.key, translation: tr)],
                            on: tab,
                            applyJs: applyJs,
                            fontScale: currentFontScale,
                            lineHeight: currentLineHeight,
                            bilingual: bilingual,
                            uiJson: uiJson
                        )
                    }
                }

                tab.displayMode = bilingual ? .bilingual : .inline
                _ = try? await tab.webView.evaluateJavaScript("window.__fluoSetDynamicObserver && window.__fluoSetDynamicObserver(true);")
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
                if !error.localizedDescription.lowercased().contains("cancelled") {
                    translationError = error.localizedDescription
                }
            }
        }
    }
}
