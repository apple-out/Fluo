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

fn normalize_target_lang(lang: &str) -> &'static str {
    match lang.trim().to_lowercase().as_str() {
        "ko" | "kor" | "korean" | "한국어" => "Korean",
        "en" | "eng" | "english" | "영어" => "English",
        "ja" | "jpn" | "japanese" | "일본어" | "日本語" => "Japanese",
        "zh" | "zh-cn" | "zh-hans" | "chinese" | "중국어" | "中文" | "中文(简体)" | "简体中文" => {
            "Chinese"
        }
        "zh-tw" | "zh-hant" | "繁體中文" => "Traditional Chinese",
        _ => "Korean",
    }
}

fn hy_mt2_zh_language_name(norm_lang: &str) -> &str {
    match norm_lang {
        "Korean" => "韩语",
        "English" => "英语",
        "Japanese" => "日语",
        "Chinese" => "中文",
        "Traditional Chinese" => "繁体中文",
        _ => norm_lang,
    }
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
    const PAIRS: [(char, char); 5] = [
        ('「', '」'),
        ('『', '』'),
        ('“', '”'),
        ('"', '"'),
        ('‘', '’'),
    ];
    PAIRS
        .iter()
        .any(|(open, close)| trimmed.starts_with(*open) && trimmed.ends_with(*close))
}

fn hy_mt2_style_tag(style: &str, is_dialogue: bool) -> Option<&'static str> {
    match style.trim().to_ascii_lowercase().as_str() {
        "literal" => {
            Some("faithful literal translation preserving original meaning and sentence structure")
        }
        "novel" | "auto:novel" => is_dialogue.then_some("natural colloquial dialogue"),
        "auto:casual" => is_dialogue.then_some("natural casual dialogue"),
        _ => is_dialogue.then_some("natural colloquial dialogue"),
    }
}

fn build_prompt(
    model: &LlamaModel,
    source: &str,
    target_lang: &str,
    style: &str,
    glossary_hint: &str,
) -> (String, bool) {
    let target = normalize_target_lang(target_lang);
    let tmpl_raw = model
        .meta_val_str("tokenizer.chat_template")
        .unwrap_or_default();
    let uses_hy_template =
        tmpl_raw.is_empty() || tmpl_raw.contains("hy_User") || tmpl_raw.contains("hy_Assistant");

    let is_dialogue = is_dialogue_segment(source);
    let style_clause = hy_mt2_style_tag(style, is_dialogue)
        .map(|tag| format!(" Style: [{tag}]."))
        .unwrap_or_default();
    let is_zh_pair =
        matches!(target, "Chinese" | "Traditional Chinese") || source_is_chinese(source);
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

    let has_markers = source.contains("[[") && source.contains("]]");
    let has_link_markers = has_markers && source.contains('|');
    let has_img_markers = source.contains("[[img");

    if use_zh_template {
        let zh_lang = hy_mt2_zh_language_name(target);
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
            "Translate the following segment into {target}{marker_clause}, without additional explanation.{style_clause}\n\n{source}"
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
            "[Translation]",
            "Translation:",
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
        if !l.is_empty() {
            kept.push(l);
        }
    }
    if kept.is_empty() {
        s.trim().to_string()
    } else {
        kept.join("\n").trim().to_string()
    }
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
    let (prompt, uses_hy_template) =
        build_prompt(&loaded.model, source, target_lang, style, glossary_hint);
    let vocab = loaded.model.vocab();
    let parse_special = prompt.contains("<｜hy_") || prompt.contains("<|") || prompt.contains("[|");
    let tokens = vocab.tokenize(prompt.as_bytes(), !uses_hy_template, parse_special);
    if tokens.is_empty() {
        return Ok(source.to_string());
    }

    let max_new_tokens: u32 = ((source.chars().count() as u32) * 3 + 96).clamp(128, 1024);
    let needed_ctx = ((tokens.len() as u32) + max_new_tokens + 32)
        .min(loaded.n_ctx)
        .max(1024);

    let ctx_params = LlamaContextParams::default()
        .with_n_ctx(NonZeroU32::new(needed_ctx))
        .with_n_batch(256)
        .with_n_ubatch(256)
        .with_n_seq_max(1)
        .with_no_perf(true)
        .with_n_threads(4)
        .with_n_threads_batch(4);

    let mut ctx = loaded
        .model
        .new_context(&loaded.backend, ctx_params)
        .map_err(|e| format!("Context creation failed: {e}"))?;

    let mut batch = LlamaBatch::new(256, 1);
    let total_prompt = tokens.len();
    for (chunk_idx, chunk) in tokens.chunks(256).enumerate() {
        if CANCEL_REQUESTED.load(Ordering::Relaxed) {
            return Err("Translation cancelled".to_string());
        }
        batch.clear();
        let base_pos = chunk_idx * 256;
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
        piece_buf.clear();
        vocab.token_to_piece_into(token, &mut piece_buf, false, None);
        if !piece_buf.is_empty() {
            if let Some(cap) = decoder.max_utf8_buffer_length(piece_buf.len()) {
                output.reserve(cap);
            }
            let _ = decoder.decode_to_string(&piece_buf, &mut output, false);
        }
        batch.clear();
        batch
            .add(token, cur_pos, &[0], true)
            .map_err(|e| format!("Token batch add failed: {e}"))?;
        cur_pos += 1;
        ctx.decode(&mut batch)
            .map_err(|e| format!("Token decode failed: {e}"))?;
    }

    Ok(clean_translation_output(&output))
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
