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

fn is_dialogue_line(line: &str, is_cjk: bool) -> bool {
    let l = line.trim();
    if l.is_empty() {
        return false;
    }
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
        .any(|&(open, close)| l.starts_with(open) && l.ends_with(close))
        || (is_cjk && l.starts_with('(') && l.ends_with(')'))
    {
        return true;
    }
    const OPEN_DIALOGUE_ONLY: [char; 5] = ['「', '“', '"', '〝', '（'];
    l.starts_with(OPEN_DIALOGUE_ONLY)
        || (is_cjk && l.starts_with('('))
        || l.starts_with("— ")
        || l.starts_with("― ")
}

fn split_list_bullet_prefix(line: &str) -> Option<(&str, &str)> {
    let trimmed_start = line.trim_start();
    let indent_len = line.len() - trimmed_start.len();
    for bullet in ["- ", "* ", "• ", "· "] {
        if let Some(body) = trimmed_start.strip_prefix(bullet) {
            let body_trim = body.trim();
            if !body_trim.is_empty()
                && !body_trim
                    .chars()
                    .next()
                    .map(|c| c.is_ascii_digit())
                    .unwrap_or(false)
            {
                let prefix_len = indent_len + bullet.len();
                return Some((&line[..prefix_len], body_trim));
            }
        }
    }
    None
}

fn has_structured_multiline_entries(text: &str) -> bool {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() < 2 {
        return false;
    }
    let bullet_count = lines
        .iter()
        .filter(|l| split_list_bullet_prefix(l).is_some())
        .count();
    if bullet_count >= 2 {
        return true;
    }
    if lines.len() <= 2 || text.chars().count() <= 160 {
        return false;
    }
    let has_soft_wrap = lines.windows(2).any(|w| {
        let prev = w[0];
        let next = w[1];
        let prev_is_heading = prev.chars().count() <= 36;
        let prev_is_structured = prev.contains(';')
            || prev.ends_with([
                '.', '!', '?', ':', ';', '。', '！', '？', '」', '』', '”', '"', ')', ']',
            ]);
        let next_is_list = split_list_bullet_prefix(next).is_some();
        let next_starts_mid_phrase = next
            .chars()
            .next()
            .map(|c| c.is_ascii_lowercase() || c.is_ascii_uppercase())
            .unwrap_or(false);
        !prev_is_heading && !prev_is_structured && !next_is_list && next_starts_mid_phrase
    });
    if has_soft_wrap {
        return false;
    }
    let is_cjk = has_cjk_or_kana(text);
    let structured_count = lines
        .iter()
        .filter(|l| {
            is_dialogue_line(l, is_cjk)
                || l.contains(';')
                || l.ends_with([
                    '.', '!', '?', ':', ';', '。', '！', '？', '」', '』', '”', '"',
                ])
                || l.chars().count() <= 36
        })
        .count();
    structured_count * 2 >= lines.len()
}

fn has_mixed_dialogue_and_narration(text: &str) -> bool {
    let lines: Vec<&str> = text
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() < 2 {
        return false;
    }
    let is_cjk = has_cjk_or_kana(text);
    let mut has_dlg = false;
    let mut has_narration = false;
    for line in lines {
        if is_dialogue_line(line, is_cjk) {
            has_dlg = true;
        } else {
            has_narration = true;
        }
        if has_dlg && has_narration {
            return true;
        }
    }
    false
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
    let lines: Vec<&str> = trimmed
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .collect();
    if lines.len() > 1 {
        let dialogue_lines = lines.iter().filter(|l| is_dialogue_line(l, is_cjk)).count();
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
    has_structured_multiline_entries(source_text) || has_mixed_dialogue_and_narration(source_text)
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

#[derive(Clone, Debug)]
struct IosGlossaryRule {
    source: String,
    target: String,
    do_not_translate: bool,
}

fn sanitize_page_title_context(title: &str) -> Option<String> {
    let mut collapsed = title.split_whitespace().collect::<Vec<_>>().join(" ");
    let trimmed = collapsed.trim();
    if trimmed.is_empty() {
        return None;
    }
    let lower = trimmed.to_ascii_lowercase();
    if lower.starts_with("http://") || lower.starts_with("https://") || lower.starts_with("tb://") {
        return None;
    }
    for suffix in [
        " - Wikipedia",
        " — Wikipedia",
        " - Wikipédia",
        " — Wikipédia",
        " - 위키백과",
        " - 维基百科",
        " - 維基百科",
        " - GitHub",
        " — GitHub",
        " | GitHub",
        " | MDN",
        " - MDN",
        " - YouTube",
        " - Google Search",
        " - Google 검색",
    ] {
        if let Some(stripped) = collapsed.strip_suffix(suffix) {
            let s = stripped.trim();
            if s.chars().count() >= 2 {
                collapsed = s.to_string();
            }
        }
    }
    let clean = collapsed.trim();
    if clean.is_empty() {
        return None;
    }
    let clean_lower = clean.to_ascii_lowercase();
    if matches!(
        clean_lower.as_str(),
        "fluo" | "new tab" | "새 탭" | "untitled" | "blank" | "start" | "home" | "about:blank"
    ) {
        return None;
    }
    let clipped: String = clean.chars().take(80).collect();
    let has_word_char = clipped.chars().any(|c| c.is_alphabetic());
    has_word_char.then(|| clipped.trim().to_string())
}

fn parse_ios_glossary_hint(glossary_hint: &str) -> (Option<String>, Vec<IosGlossaryRule>) {
    let mut page_title: Option<String> = None;
    let mut rules = Vec::new();
    for raw_line in glossary_hint.lines() {
        let trimmed = raw_line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let pair = if let Some((s, t)) = trimmed.split_once('\t') {
            Some((s, t))
        } else {
            trimmed.split_once(" -> ")
        };
        if let Some((src, rhs)) = pair {
            let clean_src = src.trim();
            if clean_src == "@page_title" {
                if page_title.is_none() {
                    page_title = sanitize_page_title_context(rhs);
                }
                continue;
            }
            if clean_src.is_empty() {
                continue;
            }
            let mut clean_tgt = rhs.trim();
            let mut explicit_dnt = false;
            if let Some(pos) = clean_tgt.find("(do NOT translate") {
                clean_tgt = clean_tgt[..pos].trim();
                explicit_dnt = true;
            }
            let final_tgt = if clean_tgt.is_empty() {
                clean_src.to_string()
            } else {
                clean_tgt.to_string()
            };
            let is_dnt =
                explicit_dnt || clean_tgt.is_empty() || final_tgt.eq_ignore_ascii_case(clean_src);
            rules.push(IosGlossaryRule {
                source: clean_src.to_string(),
                target: final_tgt,
                do_not_translate: is_dnt,
            });
        }
    }
    (page_title, rules)
}

fn is_ascii_word_char_byte(b: u8) -> bool {
    b.is_ascii_alphanumeric() || b == b'_'
}

fn find_glossary_match_range(haystack: &str, needle: &str) -> Option<(usize, usize)> {
    let needle_trimmed = needle.trim();
    if needle_trimmed.is_empty() || haystack.is_empty() {
        return None;
    }
    if needle_trimmed.is_ascii() {
        let hay_bytes = haystack.as_bytes();
        let hay_lower = haystack.to_ascii_lowercase();
        let needle_lower = needle_trimmed.to_ascii_lowercase();
        let nlen = needle_lower.len();
        let first_ascii = needle_trimmed
            .as_bytes()
            .first()
            .map(|&b| is_ascii_word_char_byte(b))
            .unwrap_or(false);
        let last_ascii = needle_trimmed
            .as_bytes()
            .last()
            .map(|&b| is_ascii_word_char_byte(b))
            .unwrap_or(false);
        let mut from = 0usize;
        while let Some(rel) = hay_lower[from..].find(&needle_lower) {
            let start = from + rel;
            let end = start + nlen;
            let left_ok =
                !first_ascii || start == 0 || !is_ascii_word_char_byte(hay_bytes[start - 1]);
            let right_ok =
                !last_ascii || end >= hay_bytes.len() || !is_ascii_word_char_byte(hay_bytes[end]);
            if left_ok
                && right_ok
                && haystack.is_char_boundary(start)
                && haystack.is_char_boundary(end)
            {
                return Some((start, end));
            }
            from = start + 1;
            while from < hay_bytes.len() && !hay_lower.is_char_boundary(from) {
                from += 1;
            }
        }
        return None;
    }
    haystack
        .find(needle_trimmed)
        .map(|pos| (pos, pos + needle_trimmed.len()))
}

fn filter_ios_glossary<'a>(
    source_text: &str,
    rules: &'a [IosGlossaryRule],
) -> Vec<&'a IosGlossaryRule> {
    if rules.is_empty() || source_text.trim().is_empty() {
        return Vec::new();
    }
    let mut matched: Vec<&IosGlossaryRule> = rules
        .iter()
        .filter(|rule| find_glossary_match_range(source_text, &rule.source).is_some())
        .collect();
    matched.sort_by_key(|r| std::cmp::Reverse(r.source.len()));
    matched
}

