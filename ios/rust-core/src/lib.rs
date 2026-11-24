use std::ffi::{CStr, CString};
use std::fs::File;
use std::io::Read;
use std::num::NonZeroU32;
use std::os::raw::c_char;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, OnceLock};

use llama_cpp_2::context::params::LlamaContextParams;
use llama_cpp_2::llama_backend::LlamaBackend;
use llama_cpp_2::llama_batch::LlamaBatch;
use llama_cpp_2::model::params::LlamaModelParams;
use llama_cpp_2::model::{LlamaChatMessage, LlamaModel};
use llama_cpp_2::sampling::LlamaSampler;
use llama_cpp_2::token::LlamaToken;

struct LoadedModel {
    model_path: String,
    n_ctx: u32,
    backend: Arc<LlamaBackend>,
    model: LlamaModel,
}

static GLOBAL_BACKEND: OnceLock<Result<Arc<LlamaBackend>, String>> = OnceLock::new();
static ACTIVE_MODEL: OnceLock<Mutex<Option<LoadedModel>>> = OnceLock::new();
static LAST_LOAD_ERROR: OnceLock<Mutex<String>> = OnceLock::new();
static CANCEL_REQUESTED: AtomicBool = AtomicBool::new(false);

fn set_last_error(msg: &str) {
    let lock = LAST_LOAD_ERROR.get_or_init(|| Mutex::new(String::new()));
    if let Ok(mut guard) = lock.lock() {
        *guard = msg.to_string();
    }
}

fn get_backend() -> Result<Arc<LlamaBackend>, String> {
    GLOBAL_BACKEND
        .get_or_init(|| {
            LlamaBackend::init()
                .map(Arc::new)
                .map_err(|e| format!("Failed to init llama backend: {e}"))
        })
        .clone()
}

fn active_slot() -> &'static Mutex<Option<LoadedModel>> {
    ACTIVE_MODEL.get_or_init(|| Mutex::new(None))
}

