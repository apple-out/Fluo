use std::fs;
use std::path::{Path, PathBuf};

use rusqlite::{Connection, OpenFlags};
use tauri::webview::{PageLoadEvent, WebviewBuilder};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Webview, WebviewUrl};

use crate::model::{
    AppSettings, AutofillProfile, Bookmark, CookieEntry, ExternalImportSummary, HistoryItem,
    SettingsView, SyncConfig, WebExtensionInfo,
};
use crate::{AppState, ai, credentials, db, lock, text};

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct SyncSnapshot {
    version: u32,
    #[serde(default)]
    device_id: String,
    updated_at: i64,
    email: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    bookmarks: Option<Vec<Bookmark>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    settings: Option<AppSettings>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    personal_info: Option<AutofillProfile>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    history: Option<Vec<HistoryItem>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    glossary: Option<Vec<crate::model::GlossaryEntry>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    workspaces: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    open_tabs: Option<serde_json::Value>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    user_scripts: Option<Vec<crate::model::UserScriptItem>>,
}

pub fn default_sync_dir(app_data_dir: &Path) -> PathBuf {
    app_data_dir.join("sync")
}

/// Cloud drive roots that back account-based sync. Google uses the
/// "Google Drive for desktop" mount, Microsoft uses the OneDrive folder.
#[derive(serde::Serialize, Clone)]
#[serde(rename_all = "camelCase")]
pub struct CloudAccountRoot {
    pub provider: String,
    pub label: String,
    pub root: String,
    pub available: bool,
}

fn google_drive_root() -> Option<PathBuf> {
    #[cfg(windows)]
    {
        for letter in b'D'..=b'Z' {
            for name in ["My Drive", "내 드라이브"] {
                let candidate = PathBuf::from(format!("{}:\\{}", letter as char, name));
                if candidate.is_dir() {
                    return Some(candidate);
                }
            }
        }
        if let Ok(profile) = std::env::var("USERPROFILE") {
            for name in ["Google Drive\\My Drive", "Google Drive", "내 드라이브"] {
                let candidate = PathBuf::from(&profile).join(name);
                if candidate.is_dir() {
                    return Some(candidate);
                }
            }
        }
    }
    None
}

fn onedrive_root() -> Option<PathBuf> {
    for var in ["OneDriveConsumer", "OneDrive", "OneDriveCommercial"] {
        if let Ok(path) = std::env::var(var) {
            let candidate = PathBuf::from(path);
            if candidate.is_dir() {
                return Some(candidate);
            }
        }
    }
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let candidate = PathBuf::from(profile).join("OneDrive");
        if candidate.is_dir() {
            return Some(candidate);
        }
    }
    None
}

pub fn detect_cloud_accounts() -> Vec<CloudAccountRoot> {
    let google = google_drive_root();
    let microsoft = onedrive_root();
    vec![
        CloudAccountRoot {
            provider: "google".into(),
            label: "Google 계정 (Google Drive)".into(),
            root: google
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            available: google.is_some(),
        },
        CloudAccountRoot {
            provider: "microsoft".into(),
            label: "Microsoft 계정 (OneDrive)".into(),
            root: microsoft
                .as_ref()
                .map(|p| p.to_string_lossy().into_owned())
                .unwrap_or_default(),
            available: microsoft.is_some(),
        },
    ]
}

fn sanitize_account(email: &str) -> String {
    email
        .trim()
        .to_ascii_lowercase()
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '.' || c == '-' || c == '_' || c == '@' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

pub fn connect_sync_account(
    app: &AppHandle,
    state: &AppState,
    provider: &str,
    email: &str,
    display_name: &str,
    custom_sync_dir: Option<&str>,
) -> Result<SyncConfig, String> {
    let provider_norm = provider.trim().to_ascii_lowercase();
    let email = email.trim();
    match provider_norm.as_str() {
        "google" | "microsoft" => {
            if !email.contains('@') || email.len() > 254 {
                return Err("동기화할 계정 이메일을 정확히 입력하세요.".into());
            }
        }
        "local" => {
            if email.is_empty() || email.len() > 254 {
                return Err("동기화 프로필 이름 또는 식별자를 입력하세요.".into());
            }
        }
        _ => return Err("지원하지 않는 계정 종류입니다.".into()),
    }
    let dir = if let Some(custom) = custom_sync_dir.map(str::trim).filter(|s| !s.is_empty()) {
        if custom.chars().any(char::is_control) {
            return Err("동기화 폴더 경로가 올바르지 않습니다.".into());
        }
        PathBuf::from(custom)
    } else {
        let cloud_root = match provider_norm.as_str() {
            "google" => google_drive_root(),
            "microsoft" => onedrive_root(),
            _ => None,
        };
        match cloud_root {
            Some(root) => root
                .join("TransrateBrowserSync")
                .join(sanitize_account(email)),
            None => state
                .data_dir
                .join("sync")
                .join(&provider_norm)
                .join(sanitize_account(email)),
        }
    };
    fs::create_dir_all(&dir)
        .map_err(|error| format!("동기화 폴더를 만들지 못했습니다: {error}"))?;
    let next_sync_dir = dir.to_string_lossy().into_owned();
    let mut cfg = {
        let db = lock(&state.db);
        load_sync_config(&db, &state.data_dir)
    };
    let switched_account = !cfg.signed_in
        || cfg.provider != provider_norm
        || !cfg.email.eq_ignore_ascii_case(email)
        || cfg.sync_dir != next_sync_dir;
    if switched_account {
        cfg.last_synced_at = None;
    }
    cfg.provider = provider_norm;
    cfg.email = email.to_string();
    if !display_name.trim().is_empty() {
        cfg.display_name = display_name.trim().chars().take(60).collect();
    } else if cfg.display_name.trim().is_empty() {
        cfg.display_name = email
            .split('@')
            .next()
            .unwrap_or(email)
            .chars()
            .take(60)
            .collect();
    }
    cfg.sync_dir = next_sync_dir;
    cfg.signed_in = true;
    {
        let db = lock(&state.db);
        save_sync_config(&db, &cfg)?;
    }
    run_sync_now(app, state)
}

pub fn disconnect_sync_account(state: &AppState) -> Result<SyncConfig, String> {
    let db = lock(&state.db);
    let mut cfg = load_sync_config(&db, &state.data_dir);
    cfg.signed_in = false;
    cfg.provider.clear();
    cfg.email.clear();
    cfg.display_name = "개인".into();
    cfg.last_synced_at = None;
    cfg.sync_dir = default_sync_dir(&state.data_dir)
        .to_string_lossy()
        .into_owned();
    save_sync_config(&db, &cfg)?;
    Ok(cfg)
}

pub fn load_sync_config(db: &Connection, data_dir: &Path) -> SyncConfig {
    let mut cfg: SyncConfig = db::get_setting(db, "sync_config")
        .and_then(|json| serde_json::from_str(&json).ok())
        .unwrap_or_default();
    if cfg.sync_dir.trim().is_empty() {
        cfg.sync_dir = default_sync_dir(data_dir).to_string_lossy().into_owned();
    }
    cfg
}

pub fn save_sync_config(db: &Connection, config: &SyncConfig) -> Result<(), String> {
    let json = serde_json::to_string(config).map_err(|error| error.to_string())?;
    db::set_setting(db, "sync_config", &json)
}

fn get_or_create_sync_device_id(db: &Connection) -> String {
    if let Some(existing) = db::get_setting(db, "sync_device_id") {
        let trimmed = existing.trim();
        if !trimmed.is_empty() {
            return trimmed.to_string();
        }
    }
    let nanos = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_nanos())
        .unwrap_or(0);
    let id = format!("dev-{:x}-{}", nanos, std::process::id());
    let _ = db::set_setting(db, "sync_device_id", &id);
    id
}

#[derive(Default)]
struct SyncPullEffects {
    workspaces_changed: bool,
    settings_updated: Option<AppSettings>,
}

fn apply_portable_settings(local: &mut AppSettings, remote: &AppSettings) -> bool {
    let mut changed = false;
    macro_rules! sync_field {
        ($field:ident) => {
            if local.$field != remote.$field {
                local.$field = remote.$field.clone();
                changed = true;
            }
        };
    }
    sync_field!(ui_language);
    sync_field!(search_engine);
    sync_field!(startup_mode);
    sync_field!(home_url);
    sync_field!(target_language);
    sync_field!(style);
    sync_field!(theme);
    sync_field!(ad_block_allowlist);
    sync_field!(tracking_prevention);
    sync_field!(popup_policy);
    sync_field!(popup_allowed_origins);
    sync_field!(sleep_allowlist);
    if local.ad_block_enabled != remote.ad_block_enabled {
        local.ad_block_enabled = remote.ad_block_enabled;
        changed = true;
    }
    if local.sleeping_tabs_enabled != remote.sleeping_tabs_enabled {
        local.sleeping_tabs_enabled = remote.sleeping_tabs_enabled;
        changed = true;
    }
    if local.sleep_timeout_minutes != remote.sleep_timeout_minutes {
        local.sleep_timeout_minutes = remote.sleep_timeout_minutes;
        changed = true;
    }
    if local.max_awake_tabs != remote.max_awake_tabs {
        local.max_awake_tabs = remote.max_awake_tabs;
        changed = true;
    }
    if (local.translation_font_scale - remote.translation_font_scale).abs() > f64::EPSILON {
        local.translation_font_scale = remote.translation_font_scale;
        changed = true;
    }
    if (local.translation_line_height - remote.translation_line_height).abs() > f64::EPSILON {
        local.translation_line_height = remote.translation_line_height;
        changed = true;
    }
    if remote.onboarding_completed && !local.onboarding_completed {
        local.onboarding_completed = true;
        changed = true;
    }
    changed
}

