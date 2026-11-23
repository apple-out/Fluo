mod adblock;
mod ai;
mod browser;
mod browser_features;
mod credentials;
mod db;
mod hy_mt2;
mod model;
mod text;

use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};
use std::io::Write;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex, MutexGuard};

use rusqlite::Connection;
use tauri::webview::WebviewBuilder;
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, State, WebviewUrl};
use tauri_plugin_opener::OpenerExt;
use tokio_util::sync::CancellationToken;

use browser_features::{CloudAccountRoot, DetectedBrowserProfile};
use hy_mt2::{
    HardwareProfile, HyMt2Engine, HyMt2ModelState, ModelDownloadProgress, TranslationStreamEvent,
};
use model::{
    AddressSuggestion, AppData, AppSettings, AutofillProfile, Bookmark, BrowserState,
    ConnectionTest, ContextPair, CookieEntry, DownloadItem, ExternalImportSummary,
    FeedbackConfigStatus, FeedbackReportInput, FeedbackSubmitResult, FindResult, GlossaryEntry,
    HistoryItem, ModelInfo, ProviderKind, SavedLogin, SearchHistoryItem, Secrets, SegmentInput,
    SegmentResult, SettingsView, SyncConfig, TranslationHistory, UserScriptItem, WebExtensionInfo,
};

pub const FEEDBACK_REPO_URL: &str = "https://github.com/apple-out/transratebrowser";
const FEEDBACK_APP_TOKEN: &str = "transrate-browser-feedback";
const BUILTIN_FEEDBACK_WEBHOOK_URL: Option<&str> = option_env!("TB_FEEDBACK_WEBHOOK_URL");

pub struct AppState {
    pub data_dir: PathBuf,
    pub local_data_dir: PathBuf,
    pub models_dir: PathBuf,
    pub extensions_dir: PathBuf,
    pub webview_args: String,
    pub webview_extensions_enabled: bool,
    pub db: Mutex<Connection>,
    pub settings: Mutex<AppSettings>,
    pub settings_write_lock: Mutex<()>,
    pub secrets: Mutex<Secrets>,
    pub client: reqwest::Client,
    pub tabs: Mutex<browser::Tabs>,
    pub budgets: Mutex<HashMap<ProviderKind, ai::RequestBudget>>,
    pub ai_request_gate: tokio::sync::Mutex<()>,
    pub cancel_generation: tokio::sync::watch::Sender<u64>,
    pub active_cancel_token: Mutex<Option<CancellationToken>>,
    pub hy_mt2: Arc<HyMt2Engine>,
    pub ad_block_enabled: Arc<AtomicBool>,
    pub diagnostics_log: Mutex<VecDeque<String>>,
}

pub fn push_diagnostic(state: &AppState, category: &str, message: &str) {
    let cleaned = sanitize_diagnostic_message(state, message);
    let ts = db::now_secs();
    let line = format!("[{ts}] [{category}] {cleaned}");
    let mut log = lock(&state.diagnostics_log);
    log.push_back(line);
    while log.len() > 60 {
        log.pop_front();
    }
}

fn sanitize_diagnostic_message(state: &AppState, raw: &str) -> String {
    let models = state.models_dir.to_string_lossy();
    let local = state.local_data_dir.to_string_lossy();
    let data = state.data_dir.to_string_lossy();
    let home = std::env::var_os("USERPROFILE")
        .or_else(|| std::env::var_os("HOME"))
        .map(|s| s.to_string_lossy().into_owned())
        .unwrap_or_default();
    sanitize_diagnostic_text(&[models.as_ref(), local.as_ref(), data.as_ref()], &home, raw)
}

fn sanitize_diagnostic_text(app_dirs: &[&str], home_dir: &str, raw: &str) -> String {
    let mut out = raw.replace(['\r', '\n'], " ");
    for sensitive_dir in app_dirs {
        if !sensitive_dir.is_empty() {
            out = out.replace(sensitive_dir, "<APP_DIR>");
        }
    }
    if !home_dir.is_empty() {
        out = out.replace(home_dir, "<HOME_DIR>");
    }
    // Redact URL query strings / tokens that could carry personal session params.
    if let Some(q_idx) = out.find("?token=") {
        out.truncate(q_idx);
        out.push_str("?token=<REDACTED>");
    }
    if out.chars().count() > 300 {
        out = out.chars().take(300).collect::<String>() + "…";
    }
    out
}

fn normalize_ui_language(raw: &str) -> Option<&'static str> {
    let clean = raw.trim().to_ascii_lowercase().replace('_', "-");
    if clean == "ko" || clean.starts_with("ko-") {
        Some("ko")
    } else if clean == "en" || clean.starts_with("en-") {
        Some("en")
    } else if clean == "ja" || clean.starts_with("ja-") {
        Some("ja")
    } else if clean == "zh" || clean == "zh-cn" || clean.starts_with("zh-hans") {
        Some("zh-CN")
    } else {
        None
    }
}

fn default_target_language_for_ui(ui_language: &str) -> &'static str {
    match ui_language {
        "en" => "English",
        "ja" => "日本語",
        "zh-CN" => "中文(简体)",
        _ => "한국어",
    }
}

fn detect_system_ui_language() -> String {
    #[cfg(windows)]
    {
        use windows::Win32::Globalization::GetUserDefaultLocaleName;
        let mut buf = [0_u16; 85];
        let len = unsafe { GetUserDefaultLocaleName(&mut buf) };
        if len > 1 {
            let slice_len = (len as usize).saturating_sub(1).min(buf.len());
            if let Ok(locale) = String::from_utf16(&buf[..slice_len]) {
                if let Some(norm) = normalize_ui_language(&locale) {
                    return norm.to_string();
                }
                return "en".to_string();
            }
        }
    }
    for key in ["LC_ALL", "LANG"] {
        if let Ok(val) = std::env::var(key)
            && let Some(norm) = normalize_ui_language(&val)
        {
            return norm.to_string();
        }
    }
    "ko".to_string()
}

fn starter_settings_for_hardware(hw: &hy_mt2::HardwareProfile) -> AppSettings {
    let mut starter = AppSettings::default();
    let ui_lang = detect_system_ui_language();
    starter.ui_language = ui_lang.clone();
    starter.target_language = default_target_language_for_ui(&ui_lang).into();
    if hw.recommended_tier != "api-fallback" && !hw.recommended_model_id.trim().is_empty() {
        starter.provider = ProviderKind::HyMt2;
        starter.model = hw.recommended_model_id.clone();
    }
    starter
}

pub fn lock<T>(mutex: &Mutex<T>) -> MutexGuard<'_, T> {
    mutex
        .lock()
        .unwrap_or_else(|poisoned| poisoned.into_inner())
}

fn boxed<E: std::fmt::Display>(error: E) -> Box<dyn std::error::Error> {
    error.to_string().into()
}

pub fn settings_view_pub(state: &AppState) -> SettingsView {
    settings_view(state)
}

fn settings_view(state: &AppState) -> SettingsView {
    let settings = lock(&state.settings);
    let secrets = lock(&state.secrets);
    let has_codex = secrets
        .codex_access_token
        .as_deref()
        .map(str::trim)
        .is_some_and(|v| !v.is_empty())
        || secrets
            .codex_refresh_token
            .as_deref()
            .map(str::trim)
            .is_some_and(|v| !v.is_empty());
    SettingsView {
        settings: settings.clone(),
        has_api_key: secrets
            .api_key
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty()),
        has_bridge_token: secrets
            .bridge_token
            .as_deref()
            .map(str::trim)
            .is_some_and(|value| !value.is_empty()),
        has_codex_auth: has_codex,
        codex_account_id: secrets.codex_account_id.clone(),
        codex_email: secrets.codex_email.clone(),
    }
}

pub(crate) fn validate_settings(mut settings: AppSettings) -> Result<AppSettings, String> {
    settings.search_engine = settings.search_engine.trim().to_ascii_lowercase();
    if !matches!(
        settings.search_engine.as_str(),
        "google" | "bing" | "duckduckgo"
    ) {
        return Err("지원하지 않는 기본 검색 엔진입니다.".into());
    }
    settings.startup_mode = settings.startup_mode.trim().to_ascii_lowercase();
    if !matches!(
        settings.startup_mode.as_str(),
        "restore" | "custom" | "search"
    ) {
        return Err("지원하지 않는 시작 페이지 모드입니다.".into());
    }
    settings.home_url = settings.home_url.trim().to_string();
    if !settings.home_url.is_empty() {
        settings.home_url = text::resolve_address(&settings.home_url, &settings.search_engine)?;
    }
    settings.tracking_prevention = settings.tracking_prevention.trim().to_ascii_lowercase();
    if !matches!(
        settings.tracking_prevention.as_str(),
        "basic" | "balanced" | "strict"
    ) {
        return Err("지원하지 않는 추적 방지 수준입니다.".into());
    }
    settings.popup_policy = settings.popup_policy.trim().to_ascii_lowercase();
    if !matches!(
        settings.popup_policy.as_str(),
        "block" | "block-all" | "allow"
    ) {
        return Err("지원하지 않는 팝업 차단 정책입니다.".into());
    }
    settings.ad_block_allowlist = clean_host_list(settings.ad_block_allowlist);
    settings.popup_allowed_origins = clean_host_list(settings.popup_allowed_origins);
    settings.sleep_allowlist = clean_host_list(settings.sleep_allowlist);
    settings.sleep_timeout_minutes = settings.sleep_timeout_minutes.clamp(1, 1_440);
    settings.max_awake_tabs = settings.max_awake_tabs.clamp(1, 50);
    settings.target_language = settings.target_language.trim().to_string();
    if settings.target_language.is_empty() || settings.target_language.chars().count() > 100 {
        return Err("번역 대상 언어는 1~100자로 입력하세요.".into());
    }
    let ui_trimmed = settings.ui_language.trim();
    if ui_trimmed.is_empty() {
        settings.ui_language = "ko".into();
    } else if let Some(norm) = normalize_ui_language(ui_trimmed) {
        settings.ui_language = norm.to_string();
    } else {
        return Err("지원하지 않는 UI 언어입니다 (ko, en, ja, zh-CN).".into());
    }
    settings.style = settings.style.trim().to_ascii_lowercase();
    if !matches!(
        settings.style.as_str(),
        "auto" | "natural" | "literal" | "polished" | "novel"
    ) {
        return Err("지원하지 않는 번역 스타일입니다.".into());
    }
    settings.theme = settings.theme.trim().to_ascii_lowercase();
    if !matches!(settings.theme.as_str(), "system" | "light" | "dark" | "sepia") {
        return Err("지원하지 않는 테마입니다.".into());
    }
    settings.base_url = settings.base_url.trim().to_string();
    settings.model = settings.model.trim().to_string();
    if settings.base_url.len() > 2_048 || settings.model.chars().count() > 256 {
        return Err("Provider URL은 2,048자, 모델 이름은 256자 이내로 입력하세요.".into());
    }
    if settings.provider == ProviderKind::HyMt2
        && !settings.model.is_empty()
        && hy_mt2::find_spec(&settings.model).is_none()
    {
        settings.model = hy_mt2::DEFAULT_HY_MT2_MODEL_ID.to_string();
    }
    if settings.requests_per_hour > 10_000 || settings.requests_per_day > 100_000 {
        return Err("요청 한도가 허용 범위를 초과했습니다.".into());
    }
    if settings.minimum_request_interval_ms > 60_000 {
        return Err("요청 간 최소 간격은 60,000ms 이하여야 합니다.".into());
    }
    if !settings.translation_font_scale.is_finite()
        || settings.translation_font_scale < 0.8
        || settings.translation_font_scale > 1.6
    {
        return Err("번역 글자 크기 배율은 0.8~1.6 사이여야 합니다.".into());
    }
    if !settings.translation_line_height.is_finite()
        || settings.translation_line_height < 1.2
        || settings.translation_line_height > 2.2
    {
        return Err("번역 줄 간격은 1.2~2.2 사이여야 합니다.".into());
    }
    settings.hy_mt2_tier = settings.hy_mt2_tier.trim().to_ascii_lowercase();
    if settings.hy_mt2_tier.is_empty() {
        settings.hy_mt2_tier = "auto".into();
    }
    settings.hy_mt2_gpu_layers = settings.hy_mt2_gpu_layers.clamp(-1, 200);
    settings.hy_mt2_ctx_size = settings.hy_mt2_ctx_size.clamp(2048, 32768);
    settings.hy_mt2_idle_unload_secs = settings.hy_mt2_idle_unload_secs.clamp(0, 3600);
    settings.hy_mt2_threads = settings.hy_mt2_threads.min(64);
    settings.feedback_endpoint_url = settings.feedback_endpoint_url.trim().to_string();
    if !settings.feedback_endpoint_url.is_empty() {
        validate_feedback_endpoint_url(&settings.feedback_endpoint_url)?;
    }
    Ok(settings)
}

fn clean_host_list(list: Vec<String>) -> Vec<String> {
    let mut seen = BTreeSet::new();
    for item in list {
        let trimmed = item.trim().to_ascii_lowercase();
        if !trimmed.is_empty() && trimmed.len() <= 256 {
            seen.insert(trimmed);
        }
    }
    seen.into_iter().take(500).collect()
}

async fn wait_for_request_interval(
    wait: std::time::Duration,
    cancel_generation: Option<&mut tokio::sync::watch::Receiver<u64>>,
) -> Result<(), String> {
    let mut cancel_generation = cancel_generation;
    if cancel_generation
        .as_deref()
        .is_some_and(|receiver| receiver.has_changed().unwrap_or(false))
    {
        return Err("번역을 취소했습니다.".into());
    }
    if wait.is_zero() {
        return Ok(());
    }
    if let Some(receiver) = cancel_generation.as_deref_mut() {
        tokio::select! {
            _ = tokio::time::sleep(wait) => {},
            _ = receiver.changed() => return Err("번역을 취소했습니다.".into()),
        }
    } else {
        tokio::time::sleep(wait).await;
    }
    if cancel_generation
        .as_deref()
        .is_some_and(|receiver| receiver.has_changed().unwrap_or(false))
    {
        return Err("번역을 취소했습니다.".into());
    }
    Ok(())
}

async fn reserve_provider_request(
    state: &AppState,
    settings: &AppSettings,
    cancel_generation: Option<&mut tokio::sync::watch::Receiver<u64>>,
) -> Result<(), String> {
    if settings.provider == ProviderKind::HyMt2 {
        if cancel_generation
            .as_deref()
            .is_some_and(|receiver| receiver.has_changed().unwrap_or(false))
        {
            return Err("번역을 취소했습니다.".into());
        }
        return Ok(());
    }
    let min_interval_ms = settings.minimum_request_interval_ms;
    let mut cancel_generation = cancel_generation;
    loop {
        let wait = {
            let mut budgets = lock(&state.budgets);
            let budget = budgets.entry(settings.provider).or_default();
            budget.check(settings.requests_per_hour, settings.requests_per_day)?;
            budget.remaining_interval(min_interval_ms)
        };
        wait_for_request_interval(wait, cancel_generation.as_deref_mut()).await?;

        let mut budgets = lock(&state.budgets);
        let budget = budgets.entry(settings.provider).or_default();
        budget.check(settings.requests_per_hour, settings.requests_per_day)?;
        if !budget.remaining_interval(min_interval_ms).is_zero() {
            continue;
        }
        budget.record();
        return Ok(());
    }
}