fn glossary_exact_segment_shortcut(
    source_text: &str,
    rules: &[&IosGlossaryRule],
) -> Option<String> {
    if rules.is_empty() {
        return None;
    }
    let trimmed = source_text.trim();
    if trimmed.is_empty() {
        return None;
    }
    let match_rule_exact = |candidate: &str| -> Option<String> {
        let c = candidate.trim();
        for rule in rules {
            let src = rule.source.trim();
            if src.is_empty() {
                continue;
            }
            let same = if src.is_ascii() {
                c.eq_ignore_ascii_case(src)
            } else {
                c == src
            };
            if same {
                return Some(rule.target.clone());
            }
        }
        None
    };

    if let Some(hit) = match_rule_exact(trimmed) {
        return Some(hit);
    }

    if let Some(rest) = trimmed.strip_prefix("[[") {
        if let Some(pipe_pos) = rest.find('|') {
            let id_part = &rest[..pipe_pos];
            if !id_part.is_empty() && id_part.chars().all(|c| c.is_ascii_digit()) {
                let after_pipe = &rest[pipe_pos + 1..];
                if let Some(close_pos) = find_matching_double_bracket_close(after_pipe) {
                    if close_pos + 2 == after_pipe.len() {
                        let inner = &after_pipe[..close_pos];
                        if let Some(hit) = match_rule_exact(inner) {
                            return Some(format!("[[{id_part}|{hit}]]"));
                        }
                    }
                }
            }
        }
    }
    None
}

fn apply_ios_glossary_pre_substitute(source_text: &str, rules: &[&IosGlossaryRule]) -> String {
    if rules.is_empty() {
        return source_text.to_string();
    }
    let source_lower = source_text.to_lowercase();
    let mut cur = source_text.to_string();
    for rule in rules {
        let tgt = rule.target.trim();
        if tgt.is_empty() {
            continue;
        }
        if rule.do_not_translate {
            let wrapped_tgt_lower = format!("`{}`", tgt.to_lowercase());
            if source_lower.contains(&wrapped_tgt_lower) {
                continue;
            }
            let mut out = String::with_capacity(cur.len() + 8);
            let mut rest = cur.as_str();
            while let Some((start, end)) = find_glossary_match_range(rest, &rule.source) {
                let before = &rest[..start];
                let after = &rest[end..];
                let already_wrapped =
                    before.trim_end().ends_with('`') && after.trim_start().starts_with('`');
                let touches_backtick = before.ends_with('`') || after.starts_with('`');
                out.push_str(before);
                if already_wrapped || touches_backtick {
                    out.push_str(&rest[start..end]);
                } else {
                    out.push('`');
                    out.push_str(tgt);
                    out.push('`');
                }
                rest = after;
            }
            out.push_str(rest);
            cur = out;
        } else {
            let mut out = String::with_capacity(cur.len());
            let mut rest = cur.as_str();
            while let Some((start, end)) = find_glossary_match_range(rest, &rule.source) {
                out.push_str(&rest[..start]);
                out.push_str(tgt);
                rest = &rest[end..];
            }
            out.push_str(rest);
            cur = out;
        }
    }
    cur
}

fn unwrap_synthetic_dnt_quotes_ios(
    out: &mut String,
    source_lower: &str,
    rule: &IosGlossaryRule,
    target: &str,
) {
    let mut candidates = vec![target];
    let main = rule.source.trim();
    if !main.is_empty() && !main.eq_ignore_ascii_case(target) {
        candidates.push(main);
    }
    for (open_ch, close_ch) in [('`', '`'), ('‘', '’'), ('\'', '\''), ('“', '”'), ('"', '"')]
    {
        let already_in_src = candidates
            .iter()
            .any(|c| source_lower.contains(&format!("{open_ch}{}{close_ch}", c.to_lowercase())));
        if already_in_src {
            continue;
        }
        let mut rebuilt = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(open) = rest.find(open_ch) {
            rebuilt.push_str(&rest[..open]);
            let after_open = &rest[open + open_ch.len_utf8()..];
            if let Some(close) = after_open.find(close_ch) {
                let inner = after_open[..close].trim();
                if candidates.iter().any(|c| inner.eq_ignore_ascii_case(c)) {
                    rebuilt.push_str(target);
                    rest = &after_open[close + close_ch.len_utf8()..];
                    continue;
                }
            }
            rebuilt.push(open_ch);
            rest = after_open;
        }
        rebuilt.push_str(rest);
        *out = rebuilt;
    }
}