fn write_sync_snapshot_file(dir: &Path, sync_file: &Path, json: &str) -> Result<(), String> {
    let tmp = dir.join("sync-profile.json.tmp");
    fs::write(&tmp, json).map_err(|error| format!("동기화 파일을 쓰지 못했습니다: {error}"))?;
    if fs::rename(&tmp, sync_file).is_ok() {
        return Ok(());
    }
    // Windows cloud sync daemons (OneDrive/Google Drive) briefly lock newly touched files; retry once.
    std::thread::sleep(std::time::Duration::from_millis(120));
    if fs::rename(&tmp, sync_file).is_ok() {
        return Ok(());
    }
    let _ = fs::remove_file(&tmp);
    fs::write(sync_file, json)
        .map_err(|error| format!("동기화 파일을 저장하지 못했습니다: {error}"))
}

fn run_sync_core(
    db: &Connection,
    settings_state: &mut AppSettings,
    tabs_state: &mut crate::browser::Tabs,
    cfg: &mut SyncConfig,
    device_id: &str,
    now: i64,
) -> Result<SyncPullEffects, String> {
    if !cfg.signed_in || cfg.provider.is_empty() {
        return Err("Google 또는 Microsoft 계정으로 먼저 로그인하세요.".into());
    }
    let dir = PathBuf::from(cfg.sync_dir.trim());
    fs::create_dir_all(&dir).map_err(|error| format!("동기화 폴더를 열지 못했습니다: {error}"))?;
    let sync_file = dir.join("sync-profile.json");
    let mut effects = SyncPullEffects::default();

    // 1) Merge from remote snapshot only when connecting a fresh device/account or when a newer
    //    snapshot came from a different device, so local deletions are never resurrected.
    if let Ok(raw) = fs::read_to_string(&sync_file)
        && let Ok(remote) = serde_json::from_str::<SyncSnapshot>(&raw)
    {
        let remote_is_newer = remote.updated_at > cfg.last_synced_at.unwrap_or(0)
            && (remote.device_id.is_empty() || remote.device_id != device_id);
        let should_pull = cfg.last_synced_at.is_none() || remote_is_newer;
        if should_pull {
            if cfg.sync_bookmarks
                && let Some(remote_bookmarks) = remote.bookmarks
            {
                for bookmark in remote_bookmarks {
                    let _ = db::save_bookmark(db, &bookmark);
                }
            }
            if cfg.sync_glossary
                && let Some(remote_glossary) = remote.glossary
            {
                for entry in remote_glossary {
                    let _ = db::upsert_glossary(db, &entry);
                }
            }
            if cfg.sync_extensions
                && let Some(remote_scripts) = remote.user_scripts
            {
                for script in remote_scripts {
                    let _ = db::save_user_script(db, &script);
                }
            }
            if cfg.sync_history
                && let Some(remote_history) = remote.history
            {
                let existing: std::collections::HashSet<String> = db::list_history(db, "", 500)
                    .unwrap_or_default()
                    .into_iter()
                    .map(|h| format!("{}@{}", h.url, h.visited_at))
                    .collect();
                for item in remote_history {
                    let key = format!("{}@{}", item.url, item.visited_at);
                    if !existing.contains(&key) {
                        let _ = db::insert_history_item(db, &item);
                    }
                }
            }
            if cfg.sync_personal_info
                && let Some(remote_profile) = remote.personal_info
            {
                let mut local_profile: AutofillProfile = db::get_setting(db, "autofill_profile")
                    .and_then(|s| serde_json::from_str(&s).ok())
                    .unwrap_or_default();
                let prefer_remote = remote_is_newer || cfg.last_synced_at.is_none();
                macro_rules! merge_profile {
                    ($field:ident) => {
                        let rem = remote_profile.$field.trim();
                        if !rem.is_empty()
                            && (local_profile.$field.trim().is_empty() || prefer_remote)
                        {
                            local_profile.$field = rem.to_string();
                        }
                    };
                }
                merge_profile!(name);
                merge_profile!(email);
                merge_profile!(phone);
                merge_profile!(address);
                merge_profile!(postal_code);
                if let Ok(profile_json) = serde_json::to_string(&local_profile) {
                    let _ = db::set_setting(db, "autofill_profile", &profile_json);
                }
            }
            if cfg.sync_workspaces
                && let Some(remote_ws_val) = remote.workspaces
                && let Ok(remote_ws) =
                    serde_json::from_value::<crate::browser::WorkspaceSnapshot>(remote_ws_val)
            {
                let mut added_ws = false;
                for ws in remote_ws.workspaces {
                    let name_clean: String = ws.name.trim().chars().take(80).collect();
                    if ws.id.trim().is_empty() || name_clean.is_empty() {
                        continue;
                    }
                    let dup = tabs_state
                        .workspaces
                        .iter()
                        .any(|w| w.id == ws.id || w.name.eq_ignore_ascii_case(&name_clean));
                    if !dup {
                        tabs_state.workspaces.push(crate::model::Workspace {
                            id: ws.id,
                            name: name_clean,
                        });
                        added_ws = true;
                    }
                }
                if added_ws {
                    if let Ok(ws_json) = serde_json::to_string(&crate::browser::WorkspaceSnapshot {
                        workspaces: tabs_state.workspaces.clone(),
                        active_workspace: tabs_state.active_workspace.clone(),
                    }) {
                        let _ = db::set_setting(db, "workspaces", &ws_json);
                    }
                    effects.workspaces_changed = true;
                }
            }
            if cfg.sync_settings
                && let Some(remote_settings) = remote.settings
                && let Ok(valid_remote) = crate::validate_settings(remote_settings)
                && (remote_is_newer || cfg.last_synced_at.is_none())
                && apply_portable_settings(settings_state, &valid_remote)
                && let Ok(settings_json) = serde_json::to_string(settings_state)
            {
                let _ = db::set_setting(db, "app_settings", &settings_json);
                effects.settings_updated = Some(settings_state.clone());
            }
        }
    }

    // 2) Write fresh merged snapshot according to user toggles (passwords & secrets never exported).
    let snapshot = SyncSnapshot {
        version: 1,
        device_id: device_id.to_string(),
        updated_at: now,
        email: cfg.email.clone(),
        bookmarks: cfg
            .sync_bookmarks
            .then(|| db::list_bookmarks(db).unwrap_or_default()),
        settings: cfg.sync_settings.then(|| settings_state.clone()),
        personal_info: cfg.sync_personal_info.then(|| {
            db::get_setting(db, "autofill_profile")
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or_default()
        }),
        history: cfg
            .sync_history
            .then(|| db::list_history(db, "", 500).unwrap_or_default()),
        glossary: cfg
            .sync_glossary
            .then(|| db::list_glossary(db).unwrap_or_default()),
        workspaces: cfg.sync_workspaces.then(|| {
            serde_json::to_value(crate::browser::WorkspaceSnapshot {
                workspaces: tabs_state.workspaces.clone(),
                active_workspace: tabs_state.active_workspace.clone(),
            })
            .unwrap_or(serde_json::Value::Null)
        }),
        open_tabs: cfg.sync_open_tabs.then(|| {
            db::load_session(db)
                .and_then(|s| serde_json::from_str(&s).ok())
                .unwrap_or(serde_json::Value::Null)
        }),
        user_scripts: cfg
            .sync_extensions
            .then(|| db::list_user_scripts(db).unwrap_or_default()),
    };

    let json = serde_json::to_string_pretty(&snapshot).map_err(|error| error.to_string())?;
    write_sync_snapshot_file(&dir, &sync_file, &json)?;

    cfg.last_synced_at = Some(now);
    save_sync_config(db, cfg)?;
    Ok(effects)
}

pub fn run_sync_now(app: &AppHandle, state: &AppState) -> Result<SyncConfig, String> {
    let now = db::now_secs();
    let (mut cfg, effects, browser_snapshot) = {
        let db = lock(&state.db);
        let device_id = get_or_create_sync_device_id(&db);
        let mut cfg = load_sync_config(&db, &state.data_dir);
        let mut settings_guard = lock(&state.settings);
        let mut tabs_guard = lock(&state.tabs);
        let effects = run_sync_core(
            &db,
            &mut settings_guard,
            &mut tabs_guard,
            &mut cfg,
            &device_id,
            now,
        )?;
        let snapshot = effects.workspaces_changed.then(|| tabs_guard.snapshot());
        (cfg, effects, snapshot)
    };
    if let Some(snapshot) = browser_snapshot {
        let _ = app.emit_to("shell", "browser-state", snapshot);
    }
    if let Some(updated_settings) = effects.settings_updated {
        state.ad_block_enabled.store(
            updated_settings.ad_block_enabled,
            std::sync::atomic::Ordering::SeqCst,
        );
        crate::browser::sync_adblock_state(app, updated_settings.ad_block_enabled);
        let _ = crate::browser::sync_ui_language_to_tabs(app, &updated_settings.ui_language);
        let _ = app.emit("settings-updated", crate::settings_view_pub(state));
    }
    let _ = &mut cfg;
    Ok(cfg)
}

