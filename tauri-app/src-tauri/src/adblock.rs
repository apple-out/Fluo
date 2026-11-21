const BLOCKED_DOMAINS: &[&str] = &[
    "2mdn.net",
    "ad.namu.live",
    "adcr.naver.com",
    "adfit.kakao.com",
    "adform.net",
    "adnxs.com",
    "adroll.com",
    "adsafeprotected.com",
    "adserver.com",
    "adservice.google.co.kr",
    "adservice.google.com",
    "adsrvr.org",
    "advertising.com",
    "amazon-adsystem.com",
    "bidswitch.net",
    "casalemedia.com",
    "cauly.co.kr",
    "contextweb.com",
    "criteo.com",
    "criteo.net",
    "dable.io",
    "demdex.net",
    "doubleclick.net",
    "fundingchoicesmessages.google.com",
    "gfa.naver.com",
    "googleadservices.com",
    "googlesyndication.com",
    "googletagservices.com",
    "imrworldwide.com",
    "indexww.com",
    "lijit.com",
    "mathtag.com",
    "media.net",
    "mgid.com",
    "moatads.com",
    "mobon.net",
    "openx.net",
    "outbrain.com",
    "pubmatic.com",
    "quantserve.com",
    "realclick.co.kr",
    "revcontent.com",
    "rubiconproject.com",
    "scorecardresearch.com",
    "sharethrough.com",
    "smartadserver.com",
    "taboola.com",
    "teads.tv",
    "widerplanet.com",
    "yieldmo.com",
];

const ANALYTICS_TRACKER_DOMAINS: &[&str] = &[
    "analytics.tiktok.com",
    "bat.bing.com",
    "chartbeat.com",
    "clarity.ms",
    "connect.facebook.net",
    "facebook.net",
    "fullstory.com",
    "google-analytics.com",
    "googletagmanager.com",
    "hotjar.com",
    "logx.optimizely.com",
    "mc.yandex.ru",
    "mixpanel.com",
    "newrelic.com",
    "nr-data.net",
    "omtrdc.net",
    "pixel.wp.com",
    "segment.io",
    "segment.com",
    "sentry.io",
    "stats.wp.com",
];

const STRICT_EXTRA_TRACKER_DOMAINS: &[&str] = &[
    "addthis.com",
    "amplitude.com",
    "branch.io",
    "bugsnag.com",
    "clicktale.net",
    "crazyegg.com",
    "disqusads.com",
    "fingerprintjs.com",
    "fpjs.io",
    "heapanalytics.com",
    "intercom.io",
    "mouseflow.com",
    "onesignal.com",
    "optimizely.com",
    "pendo.io",
    "posthog.com",
    "sharethis.com",
];

const AD_HOST_LABELS: &[&str] = &["ad", "ads", "adserver", "adservice", "banner", "banners"];

#[inline]
fn host_matches_domain_ascii(host_lower: &str, domain: &str) -> bool {
    let hl = host_lower.as_bytes();
    let dl = domain.as_bytes();
    hl == dl || (hl.len() > dl.len() && hl[hl.len() - dl.len() - 1] == b'.' && hl.ends_with(dl))
}

#[inline]
fn hosts_same_site_ascii(a_lower: &str, b_lower: &str) -> bool {
    host_matches_domain_ascii(a_lower, b_lower) || host_matches_domain_ascii(b_lower, a_lower)
}

fn matches_domain_list(host_lower: &str, domains: &[&str]) -> bool {
    domains
        .iter()
        .any(|domain| host_matches_domain_ascii(host_lower, domain))
}

pub fn is_host_allowlisted(page_url: Option<&str>, allowlist: &[String]) -> bool {
    if allowlist.is_empty() {
        return false;
    }
    let Some(raw) = page_url else {
        return false;
    };
    let Ok(parsed) = url::Url::parse(raw) else {
        return false;
    };
    let Some(page_host) = parsed.host_str() else {
        return false;
    };
    let page_host = page_host.to_ascii_lowercase();
    is_parsed_host_allowlisted(&page_host, allowlist)
}