fn cstr_to_string(ptr: *const c_char) -> Option<String> {
    if ptr.is_null() {
        return None;
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_str()
        .ok()
        .map(str::to_string)
}

fn json_ok(translation: &str) -> *mut c_char {
    let payload = serde_json::json!({ "ok": true, "translation": translation }).to_string();
    CString::new(payload)
        .unwrap_or_else(|_| CString::new(r#"{"ok":false,"error":"CString nul byte"}"#).unwrap())
        .into_raw()
}

fn json_err(message: &str) -> *mut c_char {
    let payload = serde_json::json!({ "ok": false, "error": message }).to_string();
    CString::new(payload)
        .unwrap_or_else(|_| CString::new(r#"{"ok":false,"error":"CString nul byte"}"#).unwrap())
        .into_raw()
}

fn normalize_target_lang(lang: &str) -> String {
    let trimmed = lang.trim();
    match trimmed.to_lowercase().as_str() {
        "ko" | "kor" | "korean" | "한국어" => "Korean".into(),
        "en" | "eng" | "english" | "영어" => "English".into(),
        "ja" | "jpn" | "japanese" | "일본어" | "日本語" => "Japanese".into(),
        "zh" | "zh-cn" | "zh-hans" | "chinese" | "중국어" | "中文" | "中文(简体)"
        | "중국어(간체)" | "简体中文" => "Chinese".into(),
        "zh-tw" | "zh-hant" | "중국어(번체)" | "번체중국어" | "繁體中文" | "中文(繁體)" => {
            "Traditional Chinese".into()
        }
        "yue" | "cantonese" | "광둥어" | "粵語" => "Cantonese".into(),
        "fr" | "fra" | "french" | "프랑스어" | "français" => "French".into(),
        "de" | "deu" | "german" | "독일어" | "deutsch" => "German".into(),
        "es" | "spa" | "spanish" | "스페인어" | "español" => "Spanish".into(),
        "pt" | "por" | "portuguese" | "포르투갈어" | "português" => "Portuguese".into(),
        "it" | "ita" | "italian" | "이탈리아어" | "italiano" => "Italian".into(),
        "ru" | "rus" | "russian" | "러시아어" | "русский" => "Russian".into(),
        "ar" | "ara" | "arabic" | "아랍어" | "العربية" => "Arabic".into(),
        "vi" | "vie" | "vietnamese" | "베트남어" | "tiếng việt" => "Vietnamese".into(),
        "th" | "tha" | "thai" | "태국어" | "ไทย" => "Thai".into(),
        "id" | "ind" | "indonesian" | "인도네시아어" | "bahasa indonesia" => {
            "Indonesian".into()
        }
        "hi" | "hin" | "hindi" | "힌디어" | "हिन्दी" => "Hindi".into(),
        "tr" | "tur" | "turkish" | "터키어" | "튀르키예어" | "türkçe" => "Turkish".into(),
        "pl" | "pol" | "polish" | "폴란드어" | "polski" => "Polish".into(),
        "nl" | "nld" | "dutch" | "네덜란드어" | "nederlands" => "Dutch".into(),
        "uk" | "ukr" | "ukrainian" | "우크라이나어" | "українська" => {
            "Ukrainian".into()
        }
        "cs" | "ces" | "czech" | "체코어" | "čeština" => "Czech".into(),
        "fa" | "fas" | "persian" | "페르시아어" | "فارسی" => "Persian".into(),
        "he" | "heb" | "hebrew" | "히브리어" | "עברית" => "Hebrew".into(),
        "ms" | "msa" | "malay" | "말레이어" | "bahasa melayu" => "Malay".into(),
        "tl" | "fil" | "filipino" | "tagalog" | "필리핀어" | "타갈로그어" => {
            "Filipino".into()
        }
        "bo" | "tibetan" | "티베트어" => "Tibetan".into(),
        "mn" | "mongolian" | "몽골어" => "Mongolian".into(),
        "ug" | "uyghur" | "위구르어" => "Uyghur".into(),
        "kk" | "kazakh" | "카자흐어" => "Kazakh".into(),
        "km" | "khmer" | "크메르어" => "Khmer".into(),
        "my" | "burmese" | "미얀마어" => "Burmese".into(),
        "" => "Korean".into(),
        _ => trimmed.to_string(),
    }
}

fn hy_mt2_zh_language_name(norm_lang: &str) -> &str {
    match norm_lang {
        "Korean" => "韩语",
        "English" => "英语",
        "Japanese" => "日语",
        "Chinese" => "中文",
        "Traditional Chinese" => "繁体中文",
        "Cantonese" => "粤语",
        "French" => "法语",
        "German" => "德语",
        "Spanish" => "西班牙语",
        "Portuguese" => "葡萄牙语",
        "Italian" => "意大利语",
        "Russian" => "俄语",
        "Arabic" => "阿拉伯语",
        "Vietnamese" => "越南语",
        "Thai" => "泰语",
        "Indonesian" => "印度尼西亚语",
        "Hindi" => "印地语",
        "Turkish" => "土耳其语",
        "Polish" => "波兰语",
        "Dutch" => "荷兰语",
        "Ukrainian" => "乌克兰语",
        "Czech" => "捷克语",
        "Persian" => "波斯语",
        "Hebrew" => "希伯来语",
        "Malay" => "马来语",
        "Filipino" => "菲律宾语",
        "Tibetan" => "藏语",
        "Mongolian" => "蒙古语",
        "Uyghur" => "维吾尔语",
        "Kazakh" => "哈萨克语",
        "Khmer" => "高棉语",
        "Burmese" => "缅甸语",
        _ => norm_lang,
    }
}

fn has_cjk_or_kana(text: &str) -> bool {
    text.chars().any(|c| {
        ('\u{3040}'..='\u{30FF}').contains(&c)
            || ('\u{4E00}'..='\u{9FFF}').contains(&c)
            || ('\u{3400}'..='\u{4DBF}').contains(&c)
    })
}

fn source_is_chinese(source_text: &str) -> bool {
    let mut hanzi = 0usize;
    for ch in source_text.chars() {
        match ch {
            '\u{AC00}'..='\u{D7A3}' | '\u{3131}'..='\u{318E}' => return false,
            '\u{3040}'..='\u{30FF}' | '\u{FF65}'..='\u{FF9F}' => return false,
            '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' => hanzi += 1,
            _ => {}
        }
    }
    hanzi > 0
}

fn is_dialogue_segment(text: &str) -> bool {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return false;
    }
    let is_cjk = has_cjk_or_kana(trimmed);
    const QUOTE_PAIRS: [(char, char); 6] = [
        ('「', '」'),
        ('『', '』'),
        ('“', '”'),
        ('"', '"'),
        ('〝', '〟'),
        ('（', '）'),
    ];
    if QUOTE_PAIRS
        .iter()
        .any(|&(open, close)| trimmed.starts_with(open) && trimmed.ends_with(close))
        || (is_cjk && trimmed.starts_with('(') && trimmed.ends_with(')'))
    {
        return true;
    }
    const OPEN_QUOTES: [char; 6] = ['「', '『', '“', '"', '〝', '（'];
    let lines: Vec<&str> = trimmed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() > 1 {
        let dialogue_lines = lines
            .iter()
            .filter(|l| {
                l.starts_with(OPEN_QUOTES)
                    || (is_cjk && l.starts_with('('))
                    || l.starts_with('—')
                    || l.starts_with('-')
            })
            .count();
        if dialogue_lines * 2 >= lines.len() {
            return true;
        }
    }
    false
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum DialogueRegister {
    Polite,
    Casual,
    Neutral,
}

fn detect_dialogue_register(text: &str) -> DialogueRegister {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return DialogueRegister::Neutral;
    }
    const JA_POLITE: &[&str] = &[
        "ございます",
        "いたします",
        "おります",
        "存じます",
        "くださいませ",
        "ください",
        "お願い",
        "申し訳",
        "恐れ入り",
        "失礼",
        "ませんか",
        "ましょう",
        "でしょう",
        "ですね",
        "ですよ",
        "ますね",
        "ますよ",
        "ますか",
        "ですか",
        "ました",
        "ません",
        "です。",
        "ます。",
        "です」",
        "ます」",
        "です』",
        "ます』",
        "です”",
        "ます”",
        "です？",
        "ます？",
        "です！",
        "ます！",
        "です…",
        "ます…",
        "様",
        "先輩",
        "先生",
        "閣下",
        "殿下",
    ];
    const JA_CASUAL: &[&str] = &[
        "だぜ",
        "だぞ",
        "だろ",
        "だろう",
        "じゃん",
        "じゃねえ",
        "じゃねー",
        "てめえ",
        "てめぇ",
        "お前",
        "あんた",
        "貴様",
        "おい、",
        "おい ",
        "ちっ",
        "くそ",
        "クソ",
        "ふざけ",
        "俺は",
        "俺が",
        "俺を",
        "俺の",
        "あたし",
        "わよ",
        "のよ",
        "なの？",
        "るの？",
        "んだよ",
        "んだぜ",
        "なんだな",
        "んだな",
        "のだな",
        "だな」",
        "だな』",
        "だな。",
        "てくれ",
        "でくれ",
        "くれ」",
        "あるんだ",
        "なんだ？",
        "のか？",
        "出た？",
        "覚えておけ",
        "事か）",
        "ことか）",
        "……ふむ",
        "「ああ」",
        "うん。",
        "うん、",
        "まかせて",
        "だろ？",
        "なのか",
        "するぞ",
    ];
    let polite_hits = JA_POLITE
        .iter()
        .filter(|pat| trimmed.contains(**pat))
        .count();
    let casual_hits = JA_CASUAL
        .iter()
        .filter(|pat| trimmed.contains(**pat))
        .count();

    let has_ja_kana = trimmed
        .chars()
        .any(|c| ('\u{3040}'..='\u{309F}').contains(&c) || ('\u{30A0}'..='\u{30FF}').contains(&c));
    if has_ja_kana {
        if polite_hits > 0 && polite_hits >= casual_hits {
            return DialogueRegister::Polite;
        }
        return DialogueRegister::Casual;
    }

    let lower = trimmed.to_ascii_lowercase();
    const EN_POLITE: &[&str] = &[
        "excuse me",
        "pardon me",
        "thank you",
        "please,",
        "please ",
        "i apologize",
        "my lord",
        "my lady",
        "your highness",
        "your majesty",
        "mr. ",
        "ms. ",
        "mrs. ",
        "professor ",
        ", sir",
        " sir.",
        " ma'am",
    ];
    const EN_CASUAL: &[&str] = &[
        "gonna ",
        "wanna ",
        "gotta ",
        "ain't ",
        "yeah,",
        "yeah ",
        "nope",
        "shut up",
        "damn",
        "crap",
        "what the hell",
        "hey!",
        "hey,",
        "buddy",
        "dude",
    ];
    let en_polite_hits = EN_POLITE.iter().filter(|pat| lower.contains(**pat)).count();
    let en_casual_hits = EN_CASUAL.iter().filter(|pat| lower.contains(**pat)).count();

    let total_polite = polite_hits + en_polite_hits;
    let total_casual = casual_hits + en_casual_hits;
    if total_polite > total_casual {
        DialogueRegister::Polite
    } else if total_casual > total_polite {
        DialogueRegister::Casual
    } else {
        DialogueRegister::Neutral
    }
}

fn hy_mt2_style_tag(style: &str, source_text: &str, is_dialogue: bool) -> Option<&'static str> {
    match style.trim().to_ascii_lowercase().as_str() {
        "literal" => {
            Some("faithful literal translation preserving original meaning and sentence structure")
        }
        "auto:casual" => {
            if is_dialogue {
                Some("natural casual dialogue")
            } else {
                None
            }
        }
        "novel" | "auto:novel" => {
            if is_dialogue {
                match detect_dialogue_register(source_text) {
                    DialogueRegister::Polite => {
                        Some("natural polite colloquial dialogue preserving respectful tone")
                    }
                    DialogueRegister::Casual => Some("natural casual informal colloquial dialogue"),
                    DialogueRegister::Neutral => Some("natural colloquial dialogue"),
                }
            } else {
                None
            }
        }
        _ => {
            if is_dialogue {
                Some("natural colloquial dialogue")
            } else {
                None
            }
        }
    }
}

fn find_matching_double_bracket_close(s: &str) -> Option<usize> {
    let bytes = s.as_bytes();
    let mut depth = 0usize;
    let mut idx = 0usize;
    while idx < bytes.len() {
        if bytes[idx] == b'[' {
            depth += 1;
            idx += 1;
        } else if bytes[idx] == b']' {
            if depth > 0 {
                depth -= 1;
                idx += 1;
            } else if idx + 1 < bytes.len() && bytes[idx + 1] == b']' {
                return Some(idx);
            } else {
                idx += 1;
            }
        } else {
            idx += 1;
        }
    }
    None
}

fn unwrap_inline_markers_for_shortcut(source_text: &str) -> String {
    if !source_text.contains("[[") || !source_text.contains("]]") {
        return source_text.to_string();
    }
    let mut out = String::with_capacity(source_text.len());
    let mut rest = source_text;
    while let Some(open) = rest.find("[[") {
        out.push_str(&rest[..open]);
        let after_open = &rest[open + 2..];
        if let Some(close) = find_matching_double_bracket_close(after_open) {
            let inner = &after_open[..close];
            if inner.starts_with("img")
                && inner.len() > 3
                && inner[3..].chars().all(|c| c.is_ascii_digit())
            {
                // Skip standalone [[imgN]] placeholders
            } else if let Some((id_str, body)) = inner.split_once('|') {
                if !id_str.trim().is_empty() && id_str.trim().chars().all(|c| c.is_ascii_digit()) {
                    out.push_str(body);
                } else {
                    out.push_str(&rest[open..open + 2 + close + 2]);
                }
            } else {
                out.push_str(&rest[open..open + 2 + close + 2]);
            }
            rest = &after_open[close + 2..];
        } else {
            out.push_str(&rest[open..]);
            rest = "";
            break;
        }
    }
    out.push_str(rest);
    out
}

fn silent_ellipsis_shortcut(source_text: &str) -> Option<String> {
    let trimmed = source_text.trim();
    match trimmed {
        "「……」" | "「…」" | "「・・・・・・」" | "「・・・」" | "\"...\"" | "“……”" => {
            Some("“……”".to_string())
        }
        "『……』" | "『…』" | "（……）" | "（…）" | "(...)" | "(……)" => {
            Some("‘……’".to_string())
        }
        "……" | "…" => Some("……".to_string()),
        _ => None,
    }
}

fn non_translatable_shortcut(source_text: &str) -> Option<String> {
    let trimmed = source_text.trim();
    if trimmed.is_empty() {
        return Some(source_text.to_string());
    }
    let unwrapped_buf;
    let check = if trimmed.contains("[[") && trimmed.contains("]]") {
        unwrapped_buf = unwrap_inline_markers_for_shortcut(trimmed);
        unwrapped_buf.trim()
    } else {
        trimmed
    };
    if check.is_empty() {
        return Some(source_text.to_string());
    }
    if !check.chars().any(|c| c.is_alphabetic()) {
        return Some(source_text.to_string());
    }
    if !check.contains(char::is_whitespace) {
        let lower = check.to_ascii_lowercase();
        if lower.starts_with("http://")
            || lower.starts_with("https://")
            || lower.starts_with("mailto:")
            || lower.starts_with("ftp://")
        {
            return Some(source_text.to_string());
        }
        if let Some(hex_body) = lower.strip_prefix("0x") {
            if !hex_body.is_empty() && hex_body.chars().all(|c| c.is_ascii_hexdigit()) {
                return Some(source_text.to_string());
            }
        }
        if check.contains('_')
            && check
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | ':' | '/'))
        {
            return Some(source_text.to_string());
        }
    }
    None
}

fn already_in_target_language_shortcut(source_text: &str, norm_target: &str) -> Option<String> {
    let trimmed = source_text.trim();
    if trimmed.is_empty() {
        return Some(source_text.to_string());
    }
    let mut hangul = 0usize;
    let mut kana = 0usize;
    let mut hanzi = 0usize;
    let mut lower_latin = 0usize;
    let mut cyrillic = 0usize;
    let mut arabic = 0usize;
    let mut thai = 0usize;
    for ch in trimmed.chars() {
        match ch {
            '\u{AC00}'..='\u{D7A3}' | '\u{3131}'..='\u{318E}' => hangul += 1,
            '\u{3040}'..='\u{309F}' | '\u{30A0}'..='\u{30FF}' | '\u{FF65}'..='\u{FF9F}' => {
                kana += 1;
            }
            '\u{4E00}'..='\u{9FFF}' | '\u{3400}'..='\u{4DBF}' => hanzi += 1,
            'a'..='z' => lower_latin += 1,
            '\u{0400}'..='\u{04FF}' => cyrillic += 1,
            '\u{0600}'..='\u{06FF}' => arabic += 1,
            '\u{0E00}'..='\u{0E7F}' => thai += 1,
            _ => {}
        }
    }
    let is_native = match norm_target {
        "Korean" => {
            hangul >= 2
                && kana == 0
                && hanzi == 0
                && lower_latin == 0
                && cyrillic == 0
                && arabic == 0
                && thai == 0
        }
        "Japanese" => {
            kana >= 2
                && hangul == 0
                && lower_latin == 0
                && cyrillic == 0
                && arabic == 0
                && thai == 0
        }
        "Thai" => {
            thai >= 2
                && hangul == 0
                && kana == 0
                && hanzi == 0
                && lower_latin == 0
                && cyrillic == 0
                && arabic == 0
        }
        "Arabic" => {
            arabic >= 2
                && hangul == 0
                && kana == 0
                && hanzi == 0
                && lower_latin == 0
                && cyrillic == 0
                && thai == 0
        }
        _ => false,
    };
    if is_native {
        Some(source_text.to_string())
    } else {
        None
    }
}

fn shortcut_translation(source_text: &str, norm_target: &str) -> Option<String> {
    if source_text.trim().is_empty() {
        return Some(source_text.to_string());
    }
    if let Some(silent) = silent_ellipsis_shortcut(source_text) {
        return Some(silent);
    }
    if let Some(passthrough) = non_translatable_shortcut(source_text) {
        return Some(passthrough);
    }
    if let Some(native) = already_in_target_language_shortcut(source_text, norm_target) {
        return Some(native);
    }
    None
}

fn segment_needs_line_split(source_text: &str) -> bool {
    source_text.chars().count() > 320 && source_text.lines().count() > 2
}

fn is_western_abbreviation_before_dot(chars: &[(usize, char)], dot_idx: usize) -> bool {
    if dot_idx == 0 {
        return false;
    }
    if dot_idx + 1 < chars.len()
        && chars[dot_idx - 1].1.is_ascii_digit()
        && chars[dot_idx + 1].1.is_ascii_digit()
    {
        return true;
    }
    let mut k = dot_idx;
    while k > 0 && chars[k - 1].1.is_ascii_alphanumeric() {
        k -= 1;
    }
    let word: String = chars[k..dot_idx].iter().map(|(_, c)| *c).collect();
    if word.is_empty() {
        return false;
    }
    if word.len() == 1 && word.chars().all(|c| c.is_ascii_alphabetic()) {
        return true;
    }
    matches!(
        word.to_ascii_lowercase().as_str(),
        "mr" | "mrs"
            | "ms"
            | "dr"
            | "prof"
            | "sr"
            | "jr"
            | "st"
            | "vs"
            | "etc"
            | "e.g"
            | "i.e"
            | "no"
            | "inc"
            | "ltd"
            | "corp"
            | "vol"
            | "ch"
            | "fig"
            | "al"
            | "jan"
            | "feb"
            | "mar"
            | "apr"
            | "aug"
            | "sep"
            | "sept"
            | "oct"
            | "nov"
            | "dec"
    )
}

fn split_long_paragraph_sentences(text: &str) -> Option<Vec<String>> {
    if text.chars().count() <= 300 || text.contains('\n') || is_dialogue_segment(text) {
        return None;
    }
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    let track_ascii_quotes = text.chars().filter(|&c| c == '"').count() % 2 == 0;
    let mut bracket_depth = 0usize;
    let mut paren_depth = 0usize;
    let mut in_ascii_quote = false;
    let mut cjk_quote_depth = 0usize;
    let mut raw_sentences: Vec<String> = Vec::new();
    let mut sent_start_byte = 0usize;
    let mut i = 0usize;

    while i < chars.len() {
        let (_, ch) = chars[i];
        match ch {
            '[' => bracket_depth += 1,
            ']' => bracket_depth = bracket_depth.saturating_sub(1),
            '(' => paren_depth += 1,
            ')' => paren_depth = paren_depth.saturating_sub(1),
            '"' if track_ascii_quotes => in_ascii_quote = !in_ascii_quote,
            '“' | '「' | '『' => cjk_quote_depth += 1,
            '”' | '」' | '』' => cjk_quote_depth = cjk_quote_depth.saturating_sub(1),
            '。' | '！' | '？' | '.' | '!' | '?'
                if bracket_depth == 0
                    && paren_depth == 0
                    && !in_ascii_quote
                    && cjk_quote_depth == 0 =>
            {
                let is_cjk_end = matches!(ch, '。' | '！' | '？');
                if ch == '.' && is_western_abbreviation_before_dot(&chars, i) {
                    i += 1;
                    continue;
                }
                let mut end_idx = i + 1;
                let mut end_byte = if end_idx < chars.len() {
                    chars[end_idx].0
                } else {
                    text.len()
                };
                while end_byte < text.len() && text[end_byte..].starts_with("[[") {
                    if let Some(rel_close) =
                        find_matching_double_bracket_close(&text[end_byte + 2..])
                    {
                        let inner = &text[end_byte + 2..end_byte + 2 + rel_close];
                        let is_footnote = inner
                            .split_once('|')
                            .map(|(_, rhs)| rhs.trim_start().starts_with('['))
                            .unwrap_or(false)
                            || inner.starts_with("img");
                        if is_footnote {
                            end_byte = end_byte + 2 + rel_close + 2;
                            while end_idx < chars.len() && chars[end_idx].0 < end_byte {
                                end_idx += 1;
                            }
                            continue;
                        }
                    }
                    break;
                }

                let mut ws_idx = end_idx;
                while ws_idx < chars.len() && chars[ws_idx].1.is_whitespace() {
                    ws_idx += 1;
                }
                if (is_cjk_end || ws_idx > end_idx) && ws_idx < chars.len() {
                    let next_byte = chars[ws_idx].0;
                    let next_ch = chars[ws_idx].1;
                    let next_starts_sentence = next_ch.is_uppercase()
                        || next_ch.is_ascii_digit()
                        || matches!(next_ch, '"' | '“' | '\'' | '‘' | '「' | '『')
                        || text[next_byte..].starts_with("[[")
                        || !next_ch.is_ascii();
                    if next_starts_sentence {
                        let piece = text[sent_start_byte..end_byte].trim();
                        if !piece.is_empty() {
                            raw_sentences.push(piece.to_string());
                            sent_start_byte = next_byte;
                            i = ws_idx;
                            continue;
                        }
                    }
                }
            }
            _ => {}
        }
        i += 1;
    }

    if sent_start_byte < text.len() {
        let tail = text[sent_start_byte..].trim();
        if !tail.is_empty() {
            raw_sentences.push(tail.to_string());
        }
    }
    if raw_sentences.len() < 2 {
        return None;
    }

    let mut grouped: Vec<String> = Vec::with_capacity(raw_sentences.len());
    for sent in raw_sentences {
        let s_len = sent.chars().count();
        if let Some(prev) = grouped.last_mut() {
            let p_len = prev.chars().count();
            if (p_len < 150 && p_len + 1 + s_len <= 300) || s_len < 50 {
                if !prev.ends_with(['。', '！', '？']) {
                    prev.push(' ');
                }
                prev.push_str(&sent);
                continue;
            }
        }
        grouped.push(sent);
    }

    if grouped.len() >= 2 {
        Some(grouped)
    } else {
        None
    }
}

fn join_translated_sentences(parts: &[String], norm_target: &str) -> String {
    let sep = if matches!(norm_target, "Japanese" | "Chinese" | "Traditional Chinese") {
        ""
    } else {
        " "
    };
    let mut joined = String::new();
    for part in parts {
        let trimmed = part.trim();
        if trimmed.is_empty() {
            continue;
        }
        if !joined.is_empty() {
            joined.push_str(sep);
        }
        joined.push_str(trimmed);
    }
    joined
}

fn build_prompt_ext(
    model: &LlamaModel,
    source: &str,
    norm_target: &str,
    style: &str,
    glossary_hint: &str,
    omit_marker_clause: bool,
) -> (String, bool) {
    let tmpl_raw = model
        .meta_val_str("tokenizer.chat_template")
        .unwrap_or_default();
    let uses_hy_template =
        tmpl_raw.is_empty() || tmpl_raw.contains("hy_User") || tmpl_raw.contains("hy_Assistant");

    let is_dialogue = is_dialogue_segment(source);
    let style_clause = hy_mt2_style_tag(style, source, is_dialogue)
        .map(|tag| format!(" Style: [{tag}]."))
        .unwrap_or_default();
    let is_zh_pair =
        matches!(norm_target, "Chinese" | "Traditional Chinese") || source_is_chinese(source);
    let use_zh_template = is_zh_pair && style_clause.is_empty();

    let mut sections: Vec<String> = Vec::new();
    if !glossary_hint.trim().is_empty() {
        let mut lines = Vec::new();
        lines.push(if use_zh_template {
            "参考下面的翻译：".to_string()
        } else {
            "Reference the following translations:".to_string()
        });
        for raw_line in glossary_hint.lines().take(12) {
            let trimmed = raw_line.trim();
            if trimmed.is_empty() {
                continue;
            }
            if let Some((src, tgt)) = trimmed.split_once("->") {
                let s = src.trim();
                let mut t = tgt.trim();
                if let Some(pos) = t.find("(do NOT translate") {
                    t = t[..pos].trim();
                }
                if use_zh_template {
                    lines.push(format!("{s} 翻译成 {t}"));
                } else {
                    lines.push(format!("\"{s}\" translates to \"{t}\""));
                }
            } else {
                lines.push(trimmed.to_string());
            }
        }
        sections.push(lines.join("\n"));
    }

    let has_markers = !omit_marker_clause && source.contains("[[") && source.contains("]]");
    let has_link_markers = has_markers && source.contains('|');
    let has_img_markers = !omit_marker_clause && source.contains("[[img");

    if use_zh_template {
        let zh_lang = hy_mt2_zh_language_name(norm_target);
        let marker_clause_zh = if has_link_markers && has_img_markers {
            "，原样保留 [[0|...]] 和 [[img0]] 标记"
        } else if has_link_markers {
            "，原样保留 [[0|...]] 标记"
        } else if has_img_markers {
            "，原样保留 [[img0]] 标记"
        } else if has_markers {
            "，原样保留 [[0|...]] 标记"
        } else {
            ""
        };
        sections.push(format!(
            "将以下文本翻译为{zh_lang}{marker_clause_zh}，注意只需要输出翻译后的结果，不要额外解释：\n\n{source}"
        ));
    } else {
        let marker_clause = if has_link_markers && has_img_markers {
            ", preserving [[0|...]] and [[img0]] markers intact"
        } else if has_link_markers {
            ", preserving [[0|...]] markers intact"
        } else if has_img_markers {
            ", preserving [[img0]] markers intact"
        } else if has_markers {
            ", preserving [[0|...]] markers intact"
        } else {
            ""
        };
        sections.push(format!(
            "Translate the following segment into {norm_target}{marker_clause}, without additional explanation.{style_clause}\n\n{source}"
        ));
    }

    let user_prompt = sections.join("\n\n");
    if uses_hy_template {
        return (
            format!("<｜hy_begin▁of▁sentence｜><｜hy_User｜>{user_prompt}<｜hy_Assistant｜>"),
            true,
        );
    }
    if let Ok(tmpl) = model.chat_template(None) {
        if let Ok(msg) = LlamaChatMessage::new("user".to_string(), user_prompt.clone()) {
            if let Ok(rendered) = model.apply_chat_template(&tmpl, &[msg], true) {
                if !rendered.trim().is_empty() {
                    return (rendered, false);
                }
            }
        }
    }
    (
        format!("<｜hy_begin▁of▁sentence｜><｜hy_User｜>{user_prompt}<｜hy_Assistant｜>"),
        true,
    )
}

#[inline]
fn has_stop_marker(output: &str) -> bool {
    output.contains("<｜hy_place▁holder▁no▁2｜>")
        || output.contains("<｜hy_EOT｜>")
        || output.contains("<|im_end|>")
        || output.contains("[|endofturn|]")
        || output.contains("<|eot_id|>")
}

#[inline]
fn piece_might_complete_stop_marker(piece: &[u8]) -> bool {
    piece
        .iter()
        .any(|&b| matches!(b, b'<' | b'>' | b'[' | b']'))
}

struct RepetitionGuard {
    tokens: Vec<LlamaToken>,
    byte_starts: Vec<usize>,
}

impl RepetitionGuard {
    fn new() -> Self {
        Self {
            tokens: Vec::with_capacity(64),
            byte_starts: Vec::with_capacity(64),
        }
    }

    fn push_and_check(&mut self, token: LlamaToken, pre_append_byte_len: usize) -> Option<usize> {
        self.tokens.push(token);
        self.byte_starts.push(pre_append_byte_len);
        let n = self.tokens.len();

        if n >= 12 && self.tokens[n - 12..n].iter().all(|&t| t == token) {
            return Some(self.byte_starts[n - 11]);
        }

        for p in 2..=12usize {
            let reps = if p <= 3 { 4 } else { 3 };
            let span = p * reps;
            if n < span {
                continue;
            }
            let tail_start = n - span;
            let pat = &self.tokens[n - p..n];
            let mut matched = true;
            for r in 0..(reps - 1) {
                let slice = &self.tokens[tail_start + r * p..tail_start + (r + 1) * p];
                if slice != pat {
                    matched = false;
                    break;
                }
            }
            if matched {
                return Some(self.byte_starts[tail_start + p]);
            }
        }

        if self.tokens.len() > 128 {
            self.tokens.drain(..64);
            self.byte_starts.drain(..64);
        }
        None
    }
}

fn estimate_max_output_tokens_for_prompt(
    source_token_len: u32,
    source_text: &str,
    norm_target: &str,
    ctx_size: u32,
) -> u32 {
    let has_hanzi = source_text
        .chars()
        .any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c) || ('\u{3400}'..='\u{4DBF}').contains(&c));
    let target_is_cjk = matches!(
        norm_target,
        "Korean" | "Japanese" | "Chinese" | "Traditional Chinese"
    );
    let est = if has_hanzi && !target_is_cjk {
        source_token_len.saturating_mul(3).saturating_add(56)
    } else if source_token_len <= 24 {
        source_token_len.saturating_mul(2).saturating_add(48)
    } else {
        source_token_len
            .saturating_mul(9)
            .saturating_div(4)
            .saturating_add(64)
    };
    est.clamp(64, ctx_size.saturating_sub(256).max(64))
}

