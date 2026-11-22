import Foundation
import WebKit

@MainActor
final class AdBlockManager {
    static let shared = AdBlockManager()
    private var compiledList: WKContentRuleList?
    private let ruleIdentifier = "com.appleout.fluo.adblock.rules"

    private static let rulesJSON: String = """
    [
      {"trigger":{"url-filter":".*","if-domain":["*doubleclick.net","*googlesyndication.com","*googleadservices.com","*googletagmanager.com","*google-analytics.com","*adnxs.com","*taboola.com","*outbrain.com","*criteo.com","*scorecardresearch.com","*ads-twitter.com","*adsystem.com","*amazon-adsystem.com"]},"action":{"type":"block"}}
    ]
    """

    func prepareIfNeeded() async -> WKContentRuleList? {
        if let compiledList { return compiledList }
        guard let store = WKContentRuleListStore.default() else { return nil }
        if let existing = try? await store.contentRuleList(forIdentifier: ruleIdentifier) {
            self.compiledList = existing
            return existing
        }
        if let compiled = try? await store.compileContentRuleList(forIdentifier: ruleIdentifier, encodedContentRuleList: Self.rulesJSON) {
            self.compiledList = compiled
            return compiled
        }
        return nil
    }

    func apply(to webView: WKWebView, enabled: Bool, host: String?, allowlist: [String]) {
        Task { @MainActor in
            let ucc = webView.configuration.userContentController
            ucc.removeAllContentRuleLists()
            let cleanHost = (host ?? "").lowercased()
            let isAllowed = !cleanHost.isEmpty && allowlist.contains(where: { $0.lowercased() == cleanHost })
            if enabled && !isAllowed, let rules = await prepareIfNeeded() {
                ucc.add(rules)
            }
        }
    }
}
