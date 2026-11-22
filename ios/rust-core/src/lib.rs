use std::ffi::{CStr, CString};
use std::num::NonZeroU32;
use std::os::raw::c_char;
use std::path::PathBuf;
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
        "zh" | "zh-cn" | "zh-hans" | "chinese" | "중국어" | "中文" | "中文(简体)" | "简体中文" => "Chinese",
        "zh-tw" | "zh-hant" | "繁體中文" => "Traditional Chinese",
        _ => "Korean",
    }
}

fn build_prompt(
    model: &LlamaModel,
    source: &str,
    target_lang: &str,
    style: &str,
    glossary_hint: &str,
) -> String {
    let target = normalize_target_lang(target_lang);
    let tone_hint = match style.trim().to_ascii_lowercase().as_str() {
        "literal" => "Translate accurately and closely to the source wording.",
        "novel" => "Use vivid literary prose matching dialogue register and tone.",
        "polished" => "Translate cleanly in a refined editorial style.",
        "natural" => "Translate naturally and idiomatically.",
        _ => "Choose the clearest natural tone fitting the context (auto).",
    };
    let glossary_section = if glossary_hint.trim().is_empty() {
        String::new()
    } else {
        format!("\nMandatory Glossary (apply strictly):\n{}\n", glossary_hint.trim())
    };
    let user_prompt = format!(
        "Translate the following segment into {target}, preserving inline link/media tags like [[1|text]] or [[img1]] verbatim. Output ONLY the translated text.\nStyle: {tone_hint}{glossary_section}\n\n{source}"
    );
    if let Ok(tmpl) = model.chat_template(None) {
        if let Ok(msg) = LlamaChatMessage::new("user".to_string(), user_prompt.clone()) {
            if let Ok(rendered) = model.apply_chat_template(&tmpl, &[msg], true) {
                if !rendered.trim().is_empty() {
                    return rendered;
                }
            }
        }
    }
    format!("<｜hy_begin▁of▁sentence｜><｜hy_User｜>{user_prompt}<｜hy_Assistant｜>")
}

fn clean_translation_output(raw: &str) -> String {
    let mut s = raw.trim().to_string();
    if let Some(end_think) = s.find("</think>") {
        s = s[end_think + "</think>".len()..].trim().to_string();
    }
    for stop in ["<｜hy_User｜>", "<｜end▁of▁sentence｜>", "<|endoftext|>"] {
        if let Some(pos) = s.find(stop) {
            s = s[..pos].trim().to_string();
        }
    }
    s
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
    let prompt = build_prompt(&loaded.model, source, target_lang, style, glossary_hint);
    let uses_hy_template = prompt.starts_with("<｜hy_begin▁of▁sentence｜>");
    let vocab = loaded.model.vocab();
    let tokens = vocab.tokenize(prompt.as_bytes(), !uses_hy_template, true);
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

    let mut sampler = LlamaSampler::chain_simple([
        LlamaSampler::penalties(vocab.n_tokens(), 64, 1.05, 0.0, 0.0),
        LlamaSampler::temp(0.2),
        LlamaSampler::dist(42),
    ]);

    let mut decoder = encoding_rs::UTF_8.new_decoder();
    let mut piece_buf = Vec::<u8>::with_capacity(32);
    let mut output = String::with_capacity((source.len() * 2).min(4096));
    let mut cur_pos = total_prompt as i32;
    let max_pos = (total_prompt as u32)
        .saturating_add(max_new_tokens)
        .min(needed_ctx.saturating_sub(4));

    while (cur_pos as u32) < max_pos {
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
pub extern "C" fn fluo_load_model(
    model_path: *const c_char,
    n_gpu_layers: i32,
    n_ctx: u32,
) -> i32 {
    let Some(path_str) = cstr_to_string(model_path) else {
        return -1;
    };
    let path = PathBuf::from(&path_str);
    if !path.is_file() {
        return -2;
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

    let Ok(backend) = get_backend() else {
        return -3;
    };
    let ngl = if n_gpu_layers < 0 { 99 } else { n_gpu_layers as u32 };
    let model_params = LlamaModelParams::default().with_n_gpu_layers(ngl);
    let Ok(model) = LlamaModel::load_from_file(&backend, &path, &model_params) else {
        return -4;
    };
    *guard = Some(LoadedModel {
        model_path: path_str,
        n_ctx: safe_ctx,
        backend,
        model,
    });
    0
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

    let guard = match active_slot().lock() {
        Ok(g) => g,
        Err(poisoned) => poisoned.into_inner(),
    };
    let Some(loaded) = guard.as_ref() else {
        return json_err("No on-device model loaded");
    };

    match translate_with_model(loaded, &source, &target, &style_str, &glossary) {
        Ok(tr) => json_ok(&tr),
        Err(err) => json_err(&err),
    }
}

#[no_mangle]
pub extern "C" fn fluo_unload_model() {
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