#[inline]
fn is_parsed_host_allowlisted(page_host_lower: &str, allowlist: &[String]) -> bool {
    allowlist.iter().any(|entry| {
        let trimmed = entry.trim();
        if trimmed.is_empty() {
            return false;
        }
        if trimmed.bytes().all(|b| !b.is_ascii_uppercase()) {
            host_matches_domain_ascii(page_host_lower, trimmed)
        } else {
            host_matches_domain_ascii(page_host_lower, &trimmed.to_ascii_lowercase())
        }
    })
}

#[inline]
fn blocks_host_lower(host_lower: &str) -> bool {
    matches_domain_list(host_lower, BLOCKED_DOMAINS)
        || host_lower
            .split('.')
            .any(|label| AD_HOST_LABELS.contains(&label))
}

#[cfg(test)]
fn blocks_url(raw_url: &str) -> bool {
    let Ok(url) = url::Url::parse(raw_url) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let Some(host) = url.host_str() else {
        return false;
    };
    if host.bytes().any(|b| b.is_ascii_uppercase()) {
        blocks_host_lower(&host.to_ascii_lowercase())
    } else {
        blocks_host_lower(host)
    }
}

pub fn blocks_request(
    raw_url: &str,
    page_url: Option<&str>,
    ad_block_enabled: bool,
    ad_block_allowlist: &[String],
    tracking_prevention: &str,
) -> bool {
    if !ad_block_enabled && tracking_prevention == "basic" {
        return false;
    }
    let Ok(url) = url::Url::parse(raw_url) else {
        return false;
    };
    if !matches!(url.scheme(), "http" | "https") {
        return false;
    }
    let Some(host_raw) = url.host_str() else {
        return false;
    };
    let host = host_raw.to_ascii_lowercase();

    let page_host_lower = page_url
        .and_then(|p| url::Url::parse(p).ok())
        .and_then(|p| p.host_str().map(|s| s.to_ascii_lowercase()));

    let is_first_party = page_host_lower
        .as_deref()
        .is_some_and(|ph| hosts_same_site_ascii(&host, ph));

    // Tracking prevention works according to selected strictness level.
    let tracker_blocked = match tracking_prevention {
        "strict" => {
            !is_first_party
                && (matches_domain_list(&host, ANALYTICS_TRACKER_DOMAINS)
                    || matches_domain_list(&host, STRICT_EXTRA_TRACKER_DOMAINS)
                    || host.starts_with("metrics.")
                    || host.starts_with("telemetry.")
                    || host.starts_with("tracker."))
        }
        "balanced" => !is_first_party && matches_domain_list(&host, ANALYTICS_TRACKER_DOMAINS),
        _ => false,
    };

    if tracker_blocked {
        return true;
    }

    if !ad_block_enabled
        || page_host_lower
            .as_deref()
            .is_some_and(|ph| is_parsed_host_allowlisted(ph, ad_block_allowlist))
    {
        return false;
    }

    blocks_host_lower(&host)
}

#[cfg(test)]
mod tests {
    use super::{blocks_request, blocks_url};

    #[test]
    fn blocks_known_ad_networks_and_ad_subdomains() {
        assert!(blocks_url(
            "https://pagead2.googlesyndication.com/pagead/js/ads.js"
        ));
        assert!(blocks_url("https://cdn.ads.example.test/banner.png"));
        assert!(blocks_url("https://www.doubleclick.net/pagead/"));
    }

    #[test]
    fn does_not_block_first_party_or_non_http_resources() {
        assert!(!blocks_url("https://example.org/articles/adventure"));
        assert!(!blocks_url("https://address.example.org/assets/app.js"));
        assert!(!blocks_url("data:text/plain,ads.example"));
        assert!(!blocks_url("not a URL"));
    }

    #[test]
    fn blocks_request_honors_allowlist_and_tracking_modes() {
        let allowlist = vec!["Example.org".to_string()];
        assert!(!blocks_request(
            "https://www.doubleclick.net/ad.js",
            Some("https://sub.example.org/post"),
            true,
            &allowlist,
            "basic"
        ));
        assert!(blocks_request(
            "https://www.google-analytics.com/analytics.js",
            Some("https://example.org/post"),
            false,
            &[],
            "balanced"
        ));
        assert!(blocks_request(
            "https://metrics.thirdparty.test/ping",
            Some("https://example.org/post"),
            false,
            &[],
            "strict"
        ));
        assert!(!blocks_request(
            "https://metrics.example.org/ping",
            Some("https://example.org/post"),
            false,
            &[],
            "strict"
        ));
    }
}
