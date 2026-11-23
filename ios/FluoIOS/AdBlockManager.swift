import Foundation
import WebKit

@MainActor
final class AdBlockManager {
    static let shared = AdBlockManager()
    private var compiledList: WKContentRuleList?
    private let ruleIdentifier = "com.appleout.fluo.adblock.rules.v2"

    private static let blockedDomains: [String] = [
        "doubleclick.net",
        "googlesyndication.com",
        "googleadservices.com",
        "googletagmanager.com",
        "google-analytics.com",
        "adservice.google.com",
        "2mdn.net",
        "adnxs.com",
        "adsrvr.org",
        "taboola.com",
        "outbrain.com",
        "criteo.com",
        "criteo.net",
        "scorecardresearch.com",
        "ads-twitter.com",
        "amazon-adsystem.com",
        "moatads.com",
        "rubiconproject.com",
        "pubmatic.com",
        "openx.net",
        "casalemedia.com",
        "smartadserver.com",
        "teads.tv",
        "quantserve.com",
        "advertising.com",
        "adform.net",
        "bidswitch.net",
        "sharethrough.com",
        "triplelift.com",
        "indexww.com",
        "yieldmo.com",
        "media.net",
        "popads.net",
        "propellerads.com",
        "mgid.com",
        "revcontent.com",
        "ad.namu.live",
        "adcr.naver.com",
        "ader.naver.com",
        "siape.veta.naver.com",
        "adfit.kakao.com",
        "t1.daumcdn.net/kas",
        "dable.io",
        "cauly.co.kr",
        "mobon.net",
        "mediacategory.com",
        "widerplanet.com",
        " realclick.co.kr",
        "adop.cc",
        "exelbid.com",
        "tnkfactory.com"
    ]

    private static var rulesJSON: String {
        var rules: [[String: Any]] = blockedDomains.map { rawDomain in
            let d = rawDomain.trimmingCharacters(in: .whitespacesAndNewlines)
                .replacingOccurrences(of: ".", with: "\\.")
            return [
                "trigger": [
                    "url-filter": "^https?://([a-z0-9-]+\\.)*\(d)([/:?]|$)",
                    "load-type": ["third-party"]
                ],
                "action": ["type": "block"]
            ]
        }
        rules.append([
            "trigger": ["url-filter": "^https?://"],
            "action": [
                "type": "css-display-none",
                "selector": "ins.adsbygoogle, [id^='google_ads_'], [id^='div-gpt-ad'], iframe[src*='doubleclick.net'], iframe[src*='googlesyndication.com'], iframe[src*='adfit.kakao.com'], iframe[src*='ad.namu.live'], .ad_area, .ad-banner, .ad-container, .ad-wrapper, .advertisement, [data-ad-client], [data-ad-slot], [class*='adfit'], [id*='dablewidget']"
            ]
        ])
        guard let data = try? JSONSerialization.data(withJSONObject: rules),
              let json = String(data: data, encoding: .utf8) else {
            return "[]"
        }
        return json
    }

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
            let isAllowed = !cleanHost.isEmpty && allowlist.contains(where: {
                let entry = $0.lowercased()
                return entry == cleanHost || cleanHost.hasSuffix(".\(entry)")
            })
            if enabled && !isAllowed, let rules = await prepareIfNeeded() {
                ucc.add(rules)
            }
        }
    }
}