fn clean_translation_output(raw: &str) -> String {
    let mut s = raw
        .replace("<｜hy_place▁holder▁no▁2｜>", "")
        .replace("<｜hy_EOT｜>", "")
        .replace("<｜hy_Assistant｜>", "")
        .replace("<｜hy_begin▁of▁sentence｜>", "")
        .replace("<｜hy_end▁of▁sentence｜>", "")
        .replace("<|im_end|>", "")
        .replace("<|eot_id|>", "")
        .replace("<end_of_turn>", "")
        .replace("[|endofturn|]", "");
    if let Some(end_think) = s.find("</think>") {
        s = s[end_think + "</think>".len()..].to_string();
    }
    for stop in ["<｜hy_User｜>", "<｜end▁of▁sentence｜>", "<|endoftext|>"] {
        if let Some(pos) = s.find(stop) {
            s = s[..pos].to_string();
        }
    }
    for marker in [
        "[출처 텍스트]",
        "[Source Text]",
        "[원문 텍스트]",
        "[原文]",
        "[번역 결과]",
        "[번역문]",
        "[译文]",
    ] {
        if let Some(pos) = s.rfind(marker) {
            let after = s[pos + marker.len()..].trim();
            if !after.is_empty() {
                s = after.to_string();
                break;
            }
        }
    }
    let mut kept = Vec::new();
    let mut last_arrow_rhs: Option<String> = None;
    for line in s.lines() {
        let mut l = line.trim();
        let lower = l.to_ascii_lowercase();
        if lower.starts_with("style:")
            || lower.starts_with("note:")
            || lower.starts_with("translate the following segment")
            || lower.starts_with("output only")
        {
            continue;
        }
        for header in [
            "[Source Text]",
            "[출처 텍스트]",
            "[원문 텍스트]",
            "[원문]",
            "[原文]",
            "[Background Information]",
            "[배경 정보]",
            "[背景信息]",
            "[Translation]",
            "[번역 결과]",
            "[번역문]",
            "[译文]",
            "Translation:",
            "Translate into Korean:",
            "Korean translation:",
            "English translation:",
            "Japanese translation:",
            "Chinese translation:",
            "译文：",
            "译文:",
            "翻译结果：",
            "翻译结果:",
            "日本語訳：",
            "日本語訳:",
        ] {
            if let Some(rest) = l.strip_prefix(header) {
                l = rest.trim();
            }
        }
        if l.is_empty() {
            continue;
        }
        if l.contains(" -> ") || l.contains(" → ") {
            if let Some((_, rhs)) = l.rsplit_once(" -> ").or_else(|| l.rsplit_once(" → ")) {
                let rhs_clean = rhs.trim();
                if !rhs_clean.is_empty() && !rhs_clean.contains("[배경 정보]") {
                    last_arrow_rhs = Some(rhs_clean.to_string());
                }
            }
            continue;
        }
        kept.push(l.to_string());
    }
    if kept.is_empty() {
        if let Some(fallback_rhs) = last_arrow_rhs {
            fallback_rhs
        } else {
            s.trim().to_string()
        }
    } else {
        kept.join("\n").trim().to_string()
    }
}