#[derive(serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DetectedBrowserProfile {
    pub id: String,
    pub label: String,
    pub has_bookmarks: bool,
    pub has_history: bool,
}

fn external_browser_roots() -> Vec<(&'static str, &'static str, PathBuf)> {
    let mut roots = Vec::new();
    #[cfg(windows)]
    if let Ok(local_app_data) = std::env::var("LOCALAPPDATA") {
        let base = PathBuf::from(local_app_data);
        roots.push((
            "edge",
            "Microsoft Edge",
            base.join("Microsoft")
                .join("Edge")
                .join("User Data")
                .join("Default"),
        ));
        roots.push((
            "chrome",
            "Google Chrome",
            base.join("Google")
                .join("Chrome")
                .join("User Data")
                .join("Default"),
        ));
        roots.push((
            "whale",
            "Naver Whale",
            base.join("Naver")
                .join("Naver Whale")
                .join("User Data")
                .join("Default"),
        ));
    }
    roots
}

pub fn detect_external_browsers() -> Vec<DetectedBrowserProfile> {
    external_browser_roots()
        .into_iter()
        .filter(|(_, _, dir)| dir.is_dir())
        .map(|(id, label, dir)| DetectedBrowserProfile {
            id: id.into(),
            label: label.into(),
            has_bookmarks: dir.join("Bookmarks").is_file(),
            has_history: dir.join("History").is_file(),
        })
        .collect()
}

fn collect_chromium_bookmarks(node: &serde_json::Value, folder: &str, out: &mut Vec<Bookmark>) {
    let kind = node.get("type").and_then(|v| v.as_str()).unwrap_or("");
    let name = node
        .get("name")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .trim();
    if kind == "url" {
        if let Some(raw_url) = node.get("url").and_then(|v| v.as_str())
            && let Ok(url) = text::normalize_page_url(raw_url)
        {
            let now = db::now_secs();
            out.push(Bookmark {
                id: format!("imp-{}", text::page_id(&url)),
                url: url.clone(),
                title: if name.is_empty() {
                    url
                } else {
                    name.chars().take(300).collect()
                },
                folder: folder.chars().take(100).collect(),
                favorite: false,
                created_at: now,
                updated_at: now,
            });
        }
    } else if let Some(children) = node.get("children").and_then(|v| v.as_array()) {
        let next_folder = if name.is_empty()
            || matches!(
                name,
                "북마크 모음"
                    | "Bookmark bar"
                    | "Bookmarks bar"
                    | "기타 북마크"
                    | "Other bookmarks"
            ) {
            folder.to_string()
        } else if folder.is_empty() {
            name.to_string()
        } else {
            format!("{folder}/{name}")
        };
        for child in children {
            collect_chromium_bookmarks(child, &next_folder, out);
        }
    }
}

pub fn import_from_external_browser(
    state: &AppState,
    browser_id: &str,
    include_bookmarks: bool,
    include_history: bool,
) -> Result<ExternalImportSummary, String> {
    let Some((_, label, dir)) = external_browser_roots()
        .into_iter()
        .find(|(id, _, _)| *id == browser_id)
    else {
        return Err("선택한 브라우저 프로필 폴더를 찾지 못했습니다.".into());
    };

    let mut summary = ExternalImportSummary {
        source: label.into(),
        bookmarks: 0,
        history: 0,
        logins: 0,
    };

    if include_bookmarks {
        let bm_path = dir.join("Bookmarks");
        if bm_path.is_file() {
            let raw = fs::read_to_string(&bm_path)
                .map_err(|error| format!("북마크 파일을 읽지 못했습니다: {error}"))?;
            let json: serde_json::Value = serde_json::from_str(&raw)
                .map_err(|error| format!("북마크 형식을 해석하지 못했습니다: {error}"))?;
            let mut imported = Vec::new();
            if let Some(roots) = json.get("roots").and_then(|v| v.as_object()) {
                for (_, node) in roots {
                    collect_chromium_bookmarks(node, "", &mut imported);
                }
            }
            let db = lock(&state.db);
            for bookmark in imported {
                if !bookmark.folder.is_empty() {
                    let _ = db::create_bookmark_folder(&db, &bookmark.folder);
                }
                if db::save_bookmark(&db, &bookmark).is_ok() {
                    summary.bookmarks += 1;
                }
            }
        }
    }

    if include_history {
        let hist_path = dir.join("History");
        if hist_path.is_file() {
            let tmp_copy = state.data_dir.join("external_history_import.sqlite3");
            if fs::copy(&hist_path, &tmp_copy).is_ok() {
                if let Ok(ext_conn) =
                    Connection::open_with_flags(&tmp_copy, OpenFlags::SQLITE_OPEN_READ_ONLY)
                {
                    if let Ok(mut stmt) = ext_conn.prepare(
                        "SELECT url, title, last_visit_time FROM urls
                         WHERE url LIKE 'http%'
                         ORDER BY last_visit_time DESC LIMIT 400",
                    ) {
                        if let Ok(rows) = stmt.query_map([], |row| {
                            let url: String = row.get(0)?;
                            let title: String = row.get(1)?;
                            let webkit_ts: i64 = row.get(2).unwrap_or(0);
                            // Convert Chrome WebKit timestamp (us since 1601-01-01) to Unix seconds.
                            let unix_ts = if webkit_ts > 11_644_473_600_000_000 {
                                (webkit_ts / 1_000_000) - 11_644_473_600
                            } else {
                                db::now_secs()
                            };
                            Ok(HistoryItem {
                                id: 0,
                                url,
                                title,
                                visited_at: unix_ts,
                            })
                        }) {
                            let db = lock(&state.db);
                            for row in rows.flatten() {
                                if db::insert_history_item(&db, &row).is_ok() {
                                    summary.history += 1;
                                }
                            }
                        }
                    }
                }
                let _ = fs::remove_file(&tmp_copy);
            }
        }
    }

    Ok(summary)
}

pub fn import_passwords_csv(state: &AppState, csv_text: &str) -> Result<u32, String> {
    let mut lines = csv_text.lines();
    let Some(header) = lines.next() else {
        return Ok(0);
    };
    let cols: Vec<String> = header
        .split(',')
        .map(|c| c.trim().trim_matches('"').to_ascii_lowercase())
        .collect();
    let url_idx = cols
        .iter()
        .position(|c| c == "url" || c == "origin")
        .unwrap_or(1);
    let user_idx = cols
        .iter()
        .position(|c| c == "username" || c == "user")
        .unwrap_or(2);
    let pass_idx = cols
        .iter()
        .position(|c| c == "password" || c == "pass")
        .unwrap_or(3);

    let db = lock(&state.db);
    let mut count = 0_u32;
    for line in lines {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            continue;
        }
        let parts: Vec<String> = trimmed
            .split(',')
            .map(|p| p.trim().trim_matches('"').to_string())
            .collect();
        let raw_url = parts.get(url_idx).map(String::as_str).unwrap_or("");
        let username = parts.get(user_idx).map(String::as_str).unwrap_or("");
        let password = parts.get(pass_idx).map(String::as_str).unwrap_or("");
        if raw_url.is_empty() || username.is_empty() || password.is_empty() {
            continue;
        }
        let origin = url::Url::parse(raw_url)
            .map(|u| u.origin().unicode_serialization())
            .unwrap_or_else(|_| raw_url.to_string());
        let id = format!("pw-{}-{}", text::page_id(&origin), count);
        if db::save_login(&db, &id, &origin, username, password).is_ok() {
            count += 1;
        }
    }
    Ok(count)
}

#[cfg(windows)]
pub async fn list_cookies(app: &AppHandle, uri: &str) -> Result<Vec<CookieEntry>, String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_2;
    use webview2_com::{GetCookiesCompletedHandler, take_pwstr};
    use windows::core::{HSTRING, Interface, PWSTR};

    let webview: Webview = crate::browser::active_webview(app).or_else(|_| {
        app.get_webview("shell")
            .ok_or_else(|| "WebView 없음".to_string())
    })?;
    let uri = uri.trim().to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx_box = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
    let tx_cb = tx_box.clone();

    webview
        .with_webview(move |platform| unsafe {
            let result = (|| -> windows::core::Result<()> {
                let core2: ICoreWebView2_2 = platform.controller().CoreWebView2()?.cast()?;
                let manager = core2.CookieManager()?;
                let uri_h = HSTRING::from(uri);
                let handler = GetCookiesCompletedHandler::create(Box::new(move |hr, list| {
                    let mut entries = Vec::new();
                    if hr.is_ok()
                        && let Some(list) = list
                    {
                        let mut count = 0_u32;
                        if list.Count(&mut count).is_ok() {
                            for i in 0..count.min(300) {
                                if let Ok(cookie) = list.GetValueAtIndex(i) {
                                    let mut name_p = PWSTR::null();
                                    let mut domain_p = PWSTR::null();
                                    let mut path_p = PWSTR::null();
                                    let mut val_p = PWSTR::null();
                                    let mut is_secure = windows::core::BOOL::from(false);
                                    let mut is_http_only = windows::core::BOOL::from(false);
                                    let mut is_session = windows::core::BOOL::from(false);
                                    let _ = cookie.Name(&mut name_p);
                                    let _ = cookie.Domain(&mut domain_p);
                                    let _ = cookie.Path(&mut path_p);
                                    let _ = cookie.Value(&mut val_p);
                                    let _ = cookie.IsSecure(&mut is_secure);
                                    let _ = cookie.IsHttpOnly(&mut is_http_only);
                                    let _ = cookie.IsSession(&mut is_session);
                                    let val = take_pwstr(val_p);
                                    let preview: String = val.chars().take(48).collect();
                                    entries.push(CookieEntry {
                                        name: take_pwstr(name_p),
                                        domain: take_pwstr(domain_p),
                                        path: take_pwstr(path_p),
                                        value_preview: preview,
                                        secure: is_secure.as_bool(),
                                        http_only: is_http_only.as_bool(),
                                        session: is_session.as_bool(),
                                    });
                                }
                            }
                        }
                    }
                    if let Some(tx) = tx_cb.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = tx.send(Ok(entries));
                    }
                    Ok(())
                }));
                manager.GetCookies(&uri_h, &handler)?;
                Ok(())
            })();
            if let Err(err) = result
                && let Some(tx) = tx_box.lock().unwrap_or_else(|e| e.into_inner()).take()
            {
                let _ = tx.send(Err(err.to_string()));
            }
        })
        .map_err(|e| e.to_string())?;

    rx.await
        .map_err(|_| "쿠키 조회 응답을 받지 못했습니다.".to_string())?
}

