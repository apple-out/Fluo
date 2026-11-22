import Foundation
import WebKit

enum InlineDisplayMode: String {
    case original
    case inline
    case bilingual
}

final class BrowserTab: NSObject, Identifiable, ObservableObject {
    let id = UUID()
    let webView: WKWebView

    @Published var urlString: String
    @Published var title: String
    @Published var progress: Double = 0
    @Published var isLoading: Bool = false
    @Published var canGoBack: Bool = false
    @Published var canGoForward: Bool = false
    @Published var displayMode: InlineDisplayMode = .original

    private var obsProgress: NSKeyValueObservation?
    private var obsLoading: NSKeyValueObservation?
    private var obsTitle: NSKeyValueObservation?
    private var obsUrl: NSKeyValueObservation?

    init(initialURL: URL) {
        let config = WKWebViewConfiguration()
        config.allowsInlineMediaPlayback = true
        self.webView = WKWebView(frame: .zero, configuration: config)
        self.urlString = initialURL.absoluteString
        self.title = initialURL.host ?? "Fluo"
        super.init()

        webView.allowsBackForwardNavigationGestures = true
        obsProgress = webView.observe(\.estimatedProgress, options: [.new]) { [weak self] wv, _ in
            Task { @MainActor in self?.progress = wv.estimatedProgress }
        }
        obsLoading = webView.observe(\.isLoading, options: [.new]) { [weak self] wv, _ in
            Task { @MainActor in
                self?.isLoading = wv.isLoading
                self?.canGoBack = wv.canGoBack
                self?.canGoForward = wv.canGoForward
            }
        }
        obsTitle = webView.observe(\.title, options: [.new]) { [weak self] wv, _ in
            Task { @MainActor in
                if let t = wv.title, !t.isEmpty { self?.title = t }
            }
        }
        obsUrl = webView.observe(\.url, options: [.new]) { [weak self] wv, _ in
            Task { @MainActor in
                if let u = wv.url?.absoluteString { self?.urlString = u }
                self?.canGoBack = wv.canGoBack
                self?.canGoForward = wv.canGoForward
            }
        }
        webView.load(URLRequest(url: initialURL))
    }

    func navigate(input: String) {
        let trimmed = input.trimmingCharacters(in: .whitespacesAndNewlines)
        guard !trimmed.isEmpty else { return }
        let url: URL?
        if trimmed.hasPrefix("http://") || trimmed.hasPrefix("https://") {
            url = URL(string: trimmed)
        } else if trimmed.contains(".") && !trimmed.contains(" ") {
            url = URL(string: "https://\(trimmed)")
        } else {
            let encoded = trimmed.addingPercentEncoding(withAllowedCharacters: .urlQueryAllowed) ?? trimmed
            url = URL(string: "https://www.google.com/search?q=\(encoded)")
        }
        if let target = url {
            webView.load(URLRequest(url: target))
        }
    }
}

@MainActor
final class BrowserStore: ObservableObject {
    @Published var tabs: [BrowserTab] = []
    @Published var activeTabId: UUID
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
    @Published var translationError: String?

    private var translationTask: Task<Void, Never>?

    init() {
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

        let first = BrowserTab(initialURL: URL(string: "https://www.google.com/")!)
        self.tabs = [first]
        self.activeTabId = first.id
    }

    var activeTab: BrowserTab {
        tabs.first(where: { $0.id == activeTabId }) ?? tabs[0]
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
    }

    func newTab(urlString: String = "https://www.google.com/") {
        let url = URL(string: urlString) ?? URL(string: "https://www.google.com")!
        let tab = BrowserTab(initialURL: url)
        tabs.append(tab)
        activeTabId = tab.id
    }

    func closeTab(_ id: UUID) {
        guard tabs.count > 1 else { return }
        tabs.removeAll { $0.id == id }
        if !tabs.contains(where: { $0.id == activeTabId }), let first = tabs.first {
            activeTabId = first.id
        }
    }

    private func loadResourceScript(_ name: String) -> String? {
        let url = Bundle.main.url(forResource: name, withExtension: "js")
            ?? Bundle.main.url(forResource: name, withExtension: "js", subdirectory: "Resources")
        guard let url else { return nil }
        return try? String(contentsOf: url, encoding: .utf8)
    }

    func restoreOriginal() {
        translationTask?.cancel()
        isTranslating = false
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
        guard let extractJs = loadResourceScript("content_extract"),
              let applyJs = loadResourceScript("inline_translate") else {
            translationError = "Bundled translation scripts not found."
            return
        }

        let currentProvider = provider
        let currentModel: String = {
            switch currentProvider {
            case .hyMt2: return selectedLocalModel
            case .googleGemini: return geminiModel
            case .openAiCompatible: return openAiModel
            }
        }()
        let currentBase = openAiBaseUrl
        let currentTarget = targetLanguage
        let currentUi = uiLang.rawValue

        translationTask = Task { @MainActor in
            isTranslating = true
            defer { isTranslating = false }

            do {
                let rawResult = try await tab.webView.evaluateJavaScript(extractJs)
                guard let dict = rawResult as? [String: Any],
                      let blocks = dict["blocks"] as? [[String: Any]] else {
                    return
                }
                let segments: [(id: String, text: String)] = blocks.compactMap { b in
                    guard let k = b["key"] as? String, let t = b["text"] as? String, !t.isEmpty else { return nil }
                    return (k, t)
                }
                translationTotal = segments.count
                translationDone = 0
                guard !segments.isEmpty else { return }

                for seg in segments {
                    try Task.checkCancellation()
                    let tr = try await TranslationEngine.shared.translateText(
                        seg.text,
                        provider: currentProvider,
                        modelId: currentModel,
                        baseUrl: currentBase,
                        targetLanguage: currentTarget
                    )
                    translationDone += 1

                    let payload: [[String: Any]] = [[
                        "id": seg.id,
                        "translation": tr,
                        "uncertain": false,
                        "notes": [String](),
                    ]]
                    let jsonData = try JSONSerialization.data(withJSONObject: payload)
                    let jsonStr = String(data: jsonData, encoding: .utf8) ?? "[]"
                    let invokeJs = "(\(applyJs))(\(jsonStr), 1.0, 1.6, \(bilingual ? "true" : "false"), \"\(currentUi)\");"
                    _ = try? await tab.webView.evaluateJavaScript(invokeJs)
                }
                tab.displayMode = bilingual ? .bilingual : .inline
            } catch is CancellationError {
                // Cancelled cleanly
            } catch {
                translationError = error.localizedDescription
            }
        }
    }
}