fn transient_secrets(
    state: &AppState,
    api_key: Option<String>,
    bridge_token: Option<String>,
) -> Secrets {
    let saved = lock(&state.secrets).clone();
    let clean = |value: Option<String>| {
        value
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };
    Secrets {
        api_key: clean(api_key).or(saved.api_key),
        bridge_token: clean(bridge_token).or(saved.bridge_token),
        ..saved
    }
}

async fn ensure_fresh_codex_secrets(state: &AppState, secrets: &mut Secrets) {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0);
    let needs_refresh = secrets
        .codex_access_token
        .as_deref()
        .map(str::trim)
        .is_none_or(str::is_empty)
        || secrets.codex_expires_at.is_some_and(|exp| now + 90 >= exp);
    if !needs_refresh {
        return;
    }
    let Some(refresh_token) = secrets
        .codex_refresh_token
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_string)
    else {
        return;
    };
    let acc_hint = secrets.codex_account_id.clone();
    if let Ok(bundle) = ai::refresh_codex_token(&state.client, &refresh_token, acc_hint.as_deref()).await {
        let updated = {
            let mut guard = lock(&state.secrets);
            guard.codex_access_token = Some(bundle.access_token.clone());
            if let Some(rt) = bundle.refresh_token.filter(|s| !s.trim().is_empty()) {
                guard.codex_refresh_token = Some(rt);
            }
            if let Some(acc) = bundle.account_id.filter(|s| !s.trim().is_empty()) {
                guard.codex_account_id = Some(acc);
            }
            if let Some(exp) = bundle.expires_at {
                guard.codex_expires_at = Some(exp);
            }
            if let Some(email) = bundle.email.filter(|s| !s.trim().is_empty()) {
                guard.codex_email = Some(email);
            }
            guard.clone()
        };
        let _ = credentials::save(&state.data_dir, &updated);
        *secrets = updated;
    }
}

#[tauri::command]
fn browser_state(state: State<'_, AppState>) -> BrowserState {
    lock(&state.tabs).snapshot()
}

#[tauri::command]
async fn new_tab(app: AppHandle, url: Option<String>) -> Result<BrowserState, String> {
    let url = url.unwrap_or_else(|| effective_home_url(&app));
    browser::open_tab(&app, &url)
}

#[tauri::command]
async fn new_private_tab(app: AppHandle, url: Option<String>) -> Result<BrowserState, String> {
    let url = url.unwrap_or_else(|| effective_home_url(&app));
    browser::open_private_tab(&app, &url)
}

#[tauri::command]
fn pin_tab(app: AppHandle, tab_id: String) -> Result<BrowserState, String> {
    browser::pin_tab(&app, &tab_id)
}

#[tauri::command]
fn toggle_mute_tab(app: AppHandle, tab_id: String) -> Result<BrowserState, String> {
    browser::toggle_mute_tab(&app, &tab_id)
}

#[tauri::command]
fn sleep_tab(app: AppHandle, tab_id: String) -> Result<BrowserState, String> {
    browser::sleep_tab(&app, &tab_id)
}

#[tauri::command]
fn set_window_title(app: AppHandle, title: String) -> Result<(), String> {
    browser::set_window_title(&app, &title)
}

#[tauri::command]
fn close_other_tabs(app: AppHandle, tab_id: String) -> Result<BrowserState, String> {
    browser::close_other_tabs(&app, &tab_id)
}

#[tauri::command]
fn close_tabs_to_right(app: AppHandle, tab_id: String) -> Result<BrowserState, String> {
    browser::close_tabs_to_right(&app, &tab_id)
}

/// Home pages offered by the startup flow (dedicated Fluo Start Page by default).
fn home_url(_engine: &str) -> &'static str {
    browser::DEFAULT_URL
}

pub fn effective_home_url(app: &AppHandle) -> String {
    let state = app.state::<AppState>();
    let settings = lock(&state.settings);
    if settings.startup_mode != "search" && !settings.home_url.trim().is_empty() {
        settings.home_url.clone()
    } else {
        home_url(&settings.search_engine).to_string()
    }
}

#[tauri::command]
async fn close_tab(app: AppHandle, tab_id: String) -> Result<BrowserState, String> {
    browser::close_tab(&app, &tab_id)
}

#[tauri::command]
async fn select_tab(app: AppHandle, tab_id: String) -> Result<BrowserState, String> {
    browser::select_tab(&app, &tab_id)
}

#[tauri::command]
fn reorder_tab(
    app: AppHandle,
    tab_id: String,
    before_id: Option<String>,
) -> Result<BrowserState, String> {
    browser::reorder_tab(&app, &tab_id, before_id.as_deref())
}

#[tauri::command]
async fn move_tab_to_workspace(
    app: AppHandle,
    tab_id: String,
    workspace_id: String,
) -> Result<BrowserState, String> {
    browser::move_tab_to_workspace(&app, &tab_id, &workspace_id)
}

#[tauri::command]
async fn create_workspace(app: AppHandle, name: String) -> Result<BrowserState, String> {
    browser::create_workspace(&app, &name)
}

#[tauri::command]
async fn switch_workspace(app: AppHandle, workspace_id: String) -> Result<BrowserState, String> {
    browser::switch_workspace(&app, &workspace_id)
}

#[tauri::command]
async fn rename_workspace(
    app: AppHandle,
    workspace_id: String,
    name: String,
) -> Result<BrowserState, String> {
    browser::rename_workspace(&app, &workspace_id, &name)
}

#[tauri::command]
async fn delete_workspace(app: AppHandle, workspace_id: String) -> Result<BrowserState, String> {
    browser::delete_workspace(&app, &workspace_id)
}

#[tauri::command]
async fn navigate(app: AppHandle, input: String) -> Result<BrowserState, String> {
    browser::navigate(&app, &input)
}

#[tauri::command]
fn go_back(app: AppHandle) -> Result<BrowserState, String> {
    browser::go_back(&app)
}

#[tauri::command]
fn go_forward(app: AppHandle) -> Result<BrowserState, String> {
    browser::go_forward(&app)
}

#[tauri::command]
fn reload(app: AppHandle) -> Result<BrowserState, String> {
    browser::reload(&app)
}

#[tauri::command]
fn stop_loading(app: AppHandle) -> Result<BrowserState, String> {
    browser::stop_loading(&app)
}

#[tauri::command]
async fn sync_reading_position(app: AppHandle) -> Result<(), String> {
    browser::sync_reading_position(&app).await
}

#[tauri::command]
async fn open_overlay(
    app: AppHandle,
    kind: String,
    anchor: Option<browser::OverlayAnchor>,
    tab_id: Option<String>,
) -> Result<(), String> {
    browser::open_overlay(&app, &kind, anchor, tab_id)
}

#[tauri::command]
fn set_shell_expanded(app: AppHandle, expanded: bool) -> Result<(), String> {
    browser::set_shell_expanded(&app, expanded)
}

#[tauri::command]
async fn poll_page_action(app: AppHandle) -> Result<Option<serde_json::Value>, String> {
    browser::poll_page_action(&app).await
}

#[tauri::command]
fn show_quick_translation(app: AppHandle, text: String, is_error: bool) -> Result<(), String> {
    browser::show_quick_translation(&app, &text, is_error)
}

#[tauri::command]
fn run_page_context_action(
    app: AppHandle,
    action: String,
    text: Option<String>,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<(), String> {
    browser::run_page_context_action(&app, &action, text, x, y)
}

#[tauri::command]
fn sync_ui_theme_to_tabs(
    app: AppHandle,
    is_dark: Option<bool>,
    accent: String,
    tinted: bool,
) -> Result<(), String> {
    browser::sync_ui_theme_to_tabs(&app, is_dark, &accent, tinted)
}

#[tauri::command]
fn save_page_translation_override(
    state: State<'_, AppState>,
    page_url: String,
    source_text: String,
    translation: String,
    target_language: Option<String>,
) -> Result<(), String> {
    let (default_lang, style) = {
        let settings = lock(&state.settings);
        (settings.target_language.clone(), settings.style.clone())
    };
    let target_language = target_language
        .map(|v| v.trim().to_string())
        .filter(|v| !v.is_empty())
        .unwrap_or(default_lang);
    let normalized = text::normalize_whitespace(&source_text);
    if normalized.is_empty() {
        return Ok(());
    }
    let source_hash = text::sha256_hex(&normalized);
    let db = lock(&state.db);
    db::save_page_translations(
        &db,
        &page_url,
        &target_language,
        &style,
        &[(source_hash, translation)],
    )
}

#[tauri::command]
async fn close_overlay(app: AppHandle) -> Result<(), String> {
    browser::close_overlay(&app)
}

#[tauri::command]
fn layout_content(app: AppHandle, top: f64) -> Result<(), String> {
    browser::layout(&app, top)
}

#[tauri::command]
async fn extract_article(app: AppHandle) -> Result<model::Article, String> {
    browser::extract_article(&app).await
}

#[tauri::command]
async fn apply_inline_translations(
    app: AppHandle,
    tab_id: String,
    page_url: String,
    translations: Vec<browser::InlineTranslation>,
    bilingual: Option<bool>,
) -> Result<usize, String> {
    browser::apply_inline_translations(
        &app,
        &tab_id,
        &page_url,
        &translations,
        bilingual.unwrap_or(false),
    )
    .await
}

#[tauri::command]
async fn restore_inline_translations(
    app: AppHandle,
    tab_id: String,
    page_url: String,
) -> Result<usize, String> {
    browser::restore_inline_translations(&app, &tab_id, &page_url).await
}

#[tauri::command]
fn load_settings(state: State<'_, AppState>) -> SettingsView {
    settings_view(&state)
}

#[tauri::command]
fn save_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    settings: AppSettings,
    api_key: Option<String>,
    bridge_token: Option<String>,
) -> Result<SettingsView, String> {
    let _save_guard = lock(&state.settings_write_lock);
    let settings = validate_settings(settings)?;
    let json = serde_json::to_string(&settings).map_err(|error| error.to_string())?;
    let previous_settings = lock(&state.settings).clone();
    let previous_secrets = lock(&state.secrets).clone();
    let mut next_secrets = previous_secrets.clone();
    if let Some(value) = api_key {
        let value = value.trim().to_string();
        next_secrets.api_key = (!value.is_empty()).then_some(value);
    }
    if let Some(value) = bridge_token {
        let value = value.trim().to_string();
        next_secrets.bridge_token = (!value.is_empty()).then_some(value);
    }
    let secrets_changed = previous_secrets.api_key != next_secrets.api_key
        || previous_secrets.bridge_token != next_secrets.bridge_token;
    if secrets_changed {
        credentials::save(&state.data_dir, &next_secrets)?;
    }
    if let Err(error) = {
        let db = lock(&state.db);
        db::set_setting(&db, "app_settings", &json)
    } {
        if secrets_changed
            && let Err(rollback_error) = credentials::save(&state.data_dir, &previous_secrets)
        {
            return Err(format!(
                "설정을 저장하지 못했습니다: {error}. 기존 인증정보 복원도 실패했습니다: {rollback_error}"
            ));
        }
        return Err(format!("설정을 저장하지 못했습니다: {error}"));
    }
    {
        let mut current_settings = lock(&state.settings);
        let mut current_secrets = lock(&state.secrets);
        *current_settings = settings.clone();
        if secrets_changed {
            *current_secrets = next_secrets;
        }
    }
    state
        .ad_block_enabled
        .store(settings.ad_block_enabled, Ordering::SeqCst);
    browser::sync_adblock_state(&app, settings.ad_block_enabled);
    if previous_settings.ui_language != settings.ui_language {
        let _ = browser::sync_ui_language_to_tabs(&app, &settings.ui_language);
    }
    let view = settings_view(&state);
    let _ = app.emit("settings-updated", view.clone());
    Ok(view)
}

#[tauri::command]
fn set_ui_language(
    app: AppHandle,
    state: State<'_, AppState>,
    ui_language: String,
    sync_target_language: Option<bool>,
) -> Result<SettingsView, String> {
    let view = {
        let _save_guard = lock(&state.settings_write_lock);
        let mut next = lock(&state.settings).clone();
        next.ui_language = ui_language;
        let mut validated = validate_settings(next)?;
        if sync_target_language == Some(true) {
            validated.target_language = default_target_language_for_ui(&validated.ui_language).to_string();
        }
        let json = serde_json::to_string(&validated).map_err(|e| e.to_string())?;
        {
            let db = lock(&state.db);
            db::set_setting(&db, "app_settings", &json)?;
        }
        *lock(&state.settings) = validated;
        settings_view(&state)
    };
    let _ = browser::sync_ui_language_to_tabs(&app, &view.settings.ui_language);
    let _ = app.emit("settings-updated", view.clone());
    Ok(view)
}

#[tauri::command]
fn complete_onboarding(
    app: AppHandle,
    state: State<'_, AppState>,
    ui_language: Option<String>,
    target_language: Option<String>,
) -> Result<SettingsView, String> {
    let view = {
        let _save_guard = lock(&state.settings_write_lock);
        let mut next = lock(&state.settings).clone();
        next.onboarding_completed = true;
        if let Some(lang) = ui_language {
            let clean = lang.trim();
            if !clean.is_empty() {
                next.ui_language = clean.to_string();
            }
        }
        if let Some(target) = target_language {
            let clean = target.trim();
            if !clean.is_empty() {
                next.target_language = clean.to_string();
            }
        }
        let validated = validate_settings(next)?;
        let json = serde_json::to_string(&validated).map_err(|e| e.to_string())?;
        {
            let db = lock(&state.db);
            db::set_setting(&db, "app_settings", &json)?;
        }
        *lock(&state.settings) = validated;
        settings_view(&state)
    };
    let _ = browser::sync_ui_language_to_tabs(&app, &view.settings.ui_language);
    let _ = app.emit("settings-updated", view.clone());
    Ok(view)
}

#[tauri::command]
fn set_translation_prefs(
    app: AppHandle,
    state: State<'_, AppState>,
    style: Option<String>,
    target_language: Option<String>,
) -> Result<SettingsView, String> {
    let view = {
        let _save_guard = lock(&state.settings_write_lock);
        let mut next = lock(&state.settings).clone();
        if let Some(s) = style {
            let clean = s.trim().to_ascii_lowercase();
            if !clean.is_empty() {
                next.style = clean;
            }
        }
        if let Some(lang) = target_language {
            let clean = lang.trim().to_string();
            if !clean.is_empty() {
                next.target_language = clean;
            }
        }
        let validated = validate_settings(next)?;
        let json = serde_json::to_string(&validated).map_err(|e| e.to_string())?;
        {
            let db = lock(&state.db);
            db::set_setting(&db, "app_settings", &json)?;
        }
        *lock(&state.settings) = validated;
        settings_view(&state)
    };
    let _ = app.emit("settings-updated", view.clone());
    Ok(view)
}

#[tauri::command]
async fn list_models(
    state: State<'_, AppState>,
    settings: AppSettings,
    api_key: Option<String>,
    bridge_token: Option<String>,
) -> Result<Vec<ModelInfo>, String> {
    let settings = validate_settings(settings)?;
    if settings.provider == ProviderKind::HyMt2 {
        return Ok(state.hy_mt2.available_provider_models().await);
    }
    let _request_guard = state.ai_request_gate.lock().await;
    let mut secrets = transient_secrets(&state, api_key, bridge_token);
    if settings.provider == ProviderKind::CodexLegacy {
        ensure_fresh_codex_secrets(&state, &mut secrets).await;
    }
    reserve_provider_request(&state, &settings, None).await?;
    ai::list_models(&state.client, &settings, &secrets).await
}