#[cfg(not(windows))]
pub async fn list_cookies(_app: &AppHandle, _uri: &str) -> Result<Vec<CookieEntry>, String> {
    Ok(Vec::new())
}

#[cfg(windows)]
pub async fn delete_cookie(
    app: &AppHandle,
    name: &str,
    domain: &str,
    path: &str,
) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2_2;
    use windows::core::{HSTRING, Interface};

    let webview: Webview = crate::browser::active_webview(app).or_else(|_| {
        app.get_webview("shell")
            .ok_or_else(|| "WebView 없음".to_string())
    })?;
    let name = name.to_string();
    let domain = domain.to_string();
    let path = path.to_string();
    webview
        .with_webview(move |platform| unsafe {
            let _ = (|| -> windows::core::Result<()> {
                let core2: ICoreWebView2_2 = platform.controller().CoreWebView2()?.cast()?;
                let manager = core2.CookieManager()?;
                manager.DeleteCookiesWithDomainAndPath(
                    &HSTRING::from(name),
                    &HSTRING::from(domain),
                    &HSTRING::from(path),
                )?;
                Ok(())
            })();
        })
        .map_err(|e| e.to_string())
}

#[cfg(not(windows))]
pub async fn delete_cookie(
    _app: &AppHandle,
    _name: &str,
    _domain: &str,
    _path: &str,
) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
pub async fn clear_browsing_data(app: &AppHandle, scope: &str) -> Result<(), String> {
    use webview2_com::ClearBrowsingDataCompletedHandler;
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_DOM_STORAGE,
        COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_SITE, COREWEBVIEW2_BROWSING_DATA_KINDS_COOKIES,
        COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE, ICoreWebView2_13, ICoreWebView2Profile2,
    };
    use windows::core::Interface;

    let webview: Webview = crate::browser::active_webview(app).or_else(|_| {
        app.get_webview("shell")
            .ok_or_else(|| "WebView 없음".to_string())
    })?;
    let kinds = match scope {
        "cookies" => COREWEBVIEW2_BROWSING_DATA_KINDS_COOKIES,
        "cache" => COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE,
        _ => webview2_com::Microsoft::Web::WebView2::Win32::COREWEBVIEW2_BROWSING_DATA_KINDS(
            COREWEBVIEW2_BROWSING_DATA_KINDS_COOKIES.0
                | COREWEBVIEW2_BROWSING_DATA_KINDS_DISK_CACHE.0
                | COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_DOM_STORAGE.0
                | COREWEBVIEW2_BROWSING_DATA_KINDS_ALL_SITE.0,
        ),
    };
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx_box = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
    let tx_cb = tx_box.clone();
    webview
        .with_webview(move |platform| unsafe {
            let result = (|| -> windows::core::Result<()> {
                let core13: ICoreWebView2_13 = platform.controller().CoreWebView2()?.cast()?;
                let profile2: ICoreWebView2Profile2 = core13.Profile()?.cast()?;
                let handler = ClearBrowsingDataCompletedHandler::create(Box::new(move |hr| {
                    if let Some(tx) = tx_cb.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = tx.send(hr.map_err(|e| e.to_string()));
                    }
                    Ok(())
                }));
                profile2.ClearBrowsingData(kinds, &handler)?;
                Ok(())
            })();
            if let Err(err) = result
                && let Some(tx) = tx_box.lock().unwrap_or_else(|e| e.into_inner()).take()
            {
                let _ = tx.send(Err(err.to_string()));
            }
        })
        .map_err(|e| e.to_string())?;
    rx.await
        .map_err(|_| "데이터 삭제 응답이 없습니다.".to_string())?
}

#[cfg(not(windows))]
pub async fn clear_browsing_data(_app: &AppHandle, _scope: &str) -> Result<(), String> {
    Ok(())
}

#[cfg(windows)]
pub async fn list_web_extensions(app: &AppHandle) -> Result<Vec<WebExtensionInfo>, String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2_13, ICoreWebView2Profile7};
    use webview2_com::{ProfileGetBrowserExtensionsCompletedHandler, take_pwstr};
    use windows::core::{BOOL, Interface, PWSTR};

    let webview = app
        .get_webview("shell")
        .ok_or_else(|| "WebView 없음".to_string())?;
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx_box = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
    let tx_cb = tx_box.clone();
    let ext_dir = app
        .state::<AppState>()
        .extensions_dir
        .to_string_lossy()
        .into_owned();

    webview
        .with_webview(move |platform| unsafe {
            let result = (|| -> windows::core::Result<()> {
                let core13: ICoreWebView2_13 = platform.controller().CoreWebView2()?.cast()?;
                let profile7: ICoreWebView2Profile7 = core13.Profile()?.cast()?;
                let handler = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(
                    move |hr, list| {
                        let mut items = Vec::new();
                        if hr.is_ok()
                            && let Some(list) = list
                        {
                            let mut count = 0_u32;
                            if list.Count(&mut count).is_ok() {
                                for i in 0..count {
                                    if let Ok(ext) = list.GetValueAtIndex(i) {
                                        let mut id_p = PWSTR::null();
                                        let mut name_p = PWSTR::null();
                                        let mut enabled = BOOL::from(true);
                                        let _ = ext.Id(&mut id_p);
                                        let _ = ext.Name(&mut name_p);
                                        let _ = ext.IsEnabled(&mut enabled);
                                        items.push(WebExtensionInfo {
                                            id: take_pwstr(id_p),
                                            name: take_pwstr(name_p),
                                            enabled: enabled.as_bool(),
                                            path: ext_dir.clone(),
                                        });
                                    }
                                }
                            }
                        }
                        if let Some(tx) = tx_cb.lock().unwrap_or_else(|e| e.into_inner()).take() {
                            let _ = tx.send(Ok(items));
                        }
                        Ok(())
                    },
                ));
                profile7.GetBrowserExtensions(&handler)?;
                Ok(())
            })();
            if let Err(err) = result
                && let Some(tx) = tx_box.lock().unwrap_or_else(|e| e.into_inner()).take()
            {
                let _ = tx.send(Err(err.to_string()));
            }
        })
        .map_err(|e| e.to_string())?;
    rx.await
        .map_err(|_| "확장 목록을 읽지 못했습니다.".to_string())?
}

#[cfg(not(windows))]
pub async fn list_web_extensions(_app: &AppHandle) -> Result<Vec<WebExtensionInfo>, String> {
    Ok(Vec::new())
}