fn strip_hallucinated_marker_urls(source_text: &str, out: &mut String) {
    if source_text.contains("http://") || source_text.contains("https://") {
        return;
    }
    if !out.contains("http://") && !out.contains("https://") {
        return;
    }
    for pat in ["](https://", "](http://", "]https://", "]http://"] {
        while let Some(pos) = out.find(pat) {
            let url_start = pos + 1;
            let rest = &out[url_start..];
            let starts_with_paren = rest.starts_with('(');
            let scan_from = if starts_with_paren { 1 } else { 0 };
            let rel_end = rest[scan_from..]
                .find(|c: char| {
                    c.is_whitespace()
                        || matches!(
                            c,
                            ')' | ']' | '」' | '』' | '"' | '\'' | '’' | '”' | '<' | '>'
                        )
                        || !c.is_ascii()
                })
                .map(|i| scan_from + i)
                .unwrap_or(rest.len());
            let mut remove_end = url_start + rel_end;
            if rest[rel_end..].starts_with(')') {
                if starts_with_paren {
                    remove_end += 1;
                } else {
                    let before = &out[..url_start];
                    let opens = before.matches('(').count();
                    let closes = before.matches(')').count();
                    if opens <= closes {
                        remove_end += 1;
                    }
                }
            }
            out.replace_range(url_start..remove_end, "");
        }
    }
}