#[tauri::command]
async fn test_connection(
    state: State<'_, AppState>,
    settings: AppSettings,
    api_key: Option<String>,
    bridge_token: Option<String>,
) -> Result<ConnectionTest, String> {
    let settings = match validate_settings(settings) {
        Ok(settings) => settings,
        Err(error) => {
            return Ok(ConnectionTest {
                ok: false,
                message: error,
                model: String::new(),
            });
        }
    };
    let (test_style, _) = ai::resolve_effective_style(
        &settings.style,
        None,
        None,
        &["The quick brown fox jumps over the lazy dog."],
    );
    if settings.provider == ProviderKind::HyMt2 {
        let _request_guard = state.ai_request_gate.lock().await;
        let test_seg = vec![SegmentInput {
            id: "test-0".into(),
            text: "The quick brown fox jumps over the lazy dog.".into(),
        }];
        let context = ai::PromptContext {
            target_language: &settings.target_language,
            style: &test_style,
            project_name: None,
            previous_context: None,
            glossary: &[],
        };
        let token = CancellationToken::new();
        return match state
            .hy_mt2
            .translate_segments(&settings, &test_seg, &context, None, &[], false, token, |_| {})
            .await
        {
            Ok((res, model_id, backend)) => Ok(ConnectionTest {
                ok: !res.is_empty(),
                message: format!(
                    "{} (엔진: {})",
                    res.first()
                        .map(|r| r.translation.as_str())
                        .unwrap_or("응답 없음"),
                    backend
                ),
                model: model_id,
            }),
            Err(error) => Ok(ConnectionTest {
                ok: false,
                message: error,
                model: settings.model,
            }),
        };
    }
    let _request_guard = state.ai_request_gate.lock().await;
    let mut secrets = transient_secrets(&state, api_key, bridge_token);
    if settings.provider == ProviderKind::CodexLegacy {
        ensure_fresh_codex_secrets(&state, &mut secrets).await;
    }
    let reported_model = settings.model.clone();
    if let Err(error) = ai::endpoint(&settings, &secrets) {
        return Ok(ConnectionTest {
            ok: false,
            message: error,
            model: reported_model,
        });
    }
    let segments = vec![SegmentInput {
        id: "test-0".into(),
        text: "The quick brown fox jumps over the lazy dog.".into(),
    }];
    let context = ai::PromptContext {
        target_language: &settings.target_language,
        style: &test_style,
        project_name: None,
        previous_context: None,
        glossary: &[],
    };

    if let Err(error) = reserve_provider_request(&state, &settings, None).await {
        return Ok(ConnectionTest {
            ok: false,
            message: error,
            model: reported_model,
        });
    }

    match ai::translate(&state.client, &settings, &secrets, &segments, &context).await {
        Ok(results) if !results.is_empty() => Ok(ConnectionTest {
            ok: true,
            message: results[0].translation.clone(),
            model: reported_model,
        }),
        Ok(_) => Ok(ConnectionTest {
            ok: false,
            message: "응답에 번역문이 없습니다.".into(),
            model: reported_model,
        }),
        Err(error) => Ok(ConnectionTest {
            ok: false,
            message: error,
            model: reported_model,
        }),
    }
}

struct MissingSegment {
    input: SegmentInput,
    cache_key: String,
    source_hash: String,
    glossary: Vec<ai::GlossaryRule>,
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn translate_blocks(
    state: State<'_, AppState>,
    segments: Vec<SegmentInput>,
    style: Option<String>,
    force: Option<bool>,
    model: Option<String>,
    page_url: Option<String>,
    page_title: Option<String>,
    target_language: Option<String>,
    prior_context: Option<Vec<ContextPair>>,
    style_sample: Option<Vec<String>>,
) -> Result<Vec<SegmentResult>, String> {
    run_translation_request(
        &state,
        segments,
        style,
        force,
        model,
        page_url,
        page_title,
        target_language,
        prior_context,
        style_sample,
        None,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
#[tauri::command]
async fn translate_blocks_stream(
    state: State<'_, AppState>,
    segments: Vec<SegmentInput>,
    style: Option<String>,
    force: Option<bool>,
    model: Option<String>,
    page_url: Option<String>,
    page_title: Option<String>,
    target_language: Option<String>,
    prior_context: Option<Vec<ContextPair>>,
    style_sample: Option<Vec<String>>,
    on_event: tauri::ipc::Channel<TranslationStreamEvent>,
) -> Result<Vec<SegmentResult>, String> {
    let total = segments.len();
    match run_translation_request(
        &state,
        segments,
        style,
        force,
        model,
        page_url,
        page_title,
        target_language,
        prior_context,
        style_sample,
        Some(&on_event),
    )
    .await
    {
        Ok(items) => {
            let uncertain = items.iter().filter(|i| i.uncertain).count();
            let _ = on_event.send(TranslationStreamEvent::Done {
                total: items.len().max(total),
                uncertain,
            });
            Ok(items)
        }
        Err(error) => {
            if error.contains("취소") {
                let _ = on_event.send(TranslationStreamEvent::Cancelled {
                    message: error.clone(),
                });
            } else {
                push_diagnostic(&state, "translate-error", &error);
                let _ = on_event.send(TranslationStreamEvent::Error {
                    message: error.clone(),
                });
            }
            Err(error)
        }
    }
}

#[allow(clippy::too_many_arguments)]
async fn run_translation_request(
    state: &AppState,
    segments: Vec<SegmentInput>,
    style: Option<String>,
    force: Option<bool>,
    model: Option<String>,
    page_url: Option<String>,
    page_title: Option<String>,
    target_language: Option<String>,
    prior_context: Option<Vec<ContextPair>>,
    style_sample: Option<Vec<String>>,
    on_event: Option<&tauri::ipc::Channel<TranslationStreamEvent>>,
) -> Result<Vec<SegmentResult>, String> {
    if segments.is_empty() {
        return Ok(Vec::new());
    }
    if segments.len() > 200 {
        return Err("한 번에 번역할 문단은 200개를 넘을 수 없습니다.".into());
    }
    let mut segment_ids = HashSet::new();
    for segment in &segments {
        if segment.id.trim().is_empty() || !segment_ids.insert(segment.id.as_str()) {
            return Err("번역 문단 ID가 비어 있거나 중복되었습니다.".into());
        }
    }
    let force = force.unwrap_or(false);
    if style.as_deref().is_some_and(|style| {
        !matches!(
            style.trim().to_ascii_lowercase().as_str(),
            "auto" | "natural" | "literal" | "polished" | "novel"
        )
    }) {
        return Err("지원하지 않는 번역 스타일입니다.".into());
    }
    if model
        .as_deref()
        .is_some_and(|model| model.trim().chars().count() > 256)
    {
        return Err("모델 이름은 256자 이내로 입력하세요.".into());
    }
    let mut cancel_generation = state.cancel_generation.subscribe();
    if cancel_generation.has_changed().unwrap_or(false) {
        return Err("번역을 취소했습니다.".into());
    }
    let active_token = CancellationToken::new();
    {
        let mut slot = lock(&state.active_cancel_token);
        *slot = Some(active_token.clone());
    }
    struct TokenGuard<'a>(&'a Mutex<Option<CancellationToken>>);
    impl Drop for TokenGuard<'_> {
        fn drop(&mut self) {
            if let Ok(mut guard) = self.0.lock() {
                *guard = None;
            }
        }
    }
    let _token_guard = TokenGuard(&state.active_cancel_token);

    let mut settings = lock(&state.settings).clone();
    if let Some(model) = model.map(|model| model.trim().to_string())
        && !model.is_empty()
    {
        settings.model = model;
    }
    let mut secrets = lock(&state.secrets).clone();
    if settings.provider == ProviderKind::CodexLegacy {
        ensure_fresh_codex_secrets(state, &mut secrets).await;
    }
    let target_language = target_language
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && value.chars().count() <= 100)
        .unwrap_or_else(|| settings.target_language.clone());
    let requested_style = style
        .map(|s| s.trim().to_ascii_lowercase())
        .unwrap_or_else(|| settings.style.clone());
    let (style, detected_style) = if requested_style == "auto" {
        let mut samples: Vec<&str> = Vec::with_capacity(24);
        if let Some(ref sample_items) = style_sample {
            for item in sample_items {
                let s = item.trim();
                if !s.is_empty() {
                    samples.push(s);
                    if samples.len() >= 24 {
                        break;
                    }
                }
            }
        }
        if samples.len() < 16 {
            for seg in &segments {
                let s = seg.text.trim();
                if !s.is_empty() {
                    samples.push(s);
                    if samples.len() >= 24 {
                        break;
                    }
                }
            }
        }
        if samples.len() < 24
            && let Some(ref ctx_items) = prior_context
        {
            for pair in ctx_items {
                let s = pair.source.trim();
                if !s.is_empty() {
                    samples.push(s);
                    if samples.len() >= 24 {
                        break;
                    }
                }
            }
        }
        ai::resolve_effective_style(
            &requested_style,
            page_url.as_deref(),
            page_title.as_deref(),
            &samples,
        )
    } else {
        (requested_style.clone(), None)
    };
    let provider_key = match settings.provider {
        ProviderKind::GoogleGemini => settings.provider.as_str().to_string(),
        ProviderKind::HyMt2 => "hy-mt2|v6".to_string(),
        ProviderKind::CodexLegacy => format!(
            "{}|{}",
            settings.provider.as_str(),
            if settings.base_url.trim().is_empty() {
                ai::CODEX_BASE_URL
            } else {
                settings.base_url.trim()
            }
        ),
        _ => format!(
            "{}|{}",
            settings.provider.as_str(),
            settings.base_url.trim()
        ),
    };
    if let Some(ch) = on_event {
        let backend_label = if settings.provider == ProviderKind::HyMt2 {
            state.hy_mt2.detect_hardware_cached().active_backend.clone()
        } else {
            settings.provider.as_str().to_string()
        };
        let _ = ch.send(TranslationStreamEvent::Started {
            total: segments.len(),
            backend: backend_label,
            model: settings.model.clone(),
            detected_style: detected_style.map(str::to_string),
        });
    }
    let entries = {
        let db = lock(&state.db);
        db::list_glossary(&db)?
    };

    let mut results: HashMap<String, SegmentResult> = HashMap::new();
    let total_segments = segments.len();
    let mut missing: Vec<MissingSegment> = Vec::new();
    let mut seen_missing_hashes: HashMap<String, usize> = HashMap::new();
    let mut duplicate_ids: HashMap<String, Vec<String>> = HashMap::new();
    let mut page_entries: Vec<(String, String)> = Vec::new();
    let is_private_tab = {
        let tabs = lock(&state.tabs);
        tabs.active
            .as_ref()
            .and_then(|id| tabs.items.iter().find(|tab| &tab.id == id))
            .map(|tab| tab.is_private)
            .unwrap_or(false)
    };

    for segment in segments.iter() {
        let normalized = text::normalize_whitespace(&segment.text);
        if normalized.is_empty() {
            continue;
        }
        let source_hash = text::sha256_hex(&normalized);
        let (glossary, glossary_hash) = ai::glossary_context(&entries, &normalized);
        let cache_key = text::cache_key(
            &source_hash,
            &provider_key,
            settings.model.trim(),
            ai::PROMPT_VERSION,
            &style,
            &target_language,
            &glossary_hash,
        );
        let cached = if force {
            None
        } else {
            let db = lock(&state.db);
            db::get_translation(&db, &cache_key)?
        };
        if let Some(cached) = cached {
            if !is_private_tab {
                page_entries.push((source_hash.clone(), cached.translation.clone()));
                if let Some(url) = page_url
                    .as_deref()
                    .and_then(|url| text::normalize_page_url(url).ok())
                {
                    let db = lock(&state.db);
                    db::add_translation_history(
                        &db,
                        &url,
                        page_title.as_deref().unwrap_or(""),
                        &segment.text,
                        &cached.translation,
                        settings.model.trim(),
                    )?;
                }
            }
            let cached_result = SegmentResult {
                id: segment.id.clone(),
                translation: cached.translation,
                uncertain: cached.uncertain,
                notes: cached.notes,
                cached: true,
            };
            if let Some(ch) = on_event {
                let _ = ch.send(TranslationStreamEvent::Segment {
                    result: cached_result.clone(),
                });
            }
            results.insert(segment.id.clone(), cached_result);
            if let Some(ch) = on_event {
                let _ = ch.send(TranslationStreamEvent::Progress {
                    done: results.len(),
                    total: total_segments,
                });
            }
            continue;
        }
        if let Some(&first_idx) = seen_missing_hashes.get(&source_hash) {
            let rep_id = missing[first_idx].input.id.clone();
            duplicate_ids
                .entry(rep_id)
                .or_default()
                .push(segment.id.clone());
            continue;
        }
        seen_missing_hashes.insert(source_hash.clone(), missing.len());
        missing.push(MissingSegment {
            input: segment.clone(),
            cache_key,
            source_hash,
            glossary,
        });
    }

    if missing.is_empty() {
        if !is_private_tab && let Some(url) = page_url.as_deref() {
            let db = lock(&state.db);
            let _ = db::save_page_translations(
                &db,
                url,
                &target_language,
                &requested_style,
                &page_entries,
            );
        }
        return Ok(order_results(&segments, results));
    }

    let mut hy_prior_pairs: Vec<ContextPair> = prior_context.unwrap_or_default();
    if hy_prior_pairs.len() > 24 {
        hy_prior_pairs.drain(..hy_prior_pairs.len() - 24);
    }
    let cached_pairs: Vec<ContextPair> = segments
        .iter()
        .filter_map(|segment| {
            results.get(&segment.id).map(|res| ContextPair {
                source: segment.text.clone(),
                translation: Some(res.translation.clone()),
            })
        })
        .collect();
    if !cached_pairs.is_empty() {
        hy_prior_pairs.extend(cached_pairs);
    }

    let mut persist_translated = |item: &MissingSegment,
                                  result: &SegmentResult,
                                  model_label: &str|
     -> Result<(), String> {
        let mut stored = SegmentResult {
            id: item.input.id.clone(),
            translation: result.translation.clone(),
            uncertain: result.uncertain,
            notes: result.notes.clone(),
            cached: false,
        };
        if !ai::numeric_tokens_match(&item.input.text, &stored.translation) {
            stored.uncertain = true;
            stored
                .notes
                .push("원문과 번역문의 숫자 또는 단위 표기가 다릅니다.".into());
        }
        if !stored.uncertain {
            let db = lock(&state.db);
            db::put_translation(
                &db,
                &item.cache_key,
                &item.source_hash,
                &provider_key,
                model_label,
                &style,
                &target_language,
                &stored,
            )?;
        }
        if let Some(ch) = on_event {
            let _ = ch.send(TranslationStreamEvent::Segment {
                result: stored.clone(),
            });
        }
        results.insert(item.input.id.clone(), stored.clone());
        if let Some(dups) = duplicate_ids.get(&item.input.id) {
            for dup_id in dups {
                let mut dup_result = stored.clone();
                dup_result.id = dup_id.clone();
                dup_result.cached = true;
                if let Some(ch) = on_event {
                    let _ = ch.send(TranslationStreamEvent::Segment {
                        result: dup_result.clone(),
                    });
                }
                results.insert(dup_id.clone(), dup_result);
            }
        }
        if !is_private_tab {
            page_entries.push((item.source_hash.clone(), stored.translation.clone()));
            if let Some(url) = page_url.as_deref()
                && let Ok(url) = text::normalize_page_url(url)
            {
                let db = lock(&state.db);
                db::add_translation_history(
                    &db,
                    &url,
                    page_title.as_deref().unwrap_or(""),
                    &item.input.text,
                    &stored.translation,
                    model_label,
                )?;
            }
        }
        if let Some(ch) = on_event {
            let _ = ch.send(TranslationStreamEvent::Progress {
                done: results.len(),
                total: total_segments,
            });
        }
        Ok(())
    };