#[cfg(windows)]
pub async fn add_web_extension(
    app: &AppHandle,
    folder_path: &str,
) -> Result<WebExtensionInfo, String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2_13, ICoreWebView2Profile7};
    use webview2_com::{ProfileAddBrowserExtensionCompletedHandler, take_pwstr};
    use windows::core::{BOOL, HSTRING, Interface, PWSTR};

    let folder = PathBuf::from(folder_path.trim());
    if !folder.join("manifest.json").is_file() {
        return Err("선택한 폴더 안에 manifest.json 파일이 없습니다. 압축 해제된 확장 프로그램 폴더를 지정하세요.".into());
    }
    let ext_root = app.state::<AppState>().extensions_dir.clone();
    let _ = fs::create_dir_all(&ext_root);
    let dest_folder = if folder.starts_with(&ext_root) {
        folder
    } else {
        let dir_name = folder
            .file_name()
            .and_then(|n| n.to_str())
            .filter(|n| !n.trim().is_empty())
            .unwrap_or("extension");
        let target = ext_root.join(format!("{}-{}", dir_name, db::now_secs()));
        copy_dir_recursive(&folder, &target)?;
        target
    };
    let webview = app
        .get_webview("shell")
        .ok_or_else(|| "WebView 없음".to_string())?;
    let path_str = dest_folder.to_string_lossy().into_owned();
    let path_clone = path_str.clone();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx_box = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
    let tx_err = tx_box.clone();
    webview
        .with_webview(move |platform| unsafe {
            let result = (|| -> windows::core::Result<()> {
                let core13: ICoreWebView2_13 = platform.controller().CoreWebView2()?.cast()?;
                let profile7: ICoreWebView2Profile7 = core13.Profile()?.cast()?;
                let tx_cb = tx_box.clone();
                let handler = ProfileAddBrowserExtensionCompletedHandler::create(Box::new(move |hr, ext| {
                    let res = if hr.is_ok()
                        && let Some(ext) = ext
                    {
                        let mut id_p = PWSTR::null();
                        let mut name_p = PWSTR::null();
                        let mut enabled = BOOL::from(true);
                        let _ = ext.Id(&mut id_p);
                        let _ = ext.Name(&mut name_p);
                        let _ = ext.IsEnabled(&mut enabled);
                        Ok(WebExtensionInfo {
                            id: take_pwstr(id_p),
                            name: take_pwstr(name_p),
                            enabled: enabled.as_bool(),
                            path: path_clone.clone(),
                        })
                    } else {
                        Err("확장 프로그램을 로드하지 못했습니다. WebView2 호환 manifest를 확인하세요.".into())
                    };
                    if let Some(tx) = tx_cb.lock().unwrap_or_else(|e| e.into_inner()).take() {
                        let _ = tx.send(res);
                    }
                    Ok(())
                }));
                profile7.AddBrowserExtension(&HSTRING::from(path_str), &handler)?;
                Ok(())
            })();
            if let Err(err) = result
                && let Some(tx) = tx_box.lock().unwrap_or_else(|e| e.into_inner()).take()
            {
                let _ = tx.send(Err(err.to_string()));
            }
        })
        .map_err(|e| {
            if let Some(tx) = tx_err.lock().unwrap_or_else(|e| e.into_inner()).take() {
                let _ = tx.send(Err(e.to_string()));
            }
            e.to_string()
        })?;
    rx.await
        .map_err(|_| "확장 설치 응답을 받지 못했습니다.".to_string())?
}

fn copy_dir_recursive(src: &Path, dst: &Path) -> Result<(), String> {
    fs::create_dir_all(dst).map_err(|e| format!("확장 폴더를 만들지 못했습니다: {e}"))?;
    for entry in fs::read_dir(src).map_err(|e| format!("확장 폴더를 읽지 못했습니다: {e}"))?
    {
        let entry = entry.map_err(|e| e.to_string())?;
        let ty = entry.file_type().map_err(|e| e.to_string())?;
        let dst_path = dst.join(entry.file_name());
        if ty.is_dir() {
            copy_dir_recursive(&entry.path(), &dst_path)?;
        } else {
            fs::copy(entry.path(), &dst_path)
                .map_err(|e| format!("확장 파일을 복사하지 못했습니다: {e}"))?;
        }
    }
    Ok(())
}

#[cfg(not(windows))]
pub async fn add_web_extension(
    _app: &AppHandle,
    _folder_path: &str,
) -> Result<WebExtensionInfo, String> {
    Err("Windows에서만 지원되는 기능입니다.".into())
}

#[cfg(windows)]
pub async fn toggle_or_remove_web_extension(
    app: &AppHandle,
    ext_id: &str,
    remove: bool,
    enabled: bool,
) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{ICoreWebView2_13, ICoreWebView2Profile7};
    use webview2_com::{
        BrowserExtensionEnableCompletedHandler, BrowserExtensionRemoveCompletedHandler,
        ProfileGetBrowserExtensionsCompletedHandler, take_pwstr,
    };
    use windows::core::{Interface, PWSTR};

    let webview = app
        .get_webview("shell")
        .ok_or_else(|| "WebView 없음".to_string())?;
    let target_id = ext_id.to_string();
    let (tx, rx) = tokio::sync::oneshot::channel();
    let tx_box = std::sync::Arc::new(std::sync::Mutex::new(Some(tx)));
    webview
        .with_webview(move |platform| unsafe {
            let result = (|| -> windows::core::Result<()> {
                let core13: ICoreWebView2_13 = platform.controller().CoreWebView2()?.cast()?;
                let profile7: ICoreWebView2Profile7 = core13.Profile()?.cast()?;
                let tx_list = tx_box.clone();
                let handler = ProfileGetBrowserExtensionsCompletedHandler::create(Box::new(
                    move |hr, list| {
                        let mut dispatched = false;
                        if hr.is_ok()
                            && let Some(list) = list
                        {
                            let mut count = 0_u32;
                            if list.Count(&mut count).is_ok() {
                                for i in 0..count {
                                    if let Ok(ext) = list.GetValueAtIndex(i) {
                                        let mut id_p = PWSTR::null();
                                        let _ = ext.Id(&mut id_p);
                                        if take_pwstr(id_p) == target_id {
                                            dispatched = true;
                                            if remove {
                                                let tx_inner = tx_list.clone();
                                                let rem_h =
                                                    BrowserExtensionRemoveCompletedHandler::create(
                                                        Box::new(move |r| {
                                                            if let Some(tx) = tx_inner
                                                                .lock()
                                                                .unwrap_or_else(|e| e.into_inner())
                                                                .take()
                                                            {
                                                                let _ =
                                                                    tx.send(r.map_err(|e| {
                                                                        e.to_string()
                                                                    }));
                                                            }
                                                            Ok(())
                                                        }),
                                                    );
                                                let _ = ext.Remove(&rem_h);
                                            } else {
                                                let tx_inner = tx_list.clone();
                                                let en_h =
                                                    BrowserExtensionEnableCompletedHandler::create(
                                                        Box::new(move |r| {
                                                            if let Some(tx) = tx_inner
                                                                .lock()
                                                                .unwrap_or_else(|e| e.into_inner())
                                                                .take()
                                                            {
                                                                let _ =
                                                                    tx.send(r.map_err(|e| {
                                                                        e.to_string()
                                                                    }));
                                                            }
                                                            Ok(())
                                                        }),
                                                    );
                                                let _ = ext.Enable(enabled, &en_h);
                                            }
                                            break;
                                        }
                                    }
                                }
                            }
                        }
                        if !dispatched
                            && let Some(tx) =
                                tx_list.lock().unwrap_or_else(|e| e.into_inner()).take()
                        {
                            let _ = tx.send(Err("확장을 찾지 못했습니다.".into()));
                        }
                        Ok(())
                    },
                ));
                profile7.GetBrowserExtensions(&handler)?;
                Ok(())
            })();
            if let Err(err) = result
                && let Some(tx) = tx_box.lock().unwrap_or_else(|e| e.into_inner()).take()
            {
                let _ = tx.send(Err(err.to_string()));
            }
        })
        .map_err(|e| e.to_string())?;
    rx.await
        .map_err(|_| "확장 제어 응답이 없습니다.".to_string())?
}

#[cfg(not(windows))]
pub async fn toggle_or_remove_web_extension(
    _app: &AppHandle,
    _ext_id: &str,
    _remove: bool,
    _enabled: bool,
) -> Result<(), String> {
    Ok(())
}