fn fix_korean_particle_after_term(text: &str, term: &str) -> String {
    let trimmed_term = term.trim();
    if trimmed_term.is_empty() || !text.contains(trimmed_term) {
        return text.to_string();
    }
    let last_hangul = trimmed_term
        .trim_end_matches(['》', '』', '」', '〉', '】', '”', '’', '"', '\'', ')', ']'])
        .chars()
        .last()
        .filter(|c| ('가'..='힣').contains(c));
    let last_ch = match last_hangul {
        Some(ch) => ch,
        None => return text.to_string(),
    };
    let has_batchim = has_korean_final_consonant(last_ch);
    let mut out = String::with_capacity(text.len() + 8);
    let mut rest = text;
    while let Some(pos) = rest.find(trimmed_term) {
        let after_term_idx = pos + trimmed_term.len();
        out.push_str(&rest[..after_term_idx]);
        let mut rem = &rest[after_term_idx..];
        while let Some(ch) = rem.chars().next() {
            if matches!(
                ch,
                '》' | '』' | '」' | '〉' | '】' | '”' | '’' | '"' | '\'' | ')' | ']'
            ) {
                out.push(ch);
                rem = &rem[ch.len_utf8()..];
            } else {
                break;
            }
        }
        if let Some(particle) = rem.chars().next() {
            let after_particle = &rem[particle.len_utf8()..];
            let next_is_sep = after_particle.is_empty()
                || after_particle.starts_with(|c: char| {
                    c.is_whitespace() || matches!(c, ',' | '.' | '!' | '?' | ';' | ':' | ')' | ']')
                });
            if next_is_sep {
                let fixed = match particle {
                    '는' | '은' => Some(if has_batchim { '은' } else { '는' }),
                    '가' | '이' => Some(if has_batchim { '이' } else { '가' }),
                    '를' | '을' => Some(if has_batchim { '을' } else { '를' }),
                    '와' | '과' => Some(if has_batchim { '과' } else { '와' }),
                    _ => None,
                };
                if let Some(replacement) = fixed {
                    out.push(replacement);
                    rest = after_particle;
                    continue;
                }
            }
        }
        rest = rem;
    }
    out.push_str(rest);
    out
}

fn apply_ios_glossary_post_enforce(
    source_text: &str,
    out: &mut String,
    rules: &[&IosGlossaryRule],
) {
    if rules.is_empty() || out.trim().is_empty() {
        return;
    }
    let source_lower = source_text.to_lowercase();
    for rule in rules {
        let tgt = rule.target.trim();
        if tgt.is_empty() {
            continue;
        }
        if rule.do_not_translate {
            unwrap_synthetic_dnt_quotes_ios(out, &source_lower, rule, tgt);
        }
        let mut replaced = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some((start, end)) = find_glossary_match_range(rest, &rule.source) {
            replaced.push_str(&rest[..start]);
            if &rest[start..end] == tgt || rest[start..].starts_with(tgt) {
                replaced.push_str(&rest[start..end]);
            } else {
                replaced.push_str(tgt);
            }
            rest = &rest[end..];
        }
        replaced.push_str(rest);
        *out = fix_korean_particle_after_term(&replaced, tgt);
    }
}