fn is_literal_code_marker_inner(inner: &str) -> bool {
    let t = inner.trim();
    if t.is_empty() || t.contains(char::is_whitespace) {
        return false;
    }
    if !t
        .chars()
        .all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.' | '/' | ':' | '\\'))
    {
        return false;
    }
    let lower = t.to_ascii_lowercase();
    t.contains('_')
        || (t.starts_with('-')
            && t.chars()
                .nth(1)
                .map(|c| c.is_ascii_alphanumeric())
                .unwrap_or(false))
        || lower.ends_with(".exe")
        || lower.ends_with(".dll")
        || lower.ends_with(".cfg")
}

fn repair_inline_markers(source_text: &str, out: &mut String) {
    if !source_text.contains("[[") || !source_text.contains("]]") {
        return;
    }
    let mut expected_ids = Vec::new();
    let mut literal_codes: Vec<(usize, String)> = Vec::new();
    let mut scan = source_text;
    while let Some(start) = scan.find("[[") {
        let rest = &scan[start + 2..];
        if let Some(end) = find_matching_double_bracket_close(rest) {
            let inner = &rest[..end];
            if let Some((id_str, body)) = inner.split_once('|') {
                if let Ok(id) = id_str.trim().parse::<usize>() {
                    expected_ids.push(id);
                    if is_literal_code_marker_inner(body) {
                        literal_codes.push((id, body.trim().to_string()));
                    }
                }
            }
            scan = &rest[end + 2..];
        } else {
            break;
        }
    }
    if expected_ids.is_empty() {
        return;
    }

    for &id in &expected_ids {
        let prefix = format!("[[{id}|");
        let is_footnote_src =
            source_text.contains(&format!("[[{id}|[")) && source_text.contains("]]]");
        let mut search_from = 0usize;
        while let Some(rel_pos) = out[search_from..].find(&prefix) {
            let abs_start = search_from + rel_pos;
            let content_start = abs_start + prefix.len();
            let tail = &out[content_start..];
            let next_open = tail.find("[[").unwrap_or(tail.len());
            let slice = &tail[..next_open];
            if is_footnote_src && slice.starts_with('[') && !slice.contains("]]]") {
                if let Some(d_close) = slice.find("]]") {
                    let close_abs = content_start + d_close;
                    out.replace_range(close_abs..close_abs + 2, "]]]");
                    search_from = close_abs + 3;
                    continue;
                }
            }
            if let Some(d_close) = slice.find("]]") {
                search_from = content_start + d_close + 2;
                continue;
            }
            if let Some(s_close) = slice.find(']') {
                let inner_end = content_start + s_close;
                if out[inner_end..].starts_with("]()]") {
                    out.replace_range(inner_end..inner_end + 4, "]]");
                } else {
                    out.replace_range(inner_end..inner_end + 1, "]]");
                }
                search_from = inner_end + 2;
            } else if let Some((rel_pos, cjk_ch)) = slice.char_indices().find(|&(_, ch)| {
                (ch == '》' && !slice.contains('《')) || (ch == '】' && !slice.contains('【'))
            }) {
                let inner_end = content_start + rel_pos;
                out.replace_range(inner_end..inner_end + cjk_ch.len_utf8(), "]]");
                search_from = inner_end + 2;
            } else {
                break;
            }
        }
    }

    for &id in &expected_ids {
        if expected_ids.iter().filter(|&&x| x == id).count() != 1 {
            continue;
        }
        let prefix = format!("[[{id}|");
        let first = out.find(&prefix);
        let last = out.rfind(&prefix);
        if let (Some(f), Some(l)) = (first, last) {
            if f != l {
                if let Some(rel_end) = out[f + prefix.len()..].find("]]") {
                    let close_start = f + prefix.len() + rel_end;
                    out.replace_range(close_start..close_start + 2, "");
                    out.replace_range(f..f + prefix.len(), "");
                }
            }
        }
    }

    if !source_text.contains('(') && out.contains("]](") {
        *out = out.replace("]](", "]]");
    }
    if !source_text.contains('】') && out.contains("]]】") {
        *out = out.replace("]]】", "]]");
    }
    if !source_text.contains('》') && out.contains("]]》") {
        *out = out.replace("]]》", "]]");
    }

    if out.contains("[[") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(open) = rest.find("[[") {
            fixed.push_str(&rest[..open]);
            let after_open = &rest[open + 2..];
            if let Some(close) = find_matching_double_bracket_close(after_open) {
                let inner = &after_open[..close];
                if inner.contains('|') || inner.starts_with("img") {
                    fixed.push_str("[[");
                    fixed.push_str(inner);
                    fixed.push_str("]]");
                } else {
                    fixed.push_str(inner);
                }
                rest = &after_open[close + 2..];
            } else {
                fixed.push_str(&rest[open..]);
                rest = "";
                break;
            }
        }
        fixed.push_str(rest);
        *out = fixed;
    }

    for (id, expected_inner) in &literal_codes {
        if expected_ids.iter().filter(|&&x| x == *id).count() != 1 {
            continue;
        }
        let prefix = format!("[[{id}|");
        let first = out.find(&prefix);
        let last = out.rfind(&prefix);
        if let (Some(pos), Some(last_pos)) = (first, last) {
            if pos == last_pos {
                let content_start = pos + prefix.len();
                if let Some(rel_end) = find_matching_double_bracket_close(&out[content_start..]) {
                    if &out[content_start..content_start + rel_end] != expected_inner {
                        out.replace_range(content_start..content_start + rel_end, expected_inner);
                    }
                }
            }
        }
    }
}

fn preserve_outer_dialogue_quotes_for_target(
    source_text: &str,
    norm_target: &str,
    translated: String,
) -> String {
    let src = source_text.trim();
    let out = translated.trim();
    if src.is_empty() || out.is_empty() {
        return translated;
    }
    let quote_pair = if norm_target == "Korean" {
        [
            ('「', '」', '“', '”'),
            ('『', '』', '‘', '’'),
            ('“', '”', '“', '”'),
            ('"', '"', '“', '”'),
        ]
    } else if norm_target == "Japanese" {
        [
            ('「', '」', '「', '」'),
            ('『', '』', '『', '』'),
            ('“', '”', '「', '」'),
            ('"', '"', '「', '」'),
        ]
    } else {
        [
            ('「', '」', '"', '"'),
            ('『', '』', '\'', '\''),
            ('“', '”', '"', '"'),
            ('"', '"', '"', '"'),
        ]
    }
    .into_iter()
    .find(|&(src_o, src_c, _, _)| src.starts_with(src_o) && src.ends_with(src_c));

    if let Some((_, _, dst_o, dst_c)) = quote_pair {
        if !out.starts_with(dst_o) && !out.ends_with(dst_c) {
            let inner = out
                .trim_start_matches(['「', '『', '“', '‘', '"', '\''])
                .trim_end_matches(['」', '』', '”', '’', '"', '\''])
                .trim();
            if !inner.is_empty() {
                return format!("{dst_o}{inner}{dst_c}");
            }
        }
    }
    translated
}