fn build_auth_extractor_js(provider: &str) -> String {
    let template = r##"(() => {
  if (window.__tbAuthPolling) return;
  window.__tbAuthPolling = true;
  const provider = "__PROVIDER__";
  const badEmail = (e) => {
    const s = String(e || "").trim().toLowerCase();
    if (!s || !s.includes("@") || s.length > 254) return true;
    return /(example\.com|contoso\.com|noreply@|no-reply@|support@|privacy@|security@|feedback@|abuse@|sentry\.io|@\d|\.(png|jpe?g|svg|webp|gif|js|css)$)/.test(s);
  };
  const pickEmail = (raw) => {
    if (!raw) return "";
    const m = String(raw).match(/[a-zA-Z0-9._%+\-]+@[a-zA-Z0-9.\-]+\.[a-zA-Z]{2,}/g);
    if (!m) return "";
    for (const item of m) {
      if (!badEmail(item)) return item;
    }
    return "";
  };
  const tryExtract = () => {
    if (window.__tbAuthSent) return true;
    const host = location.hostname.toLowerCase();
    let email = "";
    let name = "";
    if (provider === "google") {
      if (host !== "myaccount.google.com" || location.pathname.startsWith("/intro")) return false;
      const de = document.querySelector("[data-email]");
      if (de) email = pickEmail(de.getAttribute("data-email"));
      if (!email) {
        const btns = document.querySelectorAll('a[aria-label*="@"], button[aria-label*="@"]');
        for (const el of btns) {
          const aria = el.getAttribute("aria-label") || "";
          const found = pickEmail(aria);
          if (found) {
            email = found;
            const nm = aria
              .replace(/^Google\s*(계정|Account)\s*:\s*/i, "")
              .replace(found, "")
              .replace(/[()\n\r]+/g, " ")
              .trim();
            if (nm && nm.length <= 60) name = nm;
            break;
          }
        }
      }
      if (!email && window.WIZ_global_data && typeof window.WIZ_global_data.oPEP7c === "string") {
        email = pickEmail(window.WIZ_global_data.oPEP7c);
      }
      if (!email) {
        const bodyText = (document.body && document.body.innerText) || "";
        const m = bodyText.match(/(?:환영합니다,\s*|Welcome,\s*|ようこそ[、,\s]+|欢迎(?:使用)?[，,\s]+)([^\n,!。]+)[님!,\s]/);
        if (m && m[1]) name = m[1].trim().slice(0, 60);
        email = pickEmail(bodyText);
      }
      if (!name) {
        const h = document.querySelector("h1");
        const ht = ((h && h.textContent) || "").trim();
        const wm = ht.match(/(?:환영합니다,\s*|Welcome,\s*|ようこそ[、,\s]+|欢迎(?:使用)?[，,\s]+)([^\n,!。]+)/);
        if (wm && wm[1]) name = wm[1].replace(/님.*$/, "").trim().slice(0, 60);
      }
    } else if (provider === "microsoft") {
      const okHost =
        host === "account.microsoft.com" ||
        host === "account.live.com" ||
        host === "myaccount.microsoft.com" ||
        host === "portal.office.com" ||
        host === "m365.cloud.microsoft";
      if (!okHost) return false;
      const headerTxt = (document.querySelector("#mectrl_header_text")?.textContent || "").trim();
      const meSec = document.querySelector("#mectrl_currentAccount_secondary")?.textContent || "";
      const mePri = (document.querySelector("#mectrl_currentAccount_primary")?.textContent || "").trim();
      if (meSec) email = pickEmail(meSec);
      if (mePri && mePri.length <= 60) name = mePri;
      if (!email) {
        const els = document.querySelectorAll('[id*="mectrl"] [aria-label*="@"], button[aria-label*="@"], a[aria-label*="@"]');
        for (const el of els) {
          const aria = el.getAttribute("aria-label") || "";
          const found = pickEmail(aria);
          if (found) {
            email = found;
            if (!name) {
              const cleaned = aria
                .replace(/계정\s*관리자|Account\s*manager/gi, "")
                .replace(found, "")
                .replace(/[()\n\r]+/g, " ")
                .trim();
              if (cleaned && cleaned.length <= 60) name = cleaned;
            }
            break;
          }
        }
      }
      if (!email && !/^(로그인|Sign\s*in)$/i.test(headerTxt)) {
        const mainText = (document.querySelector("main")?.innerText || document.body?.innerText || "");
        email = pickEmail(mainText);
        if (email && !name) {
          const lines = mainText.split(/\r?\n/).map((s) => s.trim()).filter(Boolean);
          const idx = lines.findIndex((l) => l.includes(email));
          if (idx > 0 && lines[idx - 1].length <= 60 && !lines[idx - 1].includes("@")) {
            name = lines[idx - 1];
          }
        }
      }
      if (!email) {
        for (const sc of document.querySelectorAll("script")) {
          const t = sc.textContent || "";
          if (!t || t.length > 250000) continue;
          const m = t.match(/"(?:userPrincipalName|memberName|preferredUsername|mail|email)"\s*:\s*"([^"@]+@[^"]+)"/);
          if (m && m[1]) {
            const found = pickEmail(m[1]);
            if (found) {
              email = found;
              break;
            }
          }
        }
      }
    }
    if (!email) return false;
    window.__tbAuthSent = true;
    try {
      window.chrome?.webview?.postMessage(
        JSON.stringify({
          type: "tb-auth-done",
          provider,
          email,
          name: name || (provider === "google" ? "Google 사용자" : "Microsoft 사용자"),
        }),
      );
    } catch {}
    return true;
  };
  if (tryExtract()) return;
  const host = location.hostname.toLowerCase();
  const onTargetHost =
    (provider === "google" && host === "myaccount.google.com" && !location.pathname.startsWith("/intro")) ||
    (provider === "microsoft" &&
      (host === "account.microsoft.com" ||
        host === "account.live.com" ||
        host === "myaccount.microsoft.com" ||
        host === "portal.office.com" ||
        host === "m365.cloud.microsoft"));
  if (!onTargetHost) return;
  let attempts = 0;
  const timer = setInterval(() => {
    attempts += 1;
    if (tryExtract() || attempts >= 40) clearInterval(timer);
  }, 500);
})();"##;
    template.replace("__PROVIDER__", provider)
}

static AUTH_WIN_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

pub fn open_account_login_window(app: &AppHandle, provider: &str) -> Result<(), String> {
    let provider_norm = provider.trim().to_ascii_lowercase();
    let ui_language = {
        let state = app.state::<AppState>();
        lock(&state.settings).ui_language.clone()
    };
    let (google_hl, ms_lc, g_title, m_title) = match ui_language.as_str() {
        "en" => (
            "en",
            "1033",
            "Sign in with Google",
            "Sign in with Microsoft",
        ),
        "ja" => (
            "ja",
            "1041",
            "Google アカウントでログイン",
            "Microsoft アカウントでログイン",
        ),
        "zh-CN" => (
            "zh-CN",
            "2052",
            "使用 Google 账号登录",
            "使用 Microsoft 账号登录",
        ),
        _ => (
            "ko",
            "1042",
            "Google 계정으로 로그인",
            "Microsoft 계정으로 로그인",
        ),
    };
    let (start_url, title) = match provider_norm.as_str() {
        "google" => (
            format!(
                "https://accounts.google.com/ServiceLogin?continue=https%3A%2F%2Fmyaccount.google.com%2F&hl={google_hl}"
            ),
            g_title,
        ),
        "microsoft" => (
            format!(
                "https://login.live.com/login.srf?wreply=https%3A%2F%2Faccount.microsoft.com%2F&lc={ms_lc}"
            ),
            m_title,
        ),
        _ => return Err("지원하지 않는 계정 종류입니다.".into()),
    };

    for (label, win) in app.windows() {
        if label.starts_with("auth-login") {
            let _ = win.close();
        }
    }

    let seq = AUTH_WIN_SEQ.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let win_label = format!("auth-login-{seq}");
    let wv_label = format!("auth-login-wv-{seq}");

    let (webview_args, extensions_dir, webview_extensions_enabled, is_dark) = {
        let state = app.state::<AppState>();
        let tabs = lock(&state.tabs);
        (
            state.webview_args.clone(),
            state.extensions_dir.clone(),
            state.webview_extensions_enabled,
            tabs.ui_is_dark.unwrap_or(true),
        )
    };
    let bg_color = crate::browser::theme_bg_color(is_dark);
    let parsed_url = url::Url::parse(&start_url).map_err(|e| e.to_string())?;

    let mut win_builder = tauri::Window::builder(app, &win_label)
        .title(title)
        .decorations(true)
        .resizable(true)
        .background_color(bg_color)
        .inner_size(500.0, 680.0)
        .min_inner_size(400.0, 520.0);

    if let Some(main_win) = app.get_window("main") {
        if let (Ok(pos), Ok(size), Ok(scale)) = (
            main_win.outer_position(),
            main_win.outer_size(),
            main_win.scale_factor(),
        ) {
            let l_pos = pos.to_logical::<f64>(scale);
            let l_size = size.to_logical::<f64>(scale);
            let x = l_pos.x + (l_size.width - 500.0) / 2.0;
            let y = l_pos.y + (l_size.height - 680.0) / 2.0;
            win_builder = win_builder.position(x.max(0.0), y.max(0.0));
        }
    }

    let window = win_builder.build().map_err(|e| e.to_string())?;

    let resize_app = app.clone();
    let resize_win_label = win_label.clone();
    let resize_wv_label = wv_label.clone();
    window.on_window_event(move |event| {
        if let tauri::WindowEvent::Resized(size) = event
            && let Some(wv) = resize_app.get_webview(&resize_wv_label)
        {
            let scale = resize_app
                .get_window(&resize_win_label)
                .and_then(|w| w.scale_factor().ok())
                .unwrap_or(1.0);
            let logical = size.to_logical::<f64>(scale);
            let _ = wv.set_size(LogicalSize::new(
                logical.width.max(120.0),
                logical.height.max(120.0),
            ));
        }
    });

    let prov_for_load = provider_norm.clone();
    let mut wv_builder = WebviewBuilder::new(&wv_label, WebviewUrl::External(parsed_url))
        .devtools(false)
        .background_color(bg_color)
        .additional_browser_args(&webview_args)
        .browser_extensions_enabled(webview_extensions_enabled)
        .on_navigation(|url| matches!(url.scheme(), "http" | "https"))
        .on_page_load(move |wv, payload| {
            if matches!(payload.event(), PageLoadEvent::Finished) {
                let js = build_auth_extractor_js(&prov_for_load);
                let _ = wv.eval(&js);
            }
        });
    if webview_extensions_enabled && extensions_dir.exists() {
        wv_builder = wv_builder.extensions_path(&extensions_dir);
    }

    let _webview = window
        .add_child(
            wv_builder,
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(500.0, 680.0),
        )
        .map_err(|e| e.to_string())?;

    #[cfg(windows)]
    {
        use webview2_com::{WebMessageReceivedEventHandler, take_pwstr};
        use windows::core::PWSTR;

        let app_for_msg = app.clone();
        let default_prov = provider_norm;
        let close_win_label = win_label.clone();
        let _ = _webview.with_webview(move |platform| unsafe {
            let Ok(core) = platform.controller().CoreWebView2() else {
                return;
            };
            let msg_handler = WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                let Some(args) = args else {
                    return Ok(());
                };
                let mut raw = PWSTR::null();
                if args.TryGetWebMessageAsString(&mut raw).is_ok() && !raw.is_null() {
                    let msg_text = take_pwstr(raw);
                    if let Ok(v) = serde_json::from_str::<serde_json::Value>(&msg_text)
                        && v.get("type").and_then(|x| x.as_str()) == Some("tb-auth-done")
                    {
                        let email = v
                            .get("email")
                            .and_then(|x| x.as_str())
                            .unwrap_or("")
                            .trim()
                            .to_string();
                        if !email.is_empty() {
                            let prov = v
                                .get("provider")
                                .and_then(|x| x.as_str())
                                .unwrap_or(&default_prov)
                                .to_string();
                            let name = v
                                .get("name")
                                .and_then(|x| x.as_str())
                                .unwrap_or("")
                                .to_string();
                            let app_bg = app_for_msg.clone();
                            let win_to_close = close_win_label.clone();
                            tauri::async_runtime::spawn(async move {
                                let state = app_bg.state::<AppState>();
                                match connect_sync_account(
                                    &app_bg, &state, &prov, &email, &name, None,
                                ) {
                                    Ok(cfg) => {
                                        let _ = app_bg.emit("sync-config-updated", &cfg);
                                    }
                                    Err(err) => {
                                        let _ = app_bg.emit("sync-config-error", err);
                                    }
                                }
                                tokio::time::sleep(std::time::Duration::from_millis(40)).await;
                                let app_close = app_bg.clone();
                                let _ = app_bg.run_on_main_thread(move || {
                                    if let Some(win) = app_close.get_window(&win_to_close) {
                                        let _ = win.close();
                                    }
                                });
                            });
                        }
                    }
                }
                Ok(())
            }));
            let mut token = 0;
            let _ = core.add_WebMessageReceived(&msg_handler, &mut token);
        });
    }

    Ok(())
}