    if settings.provider == ProviderKind::HyMt2 {
        struct BufferedHyMt2DbWrite {
            cache_key: String,
            source_hash: String,
            source_text: String,
            stored: SegmentResult,
        }

        let missing_by_id: HashMap<&str, &MissingSegment> = missing
            .iter()
            .map(|item| (item.input.id.as_str(), item))
            .collect();
        let inputs: Vec<SegmentInput> = missing.iter().map(|item| item.input.clone()).collect();
        let glossary: Vec<ai::GlossaryRule> = missing
            .iter()
            .flat_map(|item| item.glossary.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let context = ai::PromptContext {
            target_language: &target_language,
            style: &style,
            project_name: None,
            previous_context: None,
            glossary: &glossary,
        };
        let model_label = if settings.model.trim().is_empty() {
            hy_mt2::DEFAULT_HY_MT2_MODEL_ID
        } else {
            settings.model.trim()
        }
        .to_string();
        let cancel_for_watch = active_token.clone();
        let watch_fut = async move {
            let _ = cancel_generation.changed().await;
            cancel_for_watch.cancel();
            std::future::pending::<()>().await;
        };

        let shared_results = std::sync::Mutex::new(std::mem::take(&mut results));
        let shared_page_entries = std::sync::Mutex::new(std::mem::take(&mut page_entries));
        let buffered_db_writes = std::sync::Mutex::new(Vec::<BufferedHyMt2DbWrite>::new());
        let done_count = std::sync::atomic::AtomicUsize::new(
            shared_results.lock().map(|g| g.len()).unwrap_or(0),
        );

        let outcome: Result<(), String> = {
            let translate_fut = state.hy_mt2.translate_segments(
                &settings,
                &inputs,
                &context,
                page_url.as_deref(),
                &hy_prior_pairs,
                force,
                active_token.clone(),
                |seg| {
                    let Some(item) = missing_by_id.get(seg.id.as_str()) else {
                        return;
                    };
                    let mut stored = SegmentResult {
                        id: item.input.id.clone(),
                        translation: seg.translation.clone(),
                        uncertain: seg.uncertain,
                        notes: seg.notes.clone(),
                        cached: false,
                    };
                    if !ai::numeric_tokens_match(&item.input.text, &stored.translation) {
                        stored.uncertain = true;
                        stored
                            .notes
                            .push("원문과 번역문의 숫자 또는 단위 표기가 다릅니다.".into());
                    }
                    if let Some(ch) = on_event {
                        let _ = ch.send(TranslationStreamEvent::Segment {
                            result: stored.clone(),
                        });
                    }
                    let mut added = 1usize;
                    if let Ok(mut res_map) = shared_results.lock() {
                        res_map.insert(item.input.id.clone(), stored.clone());
                        if let Some(dups) = duplicate_ids.get(&item.input.id) {
                            for dup_id in dups {
                                let mut dup_result = stored.clone();
                                dup_result.id = dup_id.clone();
                                dup_result.cached = true;
                                if let Some(ch) = on_event {
                                    let _ = ch.send(TranslationStreamEvent::Segment {
                                        result: dup_result.clone(),
                                    });
                                }
                                res_map.insert(dup_id.clone(), dup_result);
                                added += 1;
                            }
                        }
                    }
                    let next_done =
                        done_count.fetch_add(added, std::sync::atomic::Ordering::Relaxed) + added;
                    if let Some(ch) = on_event {
                        let _ = ch.send(TranslationStreamEvent::Progress {
                            done: next_done.min(total_segments),
                            total: total_segments,
                        });
                    }
                    if !is_private_tab && let Ok(mut pe) = shared_page_entries.lock() {
                        pe.push((item.source_hash.clone(), stored.translation.clone()));
                    }
                    if (!stored.uncertain || !is_private_tab)
                        && let Ok(mut writes) = buffered_db_writes.lock()
                    {
                        writes.push(BufferedHyMt2DbWrite {
                            cache_key: item.cache_key.clone(),
                            source_hash: item.source_hash.clone(),
                            source_text: item.input.text.clone(),
                            stored,
                        });
                    }
                },
            );
            tokio::select! {
                res = translate_fut => res.map(|_| ()),
                _ = watch_fut => Err("번역을 취소했습니다.".into()),
            }
        };

        let buffered_writes = buffered_db_writes
            .into_inner()
            .unwrap_or_else(|e| e.into_inner());
        let final_page_entries = shared_page_entries
            .into_inner()
            .unwrap_or_else(|e| e.into_inner());
        let normalized_url = if !is_private_tab {
            page_url
                .as_deref()
                .and_then(|u| text::normalize_page_url(u).ok())
        } else {
            None
        };
        if !buffered_writes.is_empty() || (!is_private_tab && page_url.is_some()) {
            let db = lock(&state.db);
            if let Ok(tx) = db.unchecked_transaction() {
                for entry in &buffered_writes {
                    if !entry.stored.uncertain {
                        let _ = db::put_translation(
                            &tx,
                            &entry.cache_key,
                            &entry.source_hash,
                            &provider_key,
                            &model_label,
                            &style,
                            &target_language,
                            &entry.stored,
                        );
                    }
                    if let Some(ref url) = normalized_url {
                        let _ = db::add_translation_history(
                            &tx,
                            url,
                            page_title.as_deref().unwrap_or(""),
                            &entry.source_text,
                            &entry.stored.translation,
                            &model_label,
                        );
                    }
                }
                if !is_private_tab
                    && !final_page_entries.is_empty()
                    && let Some(url) = page_url.as_deref()
                {
                    let _ = db::save_page_translations(
                        &tx,
                        url,
                        &target_language,
                        &requested_style,
                        &final_page_entries,
                    );
                }
                let _ = tx.commit();
            }
        }
        outcome?;
        let final_results = shared_results
            .into_inner()
            .unwrap_or_else(|e| e.into_inner());
        return Ok(order_results(&segments, final_results));
    }

    let cloud_prev_ctx: Option<String> = if hy_prior_pairs.is_empty() {
        None
    } else {
        let snippet: Vec<String> = hy_prior_pairs
            .iter()
            .rev()
            .take(5)
            .rev()
            .map(|p| match p.translation.as_deref() {
                Some(t) if !t.trim().is_empty() => format!("{} -> {}", p.source.trim(), t.trim()),
                _ => p.source.trim().to_string(),
            })
            .collect();
        Some(snippet.join("\n"))
    };

    // Keep chunks compact (~2,500 characters or 10 segments) to stay well within
    // model output token limits and reduce retries on dense wiki/table pages.
    let mut chunks: Vec<Vec<MissingSegment>> = Vec::new();
    let mut current: Vec<MissingSegment> = Vec::new();
    let mut characters = 0usize;
    for item in missing {
        let length = item.input.text.chars().count();
        if !current.is_empty() && (characters + length > 2_500 || current.len() >= 10) {
            chunks.push(std::mem::take(&mut current));
            characters = 0;
        }
        characters += length;
        current.push(item);
    }
    if !current.is_empty() {
        chunks.push(current);
    }

    use futures_util::StreamExt;
    let settings_ref = &settings;
    let secrets_ref = &secrets;
    let target_lang_ref = &target_language;
    let style_ref = &style;
    let cloud_prev_ref = cloud_prev_ctx.as_deref();
    let chunk_futures = chunks.into_iter().enumerate().map(|(index, chunk)| {
        let mut cancel_rx = cancel_generation.clone();
        async move {
            let translated = translate_chunk_with_split(
                state,
                settings_ref,
                secrets_ref,
                &chunk,
                target_lang_ref,
                style_ref,
                cloud_prev_ref,
                &mut cancel_rx,
            )
            .await
            .map_err(|error| {
                if error.contains("취소") {
                    error
                } else {
                    format!("{}번째 묶음 번역 실패: {error}", index + 1)
                }
            })?;
            Ok::<_, String>((chunk, translated))
        }
    });
    let mut completed_stream = futures_util::stream::iter(chunk_futures).buffer_unordered(4);
    while let Some(outcome) = completed_stream.next().await {
        let (chunk, translated) = outcome?;
        let by_id: HashMap<&str, &SegmentResult> = translated
            .iter()
            .map(|result| (result.id.as_str(), result))
            .collect();
        for item in &chunk {
            let Some(result) = by_id.get(item.input.id.as_str()) else {
                continue;
            };
            persist_translated(item, result, settings.model.trim())?;
        }
    }

    if !is_private_tab && let Some(url) = page_url.as_deref() {
        let db = lock(&state.db);
        let _ = db::save_page_translations(
            &db,
            url,
            &target_language,
            &requested_style,
            &page_entries,
        );
    }

    Ok(order_results(&segments, results))
}

fn should_split_chunk_error(error: &str) -> bool {
    error.contains("출력 한도")
        || error.contains("MAX_TOKENS")
        || error.contains("추론(reasoning_content)")
        || error.contains("문단별 번역")
        || error.contains("문단 형식이 올바르지")
        || error.contains("시간이 초과")
        || error.contains("timed out")
        || error.contains("413")
}

fn translate_chunk_with_split<'a>(
    state: &'a AppState,
    settings: &'a AppSettings,
    secrets: &'a Secrets,
    chunk: &'a [MissingSegment],
    target_language: &'a str,
    style: &'a str,
    previous_context: Option<&'a str>,
    cancel_generation: &'a mut tokio::sync::watch::Receiver<u64>,
) -> std::pin::Pin<
    Box<dyn std::future::Future<Output = Result<Vec<SegmentResult>, String>> + Send + 'a>,
> {
    Box::pin(async move {
        reserve_provider_request(state, settings, Some(cancel_generation)).await?;

        let inputs: Vec<SegmentInput> = chunk.iter().map(|item| item.input.clone()).collect();
        let glossary: Vec<ai::GlossaryRule> = chunk
            .iter()
            .flat_map(|item| item.glossary.iter().cloned())
            .collect::<BTreeSet<_>>()
            .into_iter()
            .collect();
        let context = ai::PromptContext {
            target_language,
            style,
            project_name: None,
            previous_context,
            glossary: &glossary,
        };

        let request = ai::translate(&state.client, settings, secrets, &inputs, &context);
        let cancelled = cancel_generation.changed();
        let result =
            match futures_util::future::select(Box::pin(request), Box::pin(cancelled)).await {
                futures_util::future::Either::Left((result, _)) => result,
                futures_util::future::Either::Right((_changed, _)) => {
                    return Err("번역을 취소했습니다.".into());
                }
            };

        match result {
            Ok(items) => Ok(items),
            Err(error) if chunk.len() > 1 && should_split_chunk_error(&error) => {
                let mid = chunk.len() / 2;
                let mut combined = translate_chunk_with_split(
                    state,
                    settings,
                    secrets,
                    &chunk[..mid],
                    target_language,
                    style,
                    previous_context,
                    cancel_generation,
                )
                .await?;
                let second = translate_chunk_with_split(
                    state,
                    settings,
                    secrets,
                    &chunk[mid..],
                    target_language,
                    style,
                    previous_context,
                    cancel_generation,
                )
                .await?;
                combined.extend(second);
                Ok(combined)
            }
            Err(error) => Err(error),
        }
    })
}

#[tauri::command]
async fn pick_region_article(app: AppHandle) -> Result<model::Article, String> {
    browser::pick_region_article(&app).await
}

#[tauri::command]
fn cancel_region_picker(app: AppHandle) -> Result<(), String> {
    browser::cancel_region_picker(&app)
}

#[tauri::command]
fn load_saved_page_translations(
    state: State<'_, AppState>,
    page_url: String,
    segments: Vec<SegmentInput>,
    target_language: Option<String>,
) -> Result<Vec<SegmentResult>, String> {
    let (default_lang, style) = {
        let settings = lock(&state.settings);
        (settings.target_language.clone(), settings.style.clone())
    };
    let target_language = target_language
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .unwrap_or(default_lang);
    let saved = {
        let db = lock(&state.db);
        db::load_page_translations(&db, &page_url, &target_language, &style)?
    };
    if saved.is_empty() {
        return Ok(Vec::new());
    }
    let mut out = Vec::new();
    for segment in segments {
        let normalized = text::normalize_whitespace(&segment.text);
        if normalized.is_empty() {
            continue;
        }
        let hash = text::sha256_hex(&normalized);
        if let Some(translation) = saved.get(&hash) {
            out.push(SegmentResult {
                id: segment.id,
                translation: translation.clone(),
                uncertain: false,
                notes: Vec::new(),
                cached: true,
            });
        }
    }
    Ok(out)
}

#[tauri::command]
fn list_bookmarks(state: State<'_, AppState>) -> Result<Vec<Bookmark>, String> {
    db::list_bookmarks(&lock(&state.db))
}

#[tauri::command]
fn save_bookmark(state: State<'_, AppState>, mut bookmark: Bookmark) -> Result<(), String> {
    bookmark.url = text::normalize_page_url(&bookmark.url)?;
    bookmark.title = bookmark.title.trim().chars().take(500).collect();
    bookmark.folder = bookmark.folder.trim().chars().take(120).collect();
    if bookmark.title.is_empty() {
        bookmark.title = bookmark.url.clone();
    }
    let db = lock(&state.db);
    db::save_bookmark(&db, &bookmark)
}

#[tauri::command]
fn delete_bookmark(state: State<'_, AppState>, id: String) -> Result<(), String> {
    db::delete_bookmark(&lock(&state.db), &id)
}

#[tauri::command]
fn rename_bookmark_folder(
    state: State<'_, AppState>,
    old_name: String,
    new_name: String,
) -> Result<usize, String> {
    db::rename_bookmark_folder(&lock(&state.db), &old_name, &new_name)
}

#[tauri::command]
fn list_bookmark_folders(state: State<'_, AppState>) -> Result<Vec<String>, String> {
    db::list_bookmark_folders(&lock(&state.db))
}

#[tauri::command]
fn create_bookmark_folder(state: State<'_, AppState>, name: String) -> Result<(), String> {
    db::create_bookmark_folder(&lock(&state.db), &name)
}

#[tauri::command]
fn delete_bookmark_folder(state: State<'_, AppState>, name: String) -> Result<(), String> {
    db::delete_bookmark_folder(&lock(&state.db), &name)
}

#[tauri::command]
async fn open_internal_tab(app: AppHandle, kind: String) -> Result<BrowserState, String> {
    browser::open_internal_tab(&app, &kind)
}