fn has_korean_final_consonant(c: char) -> bool {
    let code = c as u32;
    if !(0xAC00..=0xD7A3).contains(&code) {
        return false;
    }
    (code - 0xAC00) % 28 != 0
}

fn has_korean_ss_batchim(c: char) -> bool {
    let code = c as u32;
    if !(0xAC00..=0xD7A3).contains(&code) {
        return false;
    }
    (code - 0xAC00) % 28 == 20
}

fn replace_korean_copula_imnida(text: &str) -> String {
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("입니다") {
        let prefix = &rest[..pos];
        let after = &rest[pos + "입니다".len()..];
        let is_sentence_boundary = after.is_empty()
            || after.starts_with([
                '.', '!', '?', '…', ';', ',', ')', '(', '”', '’', '"', '\'', ' ', '\n',
            ]);
        let trimmed_prefix = prefix.trim_end_matches(['’', '”', '"', '\'', ')', '』', '」']);
        let mut prev_char = trimmed_prefix.chars().last();
        if prev_char == Some(']') {
            if let Some(inner_start) = trimmed_prefix.rfind('|') {
                let inner = trimmed_prefix[inner_start + 1..].trim_end_matches(']');
                prev_char = inner
                    .trim_end_matches(['’', '”', '"', '\'', ')', '』', '」'])
                    .chars()
                    .last();
            }
        }
        result.push_str(prefix);
        if is_sentence_boundary {
            if let Some(ch) = prev_char {
                if (0xAC00..=0xD7A3).contains(&(ch as u32)) {
                    if ch == '아' && trimmed_prefix.ends_with("아") {
                        result.push_str("입니다");
                    } else if has_korean_final_consonant(ch) {
                        result.push_str("이다");
                    } else {
                        result.push('다');
                    }
                } else if trimmed_prefix.ends_with("]]") {
                    result.push_str("이다");
                } else if ch.is_ascii_alphanumeric() {
                    if matches!(ch, '2' | '4' | '5' | '9') {
                        result.push('다');
                    } else {
                        result.push_str("이다");
                    }
                } else {
                    result.push_str("입니다");
                }
            } else if trimmed_prefix.ends_with("]]") {
                result.push_str("이다");
            } else {
                result.push_str("입니다");
            }
        } else {
            result.push_str("입니다");
        }
        rest = after;
    }
    result.push_str(rest);
    result
}

fn normalize_korean_narration_endings(out: &mut String) {
    for (formal, plain) in [
        ("있지 않습니다", "있지 않다"),
        ("되지 않습니다", "되지 않는다"),
        ("하지 않습니다", "하지 않는다"),
        ("그렇지 않습니다", "그렇지 않다"),
        ("많지 않습니다", "많지 않다"),
        ("좋지 않습니다", "좋지 않다"),
        ("아닙니다", "아니다"),
        ("있습니다", "있다"),
        ("없습니다", "없다"),
        ("같습니다", "같다"),
        ("다릅니다", "다르다"),
        ("많습니다", "많다"),
        ("적습니다", "적다"),
        ("큽니다", "크다"),
        ("작습니다", "작다"),
        ("높습니다", "높다"),
        ("낮습니다", "낮다"),
        ("좋습니다", "좋다"),
        ("나쁩니다", "나쁘다"),
        ("보입니다", "보인다"),
        ("들립니다", "들린다"),
        ("느껴집니다", "느껴진다"),
        ("생각됩니다", "생각된다"),
        ("생각합니다", "생각한다"),
        ("시작됩니다", "시작된다"),
        ("시작합니다", "시작한다"),
        ("사용합니다", "사용한다"),
        ("포함합니다", "포함한다"),
        ("나타냅니다", "나타낸다"),
        ("설명합니다", "설명한다"),
        ("제공합니다", "제공한다"),
        ("지원합니다", "지원한다"),
        ("필요합니다", "필요하다"),
        ("가능합니다", "가능하다"),
        ("중요합니다", "중요하다"),
        ("존재합니다", "존재한다"),
        ("등장합니다", "등장한다"),
        ("발생합니다", "발생한다"),
        ("유명합니다", "유명하다"),
        ("말합니다", "말한다"),
        ("됩니다", "된다"),
        ("합니다", "한다"),
        ("갑니다", "간다"),
        ("옵니다", "온다"),
        ("봅니다", "본다"),
        ("만듭니다", "만든다"),
    ] {
        for punct in [".", "!", "…", ";", ",", ")", "”", "’"] {
            let pat = format!("{formal}{punct}");
            if out.contains(&pat) {
                let rep = format!("{plain}{punct}");
                *out = out.replace(&pat, &rep);
            }
        }
        if out.ends_with(formal) {
            let new_len = out.len() - formal.len();
            out.truncate(new_len);
            out.push_str(plain);
        }
    }

    if out.contains("습니다") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find("습니다") {
            let prefix = &rest[..pos];
            let after = &rest[pos + "습니다".len()..];
            let boundary = after.is_empty()
                || after.starts_with([
                    '.', '!', '?', '…', ';', ',', ')', '(', '”', '’', '"', '\'', ' ', '\n',
                ]);
            fixed.push_str(prefix);
            if boundary
                && prefix
                    .chars()
                    .last()
                    .map(has_korean_ss_batchim)
                    .unwrap_or(false)
            {
                fixed.push('다');
            } else {
                fixed.push_str("습니다");
            }
            rest = after;
        }
        fixed.push_str(rest);
        *out = fixed;
    }

    if out.contains("입니다") {
        *out = replace_korean_copula_imnida(out);
    }
    if out.contains("됩니다") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find("됩니다") {
            let prefix = &rest[..pos];
            let after = &rest[pos + "됩니다".len()..];
            let boundary = after.is_empty()
                || after.starts_with([
                    '.', '!', '?', '…', ';', ',', ')', '(', '”', '’', '"', '\'', ' ', '\n',
                ]);
            fixed.push_str(prefix);
            if boundary {
                fixed.push_str("된다");
            } else {
                fixed.push_str("됩니다");
            }
            rest = after;
        }
        fixed.push_str(rest);
        *out = fixed;
    }

    for (bad_adj, good_adj) in [
        ("필요한다", "필요하다"),
        ("중요한다", "중요하다"),
        ("불가능한다", "불가능하다"),
        ("가능한다", "가능하다"),
        ("유명한다", "유명하다"),
        ("다양한다", "다양하다"),
        ("동일한다", "동일하다"),
        ("유사한다", "유사하다"),
        ("간단한다", "간단하다"),
        ("복잡한다", "복잡하다"),
        ("정확한다", "정확하다"),
        ("안전한다", "안전하다"),
        ("완벽한다", "완벽하다"),
        ("충분한다", "충분하다"),
        ("부족한다", "부족하다"),
    ] {
        if out.contains(bad_adj) {
            *out = out.replace(bad_adj, good_adj);
        }
    }

    if !out.contains('“') && !out.contains('‘') {
        for (non_plain, plain) in [
            ("했어요.", "했다."),
            ("됐어요.", "됐다."),
            ("왔어요.", "왔다."),
            ("갔어요.", "갔다."),
            ("났어요.", "났다."),
            ("졌어요.", "졌다."),
            ("렸어요.", "렸다."),
            ("이에요.", "이다."),
            ("예요.", "다."),
            ("네요.", "다."),
            ("군요.", "다."),
        ] {
            if out.contains(non_plain) {
                *out = out.replace(non_plain, plain);
            }
        }
    }
}