static CODEX_AUTH_SEQ: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(1);

fn persist_codex_bundle(
    app: &AppHandle,
    bundle: ai::CodexTokenBundle,
) -> Result<SettingsView, String> {
    let state = app.state::<AppState>();
    let next_secrets = {
        let mut secrets = lock(&state.secrets);
        secrets.codex_access_token = Some(bundle.access_token);
        if let Some(rt) = bundle.refresh_token.filter(|s| !s.trim().is_empty()) {
            secrets.codex_refresh_token = Some(rt);
        }
        if let Some(acc) = bundle.account_id.filter(|s| !s.trim().is_empty()) {
            secrets.codex_account_id = Some(acc);
        }
        if let Some(exp) = bundle.expires_at {
            secrets.codex_expires_at = Some(exp);
        }
        if let Some(email) = bundle.email.filter(|s| !s.trim().is_empty()) {
            secrets.codex_email = Some(email);
        }
        secrets.clone()
    };
    credentials::save(&state.data_dir, &next_secrets)?;
    Ok(crate::settings_view_pub(&state))
}

fn finish_codex_oauth_code(app: &AppHandle, code: String, verifier: String, win_to_close: String) {
    let app_bg = app.clone();
    tauri::async_runtime::spawn(async move {
        let client = app_bg.state::<AppState>().client.clone();
        match ai::exchange_codex_code(&client, &code, &verifier).await {
            Ok(bundle) => {
                if let Ok(view) = persist_codex_bundle(&app_bg, bundle) {
                    let _ = app_bg.emit("codex-auth-updated", view);
                }
            }
            Err(err) => {
                let _ = app_bg.emit("codex-auth-error", err);
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(50)).await;
        let app_close = app_bg.clone();
        let _ = app_bg.run_on_main_thread(move || {
            if let Some(win) = app_close.get_window(&win_to_close) {
                let _ = win.close();
            }
        });
    });
}

pub fn open_codex_oauth_window(app: &AppHandle) -> Result<(), String> {
    use std::io::{Read, Write};
    use std::sync::Arc;
    use std::sync::atomic::{AtomicBool, Ordering};

    for (label, win) in app.windows() {
        if label.starts_with("codex-auth") {
            let _ = win.close();
        }
    }

    let (verifier, challenge, expected_state) = ai::generate_pkce_pair();
    let parsed_url = ai::codex_authorize_url(&challenge, &expected_state)?;

    let seq = CODEX_AUTH_SEQ.fetch_add(1, Ordering::Relaxed);
    let win_label = format!("codex-auth-{seq}");
    let wv_label = format!("codex-auth-wv-{seq}");

    let (webview_args, extensions_dir, webview_extensions_enabled, is_dark) = {
        let state = app.state::<AppState>();
        let tabs = lock(&state.tabs);
        (
            state.webview_args.clone(),
            state.extensions_dir.clone(),
            state.webview_extensions_enabled,
            tabs.ui_is_dark.unwrap_or(true),
        )
    };
    let bg_color = crate::browser::theme_bg_color(is_dark);

    let mut win_builder = tauri::Window::builder(app, &win_label)
        .title("ChatGPT / 레거시 코덱스 로그인")
        .decorations(true)
        .resizable(true)
        .background_color(bg_color)
        .inner_size(520.0, 720.0)
        .min_inner_size(420.0, 540.0);

    if let Some(main_win) = app.get_window("main")
        && let (Ok(pos), Ok(size), Ok(scale)) = (
            main_win.outer_position(),
            main_win.outer_size(),
            main_win.scale_factor(),
        )
    {
        let l_pos = pos.to_logical::<f64>(scale);
        let l_size = size.to_logical::<f64>(scale);
        let x = l_pos.x + (l_size.width - 520.0) / 2.0;
        let y = l_pos.y + (l_size.height - 720.0) / 2.0;
        win_builder = win_builder.position(x.max(0.0), y.max(0.0));
    }

    let window = win_builder.build().map_err(|e| e.to_string())?;
    let handled = Arc::new(AtomicBool::new(false));

    let close_handled = handled.clone();
    let resize_app = app.clone();
    let resize_win_label = win_label.clone();
    let resize_wv_label = wv_label.clone();
    window.on_window_event(move |event| {
        if matches!(event, tauri::WindowEvent::Destroyed) {
            close_handled.store(true, Ordering::SeqCst);
        } else if let tauri::WindowEvent::Resized(size) = event
            && let Some(wv) = resize_app.get_webview(&resize_wv_label)
        {
            let scale = resize_app
                .get_window(&resize_win_label)
                .and_then(|w| w.scale_factor().ok())
                .unwrap_or(1.0);
            let logical = size.to_logical::<f64>(scale);
            let _ = wv.set_size(LogicalSize::new(
                logical.width.max(120.0),
                logical.height.max(120.0),
            ));
        }
    });

    // Optional loopback fallback on 127.0.0.1:1455 when the port is free.
    {
        let loop_handled = handled.clone();
        let loop_app = app.clone();
        let loop_verifier = verifier.clone();
        let loop_state = expected_state.clone();
        let loop_win = win_label.clone();
        std::thread::spawn(move || {
            let Ok(listener) = std::net::TcpListener::bind("127.0.0.1:1455") else {
                return;
            };
            let _ = listener.set_nonblocking(true);
            let deadline = std::time::Instant::now() + std::time::Duration::from_secs(180);
            while !loop_handled.load(Ordering::SeqCst) && std::time::Instant::now() < deadline {
                match listener.accept() {
                    Ok((mut stream, _)) => {
                        let mut buf = [0_u8; 4096];
                        let n = stream.read(&mut buf).unwrap_or(0);
                        let req_str = String::from_utf8_lossy(&buf[..n]);
                        let first_line = req_str.lines().next().unwrap_or("");
                        let path_part = first_line.split_whitespace().nth(1).unwrap_or("/");
                        if let Ok(full_url) =
                            url::Url::parse(&format!("http://localhost:1455{path_part}"))
                        {
                            let mut code = None;
                            let mut state_param = None;
                            for (k, v) in full_url.query_pairs() {
                                if k == "code" {
                                    code = Some(v.into_owned());
                                } else if k == "state" {
                                    state_param = Some(v.into_owned());
                                }
                            }
                            let html = "<!doctype html><html><meta charset='utf-8'><body style='font-family:system-ui;background:#161923;color:#f3f5fa;display:grid;place-items:center;height:100vh;margin:0'><div><h2>Codex 인증 완료</h2><p>이 창은 곧 자동으로 닫힙니다.</p></div></body></html>";
                            let resp = format!(
                                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                                html.len(),
                                html
                            );
                            let _ = stream.write_all(resp.as_bytes());
                            if let (Some(code), Some(st)) = (code, state_param)
                                && st == loop_state
                                && !loop_handled.swap(true, Ordering::SeqCst)
                            {
                                finish_codex_oauth_code(&loop_app, code, loop_verifier, loop_win);
                                break;
                            }
                        }
                    }
                    Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                        std::thread::sleep(std::time::Duration::from_millis(60));
                    }
                    Err(_) => break,
                }
            }
        });
    }

    let nav_handled = handled.clone();
    let nav_app = app.clone();
    let nav_verifier = verifier;
    let nav_state = expected_state;
    let nav_win = win_label;
    let mut wv_builder = WebviewBuilder::new(&wv_label, WebviewUrl::External(parsed_url))
        .devtools(false)
        .background_color(bg_color)
        .additional_browser_args(&webview_args)
        .browser_extensions_enabled(webview_extensions_enabled)
        .on_navigation(move |url| {
            let is_callback = url.scheme() == "http"
                && matches!(url.host_str(), Some("localhost") | Some("127.0.0.1"))
                && url.port() == Some(1455)
                && url.path().starts_with("/auth/callback");
            if is_callback {
                let mut code = None;
                let mut st = None;
                let mut err_desc = None;
                for (k, v) in url.query_pairs() {
                    if k == "code" {
                        code = Some(v.into_owned());
                    } else if k == "state" {
                        st = Some(v.into_owned());
                    } else if k == "error_description" || k == "error" {
                        err_desc = Some(v.into_owned());
                    }
                }
                if let (Some(c), Some(s)) = (code, st) {
                    if s == nav_state && !nav_handled.swap(true, Ordering::SeqCst) {
                        finish_codex_oauth_code(&nav_app, c, nav_verifier.clone(), nav_win.clone());
                    }
                    return false;
                }
                if let Some(msg) = err_desc {
                    if !nav_handled.swap(true, Ordering::SeqCst) {
                        let _ = nav_app.emit("codex-auth-error", msg);
                        let close_app = nav_app.clone();
                        let close_w = nav_win.clone();
                        let _ = nav_app.run_on_main_thread(move || {
                            if let Some(w) = close_app.get_window(&close_w) {
                                let _ = w.close();
                            }
                        });
                    }
                    return false;
                }
                return true;
            }
            matches!(url.scheme(), "http" | "https")
        });
    if webview_extensions_enabled && extensions_dir.exists() {
        wv_builder = wv_builder.extensions_path(&extensions_dir);
    }

    let _webview = window
        .add_child(
            wv_builder,
            LogicalPosition::new(0.0, 0.0),
            LogicalSize::new(520.0, 720.0),
        )
        .map_err(|e| e.to_string())?;

    Ok(())
}

