use sha2::{Digest, Sha256};

pub fn normalize_whitespace(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_space = true;
    for ch in input.chars() {
        if ch.is_control() && ch != '\n' && ch != '\t' {
            continue;
        }
        if ch.is_whitespace() {
            if !last_space {
                out.push(' ');
                last_space = true;
            }
        } else {
            out.push(ch);
            last_space = false;
        }
    }
    out.trim().to_string()
}

pub fn sha256_hex(input: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(input.as_bytes());
    hex::encode(hasher.finalize())
}

pub fn paragraph_id(text: &str, index: usize) -> String {
    let hash = sha256_hex(&normalize_whitespace(text));
    format!("{}-{index}", &hash[..8])
}

pub fn page_id(url: &str) -> String {
    sha256_hex(url)[..16].to_string()
}

pub fn cache_key(
    source_hash: &str,
    provider: &str,
    model: &str,
    prompt_version: &str,
    style: &str,
    target_language: &str,
    glossary_hash: &str,
) -> String {
    sha256_hex(
        &[
            source_hash,
            provider,
            model,
            prompt_version,
            style,
            target_language,
            glossary_hash,
        ]
        .join("\u{1f}"),
    )
}

fn is_tracking(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key.starts_with("utm_")
        || matches!(
            key.as_str(),
            "fbclid" | "gclid" | "yclid" | "mc_cid" | "mc_eid" | "igshid" | "ref_src"
        )
}

/// Normalized URL used as a page identity for snapshots and translation history.
pub fn normalize_page_url(input: &str) -> Result<String, String> {
    let mut url = url::Url::parse(input).map_err(|error| format!("invalid URL: {error}"))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err("only http and https URLs are supported".into());
    }
    url.set_fragment(None);
    let kept: Vec<(String, String)> = url
        .query_pairs()
        .filter(|(key, _)| !is_tracking(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    if kept.is_empty() {
        url.set_query(None);
    } else {
        let mut pairs = url.query_pairs_mut();
        pairs.clear();
        for (key, value) in kept {
            pairs.append_pair(&key, &value);
        }
    }
    Ok(url.to_string())
}

/// Turns whatever the user typed in the address bar into a navigable URL.
pub fn resolve_address(input: &str, search_engine: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Err("주소를 입력하세요.".into());
    }
    if let Some(index) = trimmed.find("://") {
        let scheme = trimmed[..index].to_ascii_lowercase();
        if !matches!(scheme.as_str(), "http" | "https") {
            return Err("http 또는 https 주소만 열 수 있습니다.".into());
        }
    }
    if let Some(index) = trimmed.find(':') {
        let scheme = trimmed[..index].to_ascii_lowercase();
        if matches!(
            scheme.as_str(),
            "file" | "javascript" | "data" | "about" | "mailto" | "ftp" | "chrome" | "edge"
        ) {
            return Err("http 또는 https 주소만 열 수 있습니다.".into());
        }
    }
    if let Ok(parsed) = url::Url::parse(trimmed) {
        if matches!(parsed.scheme(), "http" | "https") {
            return Ok(parsed.to_string());
        }
    }
    let looks_like_search = trimmed.contains(char::is_whitespace)
        || (!trimmed.contains('.')
            && !trimmed.starts_with("localhost:")
            && !trimmed.starts_with('['));
    if looks_like_search {
        let endpoint = match search_engine {
            "bing" => "https://www.bing.com/search",
            "duckduckgo" => "https://duckduckgo.com/",
            _ => "https://www.google.com/search",
        };
        let mut search = url::Url::parse(endpoint).expect("static URL");
        search.query_pairs_mut().append_pair("q", trimmed);
        return Ok(search.to_string());
    }
    let with_scheme = format!("https://{trimmed}");
    url::Url::parse(&with_scheme)
        .map(|url| url.to_string())
        .map_err(|error| format!("invalid URL: {error}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalizes_whitespace_and_control_characters() {
        assert_eq!(
            normalize_whitespace("  hello\u{0}\n  world\t "),
            "hello world"
        );
    }

    #[test]
    fn adds_scheme_to_bare_hosts() {
        assert_eq!(
            resolve_address("example.org", "google").unwrap(),
            "https://example.org/"
        );
    }

    #[test]
    fn routes_plain_words_to_search() {
        let url = resolve_address("hello world", "google").unwrap();
        assert!(url.starts_with("https://www.google.com/search?q="));
    }

    #[test]
    fn sends_single_word_queries_to_google() {
        assert_eq!(
            resolve_address("weather", "google").unwrap(),
            "https://www.google.com/search?q=weather"
        );
    }

    #[test]
    fn rejects_non_http_schemes() {
        assert!(resolve_address("file:///C:/secret.txt", "google").is_err());
    }

    #[test]
    fn supports_configured_search_engines() {
        assert!(
            resolve_address("hello", "bing")
                .unwrap()
                .starts_with("https://www.bing.com/search?q=")
        );
    }

    #[test]
    fn strips_tracking_parameters() {
        let normalized =
            normalize_page_url("https://Example.org/page?utm_source=x&id=7#section").unwrap();
        assert_eq!(normalized, "https://example.org/page?id=7");
    }

    #[test]
    fn paragraph_ids_are_stable_and_indexed() {
        let first = paragraph_id("同じ 文", 0);
        let second = paragraph_id("同じ 文", 0);
        assert_eq!(first, second);
        assert!(first.ends_with("-0"));
    }
}