fn enforce_casual_korean_dialogue(out: &mut String) {
    for (humble, casual) in [
        ("제가 ", "내가 "),
        ("저도 ", "나도 "),
        ("저는 ", "나는 "),
        ("저한테", "나한테"),
        ("저에게", "나에게"),
        ("제게 ", "내게 "),
        ("저희", "우리"),
    ] {
        if out.contains(humble) {
            *out = out.replace(humble, casual);
        }
    }

    for (polite_pat, casual_rep) in [
        ("말해 주세요", "말해 줘"),
        ("알려 주세요", "알려 줘"),
        ("해 주세요", "해 줘"),
        ("해주세요", "해 줘"),
        ("주세요", "줘"),
        ("할게요", "할게"),
        ("갈게요", "갈게"),
        ("볼게요", "볼게"),
        ("않아요", "않아"),
        ("같아요", "같아"),
        ("그렇군요", "그렇군"),
        ("그러네요", "그렇네"),
        ("건가요?", "거야?"),
        ("건가요", "거야"),
        ("인가요?", "이야?"),
        ("인가요", "인가"),
        ("일까요?", "일까?"),
        ("할까요?", "할까?"),
        ("될까요?", "될까?"),
        ("까요?", "까?"),
        ("거예요?", "거야?"),
        ("거예요", "거야"),
        ("이에요?", "이야?"),
        ("이에요", "이야"),
        ("예요?", "야?"),
        ("예요", "야"),
        ("거든요", "거든"),
        ("잖아요", "잖아"),
        ("네요", "네"),
        ("군요", "군"),
        ("지요?", "지?"),
        ("지요", "지"),
        ("죠?", "지?"),
        ("죠", "지"),
        ("아닙니다", "아니야"),
        ("입니다", "이야"),
        ("습니다", "다"),
    ] {
        if out.contains(polite_pat) {
            *out = out.replace(polite_pat, casual_rep);
        }
    }

    const HAECHE_VOWELS: &[char] = &[
        '아', '어', '여', '해', '워', '봐', '와', '돼', '춰', '줘', '둬', '서', '써', '떠', '커',
        '라', '마', '자', '네', '군', '지', '게', '데', '걸',
    ];
    for &v in HAECHE_VOWELS {
        for punct in [".", "?", "!", "…", "”", "’", "\"", "'"] {
            let pat = format!("{v}요{punct}");
            if out.contains(&pat) {
                let rep = format!("{v}{punct}");
                *out = out.replace(&pat, &rep);
            }
        }
        let end_pat = format!("{v}요");
        if out.ends_with(&end_pat) {
            out.pop();
        }
    }
}

fn strip_spurious_question_mark(source_text: &str, out: &mut String) {
    if source_text.contains('？') || source_text.contains('?') {
        return;
    }
    let trimmed = source_text.trim();
    for q_suffix in ["のか」", "かい」", "かね」", "の」", "かな」", "か」"] {
        if trimmed.ends_with(q_suffix) {
            return;
        }
    }
    if out.ends_with("?”") {
        let new_len = out.len() - "?”".len();
        out.truncate(new_len);
        out.push_str(".”");
    } else if out.ends_with("?’") {
        let new_len = out.len() - "?’".len();
        out.truncate(new_len);
        out.push_str(".’");
    } else if out.ends_with('?') {
        out.pop();
        out.push('.');
    }
}

fn finalize_translated_unit(source_text: &str, norm_target: &str, raw: &str) -> String {
    let cleaned = clean_translation_output(raw);
    if cleaned.is_empty() {
        return source_text.to_string();
    }
    let mut out = preserve_outer_dialogue_quotes_for_target(source_text, norm_target, cleaned);
    strip_hallucinated_marker_urls(source_text, &mut out);
    repair_inline_markers(source_text, &mut out);

    if norm_target == "Korean" {
        let src_trimmed = source_text.trim();
        if src_trimmed.starts_with('「') && src_trimmed.ends_with('」') {
            let inner = out
                .trim()
                .trim_start_matches(['「', '“', '"', '‘', '«'])
                .trim_end_matches(['」', '”', '"', '’', '»']);
            if !inner.is_empty() {
                out = format!("“{inner}”");
            }
        } else if (src_trimmed.starts_with('（') && src_trimmed.ends_with('）'))
            || (src_trimmed.starts_with('『') && src_trimmed.ends_with('』'))
        {
            let inner = out
                .trim()
                .trim_start_matches(['（', '(', '『', '‘', '“', '"'])
                .trim_end_matches(['）', ')', '』', '’', '”', '"']);
            if !inner.is_empty() {
                out = format!("‘{inner}’");
            }
        }

        strip_spurious_question_mark(source_text, &mut out);
        let is_dlg = is_dialogue_segment(source_text);
        if is_dlg {
            if detect_dialogue_register(source_text) == DialogueRegister::Casual {
                enforce_casual_korean_dialogue(&mut out);
            }
        } else {
            normalize_korean_narration_endings(&mut out);
        }
    }

    out.trim().to_string()
}

fn looks_like_prompt_echo(source_text: &str, out: &str) -> bool {
    const ECHO_PHRASES: &[&str] = &[
        "마커를 그대로 유지",
        "마커는 그대로 유지",
        "마커도 보존한",
        "배경 정보를 고려하여",
        "추가 설명 없이",
        "preserving [[0|...]]",
        "Preserve [[0|...]]",
        "without additional explanation",
    ];
    ECHO_PHRASES
        .iter()
        .any(|phrase| out.contains(phrase) && !source_text.contains(phrase))
}

fn looks_untranslated(source_text: &str, out: &str, norm_target: &str) -> bool {
    let clean_src_buf = unwrap_inline_markers_for_shortcut(source_text);
    let clean_src = clean_src_buf.trim();
    if clean_src.is_empty() || shortcut_translation(source_text, norm_target).is_some() {
        return false;
    }
    let clean_out_buf = unwrap_inline_markers_for_shortcut(out);
    let clean_out = clean_out_buf.trim();
    if clean_out.is_empty() {
        return true;
    }

    if clean_src.chars().count() >= 8 && clean_out == clean_src {
        return true;
    }

    if norm_target == "Korean" {
        let out_hangul = clean_out
            .chars()
            .filter(|c| {
                ('\u{AC00}'..='\u{D7A3}').contains(c) || ('\u{3131}'..='\u{318E}').contains(c)
            })
            .count();
        let src_has_kana = clean_src
            .chars()
            .any(|c| ('\u{3040}'..='\u{30FF}').contains(&c));
        let src_has_hanzi = clean_src
            .chars()
            .any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c));
        let src_ascii_words = clean_src
            .split_whitespace()
            .filter(|w| w.chars().filter(|c| c.is_ascii_alphabetic()).count() >= 3)
            .count();
        if (src_has_kana || src_has_hanzi || src_ascii_words >= 3) && out_hangul == 0 {
            return true;
        }
    }

    false
}

fn run_completion_once(
    loaded: &LoadedModel,
    source: &str,
    norm_target: &str,
    style: &str,
    glossary_hint: &str,
    omit_marker_clause: bool,
) -> Result<String, String> {
    let vocab = loaded.model.vocab();
    let src_toks = vocab.tokenize(source.as_bytes(), false, false).len() as u32;
    let (prompt, uses_hy_template) = build_prompt_ext(
        &loaded.model,
        source,
        norm_target,
        style,
        glossary_hint,
        omit_marker_clause,
    );
    let parse_special = prompt.contains("<｜hy_") || prompt.contains("<|") || prompt.contains("[|");
    let tokens = vocab.tokenize(prompt.as_bytes(), !uses_hy_template, parse_special);
    if tokens.is_empty() {
        return Ok(source.to_string());
    }

    let max_new_tokens =
        estimate_max_output_tokens_for_prompt(src_toks, source, norm_target, loaded.n_ctx);
    let needed_ctx = ((tokens.len() as u32) + max_new_tokens + 64)
        .min(loaded.n_ctx)
        .max(1024);

    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(needed_ctx))
        .with_n_batch(512)
        .with_n_ubatch(512)
        .with_n_seq_max(1)
        .with_no_perf(true)
        .with_n_threads(4)
        .with_n_threads_batch(4);

    let mut ctx = loaded
        .model
        .new_context(&loaded.backend, ctx_params)
        .map_err(|e| format!("Context creation failed: {e}"))?;

    let mut batch = LlamaBatch::new(512, 1);
    let total_prompt = tokens.len();
    for (chunk_idx, chunk) in tokens.chunks(512).enumerate() {
        if CANCEL_REQUESTED.load(Ordering::Relaxed) {
            return Err("Translation cancelled".to_string());
        }
        batch.clear();
        let base_pos = chunk_idx * 512;
        for (offset, &tok) in chunk.iter().enumerate() {
            let pos = (base_pos + offset) as i32;
            let is_last = base_pos + offset + 1 == total_prompt;
            batch
                .add(tok, pos, &[0], is_last)
                .map_err(|e| format!("Batch add failed: {e}"))?;
        }
        ctx.decode(&mut batch)
            .map_err(|e| format!("Prompt decode failed: {e}"))?;
    }

    let mut sampler = if style.trim().eq_ignore_ascii_case("polished") {
        LlamaSampler::chain_simple([
            LlamaSampler::penalties(vocab.n_tokens(), 64, 1.01, 0.0, 0.0),
            LlamaSampler::top_k(20),
            LlamaSampler::top_p(0.80, 1),
            LlamaSampler::temp(0.22),
            LlamaSampler::dist(42),
        ])
    } else {
        LlamaSampler::greedy()
    };

    let mut rep_guard = RepetitionGuard::new();
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut piece_buf = Vec::<u8>::with_capacity(32);
    let mut output = String::with_capacity((source.len() * 2).min(4096));
    let mut cur_pos = total_prompt as i32;
    let max_pos = (total_prompt as u32)
        .saturating_add(max_new_tokens)
        .min(needed_ctx.saturating_sub(4));

    while (cur_pos as u32) < max_pos {
        if CANCEL_REQUESTED.load(Ordering::Relaxed) {
            return Err("Translation cancelled".to_string());
        }
        let token = sampler.sample(&ctx, batch.n_tokens() - 1);
        sampler.accept(token);
        if vocab.is_eog(token) {
            break;
        }
        let pre_len = output.len();
        piece_buf.clear();
        vocab.token_to_piece_into(token, &mut piece_buf, false, None);
        if !piece_buf.is_empty() {
            if let Some(cap) = decoder.max_utf8_buffer_length(piece_buf.len()) {
                output.reserve(cap);
            }
            let _ = decoder.decode_to_string(&piece_buf, &mut output, false);
        }

        if piece_might_complete_stop_marker(&piece_buf) && has_stop_marker(&output) {
            break;
        }
        if let Some(trim_len) = rep_guard.push_and_check(token, pre_len) {
            output.truncate(trim_len);
            break;
        }

        batch.clear();
        batch
            .add(token, cur_pos, &[0], true)
            .map_err(|e| format!("Token batch add failed: {e}"))?;
        cur_pos += 1;
        ctx.decode(&mut batch)
            .map_err(|e| format!("Token decode failed: {e}"))?;
    }

    let _ = decoder.decode_to_string(&[], &mut output, true);
    Ok(finalize_translated_unit(source, norm_target, &output))
}