fn build_prompt_ext(
    model: &LlamaModel,
    source: &str,
    norm_target: &str,
    style: &str,
    page_title: Option<&str>,
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

    let clean_topic = page_title
        .and_then(sanitize_page_title_context)
        .filter(|t| {
            !is_dialogue
                && source.chars().count() >= 24
                && !source
                    .to_ascii_lowercase()
                    .contains(&t.to_ascii_lowercase())
        });
    let has_bg = clean_topic.is_some();
    if let Some(topic) = clean_topic {
        if use_zh_template {
            sections.push(format!("[背景信息]\n主题：{topic}"));
        } else {
            sections.push(format!("[Background Information]\nTopic: {topic}"));
        }
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
        if has_bg {
            sections.push(format!(
                "请参考上面提供的背景信息，将以下文本翻译为{zh_lang}{marker_clause_zh}，注意只需要输出翻译后的结果，不要额外解释：\n\n[原文]\n{source}"
            ));
        } else {
            sections.push(format!(
                "将以下文本翻译为{zh_lang}{marker_clause_zh}，注意只需要输出翻译后的结果，不要额外解释：\n\n{source}"
            ));
        }
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
        if has_bg {
            sections.push(format!(
                "Translate the following segment into {norm_target}{marker_clause}, considering the background information, without additional explanation.{style_clause}\n\n[Source Text]\n{source}"
            ));
        } else {
            sections.push(format!(
                "Translate the following segment into {norm_target}{marker_clause}, without additional explanation.{style_clause}\n\n{source}"
            ));
        }
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
            || lower.starts_with("topic:")
            || l.starts_with("主题：")
            || l.starts_with("主题:")
            || l.starts_with("주제:")
            || l.starts_with("주제：")
            || lower.starts_with("translate the following segment")
            || lower.starts_with("output only")
            || lower.starts_with("reference the following translations")
            || lower.starts_with("mandatory glossary")
            || lower.starts_with("glossary (")
            || lower.contains("translates to \"")
            || l.starts_with("参考下面的翻译")
            || l.starts_with("다음 번역을 참고")
            || l.contains("으로 번역할 것")
            || l.contains("로 번역할 것")
            || l.contains("으로 번역 할 것")
            || l.contains("로 번역 할 것")
            || l.contains("으로 번역하십시오")
            || l.contains("로 번역하십시오")
            || l.contains("do not translate")
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
            "[背景 정보]",
            "[배경信息]",
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
        || t.contains("::")
        || t.starts_with("CVE-")
        || (t.starts_with('-')
            && t.chars()
                .nth(1)
                .map(|c| c.is_ascii_alphanumeric())
                .unwrap_or(false))
        || [
            ".exe", ".dll", ".cfg", ".rs", ".ts", ".tsx", ".js", ".jsx", ".py", ".go", ".cpp",
            ".cc", ".c", ".h", ".hpp", ".json", ".toml", ".yaml", ".yml", ".md", ".sh", ".sql",
            ".css", ".html", ".swift", ".kt", ".java", ".wasm", ".gguf", ".txt",
        ]
        .iter()
        .any(|ext| lower.ends_with(ext))
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

fn is_korean_sentence_boundary(after: &str) -> bool {
    after.is_empty()
        || after.starts_with([
            '.', '!', '?', '…', ';', ':', ',', ')', '(', '”', '’', '"', '\'', ' ', '\n',
        ])
}

fn is_korean_hada_adjective_stem(prefix: &str) -> bool {
    const HADA_ADJ_MULTI_STEMS: &[&str] = &[
        "불가능",
        "불충분",
        "불안전",
        "불안정",
        "불완전",
        "불가피",
        "불분명",
        "불확실",
        "불적절",
        "부적절",
        "부적합",
        "부정확",
        "불편",
        "불리",
        "필요",
        "가능",
        "중요",
        "강력",
        "복잡",
        "정확",
        "단순",
        "간단",
        "명확",
        "분명",
        "명백",
        "적절",
        "적합",
        "유효",
        "무효",
        "안전",
        "안정",
        "완벽",
        "완전",
        "충분",
        "부족",
        "유명",
        "다양",
        "동일",
        "유사",
        "비슷",
        "독특",
        "고유",
        "특이",
        "특별",
        "투명",
        "엄격",
        "심각",
        "위험",
        "치명",
        "유용",
        "편리",
        "훌륭",
        "탁월",
        "신속",
        "원활",
        "빈번",
        "활발",
        "풍부",
        "유리",
        "익숙",
        "친숙",
        "건강",
        "신선",
        "깨끗",
        "조용",
        "친절",
        "민감",
        "취약",
        "견고",
        "유연",
        "뚜렷",
        "정밀",
        "미세",
        "거대",
        "방대",
        "막대",
        "철저",
        "신중",
        "과도",
        "타당",
        "정당",
        "부당",
        "상당",
        "무관",
        "유관",
        "유한",
        "무한",
        "급격",
        "극심",
        "현저",
        "미미",
        "희박",
        "순수",
        "온화",
        "완만",
        "당연",
        "필연",
        "우연",
        "공정",
        "평등",
        "확실",
        "유력",
        "긴급",
        "중대",
        "평범",
        "비범",
        "생생",
        "밀접",
        "긴밀",
        "느슨",
        "단단",
        "튼튼",
        "용이",
        "난해",
        "진지",
        "성실",
        "솔직",
        "겸손",
        "오만",
        "교만",
        "냉정",
        "침착",
        "치밀",
        "영리",
        "총명",
        "현명",
        "미묘",
        "기묘",
        "이상",
        "희귀",
        "진귀",
        "귀중",
        "소중",
        "막중",
        "다행",
        "불행",
        "행복",
        "유익",
        "무익",
        "유해",
        "무해",
        "무난",
        "미흡",
        "비참",
        "처참",
        "곤란",
        "불안",
        "편안",
        "유쾌",
        "불쾌",
        "대담",
        "용감",
        "과감",
        "담대",
        "능숙",
        "미숙",
        "노련",
        "미안",
        "죄송",
        "안타",
        "다정",
        "친밀",
        "긴박",
        "위급",
        "급박",
        "간절",
        "절실",
        "선명",
        "희미",
        "은은",
        "잔잔",
        "고요",
        "적막",
        "막막",
        "갑갑",
        "답답",
        "섭섭",
        "서운",
        "순진",
        "완고",
        "명쾌",
        "상쾌",
        "불길",
        "심심",
        "따뜻",
        "서늘",
        "시원",
        "한산",
        "번잡",
        "생소",
    ];
    let trimmed = prefix.trim_end();
    if trimmed.is_empty() {
        return false;
    }
    if HADA_ADJ_MULTI_STEMS
        .iter()
        .any(|stem| trimmed.ends_with(stem))
    {
        return true;
    }
    const HADA_ADJ_SINGLE_WORD_STEMS: &[&str] = &["강", "약", "흔", "귀", "진", "순", "뻔"];
    HADA_ADJ_SINGLE_WORD_STEMS.iter().any(|stem| {
        if let Some(before) = trimmed.strip_suffix(stem) {
            before.is_empty()
                || before
                    .chars()
                    .next_back()
                    .map(|c| !('\u{AC00}'..='\u{D7A3}').contains(&c))
                    .unwrap_or(true)
        } else {
            false
        }
    })
}

fn replace_korean_copula_imnida(text: &str) -> String {
    const CLOSING_QUOTES: [char; 11] =
        ['’', '”', '"', '\'', ')', '』', '」', '》', '】', '〉', '»'];
    let mut result = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(pos) = rest.find("입니다") {
        let prefix = &rest[..pos];
        let after = &rest[pos + "입니다".len()..];
        let is_sentence_boundary = is_korean_sentence_boundary(after);
        let trimmed_prefix = prefix.trim_end_matches(CLOSING_QUOTES);
        let mut prev_char = trimmed_prefix.chars().last();
        if prev_char == Some(']') {
            if let Some(inner_start) = trimmed_prefix.rfind('|') {
                let inner = trimmed_prefix[inner_start + 1..].trim_end_matches(']');
                prev_char = inner.trim_end_matches(CLOSING_QUOTES).chars().last();
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
    // 1. Explicit irregular/adjective formal -> plain endings
    for (formal, plain) in [
        ("있지 않습니다", "있지 않다"),
        ("그렇지 않습니다", "그렇지 않다"),
        ("많지 않습니다", "많지 않다"),
        ("좋지 않습니다", "좋지 않다"),
        ("높지 않습니다", "높지 않다"),
        ("크지 않습니다", "크지 않다"),
        ("작지 않습니다", "작지 않다"),
        ("곳이죠", "곳이다"),
        ("뿐이죠", "뿐이다"),
        ("거죠", "거다"),
        ("거예요", "거다"),
        ("아닙니다", "아니다"),
        ("힘듭니다", "힘들다"),
        ("올바릅니다", "올바르다"),
        ("남다릅니다", "남다르다"),
        ("다릅니다", "다르다"),
        ("빠릅니다", "빠르다"),
        ("푸릅니다", "푸르다"),
        ("게으릅니다", "게으르다"),
        ("고릅니다", "고르다"),
        ("큽니다", "크다"),
        ("나쁩니다", "나쁘다"),
        ("바쁩니다", "바쁘다"),
        ("기쁩니다", "기쁘다"),
        ("슬픕니다", "슬프다"),
        ("아픕니다", "아프다"),
        ("예쁩니다", "예쁘다"),
        ("고픕니다", "고프다"),
        ("비쌉니다", "비싸다"),
        ("값쌉니다", "값싸다"),
        ("짭니다", "짜다"),
        ("깁니다", "길다"),
        ("멉니다", "멀다"),
        ("거칩니다", "거칠다"),
        ("드뭅니다", "드물다"),
        ("가늡니다", "가늘다"),
        ("둥굽니다", "둥글다"),
        ("서툽니다", "서툴다"),
        ("있습니다", "있다"),
        ("없습니다", "없다"),
        ("같습니다", "같다"),
        ("많습니다", "많다"),
        ("적습니다", "적다"),
        ("작습니다", "작다"),
        ("높습니다", "높다"),
        ("낮습니다", "낮다"),
        ("좋습니다", "좋다"),
        ("나쁩니다", "나쁘다"),
        ("그렇습니다", "그렇다"),
        ("이렇습니다", "이렇다"),
        ("저렇습니다", "저렇다"),
        ("어떻습니다", "어떻다"),
        ("됩니다", "된다"),
    ] {
        for punct in [".", "!", "…", ";", ":", ",", ")", "”", "’"] {
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

    // 2. Context-aware `~하지 않습니다` -> `~하지 않다` (adj) vs `~하지 않는다` (verb)
    if out.contains("하지 않습니다") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find("하지 않습니다") {
            let prefix = &rest[..pos];
            let after = &rest[pos + "하지 않습니다".len()..];
            fixed.push_str(prefix);
            if is_korean_sentence_boundary(after) {
                if is_korean_hada_adjective_stem(prefix) {
                    fixed.push_str("하지 않다");
                } else {
                    fixed.push_str("하지 않는다");
                }
            } else {
                fixed.push_str("하지 않습니다");
            }
            rest = after;
        }
        fixed.push_str(rest);
        *out = fixed;
    }

    // 3. Remaining `~지 않습니다` (되지 않습니다 등)
    if out.contains("지 않습니다") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find("지 않습니다") {
            let prefix = &rest[..pos];
            let after = &rest[pos + "지 않습니다".len()..];
            fixed.push_str(prefix);
            if is_korean_sentence_boundary(after) {
                fixed.push_str("지 않는다");
            } else {
                fixed.push_str("지 않습니다");
            }
            rest = after;
        }
        fixed.push_str(rest);
        *out = fixed;
    }

    // 4. Context-aware `<stem>합니다` -> `<stem>하다` (adjective) vs `<stem>한다` (verb)
    if out.contains("합니다") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find("합니다") {
            let prefix = &rest[..pos];
            let after = &rest[pos + "합니다".len()..];
            fixed.push_str(prefix);
            if is_korean_sentence_boundary(after) {
                if is_korean_hada_adjective_stem(prefix) {
                    fixed.push_str("하다");
                } else {
                    fixed.push_str("한다");
                }
            } else {
                fixed.push_str("합니다");
            }
            rest = after;
        }
        fixed.push_str(rest);
        *out = fixed;
    }

    // 5. General Unicode Hangul past/future `ㅆ` batchim + `습니다` -> `<syllable>다`
    if out.contains("습니다") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find("습니다") {
            let prefix = &rest[..pos];
            let after = &rest[pos + "습니다".len()..];
            fixed.push_str(prefix);
            if is_korean_sentence_boundary(after)
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

    // 6. General copula `<noun>입니다` -> `<noun>이다` / `<noun>다`
    if out.contains("입니다") {
        *out = replace_korean_copula_imnida(out);
    }

    // 7. General Unicode Hangul `<syllable with ㅂ batchim>니다` -> `<syllable with ㄴ batchim>다`
    if out.contains("니다") {
        let mut fixed = String::with_capacity(out.len());
        let mut rest = out.as_str();
        while let Some(pos) = rest.find("니다") {
            let prefix = &rest[..pos];
            let after = &rest[pos + "니다".len()..];
            if is_korean_sentence_boundary(after) {
                if let Some(last_ch) = prefix.chars().last() {
                    let code = last_ch as u32;
                    if last_ch != '습'
                        && (0xAC00..=0xD7A3).contains(&code)
                        && (code - 0xAC00) % 28 == 17
                    {
                        let prefix_without_last = &prefix[..prefix.len() - last_ch.len_utf8()];
                        let replaced_syllable = char::from_u32(code - 17 + 4).unwrap_or(last_ch);
                        fixed.push_str(prefix_without_last);
                        fixed.push(replaced_syllable);
                        fixed.push('다');
                        rest = after;
                        continue;
                    }
                }
            }
            fixed.push_str(prefix);
            fixed.push_str("니다");
            rest = after;
        }
        fixed.push_str(rest);
        *out = fixed;
    }

    // 8. Fix common Korean adjective `-ㄴ다` conjugation errors (`필요한다` -> `필요하다`)
    for (bad_adj, good_adj) in [
        ("필요한다", "필요하다"),
        ("중요한다", "중요하다"),
        ("불가능한다", "불가능하다"),
        ("가능한다", "가능하다"),
        ("유명한다", "유명하다"),
        ("다양한다", "다양하다"),
        ("동일한다", "동일하다"),
        ("유사한다", "유사하다"),
        ("적합한다", "적합하다"),
        ("간단한다", "간단하다"),
        ("복잡한다", "복잡하다"),
        ("정확한다", "정확하다"),
        ("안전한다", "안전하다"),
        ("완벽한다", "완벽하다"),
        ("충분한다", "충분하다"),
        ("부족한다", "부족하다"),
        ("투명한다", "투명하다"),
        ("특별한다", "특별하다"),
        ("유용한다", "유용하다"),
        ("필요하지 않는다", "필요하지 않다"),
        ("가능하지 않는다", "가능하지 않다"),
        ("충분하지 않는다", "충분하지 않다"),
    ] {
        if out.contains(bad_adj) {
            *out = out.replace(bad_adj, good_adj);
        }
    }

    // 9. Safe sentence-ending polite `~해요 / ~어요 / ~아요 / ~이야` normalization in non-dialogue prose
    if !out.contains('“') && !out.contains('‘') {
        if out.contains("해요.") {
            let mut fixed = String::with_capacity(out.len());
            let mut rest = out.as_str();
            while let Some(pos) = rest.find("해요.") {
                let prefix = &rest[..pos];
                let after = &rest[pos + "해요.".len()..];
                fixed.push_str(prefix);
                if is_korean_hada_adjective_stem(prefix) {
                    fixed.push_str("하다.");
                } else {
                    fixed.push_str("한다.");
                }
                rest = after;
            }
            fixed.push_str(rest);
            *out = fixed;
        }
        for (non_plain, plain) in [
            ("했어요.", "했다."),
            ("됐어요.", "됐다."),
            ("왔어요.", "왔다."),
            ("갔어요.", "갔다."),
            ("났어요.", "났다."),
            ("졌어요.", "졌다."),
            ("렸어요.", "렸다."),
            ("있어요.", "있다."),
            ("없어요.", "없다."),
            ("같아요.", "같다."),
            ("많아요.", "많다."),
            ("적어요.", "적다."),
            ("좋아요.", "좋다."),
            ("작아요.", "작다."),
            ("높아요.", "높다."),
            ("낮아요.", "낮다."),
            ("넓어요.", "넓다."),
            ("좁아요.", "좁다."),
            ("길어요.", "길다."),
            ("짧아요.", "짧다."),
            ("돼요.", "된다."),
            ("봐요.", "본다."),
            ("와요.", "온다."),
            ("가요.", "간다."),
            ("이에요.", "이다."),
            ("예요.", "다."),
            ("네요.", "다."),
            ("군요.", "다."),
            ("이야.", "이다."),
            ("거야.", "거다."),
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

fn strip_ipa_pronunciation_brackets(text: &str) -> String {
    if !text.contains('[') {
        return text.to_string();
    }
    const IPA_MARKERS: &[char] = &[
        'ʁ', 'ɛ', 'ʃ', 'ʒ', 'ɑ', 'ɔ', 'ø', 'œ', 'ɥ', 'ɲ', 'ŋ', 'ɪ', 'ʊ', 'ʌ', 'ə', 'θ', 'ð', 'ː',
        'ˈ', 'ˌ', 'ɡ', 'ʔ', 'ɾ', 'ç', 'ʝ', 'β', 'ɣ', 'χ', 'ħ', 'ʕ',
    ];
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('[') {
        if rest[open..].starts_with("[[") {
            out.push_str(&rest[..open + 2]);
            rest = &rest[open + 2..];
            continue;
        }
        let after = &rest[open + 1..];
        if let Some(close) = after.find(']') {
            let inner = &after[..close];
            let has_ipa = !inner.is_empty()
                && inner.chars().count() <= 40
                && !inner.contains('|')
                && inner.chars().any(|c| IPA_MARKERS.contains(&c));
            if has_ipa {
                out.push_str(rest[..open].trim_end_matches(' '));
                let tail = &after[close + 1..];
                if !out.is_empty()
                    && !out.ends_with(char::is_whitespace)
                    && !tail.is_empty()
                    && !tail.starts_with(char::is_whitespace)
                    && !tail.starts_with([',', '.', ';', ':', ')', '!', '?'])
                {
                    out.push(' ');
                }
                rest = tail;
                continue;
            }
        }
        out.push_str(&rest[..open + 1]);
        rest = after;
    }
    out.push_str(rest);
    out
}

fn unwrap_soft_wrapped_prose(text: &str) -> String {
    if !text.contains('\n') || has_structured_multiline_entries(text) || is_dialogue_segment(text) {
        return text.to_string();
    }
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() < 2 {
        return text.to_string();
    }
    let mut out = String::with_capacity(text.len());
    for (idx, raw_line) in lines.iter().enumerate() {
        let trimmed = raw_line.trim();
        if idx == 0 {
            out.push_str(trimmed);
            continue;
        }
        let prev = out.trim_end();
        let prev_is_all_caps_heading = !prev.is_empty()
            && prev.chars().count() <= 40
            && prev.chars().any(|c| c.is_ascii_alphabetic())
            && !prev.chars().any(|c| c.is_ascii_lowercase());
        let prev_is_single_token = !prev.contains(' ') && prev.chars().count() < 24;
        let prev_ends_sentence = prev.ends_with([
            '.', '!', '?', ':', ';', '。', '！', '？', '”', '’', '」', '』', ')', ']',
        ]);
        let next_starts_continuation = trimmed
            .chars()
            .next()
            .map(|c| !matches!(c, '-' | '*' | '•' | '·' | '[' | '「' | '『'))
            .unwrap_or(false);
        let prev_was_prose = prev.contains(' ')
            && !prev_is_all_caps_heading
            && !prev_is_single_token
            && !prev_ends_sentence;
        if prev_was_prose && next_starts_continuation && !trimmed.is_empty() {
            out.push(' ');
            out.push_str(trimmed);
        } else {
            out.push('\n');
            out.push_str(trimmed);
        }
    }
    out
}

fn preprocess_source_text(source_text: &str, norm_target: &str) -> String {
    let mut s = source_text
        .replace('\u{00A0}', " ")
        .replace('\u{202F}', " ");
    while s.contains(", )") {
        s = s.replace(", )", ")");
    }
    if s.contains(" , )") {
        s = s.replace(" , )", ")");
    }
    if !s.contains('\n') {
        if let Some((_, body)) = split_list_bullet_prefix(&s) {
            s = body.to_string();
        }
    }
    s = strip_ipa_pronunciation_brackets(&s);
    s = unwrap_soft_wrapped_prose(&s);

    if norm_target != "Japanese" {
        s = s
            .replace("人気が少ない", "人の気配が少ない")
            .replace("人気がない", "人の気配がない")
            .replace("文書をご注意ください", "文書をご参照ください");
    }

    s
}

fn fix_english_billion_to_korean(source_text: &str, out: &mut String) {
    let lower = source_text.to_ascii_lowercase();
    if !lower.contains("billion") || !out.contains('억') {
        return;
    }
    let mut scan = lower.as_str();
    while let Some(pos) = scan.find("billion") {
        let before = scan[..pos].trim_end();
        let num_start = before
            .rfind(|c: char| !(c.is_ascii_digit() || c == '.'))
            .map(|i| i + 1)
            .unwrap_or(0);
        let num_str = before[num_start..].trim();
        if !num_str.is_empty() && num_str.chars().any(|c| c.is_ascii_digit()) {
            if let Ok(val) = num_str.parse::<f64>() {
                let eok_val = val * 10.0;
                let formatted_eok = if (eok_val - eok_val.round()).abs() < 1e-6 {
                    format!("{}", eok_val.round() as i64)
                } else {
                    let s = format!("{eok_val:.2}");
                    s.trim_end_matches('0').trim_end_matches('.').to_string()
                };
                let wrong_with_space = format!("{num_str} 억");
                let wrong_tight = format!("{num_str}억");
                let right_tight = format!("{formatted_eok}억");
                if out.contains(&wrong_with_space) {
                    *out = out.replace(&wrong_with_space, &right_tight);
                }
                if out.contains(&wrong_tight) {
                    *out = out.replace(&wrong_tight, &right_tight);
                }
            }
        }
        scan = &scan[pos + "billion".len()..];
    }
}

fn fix_korean_code_token_particles(out: &mut String) {
    const VOWEL_SUFFIXES: &[&str] = &[
        ".cpp", ".rs", ".py", ".js", ".ts", ".jsx", ".tsx", ".go", ".md", ".sh", ".css", ".txt",
        ".java", ".swift", ".kt", ".exe", ".wasm", "er]]", "or]]",
    ];
    for suffix in VOWEL_SUFFIXES {
        for (bad_p, good_p) in [("은", "는"), ("이", "가"), ("을", "를"), ("과", "와")] {
            let pat = format!("{suffix}{bad_p}");
            let mut search_from = 0usize;
            while let Some(rel) = out[search_from..].find(&pat) {
                let abs = search_from + rel;
                let after = &out[abs + pat.len()..];
                let ok_boundary = after.is_empty()
                    || after.starts_with(|c: char| {
                        c.is_whitespace() || matches!(c, ',' | '.' | '!' | '?' | ';' | ':')
                    });
                if ok_boundary {
                    let p_start = abs + suffix.len();
                    out.replace_range(p_start..abs + pat.len(), good_p);
                    search_from = p_start + good_p.len();
                } else {
                    search_from = abs + pat.len();
                }
            }
        }
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

        fix_english_billion_to_korean(source_text, &mut out);
        fix_korean_code_token_particles(&mut out);
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

    let mut result = out.trim().to_string();
    if !source_text.contains('\n') {
        if let Some((bullet, _)) = split_list_bullet_prefix(source_text) {
            let trimmed_out = result.trim_start();
            if !trimmed_out.is_empty()
                && !trimmed_out.starts_with('-')
                && !trimmed_out.starts_with('*')
                && !trimmed_out.starts_with('•')
                && !trimmed_out.starts_with('·')
            {
                result = format!("{bullet}{trimmed_out}");
            }
        }
    }
    result
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
        "Reference the following translations",
        "Mandatory Glossary",
        "translates to \"",
        "参考下面的翻译",
        "翻译成",
        "으로 번역할 것",
        "로 번역할 것",
        "으로 번역 할 것",
        "로 번역 할 것",
        "으로 번역하십시오",
        "로 번역하십시오",
        "do not translate",
        "topic:",
        "主题：",
        "[背景 정보]",
        "[배경 정보]",
    ];
    let out_lower = out.to_ascii_lowercase();
    let src_lower = source_text.to_ascii_lowercase();
    ECHO_PHRASES.iter().any(|phrase| {
        let pl = phrase.to_ascii_lowercase();
        out_lower.contains(&pl) && !src_lower.contains(&pl)
    })
}

#[derive(Clone, Copy, Debug, Default)]
struct ConfidenceSummary {
    avg_logprob: f32,
    min_win4_logprob: f32,
    token_count: u32,
}

impl ConfidenceSummary {
    fn is_low_confidence(self, source_chars: usize) -> bool {
        source_chars >= 48
            && self.token_count >= 12
            && (self.avg_logprob < -0.155 || self.min_win4_logprob < -0.90)
    }

    fn quality_score(self) -> f32 {
        self.avg_logprob + 0.25 * self.min_win4_logprob
    }
}

#[derive(Debug, Default)]
struct ConfidenceTracker {
    sum_logprob: f32,
    count: u32,
    win4: [f32; 4],
    min_win4: f32,
}

impl ConfidenceTracker {
    fn new() -> Self {
        Self {
            sum_logprob: 0.0,
            count: 0,
            win4: [0.0; 4],
            min_win4: 0.0,
        }
    }

    fn push(&mut self, log_prob: f32) {
        if !log_prob.is_finite() {
            return;
        }
        let lp = log_prob.min(0.0);
        self.sum_logprob += lp;
        self.win4[(self.count as usize) % 4] = lp;
        self.count += 1;
        if self.count >= 4 {
            let avg4 = self.win4.iter().sum::<f32>() * 0.25;
            if self.count == 4 || avg4 < self.min_win4 {
                self.min_win4 = avg4;
            }
        }
    }

    fn summary(&self) -> ConfidenceSummary {
        let avg_logprob = if self.count > 0 {
            self.sum_logprob / (self.count as f32)
        } else {
            0.0
        };
        let min_win4_logprob = if self.count >= 4 {
            self.min_win4
        } else {
            avg_logprob
        };
        ConfidenceSummary {
            avg_logprob,
            min_win4_logprob,
            token_count: self.count,
        }
    }
}

fn token_log_prob(logits: &[f32], chosen: LlamaToken) -> f32 {
    let idx = chosen.0;
    if idx < 0 || (idx as usize) >= logits.len() {
        return 0.0;
    }
    let chosen_logit = logits[idx as usize];
    if !chosen_logit.is_finite() {
        return 0.0;
    }
    let mut max_logit = f32::NEG_INFINITY;
    for &x in logits {
        if x > max_logit {
            max_logit = x;
        }
    }
    if !max_logit.is_finite() {
        return 0.0;
    }
    let mut sum_exp = 0.0f32;
    for &x in logits {
        let diff = x - max_logit;
        if diff > -20.0 {
            sum_exp += diff.exp();
        }
    }
    (chosen_logit - max_logit) - sum_exp.max(1e-12).ln()
}

fn has_unexpected_script_glitch(source_text: &str, candidate: &str, norm_target: &str) -> bool {
    if norm_target == "Korean" && !has_cjk_or_kana(source_text) {
        let chars: Vec<char> = candidate.chars().collect();
        for (i, &c) in chars.iter().enumerate() {
            if ('\u{3040}'..='\u{30FF}').contains(&c) {
                return true;
            }
            if ('\u{4E00}'..='\u{9FFF}').contains(&c) {
                let adj_hangul = (i > 0 && ('\u{AC00}'..='\u{D7A3}').contains(&chars[i - 1]))
                    || (i + 1 < chars.len() && ('\u{AC00}'..='\u{D7A3}').contains(&chars[i + 1]));
                if adj_hangul {
                    return true;
                }
            }
        }
    } else if norm_target == "Japanese" {
        let chars: Vec<char> = candidate.chars().collect();
        let mut i = 0usize;
        while i + 3 < chars.len() {
            if ('\u{30A0}'..='\u{30FF}').contains(&chars[i]) && chars[i + 1].is_ascii_uppercase() {
                let mut j = i + 1;
                while j < chars.len() && chars[j].is_ascii_uppercase() {
                    j += 1;
                }
                if j - (i + 1) >= 2
                    && j < chars.len()
                    && ('\u{30A0}'..='\u{30FF}').contains(&chars[j])
                {
                    let slice: String = chars[i + 1..j].iter().collect();
                    if !source_text.contains(&slice) {
                        return true;
                    }
                }
            }
            i += 1;
        }
    }
    false
}

fn is_sane_retry_candidate(source_text: &str, candidate: &str, norm_target: &str) -> bool {
    let c_trimmed = candidate.trim();
    if c_trimmed.is_empty() || looks_like_prompt_echo(source_text, c_trimmed) {
        return false;
    }
    let src_chars = source_text.trim().chars().count();
    let cand_chars = c_trimmed.chars().count();
    if src_chars >= 30 && (cand_chars < 4 || cand_chars > src_chars * 4) {
        return false;
    }
    if norm_target != "English"
        && src_chars >= 20
        && source_text.trim().eq_ignore_ascii_case(c_trimmed)
    {
        return false;
    }
    true
}

fn should_prefer_retry_candidate(
    source_text: &str,
    norm_target: &str,
    first: &str,
    first_conf: ConfidenceSummary,
    cand2: &str,
    cand2_conf: ConfidenceSummary,
) -> bool {
    if !is_sane_retry_candidate(source_text, cand2, norm_target) {
        return false;
    }
    let glitch1 = has_unexpected_script_glitch(source_text, first, norm_target);
    let glitch2 = has_unexpected_script_glitch(source_text, cand2, norm_target);
    if glitch1 && !glitch2 {
        return true;
    }
    if !glitch1 && glitch2 {
        return false;
    }
    cand2_conf.quality_score() > first_conf.quality_score() + 0.03
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

fn build_retry_sampler(n_vocab: i32, style: &str) -> LlamaSampler {
    if style.trim().eq_ignore_ascii_case("polished") {
        LlamaSampler::greedy()
    } else {
        LlamaSampler::chain_simple([
            LlamaSampler::penalties(n_vocab, 64, 1.06, 0.0, 0.0),
            LlamaSampler::min_p(0.06, 1),
            LlamaSampler::temp(0.28),
            LlamaSampler::dist(17),
        ])
    }
}

fn run_completion_once(
    loaded: &LoadedModel,
    orig_source: &str,
    prompt_source: &str,
    norm_target: &str,
    style: &str,
    page_title: Option<&str>,
    seg_glossary: &[&IosGlossaryRule],
    omit_marker_clause: bool,
    use_retry_sampler: bool,
) -> Result<(String, ConfidenceSummary), String> {
    let vocab = loaded.model.vocab();
    let src_toks = vocab.tokenize(prompt_source.as_bytes(), false, false).len() as u32;
    let (prompt, uses_hy_template) = build_prompt_ext(
        &loaded.model,
        prompt_source,
        norm_target,
        style,
        page_title,
        "",
        omit_marker_clause,
    );
    let parse_special = prompt.contains("<｜hy_") || prompt.contains("<|") || prompt.contains("[|");
    let tokens = vocab.tokenize(prompt.as_bytes(), !uses_hy_template, parse_special);
    if tokens.is_empty() {
        return Ok((orig_source.to_string(), ConfidenceSummary::default()));
    }

    let max_new_tokens =
        estimate_max_output_tokens_for_prompt(src_toks, prompt_source, norm_target, loaded.n_ctx);
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

    let mut sampler = if use_retry_sampler {
        build_retry_sampler(vocab.n_tokens(), style)
    } else if style.trim().eq_ignore_ascii_case("polished") {
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
    let mut conf_tracker = ConfidenceTracker::new();
    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut piece_buf = Vec::<u8>::with_capacity(32);
    let mut output = String::with_capacity((prompt_source.len() * 2).min(4096));
    let mut cur_pos = total_prompt as i32;
    let max_pos = (total_prompt as u32)
        .saturating_add(max_new_tokens)
        .min(needed_ctx.saturating_sub(4));

    while (cur_pos as u32) < max_pos {
        if CANCEL_REQUESTED.load(Ordering::Relaxed) {
            return Err("Translation cancelled".to_string());
        }
        let logit_idx = batch.n_tokens() - 1;
        let token = sampler.sample(&ctx, logit_idx);
        sampler.accept(token);
        if vocab.is_eog(token) {
            break;
        }
        let lp = token_log_prob(ctx.get_logits_ith(logit_idx), token);
        conf_tracker.push(lp);
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
    let mut finalized = finalize_translated_unit(orig_source, norm_target, &output);
    apply_ios_glossary_post_enforce(orig_source, &mut finalized, seg_glossary);
    Ok((finalized, conf_tracker.summary()))
}

fn translate_single_unit(
    loaded: &LoadedModel,
    source: &str,
    norm_target: &str,
    style: &str,
    page_title: Option<&str>,
    all_rules: &[IosGlossaryRule],
) -> Result<String, String> {
    if let Some(shortcut) = shortcut_translation(source, norm_target) {
        return Ok(shortcut);
    }
    let seg_rules = filter_ios_glossary(source, all_rules);
    if let Some(gloss_shortcut) = glossary_exact_segment_shortcut(source, &seg_rules) {
        return Ok(gloss_shortcut);
    }
    let preprocessed = preprocess_source_text(source, norm_target);
    let prompt_src = apply_ios_glossary_pre_substitute(&preprocessed, &seg_rules);

    let (first, first_conf) = run_completion_once(
        loaded,
        source,
        &prompt_src,
        norm_target,
        style,
        page_title,
        &seg_rules,
        false,
        false,
    )?;
    if looks_like_prompt_echo(source, &first) || looks_untranslated(source, &first, norm_target) {
        let (retried, _) = run_completion_once(
            loaded,
            source,
            &prompt_src,
            norm_target,
            style,
            None,
            &seg_rules,
            true,
            false,
        )?;
        if !looks_untranslated(source, &retried, norm_target) {
            return Ok(retried);
        }
        if source.contains("[[") && source.contains("]]") {
            let plain = unwrap_inline_markers_for_shortcut(source);
            let plain_trimmed = plain.trim();
            if !plain_trimmed.is_empty() && plain_trimmed != source.trim() {
                let plain_rules = filter_ios_glossary(plain_trimmed, all_rules);
                let plain_pre = preprocess_source_text(plain_trimmed, norm_target);
                let plain_prompt = apply_ios_glossary_pre_substitute(&plain_pre, &plain_rules);
                if let Ok((plain_tr, _)) = run_completion_once(
                    loaded,
                    plain_trimmed,
                    &plain_prompt,
                    norm_target,
                    style,
                    None,
                    &plain_rules,
                    true,
                    false,
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
    } else if first_conf.is_low_confidence(source.chars().count())
        || has_unexpected_script_glitch(source, &first, norm_target)
    {
        if let Ok((cand2, cand2_conf)) = run_completion_once(
            loaded,
            source,
            &prompt_src,
            norm_target,
            style,
            page_title,
            &seg_rules,
            false,
            true,
        ) {
            if should_prefer_retry_candidate(
                source,
                norm_target,
                &first,
                first_conf,
                &cand2,
                cand2_conf,
            ) {
                return Ok(cand2);
            }
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
    let (page_title, all_rules) = parse_ios_glossary_hint(glossary_hint);
    let top_rules = filter_ios_glossary(source, &all_rules);
    if let Some(gloss_shortcut) = glossary_exact_segment_shortcut(source, &top_rules) {
        return Ok(gloss_shortcut);
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
                    page_title.as_deref(),
                    &all_rules,
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
                page_title.as_deref(),
                &all_rules,
            )?);
        }
        return Ok(join_translated_sentences(&out_parts, &norm_target));
    }

    translate_single_unit(
        loaded,
        source,
        &norm_target,
        style,
        page_title.as_deref(),
        &all_rules,
    )
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
