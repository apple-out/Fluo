use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub enum ProviderKind {
    #[default]
    GoogleGemini,
    OpenAiCompatible,
    CodexLegacy,
    LocalBridge,
    HyMt2,
}

impl ProviderKind {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::GoogleGemini => "google-gemini",
            Self::OpenAiCompatible => "openai-compatible",
            Self::CodexLegacy => "codex-legacy",
            Self::LocalBridge => "local-bridge",
            Self::HyMt2 => "hy-mt2",
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AppSettings {
    pub search_engine: String,
    pub startup_mode: String,
    pub home_url: String,
    pub provider: ProviderKind,
    pub base_url: String,
    pub model: String,
    pub ad_block_enabled: bool,
    pub ad_block_allowlist: Vec<String>,
    pub tracking_prevention: String,
    pub popup_policy: String,
    pub popup_allowed_origins: Vec<String>,
    pub hardware_acceleration: bool,
    pub sleeping_tabs_enabled: bool,
    pub sleep_timeout_minutes: u32,
    pub max_awake_tabs: u32,
    pub sleep_allowlist: Vec<String>,
    pub browser_extensions_enabled: bool,
    pub target_language: String,
    pub ui_language: String,
    pub style: String,
    pub theme: String,
    pub requests_per_hour: u32,
    pub requests_per_day: u32,
    pub minimum_request_interval_ms: u32,
    pub translation_font_scale: f64,
    pub translation_line_height: f64,
    pub hy_mt2_tier: String,
    pub hy_mt2_gpu_layers: i32,
    pub hy_mt2_ctx_size: u32,
    pub hy_mt2_idle_unload_secs: u64,
    pub hy_mt2_threads: u32,
    pub feedback_endpoint_url: String,
    #[serde(default)]
    pub onboarding_completed: bool,
    #[serde(default)]
    pub update_notifications_disabled: bool,
    #[serde(default)]
    pub ignored_update_version: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppUpdateInfo {
    pub current_version: String,
    pub latest_version: String,
    pub tag_name: String,
    pub release_name: String,
    pub release_notes: String,
    pub release_url: String,
    pub published_at: String,
    pub has_update: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        Self {
            search_engine: "google".into(),
            startup_mode: "restore".into(),
            home_url: String::new(),
            provider: ProviderKind::GoogleGemini,
            base_url: String::new(),
            model: String::new(),
            ad_block_enabled: true,
            ad_block_allowlist: Vec::new(),
            tracking_prevention: "balanced".into(),
            popup_policy: "block".into(),
            popup_allowed_origins: Vec::new(),
            hardware_acceleration: true,
            sleeping_tabs_enabled: true,
            sleep_timeout_minutes: 15,
            max_awake_tabs: 6,
            sleep_allowlist: Vec::new(),
            browser_extensions_enabled: true,
            target_language: "한국어".into(),
            ui_language: "ko".into(),
            style: "auto".into(),
            theme: "system".into(),
            requests_per_hour: 60,
            requests_per_day: 500,
            minimum_request_interval_ms: 250,
            translation_font_scale: 1.0,
            translation_line_height: 1.6,
            hy_mt2_tier: "auto".into(),
            hy_mt2_gpu_layers: -1,
            hy_mt2_ctx_size: 8192,
            hy_mt2_idle_unload_secs: 60,
            hy_mt2_threads: 0,
            feedback_endpoint_url: String::new(),
            onboarding_completed: false,
            update_notifications_disabled: false,
            ignored_update_version: String::new(),
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackReportInput {
    pub category: String,
    pub summary: String,
    pub description: String,
    pub page_url: Option<String>,
    pub source_text: Option<String>,
    pub translated_text: Option<String>,
    pub suggested_translation: Option<String>,
    pub contact: Option<String>,
    pub include_diagnostics: bool,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackConfigStatus {
    pub repo_url: String,
    pub issues_url: String,
    pub direct_send_configured: bool,
    pub endpoint_source: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FeedbackSubmitResult {
    pub delivered: bool,
    pub method: String,
    pub title: String,
    pub body: String,
    pub github_issue_url: String,
    pub issue_url: Option<String>,
    pub message: String,
}

#[derive(Debug, Clone, Default)]
pub struct Secrets {
    pub api_key: Option<String>,
    pub bridge_token: Option<String>,
    pub codex_access_token: Option<String>,
    pub codex_refresh_token: Option<String>,
    pub codex_account_id: Option<String>,
    pub codex_expires_at: Option<i64>,
    pub codex_email: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub settings: AppSettings,
    pub has_api_key: bool,
    pub has_bridge_token: bool,
    pub has_codex_auth: bool,
    pub codex_account_id: Option<String>,
    pub codex_email: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TabState {
    pub id: String,
    pub url: String,
    pub title: String,
    pub loading: bool,
    pub workspace_id: String,
    pub zoom: f64,
    pub pinned: bool,
    pub is_private: bool,
    pub sleeping: bool,
    pub muted: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Workspace {
    pub id: String,
    pub name: String,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BrowserState {
    pub tabs: Vec<TabState>,
    pub active_tab: Option<String>,
    pub can_go_back: bool,
    pub can_go_forward: bool,
    pub workspaces: Vec<Workspace>,
    pub active_workspace: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Block {
    pub id: String,
    pub kind: String,
    #[serde(default)]
    pub doc_index: Option<usize>,
    #[serde(default)]
    pub inline_key: Option<String>,
    #[serde(default)]
    pub level: Option<u8>,
    pub text: String,
    #[serde(default)]
    pub rows: Option<Vec<Vec<String>>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ContextPair {
    pub source: String,
    #[serde(default)]
    pub translation: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Article {
    pub title: String,
    pub url: String,
    pub site: String,
    #[serde(default)]
    pub lang: Option<String>,
    pub blocks: Vec<Block>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentInput {
    pub id: String,
    pub text: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SegmentResult {
    pub id: String,
    pub translation: String,
    pub uncertain: bool,
    pub notes: Vec<String>,
    pub cached: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelInfo {
    pub name: String,
    pub display_name: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectionTest {
    pub ok: bool,
    pub message: String,
    pub model: String,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct HistoryItem {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub visited_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct DownloadItem {
    pub id: i64,
    pub url: String,
    pub file_name: String,
    pub file_path: String,
    pub status: String,
    pub started_at: i64,
    pub finished_at: Option<i64>,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct FindResult {
    pub total: usize,
    pub current: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddressSuggestion {
    pub kind: String,
    pub title: String,
    pub url: String,
}

/// Exportable user data. Credentials are never part of this structure.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppData {
    pub version: u32,
    pub glossary: Vec<GlossaryEntry>,
    pub bookmarks: Vec<Bookmark>,
    pub history: Vec<HistoryItem>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GlossaryEntry {
    pub id: String,
    pub source: String,
    #[serde(default)]
    pub reading: String,
    pub translation: String,
    #[serde(default)]
    pub alternatives: String,
    #[serde(default)]
    pub aliases: String,
    #[serde(default)]
    pub do_not_translate: bool,
    #[serde(default)]
    pub case_sensitive: bool,
    #[serde(default)]
    pub exact_match: bool,
    #[serde(default)]
    pub use_regex: bool,
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default)]
    pub project_id: Option<String>,
    #[serde(default)]
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Bookmark {
    pub id: String,
    pub url: String,
    pub title: String,
    pub folder: String,
    pub favorite: bool,
    pub created_at: i64,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct TranslationHistory {
    pub id: i64,
    pub url: String,
    pub title: String,
    pub source: String,
    pub translation: String,
    pub model: String,
    pub translated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "camelCase")]
pub struct SearchHistoryItem {
    pub id: i64,
    pub query: String,
    pub engine: String,
    pub searched_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SavedLogin {
    pub id: String,
    pub origin: String,
    pub username: String,
    pub password: String,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AutofillProfile {
    pub name: String,
    pub email: String,
    pub phone: String,
    pub address: String,
    pub postal_code: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncConfig {
    pub provider: String,
    pub signed_in: bool,
    pub display_name: String,
    pub email: String,
    pub sync_dir: String,
    pub last_synced_at: Option<i64>,
    pub sync_bookmarks: bool,
    pub sync_settings: bool,
    pub sync_personal_info: bool,
    pub sync_passwords: bool,
    pub sync_history: bool,
    pub sync_open_tabs: bool,
    pub sync_extensions: bool,
    pub sync_glossary: bool,
    pub sync_workspaces: bool,
    pub sync_usage: bool,
}

impl Default for SyncConfig {
    fn default() -> Self {
        Self {
            provider: String::new(),
            signed_in: false,
            display_name: "개인".into(),
            email: String::new(),
            sync_dir: String::new(),
            last_synced_at: None,
            sync_bookmarks: true,
            sync_settings: true,
            sync_personal_info: true,
            sync_passwords: true,
            sync_history: true,
            sync_open_tabs: true,
            sync_extensions: true,
            sync_glossary: true,
            sync_workspaces: true,
            sync_usage: false,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CookieEntry {
    pub name: String,
    pub domain: String,
    pub path: String,
    pub value_preview: String,
    pub secure: bool,
    pub http_only: bool,
    pub session: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UserScriptItem {
    pub id: String,
    pub name: String,
    pub match_pattern: String,
    pub code: String,
    pub css: String,
    pub enabled: bool,
    pub updated_at: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WebExtensionInfo {
    pub id: String,
    pub name: String,
    pub enabled: bool,
    pub path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExternalImportSummary {
    pub source: String,
    pub bookmarks: u32,
    pub history: u32,
    pub logins: u32,
}

fn default_true() -> bool {
    true
}