fn translate_single_unit(
    loaded: &LoadedModel,
    source: &str,
    norm_target: &str,
    style: &str,
    glossary_hint: &str,
) -> Result<String, String> {
    if let Some(shortcut) = shortcut_translation(source, norm_target) {
        return Ok(shortcut);
    }

    let first = run_completion_once(loaded, source, norm_target, style, glossary_hint, false)?;
    if looks_like_prompt_echo(source, &first) || looks_untranslated(source, &first, norm_target) {
        let retried = run_completion_once(loaded, source, norm_target, style, glossary_hint, true)?;
        if !looks_untranslated(source, &retried, norm_target) {
            return Ok(retried);
        }
        if source.contains("[[") && source.contains("]]") {
            let plain = unwrap_inline_markers_for_shortcut(source);
            let plain_trimmed = plain.trim();
            if !plain_trimmed.is_empty() && plain_trimmed != source.trim() {
                if let Ok(plain_tr) = run_completion_once(
                    loaded,
                    plain_trimmed,
                    norm_target,
                    style,
                    glossary_hint,
                    true,
                ) {
                    if !plain_tr.trim().is_empty()
                        && !looks_like_prompt_echo(plain_trimmed, &plain_tr)
                    {
                        return Ok(plain_tr);
                    }
                }
            }
        }
        if !retried.trim().is_empty() {
            return Ok(retried);
        }
    }
    Ok(first)
}

fn translate_with_model(
    loaded: &LoadedModel,
    source: &str,
    target_lang: &str,
    style: &str,
    glossary_hint: &str,
) -> Result<String, String> {
    if source.trim().is_empty() {
        return Ok(source.to_string());
    }
    let norm_target = normalize_target_lang(target_lang);
    if let Some(shortcut) = shortcut_translation(source, &norm_target) {
        return Ok(shortcut);
    }

    if segment_needs_line_split(source) {
        let lines: Vec<&str> = source.lines().collect();
        let mut out_lines = Vec::with_capacity(lines.len());
        for line in lines {
            if CANCEL_REQUESTED.load(Ordering::Relaxed) {
                return Err("Translation cancelled".to_string());
            }
            if line.trim().is_empty() {
                out_lines.push(line.to_string());
            } else {
                out_lines.push(translate_single_unit(
                    loaded,
                    line,
                    &norm_target,
                    style,
                    glossary_hint,
                )?);
            }
        }
        return Ok(out_lines.join("\n"));
    }

    if let Some(chunks) = split_long_paragraph_sentences(source) {
        let mut out_parts = Vec::with_capacity(chunks.len());
        for chunk in chunks {
            if CANCEL_REQUESTED.load(Ordering::Relaxed) {
                return Err("Translation cancelled".to_string());
            }
            out_parts.push(translate_single_unit(
                loaded,
                &chunk,
                &norm_target,
                style,
                glossary_hint,
            )?);
        }
        return Ok(join_translated_sentences(&out_parts, &norm_target));
    }

    translate_single_unit(loaded, source, &norm_target, style, glossary_hint)
}

#[no_mangle]
pub extern "C" fn fluo_load_model(model_path: *const c_char, n_gpu_layers: i32, n_ctx: u32) -> i32 {
    let Some(path_str) = cstr_to_string(model_path) else {
        set_last_error("Invalid UTF-8 model path");
        return -1;
    };
    let path = PathBuf::from(&path_str);
    if !path.is_file() {
        set_last_error("Model file does not exist on disk");
        return -2;
    }

    let mut magic = [0u8; 4];
    match File::open(&path).and_then(|mut f| f.read_exact(&mut magic)) {
        Ok(()) if &magic == b"GGUF" => {}
        Ok(()) => {
            set_last_error("Invalid GGUF file header (download may be corrupted — please delete and re-download)");
            return -5;
        }
        Err(e) => {
            set_last_error(&format!("Cannot read model file: {e}"));
            return -2;
        }
    }

    let safe_ctx = n_ctx.clamp(1024, 4096);
    let mut guard = match active_slot().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if let Some(existing) = guard.as_ref() {
        if existing.model_path == path_str && existing.n_ctx == safe_ctx {
            return 0;
        }
    }
    *guard = None;

    let backend = match get_backend() {
        Ok(b) => b,
        Err(e) => {
            set_last_error(&e);
            return -3;
        }
    };
    let ngl = if n_gpu_layers < 0 {
        99
    } else {
        n_gpu_layers as u32
    };
    let attempts: Vec<(u32, bool, &str)> = if ngl > 0 {
        vec![
            (ngl, true, "metal-mmap"),
            (0, true, "cpu-mmap"),
            (0, false, "cpu-read"),
        ]
    } else {
        vec![(0, true, "cpu-mmap"), (0, false, "cpu-read")]
    };

    let mut last_err = String::new();
    let mut loaded_model = None;
    for (layers, use_mmap, label) in attempts {
        let model_params = LlamaModelParams::default()
            .with_n_gpu_layers(layers)
            .with_use_mmap(use_mmap);
        match LlamaModel::load_from_file(&backend, &path, &model_params) {
            Ok(model) => {
                loaded_model = Some(model);
                break;
            }
            Err(e) => {
                last_err = format!("{label}: {e}");
            }
        }
    }

    let Some(model) = loaded_model else {
        set_last_error(&last_err);
        return -4;
    };
    set_last_error("");
    *guard = Some(LoadedModel {
        model_path: path_str,
        n_ctx: safe_ctx,
        backend,
        model,
    });
    0
}

#[no_mangle]
pub extern "C" fn fluo_last_error() -> *mut c_char {
    let lock = LAST_LOAD_ERROR.get_or_init(|| Mutex::new(String::new()));
    let msg = lock.lock().map(|g| g.clone()).unwrap_or_default();
    CString::new(msg)
        .unwrap_or_else(|_| CString::new("").unwrap())
        .into_raw()
}

#[no_mangle]
pub extern "C" fn fluo_translate_text(
    text: *const c_char,
    target_lang: *const c_char,
    style: *const c_char,
) -> *mut c_char {
    fluo_translate_with_glossary(text, target_lang, style, std::ptr::null())
}

#[no_mangle]
pub extern "C" fn fluo_translate_with_glossary(
    text: *const c_char,
    target_lang: *const c_char,
    style: *const c_char,
    glossary_hint: *const c_char,
) -> *mut c_char {
    let Some(source) = cstr_to_string(text) else {
        return json_err("Invalid UTF-8 text pointer");
    };
    let target = cstr_to_string(target_lang).unwrap_or_else(|| "Korean".to_string());
    let style_str = cstr_to_string(style).unwrap_or_else(|| "auto".to_string());
    let glossary = cstr_to_string(glossary_hint).unwrap_or_default();

    CANCEL_REQUESTED.store(false, Ordering::SeqCst);
    let guard = match active_slot().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    if CANCEL_REQUESTED.load(Ordering::Relaxed) {
        return json_err("Translation cancelled");
    }
    let Some(loaded) = guard.as_ref() else {
        return json_err("No on-device model loaded");
    };

    match translate_with_model(loaded, &source, &target, &style_str, &glossary) {
        Ok(tr) => json_ok(&tr),
        Err(err) => json_err(&err),
    }
}

#[no_mangle]
pub extern "C" fn fluo_cancel_translation() {
    CANCEL_REQUESTED.store(true, Ordering::SeqCst);
}

#[no_mangle]
pub extern "C" fn fluo_unload_model() {
    CANCEL_REQUESTED.store(true, Ordering::SeqCst);
    let mut guard = match active_slot().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    *guard = None;
}

#[no_mangle]
pub extern "C" fn fluo_free_string(ptr: *mut c_char) {
    if !ptr.is_null() {
        unsafe {
            drop(CString::from_raw(ptr));
        }
    }
}