fn parse_local_codex_auth_json(raw: &str) -> Option<ai::CodexTokenBundle> {
    let v: serde_json::Value = serde_json::from_str(raw).ok()?;
    let candidates: Vec<&serde_json::Value> = [
        v.get("tokens"),
        v.get("openai"),
        v.get("codex"),
        v.get("openai-codex"),
        Some(&v),
    ]
    .into_iter()
    .flatten()
    .collect();

    for node in candidates {
        let access = node
            .get("access_token")
            .or_else(|| node.get("access"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty());
        let Some(access_token) = access else {
            continue;
        };
        let refresh_token = node
            .get("refresh_token")
            .or_else(|| node.get("refresh"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let raw_acc = node
            .get("account_id")
            .or_else(|| node.get("accountId"))
            .and_then(serde_json::Value::as_str)
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(str::to_string);
        let id_token = node
            .get("id_token")
            .and_then(serde_json::Value::as_str)
            .map(str::trim);
        let expires_raw = node
            .get("expires")
            .or_else(|| node.get("expires_at"))
            .and_then(serde_json::Value::as_i64)
            .map(|ts| {
                if ts > 1_000_000_000_000 {
                    ts / 1000
                } else {
                    ts
                }
            });
        let (acc_access, email_access) = ai::extract_codex_claims(access_token);
        let (acc_id, email_id) = id_token
            .map(ai::extract_codex_claims)
            .unwrap_or((None, None));
        let account_id = raw_acc.or(acc_access).or(acc_id);
        let email = email_id.or(email_access);
        return Some(ai::CodexTokenBundle {
            access_token: access_token.to_string(),
            refresh_token,
            account_id,
            expires_at: expires_raw,
            email,
        });
    }
    None
}

pub fn import_local_codex_auth(app: &AppHandle) -> Result<(SettingsView, String), String> {
    let mut paths: Vec<(PathBuf, &str)> = Vec::new();
    if let Ok(profile) = std::env::var("USERPROFILE") {
        let p = PathBuf::from(&profile);
        paths.push((
            p.join(".local")
                .join("share")
                .join("opencode")
                .join("auth.json"),
            "OpenCode auth.json",
        ));
        paths.push((
            p.join(".codex").join("auth.json"),
            "Codex CLI (.codex/auth.json)",
        ));
    }
    if let Ok(appdata) = std::env::var("APPDATA") {
        paths.push((
            PathBuf::from(appdata).join("opencode").join("auth.json"),
            "OpenCode AppData auth.json",
        ));
    }
    if let Ok(local) = std::env::var("LOCALAPPDATA") {
        paths.push((
            PathBuf::from(local).join("opencode").join("auth.json"),
            "OpenCode LocalAppData auth.json",
        ));
    }
    for (path, label) in paths {
        if let Ok(text) = fs::read_to_string(&path)
            && let Some(bundle) = parse_local_codex_auth_json(&text)
        {
            let view = persist_codex_bundle(app, bundle)?;
            return Ok((view, label.to_string()));
        }
    }
    Err("PC에서 OpenCode 또는 Codex CLI 로그인 세션(auth.json)을 찾지 못했습니다. 위 ‘레거시 코덱스 로그인’ 버튼으로 직접 로그인하세요.".into())
}

pub fn clear_codex_auth(app: &AppHandle) -> Result<SettingsView, String> {
    let state = app.state::<AppState>();
    let next_secrets = {
        let mut secrets = lock(&state.secrets);
        secrets.codex_access_token = None;
        secrets.codex_refresh_token = None;
        secrets.codex_account_id = None;
        secrets.codex_expires_at = None;
        secrets.codex_email = None;
        secrets.clone()
    };
    credentials::save(&state.data_dir, &next_secrets)?;
    Ok(crate::settings_view_pub(&state))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::browser::Tabs;

    fn temp_sync_dir(tag: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!(
            "tb-sync-test-{tag}-{}-{}",
            std::process::id(),
            db::now_secs()
        ));
        let _ = fs::remove_dir_all(&dir);
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn sanitize_account_keeps_safe_filename_characters() {
        assert_eq!(
            sanitize_account("User.Name+1@Example.COM"),
            "user.name_1@example.com"
        );
        assert_eq!(sanitize_account("bad/\\:name?"), "bad___name_");
    }

    #[test]
    fn sync_merges_two_devices_without_resurrecting_local_deletions() {
        let sync_root = temp_sync_dir("merge");
        let db_a = db::open(Path::new(":memory:")).unwrap();
        let db_b = db::open(Path::new(":memory:")).unwrap();

        let mut settings_a = AppSettings::default();
        settings_a.ui_language = "en".into();
        settings_a.theme = "dark".into();
        settings_a.hy_mt2_gpu_layers = 24;

        let mut settings_b = AppSettings::default();
        settings_b.ui_language = "ko".into();
        settings_b.hy_mt2_gpu_layers = 0; // Device-local hardware setting must remain untouched.

        let mut tabs_a = Tabs::default();
        tabs_a.workspaces.push(crate::model::Workspace {
            id: "ws-remote".into(),
            name: "Game Wikis".into(),
        });
        let mut tabs_b = Tabs::default();

        db::save_bookmark(
            &db_a,
            &Bookmark {
                id: "bm-1".into(),
                url: "https://example.com/1".into(),
                title: "Page 1".into(),
                folder: "Wiki".into(),
                favorite: true,
                created_at: 10,
                updated_at: 10,
            },
        )
        .unwrap();
        db::set_setting(
            &db_a,
            "autofill_profile",
            r#"{"name":"Alice","email":"alice@example.org","phone":"","address":"","postalCode":""}"#,
        )
        .unwrap();

        let mut cfg_a = SyncConfig {
            provider: "local".into(),
            signed_in: true,
            display_name: "Alice".into(),
            email: "alice-profile".into(),
            sync_dir: sync_root.to_string_lossy().into_owned(),
            ..SyncConfig::default()
        };
        let mut cfg_b = cfg_a.clone();

        // Device A pushes snapshot at t=100.
        run_sync_core(
            &db_a,
            &mut settings_a,
            &mut tabs_a,
            &mut cfg_a,
            "device-a",
            100,
        )
        .unwrap();
        assert_eq!(cfg_a.last_synced_at, Some(100));

        // Device B pulls snapshot from Device A at t=105.
        let eff_b = run_sync_core(
            &db_b,
            &mut settings_b,
            &mut tabs_b,
            &mut cfg_b,
            "device-b",
            105,
        )
        .unwrap();
        assert!(eff_b.workspaces_changed);
        assert!(eff_b.settings_updated.is_some());
        assert_eq!(settings_b.ui_language, "en");
        assert_eq!(settings_b.theme, "dark");
        assert_eq!(settings_b.hy_mt2_gpu_layers, 0);
        assert!(tabs_b.workspaces.iter().any(|w| w.id == "ws-remote"));
        assert_eq!(db::list_bookmarks(&db_b).unwrap().len(), 1);
        let merged_profile: AutofillProfile =
            serde_json::from_str(&db::get_setting(&db_b, "autofill_profile").unwrap()).unwrap();
        assert_eq!(merged_profile.name, "Alice");

        // Now Device A deletes bm-1 locally and syncs again without another device pushing;
        // bm-1 must NOT be resurrected from the stale sync file written by Device B's pull.
        // Note: simulate same-device next sync when file timestamp is not newer or same device.
        run_sync_core(
            &db_a,
            &mut settings_a,
            &mut tabs_a,
            &mut cfg_a,
            "device-a",
            106,
        )
        .unwrap();
        db::delete_bookmark(&db_a, "bm-1").unwrap();
        run_sync_core(
            &db_a,
            &mut settings_a,
            &mut tabs_a,
            &mut cfg_a,
            "device-a",
            110,
        )
        .unwrap();
        assert!(db::list_bookmarks(&db_a).unwrap().is_empty());

        let _ = fs::remove_dir_all(&sync_root);
    }
}