#[tauri::command]
fn list_history(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<HistoryItem>, String> {
    db::list_history(&lock(&state.db), query.trim(), limit.unwrap_or(200))
}

#[tauri::command]
fn delete_history(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    db::delete_history(&lock(&state.db), id)
}

#[tauri::command]
fn clear_history(state: State<'_, AppState>) -> Result<(), String> {
    db::clear_history(&lock(&state.db))
}

#[tauri::command]
fn address_suggestions(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<AddressSuggestion>, String> {
    db::search_address_suggestions(&lock(&state.db), &query, 10)
}

#[tauri::command]
fn list_search_history(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<SearchHistoryItem>, String> {
    db::list_search_history(&lock(&state.db), query.trim(), limit.unwrap_or(200))
}

#[tauri::command]
fn delete_search_history(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    db::delete_search_history(&lock(&state.db), id)
}

#[tauri::command]
fn clear_search_history(state: State<'_, AppState>) -> Result<(), String> {
    db::clear_search_history(&lock(&state.db))
}

#[tauri::command]
fn list_saved_logins(
    state: State<'_, AppState>,
    origin: Option<String>,
) -> Result<Vec<SavedLogin>, String> {
    db::list_saved_logins(&lock(&state.db), origin.as_deref())
}

#[tauri::command]
fn save_login(
    state: State<'_, AppState>,
    id: String,
    origin: String,
    username: String,
    password: String,
) -> Result<(), String> {
    db::save_login(&lock(&state.db), &id, &origin, &username, &password)
}

#[tauri::command]
fn delete_login(state: State<'_, AppState>, id: String) -> Result<(), String> {
    db::delete_login(&lock(&state.db), &id)
}

#[tauri::command]
fn autofill_saved_login(
    app: AppHandle,
    state: State<'_, AppState>,
    origin: String,
    username: Option<String>,
) -> Result<(), String> {
    let logins = db::list_saved_logins(&lock(&state.db), Some(&origin))?;
    let target = match username.as_deref() {
        Some(name) if !name.is_empty() => logins.into_iter().find(|l| l.username == name),
        _ => logins.into_iter().next(),
    };
    let Some(login) = target else {
        return Err("저장된 로그인 정보를 찾지 못했습니다.".into());
    };
    let payload = serde_json::json!({
        "username": login.username,
        "password": login.password,
    });
    browser::eval_on_active_tab(&app, &format!("window.__tbApplyLoginAutofill?.({payload});"))
}

#[tauri::command]
fn load_autofill_profile(state: State<'_, AppState>) -> AutofillProfile {
    let db = lock(&state.db);
    db::get_setting(&db, "autofill_profile")
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default()
}

#[tauri::command]
fn save_autofill_profile(
    state: State<'_, AppState>,
    profile: AutofillProfile,
) -> Result<AutofillProfile, String> {
    let json = serde_json::to_string(&profile).map_err(|error| error.to_string())?;
    db::set_setting(&lock(&state.db), "autofill_profile", &json)?;
    Ok(profile)
}

#[tauri::command]
fn autofill_profile_active_tab(app: AppHandle, state: State<'_, AppState>) -> Result<(), String> {
    let profile: AutofillProfile = {
        let db = lock(&state.db);
        db::get_setting(&db, "autofill_profile")
            .and_then(|json| serde_json::from_str(&json).ok())
            .unwrap_or_default()
    };
    let payload = serde_json::to_string(&profile).map_err(|e| e.to_string())?;
    browser::eval_on_active_tab(&app, &format!("window.__tbApplyProfileAutofill?.({payload});"))
}

#[tauri::command]
fn load_sync_config(state: State<'_, AppState>) -> SyncConfig {
    let db = lock(&state.db);
    browser_features::load_sync_config(&db, &state.data_dir)
}

#[tauri::command]
fn save_sync_config(
    state: State<'_, AppState>,
    mut config: SyncConfig,
) -> Result<SyncConfig, String> {
    if config.sync_dir.trim().is_empty() {
        config.sync_dir = browser_features::default_sync_dir(&state.data_dir)
            .to_string_lossy()
            .into_owned();
    }
    let db = lock(&state.db);
    browser_features::save_sync_config(&db, &config)?;
    Ok(config)
}

#[tauri::command]
fn run_sync_now(app: AppHandle, state: State<'_, AppState>) -> Result<SyncConfig, String> {
    browser_features::run_sync_now(&app, &state)
}

#[tauri::command]
fn detect_cloud_accounts() -> Vec<CloudAccountRoot> {
    browser_features::detect_cloud_accounts()
}

#[tauri::command]
fn connect_sync_account(
    app: AppHandle,
    state: State<'_, AppState>,
    provider: String,
    email: String,
    display_name: String,
    sync_dir: Option<String>,
) -> Result<SyncConfig, String> {
    browser_features::connect_sync_account(
        &app,
        &state,
        &provider,
        &email,
        &display_name,
        sync_dir.as_deref(),
    )
}

#[tauri::command]
fn disconnect_sync_account(state: State<'_, AppState>) -> Result<SyncConfig, String> {
    browser_features::disconnect_sync_account(&state)
}

#[tauri::command]
async fn start_oauth_login(app: AppHandle, provider: String) -> Result<(), String> {
    browser_features::open_account_login_window(&app, &provider)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct CodexImportResult {
    view: SettingsView,
    source: String,
}

#[tauri::command]
async fn start_codex_oauth_login(app: AppHandle) -> Result<(), String> {
    browser_features::open_codex_oauth_window(&app)
}

#[tauri::command]
fn import_local_codex_auth(app: AppHandle) -> Result<CodexImportResult, String> {
    let (view, source) = browser_features::import_local_codex_auth(&app)?;
    Ok(CodexImportResult { view, source })
}

#[tauri::command]
fn clear_codex_auth(app: AppHandle) -> Result<SettingsView, String> {
    browser_features::clear_codex_auth(&app)
}

#[tauri::command]
fn detect_external_browsers() -> Vec<DetectedBrowserProfile> {
    browser_features::detect_external_browsers()
}

#[tauri::command]
fn import_external_browser(
    state: State<'_, AppState>,
    browser_id: String,
    include_bookmarks: bool,
    include_history: bool,
) -> Result<ExternalImportSummary, String> {
    browser_features::import_from_external_browser(
        &state,
        &browser_id,
        include_bookmarks,
        include_history,
    )
}

#[tauri::command]
fn import_passwords_csv(state: State<'_, AppState>, csv_text: String) -> Result<u32, String> {
    browser_features::import_passwords_csv(&state, &csv_text)
}

#[tauri::command]
async fn list_cookies(app: AppHandle, uri: String) -> Result<Vec<CookieEntry>, String> {
    browser_features::list_cookies(&app, &uri).await
}

#[tauri::command]
async fn delete_cookie(
    app: AppHandle,
    name: String,
    domain: String,
    path: String,
) -> Result<(), String> {
    browser_features::delete_cookie(&app, &name, &domain, &path).await
}

#[tauri::command]
async fn clear_browsing_data(app: AppHandle, scope: String) -> Result<(), String> {
    browser_features::clear_browsing_data(&app, &scope).await
}

#[tauri::command]
fn list_user_scripts(state: State<'_, AppState>) -> Result<Vec<UserScriptItem>, String> {
    db::list_user_scripts(&lock(&state.db))
}

#[tauri::command]
fn save_user_script(state: State<'_, AppState>, item: UserScriptItem) -> Result<(), String> {
    db::save_user_script(&lock(&state.db), &item)
}

#[tauri::command]
fn delete_user_script(state: State<'_, AppState>, id: String) -> Result<(), String> {
    db::delete_user_script(&lock(&state.db), &id)
}

#[tauri::command]
async fn list_web_extensions(app: AppHandle) -> Result<Vec<WebExtensionInfo>, String> {
    browser_features::list_web_extensions(&app).await
}

#[tauri::command]
async fn add_web_extension(
    app: AppHandle,
    folder_path: String,
) -> Result<WebExtensionInfo, String> {
    browser_features::add_web_extension(&app, &folder_path).await
}

#[tauri::command]
async fn toggle_or_remove_web_extension(
    app: AppHandle,
    ext_id: String,
    remove: bool,
    enabled: bool,
) -> Result<(), String> {
    browser_features::toggle_or_remove_web_extension(&app, &ext_id, remove, enabled).await
}

#[tauri::command]
fn toggle_adblock_site(
    app: AppHandle,
    state: State<'_, AppState>,
    url: String,
) -> Result<SettingsView, String> {
    let host = url::Url::parse(&url)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .unwrap_or_else(|| url.trim().to_ascii_lowercase());
    if host.is_empty() {
        return Err("예외로 등록할 사이트 주소를 확인하지 못했습니다.".into());
    }
    let _save_guard = lock(&state.settings_write_lock);
    let updated = {
        let mut settings = lock(&state.settings);
        if let Some(pos) = settings
            .ad_block_allowlist
            .iter()
            .position(|entry| entry.eq_ignore_ascii_case(&host))
        {
            settings.ad_block_allowlist.remove(pos);
        } else {
            settings.ad_block_allowlist.push(host);
        }
        settings.clone()
    };
    let json = serde_json::to_string(&updated).map_err(|error| error.to_string())?;
    db::set_setting(&lock(&state.db), "app_settings", &json)?;
    browser::sync_adblock_state(&app, updated.ad_block_enabled);
    Ok(settings_view(&state))
}

#[tauri::command]
fn allow_popup_site(state: State<'_, AppState>, host: String) -> Result<SettingsView, String> {
    let normalized = url::Url::parse(&host)
        .ok()
        .and_then(|u| u.host_str().map(|h| h.to_ascii_lowercase()))
        .unwrap_or_else(|| host.trim().to_ascii_lowercase());
    if normalized.is_empty() {
        return Err("허용할 사이트 호스트가 올바르지 않습니다.".into());
    }
    let _save_guard = lock(&state.settings_write_lock);
    let updated = {
        let mut settings = lock(&state.settings);
        if !settings
            .popup_allowed_origins
            .iter()
            .any(|item| item.eq_ignore_ascii_case(&normalized))
        {
            settings.popup_allowed_origins.push(normalized);
        }
        settings.clone()
    };
    let json = serde_json::to_string(&updated).map_err(|error| error.to_string())?;
    db::set_setting(&lock(&state.db), "app_settings", &json)?;
    Ok(settings_view(&state))
}

#[tauri::command]
async fn reopen_closed_tab(app: AppHandle) -> Result<BrowserState, String> {
    browser::reopen_closed_tab(&app)
}

#[tauri::command]
fn set_zoom(app: AppHandle, factor: f64) -> Result<BrowserState, String> {
    browser::set_zoom(&app, factor)
}

#[tauri::command]
fn print_page(app: AppHandle) -> Result<(), String> {
    browser::print_active(&app)
}

#[tauri::command]
fn toggle_fullscreen(app: AppHandle) -> Result<bool, String> {
    browser::toggle_fullscreen(&app)
}

#[tauri::command]
fn window_action(app: AppHandle, action: String) -> Result<(), String> {
    browser::window_action(&app, &action)
}

#[tauri::command]
async fn find_in_page(app: AppHandle, query: String, action: String) -> Result<FindResult, String> {
    browser::find_in_page(&app, &query, &action).await
}

#[tauri::command]
fn list_downloads(
    state: State<'_, AppState>,
    query: String,
    limit: Option<u32>,
) -> Result<Vec<DownloadItem>, String> {
    db::list_downloads(&lock(&state.db), query.trim(), limit.unwrap_or(200))
}

#[tauri::command]
fn delete_download(state: State<'_, AppState>, id: i64) -> Result<(), String> {
    db::delete_download(&lock(&state.db), id)
}

#[tauri::command]
fn clear_downloads(state: State<'_, AppState>) -> Result<(), String> {
    db::clear_downloads(&lock(&state.db))
}

#[tauri::command]
fn show_download_in_folder(path: String) -> Result<(), String> {
    let target = std::path::Path::new(&path);
    if path.trim().is_empty() || !target.exists() {
        return Err("다운로드한 파일을 찾을 수 없습니다.".into());
    }
    #[cfg(windows)]
    {
        std::process::Command::new("explorer.exe")
            .arg(format!("/select,{}", target.display()))
            .spawn()
            .map_err(|error| format!("탐색기를 열지 못했습니다: {error}"))?;
        return Ok(());
    }
    #[cfg(not(windows))]
    {
        Err("이 플랫폼에서는 폴더 열기를 지원하지 않습니다.".into())
    }
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct ImportSummary {
    glossary: u32,
    bookmarks: u32,
    history: u32,
}

#[tauri::command]
fn export_app_data(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let (glossary, bookmarks, history) = {
        let db = lock(&state.db);
        (
            db::list_glossary(&db)?,
            db::list_bookmarks(&db)?,
            db::list_history(&db, "", 10_000)?,
        )
    };
    let data = AppData {
        version: 1,
        glossary,
        bookmarks,
        history,
    };
    let text = serde_json::to_string_pretty(&data)
        .map_err(|error| format!("데이터를 직렬화하지 못했습니다: {error}"))?;
    let stem = format!("translate-browser-data-{}", db::now_secs());
    write_export_file(&app, &state, &stem, &text)
}

/// Imports a previously exported JSON payload. Entries are merged, not replaced.
#[tauri::command]
fn import_app_data(state: State<'_, AppState>, payload: String) -> Result<ImportSummary, String> {
    if payload.len() > 64 * 1024 * 1024 {
        return Err("가져올 파일이 너무 큽니다(64 MiB 제한).".into());
    }
    let data: AppData = serde_json::from_str(&payload)
        .map_err(|error| format!("데이터 파일을 해석하지 못했습니다: {error}"))?;
    if data.version != 1 {
        return Err(format!("지원하지 않는 데이터 버전입니다: {}", data.version));
    }
    if data.glossary.len() > 20_000 || data.bookmarks.len() > 20_000 || data.history.len() > 20_000
    {
        return Err("데이터 항목 수가 너무 많습니다(종류별 20,000개 제한).".into());
    }

    let mut summary = ImportSummary {
        glossary: 0,
        bookmarks: 0,
        history: 0,
    };
    let db = lock(&state.db);
    for entry in &data.glossary {
        if entry.source.trim().is_empty() {
            continue;
        }
        db::upsert_glossary(&db, entry)?;
        summary.glossary += 1;
    }
    for bookmark in &data.bookmarks {
        let Ok(url) = text::normalize_page_url(&bookmark.url) else {
            continue;
        };
        let mut bookmark = bookmark.clone();
        bookmark.url = url;
        db::save_bookmark(&db, &bookmark)?;
        summary.bookmarks += 1;
    }
    for item in &data.history {
        if url::Url::parse(&item.url)
            .map(|url| matches!(url.scheme(), "http" | "https"))
            .unwrap_or(false)
        {
            db::insert_history_item(&db, item)?;
            summary.history += 1;
        }
    }
    Ok(summary)
}

#[tauri::command]
fn list_translation_history(
    state: State<'_, AppState>,
    query: String,
) -> Result<Vec<TranslationHistory>, String> {
    db::list_translation_history(&lock(&state.db), query.trim())
}

fn order_results(
    segments: &[SegmentInput],
    mut results: HashMap<String, SegmentResult>,
) -> Vec<SegmentResult> {
    segments
        .iter()
        .filter_map(|segment| results.remove(&segment.id))
        .collect()
}

#[tauri::command]
fn list_glossary(state: State<'_, AppState>) -> Result<Vec<GlossaryEntry>, String> {
    let db = lock(&state.db);
    db::list_glossary(&db)
}

#[tauri::command]
fn save_glossary(state: State<'_, AppState>, entry: GlossaryEntry) -> Result<(), String> {
    let source = entry.source.trim().to_string();
    if source.is_empty() {
        return Err("원문 항목을 입력하세요.".into());
    }
    if source.chars().count() > 500 || entry.aliases.chars().count() > 2_000 {
        return Err("원문은 500자, 별칭은 2,000자 이내로 입력하세요.".into());
    }
    if entry.use_regex {
        let patterns = std::iter::once(source.as_str())
            .chain(entry.aliases.split([',', ';', '\n', '\r']).map(str::trim))
            .filter(|pattern| !pattern.is_empty());
        for pattern in patterns {
            regex::Regex::new(pattern)
                .map_err(|error| format!("정규식이 올바르지 않습니다: {error}"))?;
        }
    }
    let mut entry = entry;
    entry.source = source;
    entry.aliases = entry.aliases.trim().to_string();
    let db = lock(&state.db);
    db::upsert_glossary(&db, &entry)
}

#[tauri::command]
fn delete_glossary(state: State<'_, AppState>, id: String) -> Result<(), String> {
    let db = lock(&state.db);
    db::delete_glossary(&db, &id)
}

#[tauri::command]
async fn get_viewport_inline_order(app: AppHandle) -> Result<browser::ViewportOrder, String> {
    browser::get_viewport_inline_order(&app).await
}

#[tauri::command]
fn detect_hy_mt2_hardware(state: State<'_, AppState>) -> HardwareProfile {
    state.hy_mt2.detect_hardware()
}

#[tauri::command]
async fn list_hy_mt2_models(state: State<'_, AppState>) -> Result<Vec<HyMt2ModelState>, String> {
    Ok(state.hy_mt2.list_models().await)
}

#[tauri::command]
async fn download_hy_mt2_model(
    state: State<'_, AppState>,
    model_id: String,
    on_progress: tauri::ipc::Channel<ModelDownloadProgress>,
) -> Result<HyMt2ModelState, String> {
    push_diagnostic(&state, "model-download", &format!("start id={model_id}"));
    let res = state
        .hy_mt2
        .download_model(&model_id, move |progress| {
            let _ = on_progress.send(progress);
        })
        .await;
    match &res {
        Ok(m) => push_diagnostic(
            &state,
            "model-download",
            &format!("ok id={} bytes={}", m.id, m.expected_bytes),
        ),
        Err(err) => push_diagnostic(&state, "model-download", &format!("err id={model_id} {err}")),
    }
    res
}

#[tauri::command]
async fn cancel_hy_mt2_download(state: State<'_, AppState>, model_id: String) -> Result<bool, String> {
    Ok(state.hy_mt2.cancel_download(&model_id).await)
}

#[tauri::command]
async fn delete_hy_mt2_model(state: State<'_, AppState>, model_id: String) -> Result<(), String> {
    state.hy_mt2.delete_model(&model_id).await
}

#[tauri::command]
async fn unload_hy_mt2_model(state: State<'_, AppState>) -> Result<bool, String> {
    Ok(state.hy_mt2.unload().await)
}

#[tauri::command]
fn cancel_translation(state: State<'_, AppState>) {
    if let Some(token) = lock(&state.active_cancel_token).as_ref() {
        token.cancel();
    }
    state
        .cancel_generation
        .send_modify(|generation| *generation = generation.wrapping_add(1));
}

/// Writes a self-contained page snapshot, glossary and settings export.
#[tauri::command]
fn export_project(
    app: AppHandle,
    state: State<'_, AppState>,
    include_original: bool,
) -> Result<String, String> {
    let url = {
        let tabs = lock(&state.tabs);
        tabs.active_url()
            .ok_or_else(|| "활성 탭이 없습니다.".to_string())?
    };
    let normalized = text::normalize_page_url(&url)?;
    let page_id = text::page_id(&normalized);
    let settings =
        serde_json::to_value(lock(&state.settings).clone()).map_err(|error| error.to_string())?;

    let (snapshot, glossary) = {
        let db = lock(&state.db);
        (db::get_snapshot(&db, &page_id)?, db::list_glossary(&db)?)
    };
    let article = if include_original {
        let snapshot = snapshot.ok_or_else(|| {
            "현재 페이지의 원문 스냅샷이 없습니다. 페이지 번역을 실행해 본문을 먼저 추출하세요."
                .to_string()
        })?;
        Some(
            serde_json::from_str::<serde_json::Value>(&snapshot)
                .map_err(|error| format!("스냅샷을 해석하지 못했습니다: {error}"))?,
        )
    } else {
        None
    };

    let mut payload = serde_json::json!({
        "format": "reading-browser-project",
        "version": 1,
        "exportedAt": db::now_secs(),
        "url": normalized,
        "glossary": glossary,
        "settings": settings,
    });
    if let Some(article) = article {
        payload["article"] = article;
    }

    let text = serde_json::to_string_pretty(&payload)
        .map_err(|error| format!("프로젝트 JSON을 만들지 못했습니다: {error}"))?;
    let stem = format!("reading-browser-project-{}", db::now_secs());
    write_export_file(&app, &state, &stem, &text)
}

#[tauri::command]
async fn get_tester_diagnostics(state: State<'_, AppState>) -> Result<String, String> {
    build_tester_diagnostics_report(&state).await
}

#[tauri::command]
async fn save_tester_diagnostics(app: AppHandle, state: State<'_, AppState>) -> Result<String, String> {
    let report = build_tester_diagnostics_report(&state).await?;
    let stem = format!("translate-browser-diagnostics-{}", db::now_secs());
    write_unique_download_file(&app, &state, &stem, "txt", &report)
}

fn validate_feedback_endpoint_url(raw: &str) -> Result<url::Url, String> {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return Err("제보 웹앱 URL이 비어 있습니다.".into());
    }
    let parsed = url::Url::parse(trimmed)
        .map_err(|_| "제보 웹앱 URL 형식이 올바르지 않습니다.".to_string())?;
    if parsed.scheme() != "https" {
        return Err("제보 웹앱 URL은 보안상 https:// 주소만 사용할 수 있습니다.".into());
    }
    if !parsed.username().is_empty() || parsed.password().is_some() {
        return Err("제보 웹앱 URL에 인증 정보를 포함할 수 없습니다.".into());
    }
    let host = parsed.host_str().unwrap_or("").to_ascii_lowercase();
    let path = parsed.path();
    let allowed = host.ends_with(".workers.dev")
        || (host == "formspree.io" && path.starts_with("/f/"))
        || (host == "api.web3forms.com" && path.starts_with("/submit"));
    if !allowed {
        return Err(
            "허용된 제보 릴레이 URL(*.workers.dev, formspree.io/f/..., api.web3forms.com/submit)만 등록할 수 있습니다."
                .into(),
        );
    }
    Ok(parsed)
}

fn resolve_feedback_endpoint(settings: &AppSettings) -> Option<(String, &'static str)> {
    if let Some(raw) = BUILTIN_FEEDBACK_WEBHOOK_URL {
        let trimmed = raw.trim();
        if !trimmed.is_empty() && validate_feedback_endpoint_url(trimmed).is_ok() {
            return Some((trimmed.to_string(), "builtin"));
        }
    }
    if let Ok(exe_path) = std::env::current_exe()
        && let Some(dir) = exe_path.parent()
    {
        let cfg_path = dir.join("feedback-endpoint.txt");
        if let Ok(contents) = std::fs::read_to_string(&cfg_path) {
            for line in contents.lines() {
                let candidate = line.trim();
                if candidate.is_empty() || candidate.starts_with('#') {
                    continue;
                }
                if validate_feedback_endpoint_url(candidate).is_ok() {
                    return Some((candidate.to_string(), "app_file"));
                }
                break;
            }
        }
    }
    let custom = settings.feedback_endpoint_url.trim();
    if !custom.is_empty() && validate_feedback_endpoint_url(custom).is_ok() {
        return Some((custom.to_string(), "settings"));
    }
    None
}

fn truncate_feedback_chars(raw: &str, max_chars: usize) -> String {
    let trimmed = raw.trim();
    let count = trimmed.chars().count();
    if count <= max_chars {
        trimmed.to_string()
    } else {
        let head: String = trimmed.chars().take(max_chars).collect();
        format!("{head}\n...(truncated)")
    }
}

fn encode_uri_component_percent20(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 2);
    for &b in input.as_bytes() {
        let is_unreserved =
            b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~');
        if is_unreserved {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn build_feedback_issue_content(
    input: &FeedbackReportInput,
    app_version: &str,
    settings: &AppSettings,
    diagnostics: Option<&str>,
    sanitized_page_url: Option<&str>,
) -> Result<(String, &'static str, String, String, String), String> {
    let (norm_category, category_label, gh_label) =
        match input.category.trim().to_ascii_lowercase().as_str() {
            "bug" => ("bug", "Bug Report", "bug"),
            "ux" | "inconvenience" => ("ux", "UX / Enhancement", "enhancement"),
            "translation" | "mistranslation" => {
                ("translation", "Translation Error", "translation-error")
            }
            _ => return Err("제보 분류는 버그·불편·번역 오류 중 하나여야 합니다.".into()),
        };

    let summary_clean = input
        .summary
        .replace(['\r', '\n'], " ")
        .trim()
        .to_string();
    let description_clean = input.description.trim().to_string();
    let source_clean = input.source_text.as_deref().unwrap_or("").trim().to_string();
    let translated_clean = input
        .translated_text
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();
    let suggested_clean = input
        .suggested_translation
        .as_deref()
        .unwrap_or("")
        .trim()
        .to_string();
    let contact_clean = input
        .contact
        .as_deref()
        .unwrap_or("")
        .replace(['\r', '\n'], " ")
        .trim()
        .to_string();

    if summary_clean.is_empty()
        && description_clean.is_empty()
        && source_clean.is_empty()
        && translated_clean.is_empty()
    {
        return Err("제보 요약 또는 내용을 입력해 주세요.".into());
    }

    let effective_summary = if !summary_clean.is_empty() {
        truncate_feedback_chars(&summary_clean, 120)
    } else if !description_clean.is_empty() {
        truncate_feedback_chars(&description_clean.replace(['\r', '\n'], " "), 80)
    } else {
        truncate_feedback_chars(&source_clean.replace(['\r', '\n'], " "), 80)
    };
    let title = format!("[{category_label}] {effective_summary}");

    let mut lines = Vec::new();
    lines.push(format!("### Category: {category_label}"));
    lines.push(format!(
        "- **App Version:** {app_version} ({}/{})",
        std::env::consts::OS,
        std::env::consts::ARCH
    ));
    lines.push(format!("- **UI Language:** {}", settings.ui_language));
    lines.push(format!(
        "- **Translator:** Provider={:?} | Model={} | TargetLang={} | Style={}",
        settings.provider, settings.model, settings.target_language, settings.style
    ));
    if !contact_clean.is_empty() {
        lines.push(format!(
            "- **Reporter Contact / Handle:** {}",
            truncate_feedback_chars(&contact_clean, 120)
        ));
    }
    if let Some(url) = sanitized_page_url.filter(|u| !u.trim().is_empty()) {
        lines.push(format!("- **Page URL:** {}", truncate_feedback_chars(url, 500)));
    }
    lines.push(String::new());
    lines.push("### Description".to_string());
    lines.push(if description_clean.is_empty() {
        "_No description provided._".to_string()
    } else {
        truncate_feedback_chars(&description_clean, 4000)
    });

    if !source_clean.is_empty() {
        lines.push(String::new());
        lines.push("### Original Text".to_string());
        lines.push("```text".to_string());
        lines.push(truncate_feedback_chars(&source_clean, 4000));
        lines.push("```".to_string());
    }
    if !translated_clean.is_empty() {
        lines.push(String::new());
        lines.push("### Translated Output".to_string());
        lines.push("```text".to_string());
        lines.push(truncate_feedback_chars(&translated_clean, 4000));
        lines.push("```".to_string());
    }
    if !suggested_clean.is_empty() {
        lines.push(String::new());
        lines.push("### Suggested Translation".to_string());
        lines.push("```text".to_string());
        lines.push(truncate_feedback_chars(&suggested_clean, 2000));
        lines.push("```".to_string());
    }
    if let Some(diag) = diagnostics.filter(|d| !d.trim().is_empty()) {
        lines.push(String::new());
        lines.push("<details><summary>Diagnostics (Sanitized)</summary>".to_string());
        lines.push(String::new());
        lines.push("```markdown".to_string());
        lines.push(truncate_feedback_chars(diag, 10_000));
        lines.push("```".to_string());
        lines.push("</details>".to_string());
    }
    let body = lines.join("\n");

    let encoded_title = encode_uri_component_percent20(&title);
    let encoded_label = encode_uri_component_percent20(gh_label);
    let mut body_for_url = body.clone();
    let mut encoded_body = encode_uri_component_percent20(&body_for_url);
    if encoded_body.len() > 3200 {
        body_for_url = format!(
            "{}\n\n...(truncated for URL limit — paste full copied Markdown report from the app if needed)",
            body.chars().take(1200).collect::<String>()
        );
        encoded_body = encode_uri_component_percent20(&body_for_url);
    }
    let github_issue_url = format!(
        "{FEEDBACK_REPO_URL}/issues/new?title={encoded_title}&labels={encoded_label}&body={encoded_body}"
    );

    Ok((
        norm_category.to_string(),
        gh_label,
        title,
        body,
        github_issue_url,
    ))
}

#[tauri::command]
fn get_feedback_config(state: State<'_, AppState>) -> FeedbackConfigStatus {
    let settings = lock(&state.settings).clone();
    let resolved = resolve_feedback_endpoint(&settings);
    FeedbackConfigStatus {
        repo_url: FEEDBACK_REPO_URL.to_string(),
        issues_url: format!("{FEEDBACK_REPO_URL}/issues"),
        direct_send_configured: resolved.is_some(),
        endpoint_source: resolved.map(|(_, s)| s).unwrap_or("none").to_string(),
    }
}

#[tauri::command]
async fn submit_feedback_report(
    state: State<'_, AppState>,
    input: FeedbackReportInput,
) -> Result<FeedbackSubmitResult, String> {
    let settings = lock(&state.settings).clone();
    let sanitized_page_url = input
        .page_url
        .as_deref()
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(|url| sanitize_diagnostic_message(&state, url));
    let diagnostics = if input.include_diagnostics {
        build_tester_diagnostics_report(&state).await.ok()
    } else {
        None
    };

    let (norm_category, gh_label, title, body, github_issue_url) =
        build_feedback_issue_content(
            &input,
            env!("CARGO_PKG_VERSION"),
            &settings,
            diagnostics.as_deref(),
            sanitized_page_url.as_deref(),
        )?;

    let Some((endpoint_url, _source)) = resolve_feedback_endpoint(&settings) else {
        return Ok(FeedbackSubmitResult {
            delivered: false,
            method: "github_issue".to_string(),
            title,
            body,
            github_issue_url,
            issue_url: None,
            message: "GitHub Issues 제출 양식을 준비했습니다. 아래 버튼으로 바로 등록하거나 리포트를 복사·저장할 수 있습니다."
                .to_string(),
        });
    };

    let payload = serde_json::json!({
        "appToken": FEEDBACK_APP_TOKEN,
        "appVersion": env!("CARGO_PKG_VERSION"),
        "uiLanguage": settings.ui_language,
        "category": norm_category,
        "label": gh_label,
        "summary": truncate_feedback_chars(&input.summary, 120),
        "description": truncate_feedback_chars(&input.description, 4000),
        "pageUrl": sanitized_page_url.as_deref().unwrap_or(""),
        "sourceText": truncate_feedback_chars(input.source_text.as_deref().unwrap_or(""), 4000),
        "translatedText": truncate_feedback_chars(input.translated_text.as_deref().unwrap_or(""), 4000),
        "suggestedTranslation": truncate_feedback_chars(input.suggested_translation.as_deref().unwrap_or(""), 2000),
        "contact": truncate_feedback_chars(input.contact.as_deref().unwrap_or(""), 120),
        "provider": settings.provider.as_str(),
        "model": settings.model,
        "targetLanguage": settings.target_language,
        "style": settings.style,
        "diagnostics": diagnostics.as_deref().unwrap_or(""),
        "title": title,
        "fullBody": body,
    });

    let client = match reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(15))
        .user_agent("Fluo/0.1")
        .redirect(reqwest::redirect::Policy::none())
        .build()
    {
        Ok(c) => c,
        Err(err) => {
            push_diagnostic(&state, "feedback", &format!("client_err={err}"));
            return Ok(FeedbackSubmitResult {
                delivered: false,
                method: "github_issue".to_string(),
                title,
                body,
                github_issue_url,
                issue_url: None,
                message: format!(
                    "릴레이 클라이언트를 초기화하지 못했습니다({err}). GitHub Issues로 바로 제보해 주세요."
                ),
            });
        }
    };

    let post_outcome = client.post(&endpoint_url).json(&payload).send().await;
    match post_outcome {
        Ok(resp) => {
            let status = resp.status();
            let text_body = resp.text().await.unwrap_or_default();
            let parsed = serde_json::from_str::<serde_json::Value>(&text_body).ok();
            let json_ok_false = parsed
                .as_ref()
                .and_then(|v| v.get("ok").and_then(|b| b.as_bool()))
                == Some(false);
            let issue_url = parsed
                .as_ref()
                .and_then(|v| v.get("issueUrl").and_then(|u| u.as_str()))
                .map(str::to_string);
            if status.is_success() && !json_ok_false {
                push_diagnostic(&state, "feedback", &format!("delivered category={norm_category}"));
                Ok(FeedbackSubmitResult {
                    delivered: true,
                    method: "relay".to_string(),
                    title,
                    body,
                    github_issue_url,
                    issue_url,
                    message: "피드백이 익명 릴레이를 통해 성공적으로 등록되었습니다. 감사합니다!"
                        .to_string(),
                })
            } else {
                let err_reason = parsed
                    .as_ref()
                    .and_then(|v| v.get("error").and_then(|e| e.as_str()).map(str::to_string))
                    .unwrap_or_else(|| format!("HTTP {status}"));
                push_diagnostic(&state, "feedback", &format!("webhook_reject={err_reason}"));
                Ok(FeedbackSubmitResult {
                    delivered: false,
                    method: "github_issue".to_string(),
                    title,
                    body,
                    github_issue_url,
                    issue_url: None,
                    message: format!(
                        "릴레이 전송에 실패했습니다({err_reason}). GitHub Issues 양식으로 바로 제보할 수 있습니다."
                    ),
                })
            }
        }
        Err(err) => {
            push_diagnostic(&state, "feedback", &format!("webhook_net_err={err}"));
            Ok(FeedbackSubmitResult {
                delivered: false,
                method: "github_issue".to_string(),
                title,
                body,
                github_issue_url,
                issue_url: None,
                message: format!(
                    "네트워크 연결 문제로 릴레이 전송에 실패했습니다({err}). GitHub Issues 양식으로 바로 제보할 수 있습니다."
                ),
            })
        }
    }
}

#[tauri::command]
fn open_feedback_url(app: AppHandle, url: String) -> Result<(), String> {
    let trimmed = url.trim();
    let issues_prefix = format!("{FEEDBACK_REPO_URL}/issues");
    let discussions_prefix = format!("{FEEDBACK_REPO_URL}/discussions");
    if !trimmed.starts_with(&issues_prefix) && !trimmed.starts_with(&discussions_prefix) {
        return Err("허용되지 않은 GitHub 제보 링크입니다.".into());
    }
    app.opener()
        .open_url(trimmed, None::<&str>)
        .map_err(|e| format!("브라우저에서 제보 페이지를 열지 못했습니다: {e}"))
}

#[tauri::command]
fn save_feedback_report_file(
    app: AppHandle,
    state: State<'_, AppState>,
    title: String,
    body: String,
) -> Result<String, String> {
    let clean_title = title.replace(['\r', '\n'], " ").trim().to_string();
    let content = if clean_title.is_empty() {
        body
    } else {
        format!("# {clean_title}\n\n{body}\n")
    };
    let stem = format!("feedback-report-{}", db::now_secs());
    write_unique_download_file(&app, &state, &stem, "md", &content)
}

async fn build_tester_diagnostics_report(state: &AppState) -> Result<String, String> {
    let hw = state.hy_mt2.detect_hardware();
    let models = state.hy_mt2.list_models().await;
    let loaded = state.hy_mt2.loaded_model_info().await;
    let settings = lock(&state.settings).clone();
    let webview_ver = tauri::webview_version().unwrap_or_else(|e| format!("unavailable ({e})"));
    let logs: Vec<String> = lock(&state.diagnostics_log).iter().cloned().collect();

    let mut lines = Vec::new();
    lines.push("# Fluo 진단 리포트 (개인정보 미포함)".to_string());
    lines.push(format!("- App Version: {}", env!("CARGO_PKG_VERSION")));
    lines.push(format!("- Platform: {} / {}", std::env::consts::OS, std::env::consts::ARCH));
    lines.push(format!("- WebView2: {webview_ver}"));
    lines.push(String::new());
    lines.push("## 하드웨어 및 가속 백엔드".to_string());
    lines.push(format!(
        "- CPU: {} ({} Threads)",
        hw.cpu_name.as_deref().unwrap_or("unknown"),
        hw.cpu_cores
    ));
    lines.push(format!(
        "- RAM: total={} MiB, available={} MiB",
        hw.total_ram_mb, hw.available_ram_mb
    ));
    lines.push(format!(
        "- Primary GPU: {} (vendor={}, discrete={}, dedicated={} MiB, shared={} MiB, ggml_dev={:?})",
        hw.gpu_name.as_deref().unwrap_or("없음"),
        hw.gpu_vendor,
        hw.is_discrete_gpu,
        hw.dedicated_vram_mb,
        hw.shared_vram_mb,
        hw.selected_gpu_device_index
    ));
    if !hw.detected_gpus.is_empty() {
        lines.push(format!("- Detected GPUs: {}", hw.detected_gpus.join(" | ")));
    }
    lines.push(format!(
        "- Backends: cuda_compiled={}, cuda_available={}, vulkan_compiled={}, vulkan_available={}, active_backend={}",
        hw.cuda_compiled, hw.cuda_available, hw.vulkan_compiled, hw.vulkan_available, hw.active_backend
    ));
    lines.push(format!(
        "- Recommended Tier: {} ({})",
        hw.recommended_tier, hw.recommended_model_id
    ));
    lines.push(String::new());
    lines.push("## 활성 번역 설정 요약 (키/URL 제외)".to_string());
    lines.push(format!(
        "- Provider: {:?} | Model: {} | Style: {} | TargetLang: {}",
        settings.provider, settings.model, settings.style, settings.target_language
    ));
    lines.push(format!(
        "- Hy-MT2 Tuning: gpu_layers={}, ctx_size={}, idle_unload_secs={}, threads={}",
        settings.hy_mt2_gpu_layers,
        settings.hy_mt2_ctx_size,
        settings.hy_mt2_idle_unload_secs,
        settings.hy_mt2_threads
    ));
    lines.push(format!(
        "- Browser Flags: hw_accel={}, adblock={}, tracking={}, sleep_tabs={}",
        settings.hardware_acceleration,
        settings.ad_block_enabled,
        settings.tracking_prevention,
        settings.sleeping_tabs_enabled
    ));
    lines.push(format!(
        "- Loaded Model in Memory: {}",
        loaded
            .map(|(m, b)| format!("{m} ({b})"))
            .unwrap_or_else(|| "none".into())
    ));
    lines.push(String::new());
    lines.push("## 설치된 GGUF 모델 상태".to_string());
    for m in models {
        if m.installed || m.partial_bytes > 0 {
            lines.push(format!(
                "- {} | installed={} | installed_bytes={} | partial={}",
                m.filename, m.installed, m.installed_bytes, m.partial_bytes
            ));
        }
    }
    if let Ok(entries) = std::fs::read_dir(&state.models_dir) {
        let mut custom = Vec::new();
        for entry in entries.flatten() {
            let p = entry.path();
            let name = p.file_name().and_then(|s| s.to_str()).unwrap_or("");
            if name.to_ascii_lowercase().ends_with(".gguf") {
                if let Ok(meta) = entry.metadata() {
                    custom.push(format!("- [disk] {name} ({} bytes)", meta.len()));
                }
            }
        }
        if !custom.is_empty() {
            lines.extend(custom);
        }
    }
    lines.push(String::new());
    lines.push("## 최근 런타임 진단 로그 (최대 60건)".to_string());
    if logs.is_empty() {
        lines.push("- 기록된 진단 로그 없음".to_string());
    } else {
        lines.extend(logs);
    }
    Ok(lines.join("\n"))
}

/// Writes `text` to a new JSON file in the download folder without overwriting.
fn write_export_file(
    app: &AppHandle,
    state: &AppState,
    stem: &str,
    text: &str,
) -> Result<String, String> {
    write_unique_download_file(app, state, stem, "json", text)
}

fn write_unique_download_file(
    app: &AppHandle,
    state: &AppState,
    stem: &str,
    ext: &str,
    text: &str,
) -> Result<String, String> {
    let directory = app
        .path()
        .download_dir()
        .unwrap_or_else(|_| state.data_dir.clone());
    std::fs::create_dir_all(&directory)
        .map_err(|error| format!("내보낼 폴더를 만들지 못했습니다: {error}"))?;
    let mut suffix = 0_u32;
    let (path, mut file) = loop {
        let name = if suffix == 0 {
            format!("{stem}.{ext}")
        } else {
            format!("{stem}-{suffix}.{ext}")
        };
        let candidate = directory.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&candidate)
        {
            Ok(file) => break (candidate, file),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {
                suffix = suffix.wrapping_add(1);
            }
            Err(error) => {
                return Err(format!("파일을 만들지 못했습니다: {error}"));
            }
        }
    };
    if let Err(error) = file.write_all(text.as_bytes()) {
        drop(file);
        let _ = std::fs::remove_file(&path);
        return Err(format!("파일을 쓰지 못했습니다: {error}"));
    }
    Ok(path.to_string_lossy().into_owned())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let handle = app.handle().clone();
            let data_dir = app.path().app_data_dir().map_err(boxed)?;
            std::fs::create_dir_all(&data_dir).map_err(boxed)?;
            let local_data_dir = app
                .path()
                .app_local_data_dir()
                .unwrap_or_else(|_| data_dir.clone());
            std::fs::create_dir_all(&local_data_dir).map_err(boxed)?;
            let models_dir = local_data_dir.join("models");
            std::fs::create_dir_all(&models_dir).map_err(boxed)?;
            let hy_mt2 = Arc::new(HyMt2Engine::new(models_dir.clone()));
            let extensions_dir = data_dir.join("extensions");
            std::fs::create_dir_all(&extensions_dir).map_err(boxed)?;

            let connection = db::open(&data_dir.join("reading-browser.sqlite3")).map_err(boxed)?;
            let hw_boot = hy_mt2.detect_hardware();
            let settings: AppSettings = db::get_setting(&connection, "app_settings")
                .and_then(|json| serde_json::from_str(&json).ok())
                .and_then(|settings| validate_settings(settings).ok())
                .unwrap_or_else(|| starter_settings_for_hardware(&hw_boot));
            let webview_args = if settings.hardware_acceleration {
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection".to_string()
            } else {
                "--disable-features=msWebOOUI,msPdfOOUI,msSmartScreenProtection --disable-gpu"
                    .to_string()
            };
            let webview_extensions_enabled = settings.browser_extensions_enabled;
            let startup_mode = settings.startup_mode.clone();
            let initial_home_url = if startup_mode != "search" && !settings.home_url.trim().is_empty()
            {
                settings.home_url.clone()
            } else {
                home_url(&settings.search_engine).to_string()
            };
            let session = db::load_session(&connection);
            let secrets = credentials::load(&data_dir);
            let client = reqwest::Client::builder()
                .user_agent("Fluo/0.1")
                .redirect(reqwest::redirect::Policy::none())
                .build()
                .map_err(|error| boxed(error.to_string()))?;

            let (cancel_generation, _) = tokio::sync::watch::channel(0_u64);
            let ad_block_enabled = Arc::new(AtomicBool::new(settings.ad_block_enabled));
            let mut tabs = browser::Tabs::default();
            let workspace_state = db::get_setting(&connection, "workspaces");
            browser::load_workspaces(&mut tabs, workspace_state.as_deref());
            let initial_theme_setting = settings.theme.clone();
            let initial_ui_language = settings.ui_language.clone();
            let mut diagnostics_init = VecDeque::with_capacity(60);
            diagnostics_init.push_back(format!(
                "[{}] [startup] version={} os={}/{} webview2={} gpu={} vulkan={} tier={}",
                db::now_secs(),
                env!("CARGO_PKG_VERSION"),
                std::env::consts::OS,
                std::env::consts::ARCH,
                tauri::webview_version().unwrap_or_else(|_| "unknown".into()),
                hw_boot.gpu_name.as_deref().unwrap_or("none"),
                hw_boot.active_backend,
                hw_boot.recommended_tier
            ));
            app.manage(AppState {
                data_dir,
                local_data_dir,
                models_dir,
                extensions_dir: extensions_dir.clone(),
                webview_args: webview_args.clone(),
                webview_extensions_enabled,
                db: Mutex::new(connection),
                settings: Mutex::new(settings),
                settings_write_lock: Mutex::new(()),
                secrets: Mutex::new(secrets),
                client,
                tabs: Mutex::new(tabs),
                budgets: Mutex::new(HashMap::new()),
                ai_request_gate: tokio::sync::Mutex::new(()),
                cancel_generation,
                active_cancel_token: Mutex::new(None),
                hy_mt2: Arc::clone(&hy_mt2),
                ad_block_enabled,
                diagnostics_log: Mutex::new(diagnostics_init),
            });

            // Auto-unload on-device Hy-MT2 model after configured idle timeout (default 60s).
            let idle_handle = app.handle().clone();
            let idle_engine = Arc::clone(&hy_mt2);
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(5)).await;
                    let timeout_secs = {
                        let state = idle_handle.state::<AppState>();
                        lock(&state.settings).hy_mt2_idle_unload_secs
                    };
                    let _ = idle_engine.unload_if_idle(timeout_secs).await;
                }
            });

            let initial_bg = browser::theme_bg_color(initial_theme_setting == "dark");
            let window = tauri::Window::builder(app, "main")
                .title(browser::default_window_title(&initial_ui_language))
                .decorations(false)
                .visible(false)
                .background_color(initial_bg)
                .inner_size(1280.0, 860.0)
                .min_inner_size(960.0, 640.0)
                .build()
                .map_err(|error| boxed(error.to_string()))?;

            let system_dark = match initial_theme_setting.as_str() {
                "dark" => true,
                "light" | "sepia" => false,
                _ => matches!(window.theme(), Ok(tauri::Theme::Dark)),
            };
            let _ = window.set_background_color(Some(browser::theme_bg_color(system_dark)));
            {
                let state = app.state::<AppState>();
                let mut t = lock(&state.tabs);
                t.ui_is_dark = Some(system_dark);
            }

            let resize_handle = handle.clone();
            window.on_window_event(move |event| {
                if matches!(event, tauri::WindowEvent::Resized(_)) {
                    let _ = browser::sync_window_layout(&resize_handle);
                }
            });

            let mut shell_builder = WebviewBuilder::new("shell", WebviewUrl::App("/".into()))
                .transparent(true)
                .devtools(false)
                .additional_browser_args(&webview_args)
                .browser_extensions_enabled(webview_extensions_enabled);
            if webview_extensions_enabled && extensions_dir.exists() {
                shell_builder = shell_builder.extensions_path(&extensions_dir);
            }

            window
                .add_child(
                    shell_builder,
                    LogicalPosition::new(0.0, 0.0),
                    LogicalSize::new(1280.0, browser::DEFAULT_TOP),
                )
                .map_err(|error| boxed(error.to_string()))?;

            let restored = if startup_mode == "restore" {
                browser::restore_session(&handle, session.as_deref())
            } else {
                false
            };
            if !restored {
                browser::open_tab(&handle, &initial_home_url).map_err(boxed)?;
            }
            let _ = window.show();

            let sleep_handle = handle.clone();
            tauri::async_runtime::spawn(async move {
                loop {
                    tokio::time::sleep(std::time::Duration::from_secs(30)).await;
                    browser::sleep_idle_tabs(&sleep_handle);
                }
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            browser_state,
            new_tab,
            new_private_tab,
            close_tab,
            pin_tab,
            toggle_mute_tab,
            sleep_tab,
            set_window_title,
            close_other_tabs,
            close_tabs_to_right,
            select_tab,
            reorder_tab,
            move_tab_to_workspace,
            create_workspace,
            switch_workspace,
            rename_workspace,
            delete_workspace,
            navigate,
            go_back,
            go_forward,
            reload,
            stop_loading,
            sync_reading_position,
            open_overlay,
            close_overlay,
            set_shell_expanded,
            poll_page_action,
            show_quick_translation,
            run_page_context_action,
            sync_ui_theme_to_tabs,
            save_page_translation_override,
            layout_content,
            extract_article,
            pick_region_article,
            cancel_region_picker,
            apply_inline_translations,
            restore_inline_translations,
            load_saved_page_translations,
            load_settings,
            save_settings,
            set_ui_language,
            complete_onboarding,
            set_translation_prefs,
            list_models,
            test_connection,
            translate_blocks,
            translate_blocks_stream,
            get_viewport_inline_order,
            detect_hy_mt2_hardware,
            list_hy_mt2_models,
            download_hy_mt2_model,
            cancel_hy_mt2_download,
            delete_hy_mt2_model,
            unload_hy_mt2_model,
            get_tester_diagnostics,
            save_tester_diagnostics,
            get_feedback_config,
            submit_feedback_report,
            open_feedback_url,
            save_feedback_report_file,
            list_bookmarks,
            save_bookmark,
            delete_bookmark,
            rename_bookmark_folder,
            list_bookmark_folders,
            create_bookmark_folder,
            delete_bookmark_folder,
            open_internal_tab,
            list_history,
            delete_history,
            clear_history,
            list_downloads,
            delete_download,
            clear_downloads,
            show_download_in_folder,
            address_suggestions,
            reopen_closed_tab,
            set_zoom,
            print_page,
            toggle_fullscreen,
            window_action,
            find_in_page,
            list_search_history,
            delete_search_history,
            clear_search_history,
            list_saved_logins,
            save_login,
            delete_login,
            autofill_saved_login,
            load_autofill_profile,
            save_autofill_profile,
            autofill_profile_active_tab,
            load_sync_config,
            save_sync_config,
            run_sync_now,
            detect_cloud_accounts,
            connect_sync_account,
            disconnect_sync_account,
            start_oauth_login,
            start_codex_oauth_login,
            import_local_codex_auth,
            clear_codex_auth,
            detect_external_browsers,
            import_external_browser,
            import_passwords_csv,
            list_cookies,
            delete_cookie,
            clear_browsing_data,
            list_user_scripts,
            save_user_script,
            delete_user_script,
            list_web_extensions,
            add_web_extension,
            toggle_or_remove_web_extension,
            toggle_adblock_site,
            allow_popup_site,
            export_app_data,
            import_app_data,
            list_translation_history,
            list_glossary,
            save_glossary,
            delete_glossary,
            cancel_translation,
            export_project
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}

#[cfg(test)]
mod tests {
    use super::{
        build_feedback_issue_content, home_url, sanitize_diagnostic_text,
        starter_settings_for_hardware, validate_feedback_endpoint_url, validate_settings,
        wait_for_request_interval, FEEDBACK_REPO_URL,
    };
    use crate::hy_mt2::HardwareProfile;
    use crate::model::{AppSettings, FeedbackReportInput, ProviderKind};
    use std::time::Duration;

    #[test]
    fn settings_accept_only_supported_values_and_reasonable_limits() {
        let defaults = AppSettings::default();
        assert_eq!(defaults.style, "auto");
        assert!(validate_settings(defaults).is_ok());

        for valid_style in ["auto", "natural", "literal", "polished", "novel"] {
            let mut s = AppSettings::default();
            s.style = valid_style.into();
            assert!(validate_settings(s).is_ok(), "style={valid_style}");
        }

        let mut bad_style = AppSettings::default();
        bad_style.style = "unknown".into();
        assert!(validate_settings(bad_style).is_err());

        let mut bad_lang = AppSettings::default();
        bad_lang.ui_language = "fr".into();
        assert!(validate_settings(bad_lang).is_err());

        for (raw, expected) in [("ko", "ko"), ("EN-US", "en"), ("ja_jp", "ja"), ("zh-hans", "zh-CN")] {
            let mut s = AppSettings::default();
            s.ui_language = raw.into();
            let validated = validate_settings(s).unwrap();
            assert_eq!(validated.ui_language, expected);
        }

        let mut settings = AppSettings::default();
        settings.search_engine = "custom".into();
        assert!(validate_settings(settings).is_err());

        let mut settings = AppSettings::default();
        settings.target_language = "  ".into();
        assert!(validate_settings(settings).is_err());

        let mut settings = AppSettings::default();
        settings.requests_per_day = u32::MAX;
        assert!(validate_settings(settings).is_err());

        let mut settings = AppSettings::default();
        settings.minimum_request_interval_ms = 60_001;
        assert!(validate_settings(settings).is_err());
    }

    #[tokio::test]
    async fn request_interval_wait_is_cancelled_without_waiting_for_the_timer() {
        let (sender, mut receiver) = tokio::sync::watch::channel(0_u64);
        let cancel = async move {
            tokio::time::sleep(Duration::from_millis(10)).await;
            let _ = sender.send(1);
            std::future::pending::<()>().await;
        };
        tokio::select! {
            result = wait_for_request_interval(Duration::from_secs(30), Some(&mut receiver)) => {
                assert!(result.unwrap_err().contains("취소"));
            }
            _ = cancel => panic!("취소 신호를 보내야 합니다"),
        }
    }

    #[test]
    fn new_tabs_start_at_the_configured_search_engine() {
        assert_eq!(home_url("google"), "https://www.google.com/");
        assert_eq!(home_url("bing"), "https://www.bing.com/");
        assert_eq!(home_url("duckduckgo"), "https://duckduckgo.com/");
        assert_eq!(home_url("unknown"), "https://www.google.com/");
    }

    #[test]
    fn diagnostic_sanitizer_masks_paths_and_tokens() {
        let masked = sanitize_diagnostic_text(
            &[r"C:\Users\Tester\AppData\Local\app\models"],
            r"C:\Users\Tester",
            "open failed at C:\\Users\\Tester\\AppData\\Local\\app\\models\\m.gguf\nand C:\\Users\\Tester\\Desktop?token=secret123",
        );
        assert!(!masked.contains("Tester"));
        assert!(!masked.contains("secret123"));
        assert!(masked.contains("<APP_DIR>"));
        assert!(masked.contains("<HOME_DIR>"));
        assert!(masked.contains("?token=<REDACTED>"));
    }

    #[test]
    fn first_run_starter_prefers_local_hymt2_unless_hardware_requires_api() {
        let mut hw = HardwareProfile {
            total_ram_mb: 16_384,
            available_ram_mb: 10_000,
            cpu_cores: 8,
            cpu_name: None,
            gpu_name: None,
            dedicated_vram_mb: 0,
            shared_vram_mb: 0,
            is_amd_apu: false,
            npu_available: false,
            npu_name: None,
            vulkan_compiled: true,
            vulkan_available: false,
            active_backend: "cpu(native)".into(),
            compute_decision: String::new(),
            recommended_tier: "1.8b-q4km".into(),
            recommended_model_id: "Hy-MT2-1.8B-Q4_K_M.gguf".into(),
            recommendation_reason: "test".into(),
            gpu_vendor: "none".into(),
            is_discrete_gpu: false,
            selected_gpu_device_index: None,
            cuda_compiled: false,
            cuda_available: false,
            detected_gpus: Vec::new(),
        };
        let local_starter = starter_settings_for_hardware(&hw);
        assert_eq!(local_starter.provider, ProviderKind::HyMt2);
        assert_eq!(local_starter.model, "Hy-MT2-1.8B-Q4_K_M.gguf");
        assert!(validate_settings(local_starter).is_ok());

        hw.recommended_tier = "api-fallback".into();
        let fallback_starter = starter_settings_for_hardware(&hw);
        assert_eq!(fallback_starter.provider, ProviderKind::GoogleGemini);
    }

    #[test]
    fn feedback_endpoint_validation_enforces_https_and_allowed_hosts() {
        assert!(
            validate_feedback_endpoint_url("https://feedback-relay.apple-out.workers.dev/report")
                .is_ok()
        );
        assert!(validate_feedback_endpoint_url("https://formspree.io/f/xyzabc").is_ok());
        assert!(validate_feedback_endpoint_url("https://api.web3forms.com/submit").is_ok());

        assert!(
            validate_feedback_endpoint_url("https://script.google.com/macros/s/FAKE_SCRIPT_ID/exec")
                .is_err()
        );
        assert!(
            validate_feedback_endpoint_url("http://feedback-relay.apple-out.workers.dev/report")
                .is_err()
        );
        assert!(validate_feedback_endpoint_url("https://127.0.0.1/macros/s/abc/exec").is_err());
        assert!(validate_feedback_endpoint_url("https://evil.example.com/hook").is_err());
        assert!(validate_feedback_endpoint_url("https://user@formspree.io/f/xyz").is_err());
    }

    #[test]
    fn feedback_issue_builder_formats_markdown_and_hides_personal_email() {
        let settings = AppSettings::default();
        let input = FeedbackReportInput {
            category: "translation".into(),
            summary: "고유명사 오역 제보".into(),
            description: "Half-Life 2 문단이 부자연스럽게 번역됩니다.".into(),
            page_url: Some("https://half-life.fandom.com/wiki/Half-Life_2".into()),
            source_text: Some("Combine Overwatch".into()),
            translated_text: Some("컴바인 오버워치".into()),
            suggested_translation: Some("콤바인 감시군".into()),
            contact: Some("@tester_handle".into()),
            include_diagnostics: false,
        };
        let (cat, label, title, body, github_issue_url) = build_feedback_issue_content(
            &input,
            "0.1.0",
            &settings,
            Some("# diag"),
            Some("https://half-life.fandom.com/wiki/Half-Life_2?token=<REDACTED>"),
        )
        .expect("feedback issue content must build");

        assert_eq!(cat, "translation");
        assert_eq!(label, "translation-error");
        assert!(title.contains("[Translation Error]"));
        assert!(title.contains("고유명사 오역 제보"));
        assert!(body.contains("Combine Overwatch"));
        assert!(body.contains("콤바인 감시군"));
        assert!(body.contains("?token=<REDACTED>"));
        assert!(!body.contains("@gmail.com"));
        assert!(!github_issue_url.contains("@gmail.com"));
        assert!(github_issue_url.starts_with(&format!("{FEEDBACK_REPO_URL}/issues/new?title=")));
        assert!(github_issue_url.contains("labels=translation-error"));
        assert!(!github_issue_url.contains('+'));
    }

    #[test]
    fn onboarding_defaults_to_false_even_on_legacy_json() {
        assert!(!AppSettings::default().onboarding_completed);
        let parsed: AppSettings = serde_json::from_str(
            r#"{
                "searchEngine": "google",
                "startupMode": "restore",
                "homeUrl": "",
                "provider": "hyMt2",
                "baseUrl": "",
                "model": "Hy-MT2-1.8B-Q4_K_M.gguf",
                "adBlockEnabled": true,
                "adBlockAllowlist": [],
                "trackingPrevention": "balanced",
                "popupPolicy": "block",
                "popupAllowedOrigins": [],
                "hardwareAcceleration": true,
                "sleepingTabsEnabled": true,
                "sleepTimeoutMinutes": 15,
                "maxAwakeTabs": 6,
                "sleepAllowlist": [],
                "browserExtensionsEnabled": true,
                "targetLanguage": "한국어",
                "uiLanguage": "ko",
                "style": "auto",
                "theme": "system",
                "requestsPerHour": 60,
                "requestsPerDay": 500,
                "minimumRequestIntervalMs": 250,
                "translationFontScale": 1.0,
                "translationLineHeight": 1.6,
                "hyMt2Tier": "auto",
                "hyMt2GpuLayers": -1,
                "hyMt2CtxSize": 8192,
                "hyMt2IdleUnloadSecs": 60,
                "hyMt2Threads": 0,
                "feedbackEndpointUrl": ""
            }"#,
        )
        .expect("legacy AppSettings JSON without onboardingCompleted must deserialize");
        assert!(!parsed.onboarding_completed);
    }

    #[test]
    fn removed_models_and_legacy_provider_fall_back_safely() {
        for stale in [
            "EXAONE-3.5-2.4B-Instruct-Q4_K_M.gguf",
            "Qwen2.5-3B-Instruct-Q4_K_M.gguf",
        ] {
            let s = AppSettings {
                provider: ProviderKind::HyMt2,
                model: stale.into(),
                ..AppSettings::default()
            };
            let validated = validate_settings(s).expect("stale Hy-MT2 model should fall back cleanly");
            assert_eq!(validated.model, crate::hy_mt2::DEFAULT_HY_MT2_MODEL_ID);
        }

        let legacy_exaone = serde_json::from_str::<AppSettings>(r#"{"provider":"exaone"}"#);
        assert!(
            legacy_exaone.is_err(),
            "removed exaone provider must reject so startup uses starter_settings_for_hardware"
        );
    }
}
