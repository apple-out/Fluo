use std::sync::atomic::Ordering;
use std::time::Duration;

use tauri::webview::{
    DownloadEvent, NewWindowResponse, PageLoadEvent, PermissionResponse, WebviewBuilder,
};
use tauri::{AppHandle, Emitter, LogicalPosition, LogicalSize, Manager, Webview, WebviewUrl};

#[cfg(windows)]
use webview2_com::{
    AcceleratorKeyPressedEventHandler,
    Microsoft::Web::WebView2::Win32::COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL,
    WebMessageReceivedEventHandler, WebResourceRequestedEventHandler, take_pwstr,
};
#[cfg(windows)]
use windows::Win32::UI::Input::KeyboardAndMouse::{GetKeyState, VK_CONTROL, VK_MENU, VK_SHIFT};
#[cfg(windows)]
use windows::core::{HSTRING, PWSTR};

use crate::model::{Article, Block, BrowserState, FindResult, TabState, Workspace};
use crate::text;
use crate::{AppState, lock};

pub const DEFAULT_TOP: f64 = 108.0;
pub const DEFAULT_URL: &str = "tb://start";
pub const EXTRACT_SCRIPT: &str = include_str!("content_extract.js");
const INLINE_TRANSLATE_SCRIPT: &str = include_str!("inline_translate.js");
const INLINE_RESTORE_SCRIPT: &str = include_str!("inline_restore.js");
const FIND_IN_PAGE_SCRIPT: &str = include_str!("find_in_page.js");
const REGION_PICKER_SCRIPT: &str = include_str!("region_picker.js");
const SELECTION_HELPER_SCRIPT: &str = include_str!("selection_helper.js");

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct InlineTranslation {
    pub key: String,
    pub translation: String,
    #[serde(default)]
    pub uncertain: bool,
    #[serde(default)]
    pub notes: Vec<String>,
}

#[derive(Clone)]
pub struct HistoryEntry {
    pub url: String,
    pub scroll_x: f64,
    pub scroll_y: f64,
}

impl HistoryEntry {
    fn new(url: &str) -> Self {
        Self {
            url: url.to_string(),
            scroll_x: 0.0,
            scroll_y: 0.0,
        }
    }
}

#[derive(Clone)]
pub struct Tab {
    pub id: String,
    pub label: String,
    pub url: String,
    pub title: String,
    pub history: Vec<HistoryEntry>,
    pub cursor: usize,
    pub loading: bool,
    pub workspace_id: String,
    pub scroll_x: f64,
    pub scroll_y: f64,
    pub zoom: f64,
    pub pinned: bool,
    pub is_private: bool,
    pub sleeping: bool,
    pub muted: bool,
    pub last_active_at: i64,
}

impl Tab {
    fn new(id: usize, url: &str) -> Self {
        Self {
            id: id.to_string(),
            label: format!("content-{id}"),
            url: url.to_string(),
            title: String::new(),
            history: vec![HistoryEntry::new(url)],
            cursor: 0,
            loading: true,
            workspace_id: "default".into(),
            scroll_x: 0.0,
            scroll_y: 0.0,
            zoom: 1.0,
            pinned: false,
            is_private: false,
            sleeping: false,
            muted: false,
            last_active_at: crate::db::now_secs(),
        }
    }

    fn state(&self) -> TabState {
        TabState {
            id: self.id.clone(),
            url: self.url.clone(),
            title: self.title.clone(),
            loading: self.loading,
            workspace_id: self.workspace_id.clone(),
            zoom: self.zoom,
            pinned: self.pinned,
            is_private: self.is_private,
            sleeping: self.sleeping,
            muted: self.muted,
        }
    }
}

pub struct Tabs {
    pub items: Vec<Tab>,
    pub active: Option<String>,
    pub workspaces: Vec<Workspace>,
    pub active_workspace: String,
    pub top: f64,
    pub overlay_open: bool,
    pub overlay_blur: bool,
    pub suggestions_open: bool,
    pub ui_is_dark: Option<bool>,
    pub ui_accent: String,
    pub ui_tinted: bool,
    pub retired_labels: Vec<String>,
    next_id: usize,
    closed: ClosedTabs,
}

const MAX_CLOSED_TABS: usize = 20;
const MIN_ZOOM: f64 = 0.25;
const MAX_ZOOM: f64 = 5.0;

/// Reopenable entries, newest last. Bounds memory for very long sessions.
#[derive(Debug, Default, Clone)]
pub struct ClosedTabs {
    entries: Vec<ClosedTab>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClosedTab {
    pub url: String,
    pub title: String,
}

impl ClosedTabs {
    fn push(&mut self, url: String, title: String) {
        if url.trim().is_empty() {
            return;
        }
        self.entries.push(ClosedTab { url, title });
        if self.entries.len() > MAX_CLOSED_TABS {
            self.entries.remove(0);
        }
    }

    fn pop(&mut self) -> Option<ClosedTab> {
        self.entries.pop()
    }

    #[cfg(test)]
    fn len(&self) -> usize {
        self.entries.len()
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WorkspaceSnapshot {
    pub workspaces: Vec<Workspace>,
    pub active_workspace: String,
}

impl Default for Tabs {
    fn default() -> Self {
        Self {
            items: Vec::new(),
            active: None,
            workspaces: vec![Workspace {
                id: "default".into(),
                name: "기본 작업공간".into(),
            }],
            active_workspace: "default".into(),
            top: DEFAULT_TOP,
            overlay_open: false,
            overlay_blur: false,
            suggestions_open: false,
            ui_is_dark: None,
            ui_accent: "#684cff".into(),
            ui_tinted: false,
            retired_labels: Vec::new(),
            next_id: 1,
            closed: ClosedTabs::default(),
        }
    }
}

/// Zoom bounds accepted from the UI and keyboard shortcuts.
fn clamp_zoom(factor: f64) -> f64 {
    if factor.is_finite() {
        factor.clamp(MIN_ZOOM, MAX_ZOOM)
    } else {
        1.0
    }
}

impl Tabs {
    /// Creates a tab in the active workspace and registers it exactly once.
    /// WebView creation happens after this so load callbacks find the tab.
    fn register_tab(&mut self, url: &str) -> Tab {
        let id = self.next_id;
        self.next_id += 1;
        let mut tab = Tab::new(id, url);
        tab.workspace_id = self.active_workspace.clone();
        self.items.push(tab.clone());
        tab
    }

    fn alloc_webview_label(&mut self) -> String {
        let id = self.next_id;
        self.next_id += 1;
        format!("content-{id}")
    }

    pub fn snapshot(&self) -> BrowserState {
        let active_tab = self
            .active
            .as_ref()
            .and_then(|id| self.items.iter().find(|tab| &tab.id == id));
        BrowserState {
            tabs: self.items.iter().map(Tab::state).collect(),
            active_tab: self.active.clone(),
            can_go_back: active_tab.map(|tab| tab.cursor > 0).unwrap_or(false),
            can_go_forward: active_tab
                .map(|tab| tab.cursor + 1 < tab.history.len())
                .unwrap_or(false),
            workspaces: self.workspaces.clone(),
            active_workspace: self.active_workspace.clone(),
        }
    }

    fn active_tab(&self) -> Option<&Tab> {
        let id = self.active.as_ref()?;
        self.items.iter().find(|tab| &tab.id == id)
    }

    pub fn active_url(&self) -> Option<String> {
        self.active_tab().map(|tab| tab.url.clone())
    }
}

#[derive(serde::Serialize, serde::Deserialize)]
struct SessionTab {
    url: String,
    #[serde(default)]
    active: bool,
    #[serde(default = "default_workspace")]
    workspace_id: String,
    #[serde(default)]
    scroll_x: f64,
    #[serde(default)]
    scroll_y: f64,
    #[serde(default = "default_zoom")]
    zoom: f64,
    #[serde(default)]
    pinned: bool,
}

fn default_zoom() -> f64 {
    1.0
}

fn default_workspace() -> String {
    "default".into()
}

/// Persist the open tabs so the next launch can restore them (MVP item 4).
fn persist_session(app: &AppHandle, tabs: &Tabs) {
    let entries: Vec<SessionTab> = tabs
        .items
        .iter()
        .filter(|tab| !tab.is_private)
        .map(|tab| SessionTab {
            url: tab.url.clone(),
            active: tabs.active.as_deref() == Some(tab.id.as_str()),
            workspace_id: tab.workspace_id.clone(),
            scroll_x: tab.scroll_x,
            scroll_y: tab.scroll_y,
            zoom: tab.zoom,
            pinned: tab.pinned,
        })
        .collect();
    let Ok(json) = serde_json::to_string(&entries) else {
        return;
    };
    let state = app.state::<AppState>();
    let workspace_json = serde_json::to_string(&WorkspaceSnapshot {
        workspaces: tabs.workspaces.clone(),
        active_workspace: tabs.active_workspace.clone(),
    });
    let db = lock(&state.db);
    let _ = crate::db::save_session(&db, &json);
    if let Ok(workspace_json) = workspace_json {
        let _ = crate::db::set_setting(&db, "workspaces", &workspace_json);
    }
}

pub fn load_workspaces(tabs: &mut Tabs, raw: Option<&str>) {
    let Some(raw) = raw else {
        return;
    };
    let Ok(saved) = serde_json::from_str::<WorkspaceSnapshot>(raw) else {
        return;
    };
    let mut ids = std::collections::HashSet::new();
    if saved.workspaces.is_empty()
        || !saved
            .workspaces
            .iter()
            .any(|workspace| workspace.id == "default")
        || saved.workspaces.iter().any(|workspace| {
            workspace.id.trim().is_empty()
                || workspace.name.trim().is_empty()
                || !ids.insert(workspace.id.as_str())
        })
        || !saved
            .workspaces
            .iter()
            .any(|w| w.id == saved.active_workspace)
    {
        return;
    }
    tabs.workspaces = saved.workspaces;
    tabs.active_workspace = saved.active_workspace;
}

pub fn restore_session(app: &AppHandle, raw: Option<&str>) -> bool {
    let Some(raw) = raw else {
        return false;
    };
    let Ok(entries) = serde_json::from_str::<Vec<SessionTab>>(raw) else {
        return false;
    };
    let mut opened: Vec<(bool, String)> = Vec::new();
    for entry in entries.iter().take(16) {
        let valid = entry.url.starts_with("tb://")
            || url::Url::parse(&entry.url)
                .map(|url| matches!(url.scheme(), "http" | "https"))
                .unwrap_or(false);
        if !valid {
            continue;
        }
        if let Ok(state) = open_tab(app, &entry.url) {
            if let Some(id) = state.active_tab {
                let restore = {
                    let state = app.state::<AppState>();
                    let mut tabs = lock(&state.tabs);
                    let valid_workspace = tabs
                        .workspaces
                        .iter()
                        .any(|workspace| workspace.id == entry.workspace_id);
                    tabs.items.iter_mut().find(|tab| tab.id == id).map(|tab| {
                        tab.workspace_id = if valid_workspace {
                            entry.workspace_id.clone()
                        } else {
                            "default".into()
                        };
                        tab.scroll_x = if entry.scroll_x.is_finite() {
                            entry.scroll_x.max(0.0)
                        } else {
                            0.0
                        };
                        tab.scroll_y = if entry.scroll_y.is_finite() {
                            entry.scroll_y.max(0.0)
                        } else {
                            0.0
                        };
                        tab.zoom = clamp_zoom(entry.zoom);
                        tab.pinned = entry.pinned;
                        (tab.label.clone(), tab.scroll_x, tab.scroll_y, tab.zoom)
                    })
                };
                if let Some((label, x, y, zoom)) = restore
                    && let Some(webview) = app.get_webview(&label)
                {
                    schedule_scroll_restore(&webview, x, y);
                    let _ = apply_zoom(&webview, zoom);
                }
                opened.push((entry.active, id));
            }
        }
    }
    if opened.is_empty() {
        return false;
    }
    let selected = opened
        .iter()
        .find(|(active, _)| *active)
        .or_else(|| opened.last());
    if let Some((_, id)) = selected {
        let _ = select_tab(app, id);
    }
    true
}

fn schedule_scroll_restore(webview: &Webview, x: f64, y: f64) {
    let script = format!(
        "(()=>{{const restore=()=>window.scrollTo({{left:{},top:{},behavior:'auto'}});if(document.readyState==='complete')requestAnimationFrame(()=>requestAnimationFrame(restore));else window.addEventListener('load',()=>requestAnimationFrame(restore),{{once:true}});}})()",
        x.max(0.0),
        y.max(0.0)
    );
    let _ = webview.eval(&script);
}

fn window(app: &AppHandle) -> Result<tauri::Window, String> {
    app.get_window("main")
        .ok_or_else(|| "main window is missing".to_string())
}

fn content_size(app: &AppHandle, _top: f64) -> Result<(f64, f64), String> {
    let window = window(app)?;
    let size = window
        .inner_size()
        .map_err(|error| format!("창 크기를 읽지 못했습니다: {error}"))?;
    let scale = window
        .scale_factor()
        .map_err(|error| format!("화면 배율을 읽지 못했습니다: {error}"))?;
    let width = size.width as f64 / scale;
    let height = size.height as f64 / scale;
    Ok((width.max(1.0), height.max(1.0)))
}

pub fn theme_bg_color(is_dark: bool) -> tauri::webview::Color {
    if is_dark {
        tauri::webview::Color(22, 25, 35, 255)
    } else {
        tauri::webview::Color(244, 246, 251, 255)
    }
}

fn effective_is_dark(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    let ui_dark = lock(&state.tabs).ui_is_dark;
    if let Some(d) = ui_dark {
        return d;
    }
    let theme = lock(&state.settings).theme.clone();
    match theme.as_str() {
        "dark" => true,
        "light" | "sepia" => false,
        _ => app
            .get_window("main")
            .and_then(|w| w.theme().ok())
            .is_some_and(|t| matches!(t, tauri::Theme::Dark)),
    }
}

fn create_webview(app: &AppHandle, top: f64, tab: &Tab) -> Result<(), String> {
    let window = window(app)?;
    let (width, height) = content_size(app, top)?;
    let position = LogicalPosition::new(0.0, 0.0);
    let size = LogicalSize::new(width, height);
    let state = app.state::<AppState>();
    let webview_args = state.webview_args.clone();
    let extensions_enabled = state.webview_extensions_enabled;
    let is_dark = effective_is_dark(app);
    let bg_color = theme_bg_color(is_dark);
    let bg_css = if is_dark { "#161923" } else { "#f4f6fb" };

    let offscreen = LogicalPosition::new(-(width + 200.0), -(height + 200.0));
    let prev_active_label = {
        let tabs = lock(&state.tabs);
        tabs.active_tab()
            .filter(|t| t.id != tab.id && !t.sleeping)
            .map(|t| t.label.clone())
            .or_else(|| tabs.retired_labels.last().cloned())
    };

    if let Some(kind) = tab.url.strip_prefix("tb://") {
        let init_script = format!(
            "window.__APP_PAGE_KIND__ = {}; try {{ if (document.documentElement) document.documentElement.style.background = '{}'; }} catch {{}}",
            serde_json::to_string(kind).map_err(|error| error.to_string())?,
            bg_css
        );
        let builder = WebviewBuilder::new(tab.label.clone(), WebviewUrl::App("/".into()))
            .focused(false)
            .background_color(bg_color)
            .devtools(false)
            .additional_browser_args(&webview_args)
            .browser_extensions_enabled(extensions_enabled)
            .initialization_script(init_script);
        let webview = window
            .add_child(builder, offscreen, size)
            .map_err(|error| format!("내부 페이지 WebView를 만들지 못했습니다: {error}"))?;
        if let Some(prev_label) = prev_active_label.as_deref() {
            send_webview_to_bottom(&webview);
            let _ = webview.set_position(position);
            if let Some(prev_wv) = app.get_webview(prev_label) {
                bring_webview_to_front(&prev_wv);
            }
            if let Some(shell) = app.get_webview("shell") {
                bring_webview_to_front(&shell);
            }
        } else {
            let _ = webview.hide();
            let _ = webview.set_position(position);
        }
        #[cfg(windows)]
        install_browser_shortcuts(&webview, app.clone())?;
        return Ok(());
    }

    let url =
        url::Url::parse(&tab.url).map_err(|error| format!("URL이 올바르지 않습니다: {error}"))?;

    let app_for_title = app.clone();
    let tab_for_title = tab.id.clone();
    let app_for_load = app.clone();
    let tab_for_load = tab.id.clone();
    let app_for_popup = app.clone();
    let tab_for_popup = tab.id.clone();
    let app_for_download = app.clone();

    let builder = WebviewBuilder::new(tab.label.clone(), WebviewUrl::External(url))
        .focused(false)
        .incognito(tab.is_private)
        .background_color(bg_color)
        .devtools(true)
        .additional_browser_args(&webview_args)
        .browser_extensions_enabled(extensions_enabled)
        .initialization_script(SELECTION_HELPER_SCRIPT)
        .on_navigation(|url| matches!(url.scheme(), "http" | "https" | "about" | "blob" | "data"))
        .on_new_window(move |url, features| {
            if matches!(url.scheme(), "http" | "https") {
                let is_popup_window = features.size().is_some() || features.position().is_some();
                let source_origin = {
                    let state = app_for_popup.state::<AppState>();
                    let tabs = lock(&state.tabs);
                    tabs.items
                        .iter()
                        .find(|t| t.id == tab_for_popup)
                        .and_then(|t| url::Url::parse(&t.url).ok())
                        .and_then(|u| u.host_str().map(|s| s.to_ascii_lowercase()))
                        .unwrap_or_default()
                };
                let target_host = url
                    .host_str()
                    .map(|s| s.to_ascii_lowercase())
                    .unwrap_or_default();
                let same_host = !source_origin.is_empty() && source_origin == target_host;
                let (policy, allowlist) = {
                    let state = app_for_popup.state::<AppState>();
                    let s = lock(&state.settings);
                    (s.popup_policy.clone(), s.popup_allowed_origins.clone())
                };
                let is_allowed = same_host
                    || allowlist.iter().any(|entry| {
                        let e = entry.trim().to_ascii_lowercase();
                        !e.is_empty()
                            && (source_origin == e
                                || source_origin.ends_with(&format!(".{e}"))
                                || target_host == e
                                || target_host.ends_with(&format!(".{e}")))
                    });
                let should_block = match policy.as_str() {
                    "allow" => false,
                    "block-all" => !is_allowed,
                    _ => is_popup_window && !is_allowed,
                };
                if should_block {
                    let origin = if source_origin.is_empty() {
                        target_host
                    } else {
                        source_origin
                    };
                    let _ = app_for_popup.emit_to(
                        "shell",
                        "popup-blocked",
                        PopupBlockedNotice {
                            url: url.to_string(),
                            origin,
                        },
                    );
                } else {
                    let open_app = app_for_popup.clone();
                    let open_url = url.to_string();
                    tauri::async_runtime::spawn(async move {
                        let _ = open_tab(&open_app, &open_url);
                    });
                }
            }
            NewWindowResponse::Deny
        })
        .on_download(move |_, event| {
            let notice = match event {
                DownloadEvent::Requested {
                    url, destination, ..
                } => {
                    let file_name = destination
                        .file_name()
                        .and_then(|name| name.to_str())
                        .unwrap_or("다운로드")
                        .to_string();
                    let file_path = destination.to_string_lossy().to_string();
                    let state = app_for_download.state::<AppState>();
                    let db = lock(&state.db);
                    let _ = crate::db::record_download_started(
                        &db,
                        url.as_str(),
                        &file_name,
                        &file_path,
                    );
                    DownloadNotice {
                        state: "started",
                        file_name,
                        success: true,
                    }
                }
                DownloadEvent::Finished {
                    url, path, success, ..
                } => {
                    let file_name = path
                        .as_deref()
                        .and_then(std::path::Path::file_name)
                        .and_then(|name| name.to_str())
                        .unwrap_or("다운로드")
                        .to_string();
                    let file_path = path
                        .as_deref()
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();
                    let state = app_for_download.state::<AppState>();
                    let db = lock(&state.db);
                    let _ = crate::db::finish_download(
                        &db,
                        url.as_str(),
                        &file_name,
                        &file_path,
                        success,
                    );
                    DownloadNotice {
                        state: "finished",
                        file_name,
                        success,
                    }
                }
                _ => return true,
            };
            let _ = app_for_download.emit_to("shell", "download-status", notice);
            let _ = app_for_download.emit_to("overlay", "download-status", ());
            true
        })
        .on_permission_request(|_, _| PermissionResponse::Allow)
        .on_document_title_changed(move |_, title| {
            update_title(&app_for_title, &tab_for_title, &title);
        })
        .on_page_load(move |_, payload| {
            update_page_load(
                &app_for_load,
                &tab_for_load,
                payload.url().as_str(),
                payload.event(),
            );
        });

    let webview = window
        .add_child(builder, offscreen, size)
        .map_err(|error| format!("브라우저 WebView를 만들지 못했습니다: {error}"))?;
    if let Some(prev_label) = prev_active_label.as_deref() {
        send_webview_to_bottom(&webview);
        let _ = webview.set_position(position);
        if let Some(prev_wv) = app.get_webview(prev_label) {
            bring_webview_to_front(&prev_wv);
        }
        if let Some(shell) = app.get_webview("shell") {
            bring_webview_to_front(&shell);
        }
    } else {
        let _ = webview.hide();
        let _ = webview.set_position(position);
    }
    #[cfg(windows)]
    install_adblock_filter(&webview, app.clone(), tab.id.clone())?;
    #[cfg(windows)]
    install_browser_shortcuts(&webview, app.clone())?;
    Ok(())
}

fn reset_overlay_z_order(app: &AppHandle) {
    let _ = sync_shell_bounds(app);
}

#[cfg(windows)]
fn send_webview_to_bottom(webview: &Webview) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GA_ROOT, GetAncestor, GetParent, HWND_BOTTOM, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SetWindowPos,
    };
    let _ = webview.with_webview(|platform| unsafe {
        let mut current = HWND::default();
        if platform.controller().ParentWindow(&mut current).is_ok() && !current.0.is_null() {
            let root = GetAncestor(current, GA_ROOT);
            while !current.0.is_null() && current != root {
                let _ = SetWindowPos(
                    current,
                    Some(HWND_BOTTOM),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
                let Ok(next) = GetParent(current) else {
                    break;
                };
                if next.0.is_null() || next == current || next == root {
                    break;
                }
                current = next;
            }
        }
    });
}

#[cfg(not(windows))]
fn send_webview_to_bottom(_webview: &Webview) {}

#[cfg(windows)]
fn bring_webview_to_front(webview: &Webview) {
    use windows::Win32::Foundation::HWND;
    use windows::Win32::UI::WindowsAndMessaging::{
        GA_ROOT, GetAncestor, GetParent, HWND_TOP, SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE,
        SetWindowPos,
    };
    let _ = webview.with_webview(|platform| unsafe {
        let mut current = HWND::default();
        if platform.controller().ParentWindow(&mut current).is_ok() && !current.0.is_null() {
            let root = GetAncestor(current, GA_ROOT);
            while !current.0.is_null() && current != root {
                let _ = SetWindowPos(
                    current,
                    Some(HWND_TOP),
                    0,
                    0,
                    0,
                    0,
                    SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
                );
                let Ok(next) = GetParent(current) else {
                    break;
                };
                if next.0.is_null() || next == current || next == root {
                    break;
                }
                current = next;
            }
        }
    });
}

#[cfg(not(windows))]
fn bring_webview_to_front(_webview: &Webview) {}

pub fn set_shell_expanded(app: &AppHandle, expanded: bool) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        if tabs.suggestions_open == expanded {
            return Ok(());
        }
        tabs.suggestions_open = expanded;
    }
    sync_shell_bounds(app)
}

pub fn sync_shell_bounds(app: &AppHandle) -> Result<(), String> {
    let (top, expanded, should_blur, active_label) = {
        let state = app.state::<AppState>();
        let tabs = lock(&state.tabs);
        (
            tabs.top,
            tabs.overlay_open || tabs.suggestions_open,
            (tabs.overlay_open && tabs.overlay_blur) || tabs.suggestions_open,
            tabs.active_tab().map(|tab| tab.label.clone()),
        )
    };
    let window = window(app)?;
    let size = window
        .inner_size()
        .map_err(|error| format!("창 크기를 읽지 못했습니다: {error}"))?;
    let scale = window
        .scale_factor()
        .map_err(|error| format!("화면 배율을 읽지 못했습니다: {error}"))?;
    let width = size.width as f64 / scale;
    let height = size.height as f64 / scale;
    let shell_height = if expanded {
        height.max(1.0)
    } else {
        top.clamp(38.0, 360.0)
    };
    if let Some(label) = active_label
        && let Some(webview) = app.get_webview(&label)
    {
        let _ = webview.eval(format!(
            "window.__tbSetPopupBlur?.({});",
            if should_blur { "true" } else { "false" }
        ));
    }
    if let Some(shell) = app.get_webview("shell") {
        let _ = shell.set_position(LogicalPosition::new(0.0, 0.0));
        let _ = shell.set_size(LogicalSize::new(width, shell_height));
        bring_webview_to_front(&shell);
    }
    Ok(())
}

pub fn sync_adblock_state(app: &AppHandle, enabled: bool) {
    let (items, allowlist): (Vec<(String, String)>, Vec<String>) = {
        let state = app.state::<AppState>();
        let items = {
            let tabs = lock(&state.tabs);
            tabs.items
                .iter()
                .map(|t| (t.label.clone(), t.url.clone()))
                .collect()
        };
        let allowlist = lock(&state.settings).ad_block_allowlist.clone();
        (items, allowlist)
    };
    for (label, tab_url) in items {
        let active = enabled && !crate::adblock::is_host_allowlisted(Some(&tab_url), &allowlist);
        let flag = if active { "true" } else { "false" };
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.eval(format!("window.__tbSetAdblock?.({flag});"));
        }
    }
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct PopupBlockedNotice {
    url: String,
    origin: String,
}

pub fn sync_window_layout(app: &AppHandle) -> Result<(), String> {
    let (top, labels) = {
        let state = app.state::<AppState>();
        let tabs = lock(&state.tabs);
        let labels: Vec<String> = tabs.items.iter().map(|tab| tab.label.clone()).collect();
        (tabs.top, labels)
    };
    let (width, height) = content_size(app, top)?;
    let size = LogicalSize::new(width, height);
    for label in labels {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.set_position(LogicalPosition::new(0.0, 0.0));
            let _ = webview.set_size(size);
        }
    }
    sync_shell_bounds(app)
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct DownloadNotice {
    state: &'static str,
    file_name: String,
    success: bool,
}

#[cfg(windows)]
fn install_browser_shortcuts(webview: &Webview, app: AppHandle) -> Result<(), String> {
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_KEY_EVENT_KIND, COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN,
        COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN,
    };

    let source_label = webview.label().to_string();
    webview
        .with_webview(move |platform| unsafe {
            let result = (|| -> windows::core::Result<()> {
                let controller = platform.controller();
                let app_for_msg = app.clone();
                let label_for_msg = source_label.clone();
                let handler =
                    AcceleratorKeyPressedEventHandler::create(Box::new(move |_, args| {
                        let Some(args) = args else {
                            return Ok(());
                        };
                        let mut kind = COREWEBVIEW2_KEY_EVENT_KIND(0);
                        args.KeyEventKind(&mut kind)?;
                        if kind != COREWEBVIEW2_KEY_EVENT_KIND_KEY_DOWN
                            && kind != COREWEBVIEW2_KEY_EVENT_KIND_SYSTEM_KEY_DOWN
                        {
                            return Ok(());
                        }
                        let mut key = 0_u32;
                        args.VirtualKey(&mut key)?;
                        let control = GetKeyState(VK_CONTROL.0 as i32) < 0;
                        let shift = GetKeyState(VK_SHIFT.0 as i32) < 0;
                        let alt = GetKeyState(VK_MENU.0 as i32) < 0;
                        let shortcut = if control && !alt && key == 0x09 {
                            Some(if shift { "previous-tab" } else { "next-tab" })
                        } else if control && !alt {
                            match (key, shift) {
                                (0x54, true) => Some("reopen-closed-tab"),
                                (0x54, false) => Some("new-tab"),
                                (0x57, _) => Some("close-tab"),
                                (0x41, true) => Some("tab-search"),
                                (0x42, true) => Some("toggle-bookmark-bar"),
                                (0x4c, _) => Some("focus-address"),
                                (0x52, _) => Some("reload"),
                                (0x44, true) => Some("bookmark-all-tabs"),
                                (0x44, false) => Some("bookmark"),
                                (0x46, _) => Some("find-in-page"),
                                (0x48, false) => Some("open-history"),
                                (0x4a, _) => Some("downloads"),
                                (0x4d, false) => Some("mute-tab"),
                                (0x4f, true) => Some("open-bookmarks"),
                                (0x50, true) => Some("new-private-tab"),
                                (0x50, false) => Some("print"),
                                (0x31, false) => Some("tab-1"),
                                (0x32, false) => Some("tab-2"),
                                (0x33, false) => Some("tab-3"),
                                (0x34, false) => Some("tab-4"),
                                (0x35, false) => Some("tab-5"),
                                (0x36, false) => Some("tab-6"),
                                (0x37, false) => Some("tab-7"),
                                (0x38, false) => Some("tab-8"),
                                (0x39, false) => Some("tab-9"),
                                (0xbb | 0x6b, _) => Some("zoom-in"),
                                (0xbd | 0x6d, _) => Some("zoom-out"),
                                (0x30 | 0x6e, _) => Some("zoom-reset"),
                                (0x49, true) => Some("devtools"),
                                _ => None,
                            }
                        } else if !control && !alt && shift && key == 0x1b {
                            Some("task-manager")
                        } else if !control && !alt && key == 0x7b {
                            Some("devtools")
                        } else if !control && !alt && key == 0x7a {
                            // F11 toggles full screen like Chrome and Edge.
                            Some("fullscreen")
                        } else if alt && !control {
                            match key {
                                0x25 => Some("back"),
                                0x27 => Some("forward"),
                                _ => None,
                            }
                        } else if key == 0xa6 || key == 0xa7 {
                            let dir = if key == 0xa6 { "back" } else { "forward" };
                            let is_settings = {
                                let state = app.state::<AppState>();
                                let tabs = lock(&state.tabs);
                                tabs.active_tab()
                                    .is_some_and(|t| t.url.starts_with("tb://settings"))
                            };
                            if is_settings {
                                if let Some(wv) = active_webview(&app).ok() {
                                    let _ =
                                        wv.eval(format!("window.__tbSettingsNav?.(\"{dir}\");"));
                                }
                            } else {
                                let _ = if key == 0xa6 {
                                    go_back(&app)
                                } else {
                                    go_forward(&app)
                                };
                            }
                            args.SetHandled(true)?;
                            return Ok(());
                        } else {
                            None
                        };
                        if let Some(shortcut) = shortcut {
                            if shortcut == "devtools" {
                                open_webview_devtools(&app);
                                args.SetHandled(true)?;
                                return Ok(());
                            }
                            if (shortcut == "focus-address" || shortcut == "find-in-page")
                                && let Some(shell) = app.get_webview("shell")
                            {
                                let _ = shell.set_focus();
                            }
                            let _ = app.emit_to("shell", "browser-shortcut", shortcut);
                            args.SetHandled(true)?;
                        }
                        Ok(())
                    }));
                let mut token = 0;
                controller.add_AcceleratorKeyPressed(&handler, &mut token)?;
                if let Ok(core) = controller.CoreWebView2() {
                    let msg_handler =
                        WebMessageReceivedEventHandler::create(Box::new(move |_, args| {
                            let Some(args) = args else {
                                return Ok(());
                            };
                            let mut message = PWSTR::null();
                            if args.TryGetWebMessageAsString(&mut message).is_ok()
                                && !message.is_null()
                            {
                                let text = take_pwstr(message);
                                if text.contains("\"internal-ready\"")
                                    || text.contains("\"first-paint\"")
                                {
                                    let is_internal = text.contains("\"internal-ready\"");
                                    let is_active = {
                                        let state = app_for_msg.state::<AppState>();
                                        let tabs = lock(&state.tabs);
                                        tabs.active_tab().is_some_and(|t| t.label == label_for_msg)
                                    };
                                    if is_active {
                                        show_only_focus(&app_for_msg, &label_for_msg, is_internal);
                                    }
                                    return Ok(());
                                }
                                if text.contains("\"page-contextmenu\"") {
                                    {
                                        let state = app_for_msg.state::<AppState>();
                                        let mut tabs = lock(&state.tabs);
                                        tabs.overlay_open = true;
                                        tabs.overlay_blur = false;
                                    }
                                    let _ = sync_shell_bounds(&app_for_msg);
                                    if let Some(shell) = app_for_msg.get_webview("shell") {
                                        let _ = shell.set_focus();
                                    }
                                }
                                if let Some(shell) = app_for_msg.get_webview("shell") {
                                    let _ = shell
                                        .eval(format!("window.__ON_BROWSER_MESSAGE__?.({text});"));
                                }
                            }
                            Ok(())
                        }));
                    let mut msg_token = 0;
                    let _ = core.add_WebMessageReceived(&msg_handler, &mut msg_token);
                }
                Ok(())
            })();
            if let Err(error) = result {
                eprintln!("WebView2 단축키를 연결하지 못했습니다: {error}");
            }
        })
        .map_err(|error| format!("WebView2 단축키를 연결하지 못했습니다: {error}"))
}

#[cfg(windows)]
fn install_adblock_filter(webview: &Webview, app: AppHandle, tab_id: String) -> Result<(), String> {
    webview
        .with_webview(move |platform| unsafe {
            let result = (|| -> windows::core::Result<()> {
                let core = platform.controller().CoreWebView2()?;
                let environment = platform.environment();
                let filter = HSTRING::from("*");
                core.AddWebResourceRequestedFilter(&filter, COREWEBVIEW2_WEB_RESOURCE_CONTEXT_ALL)?;
                let handler = WebResourceRequestedEventHandler::create(Box::new(move |_, args| {
                    let Some(args) = args else {
                        return Ok(());
                    };
                    let state = app.state::<AppState>();
                    let ad_block_on = state.ad_block_enabled.load(Ordering::SeqCst);
                    let (page_url, tracking_prevention, allowlist) = {
                        let page_url = lock(&state.tabs)
                            .items
                            .iter()
                            .find(|t| t.id == tab_id)
                            .map(|t| t.url.clone());
                        let settings = lock(&state.settings);
                        let allowlist = if ad_block_on {
                            settings.ad_block_allowlist.clone()
                        } else {
                            Vec::new()
                        };
                        (page_url, settings.tracking_prevention.clone(), allowlist)
                    };

                    let request = args.Request()?;
                    let mut uri = PWSTR::null();
                    request.Uri(&mut uri)?;
                    let uri = take_pwstr(uri);
                    if crate::adblock::blocks_request(
                        &uri,
                        page_url.as_deref(),
                        ad_block_on,
                        &allowlist,
                        &tracking_prevention,
                    ) {
                        let reason = HSTRING::from("Blocked by Reading Browser");
                        let headers = HSTRING::from(
                            "Content-Type: text/plain\r\nCache-Control: no-store\r\n",
                        );
                        let response =
                            environment.CreateWebResourceResponse(None, 403, &reason, &headers)?;
                        args.SetResponse(&response)?;
                    }
                    Ok(())
                }));
                let mut token = 0;
                core.add_WebResourceRequested(&handler, &mut token)?;
                Ok(())
            })();
            if let Err(error) = result {
                eprintln!("WebView2 광고 차단을 설치하지 못했습니다: {error}");
            }
        })
        .map_err(|error| format!("WebView2 광고 차단을 연결하지 못했습니다: {error}"))
}

fn update_title(app: &AppHandle, tab_id: &str, title: &str) {
    let state = app.state::<AppState>();
    let mut tabs = lock(&state.tabs);
    let Some(tab) = tabs.items.iter_mut().find(|tab| tab.id == tab_id) else {
        return;
    };
    tab.title = title.to_string();
    let url = tab.url.clone();
    let is_private = tab.is_private;
    let snapshot = tabs.snapshot();
    drop(tabs);
    if !is_private {
        let db = lock(&state.db);
        let _ = crate::db::update_history_title(&db, &url, title);
    }
    let _ = app.emit_to("shell", "browser-state", snapshot);
}

#[derive(Clone, serde::Serialize)]
#[serde(rename_all = "camelCase")]
struct SavedTranslationNotice {
    tab_id: String,
    url: String,
}

fn update_page_load(app: &AppHandle, tab_id: &str, url: &str, event: PageLoadEvent) {
    let state = app.state::<AppState>();
    let (restore_scroll, visit, finished_url, top_bar_h, loaded_url, ui_theme) = {
        let mut tabs = lock(&state.tabs);
        let Some(tab) = tabs.items.iter_mut().find(|tab| tab.id == tab_id) else {
            return;
        };
        let mut restore_scroll = None;
        let mut visit = None;
        let mut finished_url = None;
        let is_private = tab.is_private;
        if matches!(event, PageLoadEvent::Started) {
            tab.url = url.to_string();
            tab.loading = true;
            if tab.history.get(tab.cursor).map(|entry| entry.url.as_str()) != Some(url) {
                tab.history.truncate(tab.cursor + 1);
                tab.history.push(HistoryEntry::new(url));
                tab.cursor = tab.history.len() - 1;
                tab.scroll_x = 0.0;
                tab.scroll_y = 0.0;
            }
            persist_session(app, &tabs);
        } else if !finish_page_load(tab, url) {
            // A previous navigation may finish after a newer one has already started.
            return;
        } else {
            restore_scroll = Some((tab.label.clone(), tab.scroll_x, tab.scroll_y, tab.muted));
            if !is_private {
                visit = Some((tab.url.clone(), tab.title.clone()));
                finished_url = Some(tab.url.clone());
            }
        }
        let ui_theme = serde_json::json!({
            "isDark": tabs.ui_is_dark,
            "accent": tabs.ui_accent,
            "tinted": tabs.ui_tinted,
        });
        (
            restore_scroll,
            visit,
            finished_url,
            tabs.top,
            url.to_string(),
            ui_theme,
        )
    };
    let snapshot = lock(&state.tabs).snapshot();
    if let Some((url, title)) = visit {
        let db = lock(&state.db);
        let _ = crate::db::record_history(&db, &url, &title);
    }
    if let Some(finished_url) = finished_url {
        let (target_language, style) = {
            let settings = lock(&state.settings);
            (settings.target_language.clone(), settings.style.clone())
        };
        let has_saved = {
            let db = lock(&state.db);
            crate::db::has_page_translations(&db, &finished_url, &target_language, &style)
                .unwrap_or(false)
        };
        if has_saved {
            let _ = app.emit_to(
                "shell",
                "saved-translation-available",
                SavedTranslationNotice {
                    tab_id: tab_id.to_string(),
                    url: finished_url,
                },
            );
        }
    }
    if let Some((label, x, y, is_muted)) = restore_scroll
        && let Some(webview) = app.get_webview(&label)
    {
        schedule_scroll_restore(&webview, x, y);
        let _ = webview.eval(SELECTION_HELPER_SCRIPT);
        let (allowlist, ui_language) = {
            let settings = lock(&state.settings);
            (
                settings.ad_block_allowlist.clone(),
                settings.ui_language.clone(),
            )
        };
        let ui_lang_json = serde_json::to_string(&ui_language).unwrap_or_else(|_| "\"ko\"".into());
        let origin = url::Url::parse(&loaded_url)
            .ok()
            .map(|u| u.origin().ascii_serialization())
            .unwrap_or_default();
        let (saved_usernames, has_profile) = {
            let db = lock(&state.db);
            let logins = if origin.is_empty() {
                Vec::new()
            } else {
                crate::db::list_saved_logins(&db, Some(&origin))
                    .unwrap_or_default()
                    .into_iter()
                    .map(|l| l.username)
                    .collect()
            };
            let has_profile = crate::db::get_setting(&db, "autofill_profile")
                .and_then(|json| serde_json::from_str::<crate::model::AutofillProfile>(&json).ok())
                .is_some_and(|p| !p.name.trim().is_empty() || !p.email.trim().is_empty());
            (logins, has_profile)
        };
        let autofill_hints = serde_json::json!({
            "origin": origin,
            "usernames": saved_usernames,
            "hasProfile": has_profile,
        });
        let adblock_on = state.ad_block_enabled.load(Ordering::SeqCst)
            && !crate::adblock::is_host_allowlisted(Some(&loaded_url), &allowlist);
        let _ = webview.eval(format!(
            "window.__tbSetUiLang?.({});window.__tbSetAdblock?.({});window.__tbSetTopBarHeight?.({:.1});window.__tbSetUiTheme?.({});window.__tbSetAutofillHints?.({});window.__tbPageFadeIn?.();",
            ui_lang_json,
            if adblock_on { "true" } else { "false" },
            top_bar_h,
            ui_theme,
            autofill_hints
        ));
        inject_matching_userscripts(&webview, &state, &loaded_url);
        if is_muted {
            apply_mute(&webview, true);
        }
        let is_active = lock(&state.tabs).active.as_deref() == Some(tab_id);
        if is_active {
            show_only_focus(app, &label, false);
        } else {
            reset_overlay_z_order(app);
        }
    }
    let _ = app.emit_to("shell", "browser-state", snapshot);
}

fn finish_page_load(tab: &mut Tab, url: &str) -> bool {
    if tab.url != url {
        return false;
    }
    tab.loading = false;
    true
}

fn show_only(app: &AppHandle, label: &str) {
    show_only_focus(app, label, true);
}

fn show_only_focus(app: &AppHandle, label: &str, focus: bool) {
    if let Some(target_wv) = app.get_webview(label) {
        let _ = target_wv.show();
        bring_webview_to_front(&target_wv);
        if focus {
            let _ = target_wv.set_focus();
        }
    }
    let _ = sync_shell_bounds(app);
    let (labels_to_hide, retired_to_close): (Vec<String>, Vec<String>) = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        let hide = tabs
            .items
            .iter()
            .filter(|tab| tab.label != label)
            .map(|tab| tab.label.clone())
            .collect();
        let retired = std::mem::take(&mut tabs.retired_labels);
        (hide, retired)
    };
    if labels_to_hide.is_empty() && retired_to_close.is_empty() {
        return;
    }
    let hide_app = app.clone();
    let expected_label = label.to_string();
    tauri::async_runtime::spawn(async move {
        // Keep the previous/closed tab behind the newly raised tab for a couple frames so
        // DComp finishes presenting before the background WebView is hidden or destroyed.
        tokio::time::sleep(std::time::Duration::from_millis(48)).await;
        let still_active = {
            let state = hide_app.state::<AppState>();
            let tabs = lock(&state.tabs);
            tabs.active_tab()
                .is_some_and(|tab| tab.label == expected_label)
        };
        if !still_active {
            if !retired_to_close.is_empty() {
                let state = hide_app.state::<AppState>();
                lock(&state.tabs).retired_labels.extend(retired_to_close);
            }
            return;
        }
        for other_label in labels_to_hide {
            if let Some(webview) = hide_app.get_webview(&other_label) {
                let _ = webview.hide();
            }
        }
        for closed_label in retired_to_close {
            if let Some(webview) = hide_app.get_webview(&closed_label) {
                let _ = webview.close();
            }
        }
        let _ = sync_shell_bounds(&hide_app);
    });
}

pub fn open_tab(app: &AppHandle, url: &str) -> Result<BrowserState, String> {
    open_tab_with_privacy(app, url, false)
}

pub fn open_private_tab(app: &AppHandle, url: &str) -> Result<BrowserState, String> {
    open_tab_with_privacy(app, url, true)
}

pub fn default_window_title(_ui_language: &str) -> &'static str {
    "Fluo"
}

fn internal_page_title(kind: &str, ui_language: &str) -> Option<&'static str> {
    match (kind, ui_language) {
        ("start", "en") => Some("Start Page"),
        ("start", "ja") => Some("スタートページ"),
        ("start", "zh-CN") => Some("起始页"),
        ("start", _) => Some("시작 페이지"),
        ("bookmarks", "en") => Some("Bookmarks"),
        ("bookmarks", "ja") => Some("ブックマーク"),
        ("bookmarks", "zh-CN") => Some("书签"),
        ("bookmarks", _) => Some("북마크"),
        ("visits", "en") => Some("History"),
        ("visits", "ja") => Some("閲覧履歴"),
        ("visits", "zh-CN") => Some("浏览历史"),
        ("visits", _) => Some("방문 기록"),
        ("downloads", "en") => Some("Downloads"),
        ("downloads", "ja") => Some("ダウンロード"),
        ("downloads", "zh-CN") => Some("下载"),
        ("downloads", _) => Some("다운로드"),
        ("glossary", "en") => Some("Glossary"),
        ("glossary", "ja") => Some("用語集"),
        ("glossary", "zh-CN") => Some("术语表"),
        ("glossary", _) => Some("용어집"),
        ("history", "en") => Some("Translation History"),
        ("history", "ja") => Some("翻訳履歴"),
        ("history", "zh-CN") => Some("翻译历史"),
        ("history", _) => Some("번역 기록"),
        ("settings", "en") => Some("Settings"),
        ("settings", "ja") => Some("設定"),
        ("settings", "zh-CN") => Some("设置"),
        ("settings", _) => Some("설정"),
        _ => None,
    }
}

pub fn open_internal_tab(app: &AppHandle, kind: &str) -> Result<BrowserState, String> {
    let ui_language = lock(&app.state::<AppState>().settings).ui_language.clone();
    let title = internal_page_title(kind, &ui_language)
        .ok_or_else(|| "알 수 없는 브라우저 페이지입니다.".to_string())?;
    let internal_url = format!("tb://{kind}");
    let existing_id = {
        let state = app.state::<AppState>();
        let tabs = lock(&state.tabs);
        tabs.items
            .iter()
            .find(|tab| tab.workspace_id == tabs.active_workspace && tab.url == internal_url)
            .map(|tab| tab.id.clone())
    };
    if let Some(id) = existing_id {
        return select_tab(app, &id);
    }
    let tab = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        let mut tab = tabs.register_tab(&internal_url);
        tab.loading = false;
        tab.title = title.to_string();
        if let Some(stored) = tabs.items.last_mut() {
            stored.loading = false;
            stored.title = title.to_string();
        }
        tab
    };
    let top = lock(&app.state::<AppState>().tabs).top;
    if let Err(error) = create_webview(app, top, &tab) {
        if let Some(webview) = app.get_webview(&tab.label) {
            let _ = webview.close();
        }
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        tabs.items.retain(|existing| existing.id != tab.id);
        persist_session(app, &tabs);
        return Err(error);
    }
    let (only_tab, snapshot) = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        tabs.active = Some(tab.id.clone());
        persist_session(app, &tabs);
        (tabs.items.len() <= 1, tabs.snapshot())
    };
    if only_tab {
        show_only(app, &tab.label);
    } else {
        let reveal_app = app.clone();
        let reveal_id = tab.id.clone();
        let reveal_label = tab.label.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(450)).await;
            let state = reveal_app.state::<AppState>();
            let should_show = lock(&state.tabs).active.as_deref() == Some(&reveal_id);
            if should_show {
                show_only_focus(&reveal_app, &reveal_label, false);
            }
        });
    }
    let _ = sync_shell_bounds(app);
    sleep_inactive_tabs(app, 6);
    let _ = app.emit_to("shell", "browser-state", snapshot.clone());
    Ok(snapshot)
}

fn apply_mute(webview: &Webview, muted: bool) {
    let script = format!(
        "(() => {{
            const muted = {};
            window.__tbMuted = muted;
            const apply = (root = document) => {{
                root.querySelectorAll?.('video, audio').forEach((el) => {{
                    try {{ el.muted = muted; }} catch (_) {{}}
                }});
            }};
            apply();
            if (!window.__tbMuteHooked) {{
                window.__tbMuteHooked = true;
                document.addEventListener('play', (e) => {{
                    if (window.__tbMuted && e.target && 'muted' in e.target) {{
                        try {{ e.target.muted = true; }} catch (_) {{}}
                    }}
                }}, true);
                let mute_raf = 0;
                const obs = new MutationObserver((mutations) => {{
                    if (!window.__tbMuted || mute_raf) return;
                    let hasAdded = false;
                    for (const m of mutations) {{
                        if (m.addedNodes && m.addedNodes.length > 0) {{ hasAdded = true; break; }}
                    }}
                    if (!hasAdded) return;
                    mute_raf = requestAnimationFrame(() => {{
                        mute_raf = 0;
                        if (window.__tbMuted) apply();
                    }});
                }});
                if (document.documentElement) {{
                    obs.observe(document.documentElement, {{ childList: true, subtree: true }});
                }}
            }}
        }})()",
        if muted { "true" } else { "false" }
    );
    let _ = webview.eval(&script);
}

pub fn toggle_mute_tab(app: &AppHandle, tab_id: &str) -> Result<BrowserState, String> {
    let state = app.state::<AppState>();
    let mut tabs = lock(&state.tabs);
    let Some(tab) = tabs.items.iter_mut().find(|tab| tab.id == tab_id) else {
        return Err("탭을 찾지 못했습니다.".into());
    };
    tab.muted = !tab.muted;
    let muted = tab.muted;
    let label = tab.label.clone();
    let snapshot = tabs.snapshot();
    drop(tabs);
    if let Some(webview) = app.get_webview(&label) {
        apply_mute(&webview, muted);
    }
    let _ = app.emit_to("shell", "browser-state", snapshot.clone());
    Ok(snapshot)
}

pub fn sleep_tab(app: &AppHandle, tab_id: &str) -> Result<BrowserState, String> {
    let label_to_close = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        if tabs.active.as_deref() == Some(tab_id) {
            return Err("현재 보고 있는 활성 탭은 절전 모드로 전환할 수 없습니다.".into());
        }
        let Some(tab) = tabs.items.iter_mut().find(|tab| tab.id == tab_id) else {
            return Err("탭을 찾지 못했습니다.".into());
        };
        if tab.sleeping {
            return Ok(tabs.snapshot());
        }
        tab.sleeping = true;
        tab.loading = false;
        tab.label.clone()
    };
    if let Some(webview) = app.get_webview(&label_to_close) {
        let _ = webview.close();
    }
    let state = app.state::<AppState>();
    let snapshot = lock(&state.tabs).snapshot();
    let _ = app.emit_to("shell", "browser-state", snapshot.clone());
    Ok(snapshot)
}

fn open_tab_with_privacy(
    app: &AppHandle,
    url: &str,
    is_private: bool,
) -> Result<BrowserState, String> {
    let ui_language = lock(&app.state::<AppState>().settings).ui_language.clone();
    let internal_title = url
        .strip_prefix("tb://")
        .and_then(|kind| internal_page_title(kind, &ui_language));
    let tab = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        let mut tab = tabs.register_tab(url);
        if is_private {
            tab.is_private = true;
            if let Some(stored) = tabs.items.last_mut() {
                stored.is_private = true;
            }
        }
        if let Some(title) = internal_title {
            tab.loading = false;
            tab.title = title.to_string();
            if let Some(stored) = tabs.items.last_mut() {
                stored.loading = false;
                stored.title = title.to_string();
            }
        }
        tab
    };
    let top = lock(&app.state::<AppState>().tabs).top;
    if let Err(error) = create_webview(app, top, &tab) {
        if let Some(webview) = app.get_webview(&tab.label) {
            let _ = webview.close();
        }
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        tabs.items.retain(|existing| existing.id != tab.id);
        persist_session(app, &tabs);
        return Err(error);
    }

    let (only_tab, snapshot) = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        tabs.active = Some(tab.id.clone());
        debug_assert_eq!(
            tabs.items.iter().filter(|item| item.id == tab.id).count(),
            1,
            "a tab must be registered in browser state exactly once"
        );
        persist_session(app, &tabs);
        (tabs.items.len() <= 1, tabs.snapshot())
    };
    if only_tab {
        show_only(app, &tab.label);
    } else {
        let reveal_app = app.clone();
        let reveal_id = tab.id.clone();
        let reveal_label = tab.label.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(750)).await;
            let state = reveal_app.state::<AppState>();
            let should_show = lock(&state.tabs).active.as_deref() == Some(&reveal_id);
            if should_show {
                show_only_focus(&reveal_app, &reveal_label, false);
            }
        });
    }
    let _ = sync_shell_bounds(app);
    sleep_inactive_tabs(app, 6);
    let _ = app.emit_to("shell", "browser-state", snapshot.clone());
    Ok(snapshot)
}

pub fn close_tab(app: &AppHandle, tab_id: &str) -> Result<BrowserState, String> {
    let (label, was_active, next_active_id, active_workspace_empty, no_tabs) = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        let Some(position) = tabs.items.iter().position(|tab| tab.id == tab_id) else {
            return Ok(tabs.snapshot());
        };
        let was_active = tabs.active.as_deref() == Some(tab_id);
        let removed = tabs.items.remove(position);
        if !removed.is_private {
            tabs.closed.push(removed.url.clone(), removed.title.clone());
        }
        if was_active {
            tabs.active = tabs
                .items
                .iter()
                .skip(position)
                .find(|tab| tab.workspace_id == removed.workspace_id)
                .or_else(|| {
                    tabs.items[..position.min(tabs.items.len())]
                        .iter()
                        .rev()
                        .find(|tab| tab.workspace_id == removed.workspace_id)
                })
                .map(|tab| tab.id.clone());
            tabs.retired_labels.push(removed.label.clone());
        }
        let active_workspace_empty =
            tabs.active.is_none() && tabs.active_workspace == removed.workspace_id;
        let next_active_id = tabs.active.clone();
        persist_session(app, &tabs);
        (
            removed.label,
            was_active,
            next_active_id,
            active_workspace_empty,
            tabs.items.is_empty(),
        )
    };

    if !was_active {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.close();
        }
        let state = app.state::<AppState>();
        return Ok(lock(&state.tabs).snapshot());
    }

    if no_tabs || active_workspace_empty {
        let home = crate::effective_home_url(app);
        return open_tab(app, &home);
    }

    if let Some(next_id) = next_active_id {
        return select_tab(app, &next_id);
    }

    let state = app.state::<AppState>();
    let active_label = {
        let tabs = lock(&state.tabs);
        tabs.active_tab().map(|tab| tab.label.clone())
    };
    if let Some(label) = active_label {
        show_only(app, &label);
    }
    Ok(lock(&state.tabs).snapshot())
}

pub fn pin_tab(app: &AppHandle, tab_id: &str) -> Result<BrowserState, String> {
    let state = app.state::<AppState>();
    let mut tabs = lock(&state.tabs);
    let Some(pos) = tabs.items.iter().position(|tab| tab.id == tab_id) else {
        return Err("탭을 찾지 못했습니다.".into());
    };
    let next_pinned = !tabs.items[pos].pinned;
    let mut tab = tabs.items.remove(pos);
    tab.pinned = next_pinned;
    let workspace = tab.workspace_id.clone();
    if next_pinned {
        let insert_at = tabs
            .items
            .iter()
            .position(|item| item.workspace_id == workspace && !item.pinned)
            .unwrap_or(tabs.items.len());
        tabs.items.insert(insert_at, tab);
    } else {
        let insert_at = tabs
            .items
            .iter()
            .rposition(|item| item.workspace_id == workspace && item.pinned)
            .map(|idx| idx + 1)
            .unwrap_or(pos.min(tabs.items.len()));
        tabs.items.insert(insert_at, tab);
    }
    persist_session(app, &tabs);
    let snapshot = tabs.snapshot();
    drop(tabs);
    sleep_inactive_tabs(app, 6);
    let _ = app.emit_to("shell", "browser-state", snapshot.clone());
    Ok(snapshot)
}

pub fn close_other_tabs(app: &AppHandle, tab_id: &str) -> Result<BrowserState, String> {
    {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        let Some(target) = tabs.items.iter().find(|tab| tab.id == tab_id).cloned() else {
            return Err("기준 탭을 찾지 못했습니다.".into());
        };
        let mut labels = Vec::new();
        let mut closed_entries = Vec::new();
        tabs.items.retain(|tab| {
            let keep = tab.id == tab_id || tab.workspace_id != target.workspace_id || tab.pinned;
            if !keep {
                labels.push(tab.label.clone());
                if !tab.is_private {
                    closed_entries.push((tab.url.clone(), tab.title.clone()));
                }
            }
            keep
        });
        for (url, title) in closed_entries {
            tabs.closed.push(url, title);
        }
        tabs.active = Some(tab_id.to_string());
        tabs.retired_labels.extend(labels);
        persist_session(app, &tabs);
    };
    let state = app.state::<AppState>();
    let active_label = {
        let tabs = lock(&state.tabs);
        tabs.active_tab().map(|tab| tab.label.clone())
    };
    if let Some(label) = active_label {
        show_only(app, &label);
    }
    Ok(lock(&state.tabs).snapshot())
}

pub fn close_tabs_to_right(app: &AppHandle, tab_id: &str) -> Result<BrowserState, String> {
    {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        let Some(pos) = tabs.items.iter().position(|tab| tab.id == tab_id) else {
            return Err("기준 탭을 찾지 못했습니다.".into());
        };
        let workspace = tabs.items[pos].workspace_id.clone();
        let mut labels = Vec::new();
        let mut closed_entries = Vec::new();
        let mut index = 0;
        tabs.items.retain(|tab| {
            let current = index;
            index += 1;
            let keep = current <= pos || tab.workspace_id != workspace || tab.pinned;
            if !keep {
                labels.push(tab.label.clone());
                if !tab.is_private {
                    closed_entries.push((tab.url.clone(), tab.title.clone()));
                }
            }
            keep
        });
        for (url, title) in closed_entries {
            tabs.closed.push(url, title);
        }
        if !tabs
            .active
            .as_ref()
            .is_some_and(|active| tabs.items.iter().any(|tab| &tab.id == active))
        {
            tabs.active = Some(tab_id.to_string());
        }
        tabs.retired_labels.extend(labels);
        persist_session(app, &tabs);
    };
    let state = app.state::<AppState>();
    let active_label = {
        let tabs = lock(&state.tabs);
        tabs.active_tab().map(|tab| tab.label.clone())
    };
    if let Some(label) = active_label {
        show_only(app, &label);
    }
    Ok(lock(&state.tabs).snapshot())
}

fn inject_matching_userscripts(webview: &Webview, state: &AppState, url: &str) {
    if !url.starts_with("http://") && !url.starts_with("https://") {
        return;
    }
    let scripts = {
        let db = lock(&state.db);
        crate::db::list_user_scripts(&db).unwrap_or_default()
    };
    let host = url::Url::parse(url)
        .ok()
        .and_then(|u| u.host_str().map(str::to_ascii_lowercase))
        .unwrap_or_default();
    for script in scripts {
        if !script.enabled {
            continue;
        }
        let pat = script.match_pattern.trim().to_ascii_lowercase();
        let matched = pat == "*"
            || pat == "<all_urls>"
            || url.to_ascii_lowercase().contains(&pat)
            || (!host.is_empty() && (host == pat || host.ends_with(&format!(".{pat}"))));
        if !matched {
            continue;
        }
        if !script.css.trim().is_empty()
            && let Ok(css_json) = serde_json::to_string(&script.css)
        {
            let _ = webview.eval(format!(
                "(()=>{{try{{const s=document.createElement('style');s.textContent={css_json};(document.head||document.documentElement).appendChild(s);}}catch{{}}}})();"
            ));
        }
        if !script.code.trim().is_empty() {
            let _ = webview.eval(format!(
                "(()=>{{try{{{}}}catch(e){{console.warn('UserScript error',e);}}}})();",
                script.code
            ));
        }
    }
}

fn sleep_inactive_tabs(app: &AppHandle, _default_max_awake: usize) {
    let (enabled, max_awake, allowlist) = {
        let state = app.state::<AppState>();
        let s = lock(&state.settings);
        (
            s.sleeping_tabs_enabled,
            (s.max_awake_tabs as usize).clamp(1, 50),
            s.sleep_allowlist.clone(),
        )
    };
    if !enabled {
        return;
    }
    let labels_to_sleep = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        let active_id = tabs.active.clone();
        let mut candidates: Vec<(usize, i64)> = tabs
            .items
            .iter()
            .enumerate()
            .filter(|(_, tab)| {
                Some(&tab.id) != active_id.as_ref()
                    && !tab.sleeping
                    && !tab.pinned
                    && !tab.is_private
                    && !tab.url.starts_with("tb://")
                    && !crate::adblock::is_host_allowlisted(Some(&tab.url), &allowlist)
            })
            .map(|(idx, tab)| (idx, tab.last_active_at))
            .collect();
        if candidates.len() <= max_awake {
            return;
        }
        candidates.sort_by(|a, b| b.1.cmp(&a.1));
        let mut labels = Vec::new();
        for (idx, _) in candidates.into_iter().skip(max_awake) {
            if let Some(tab) = tabs.items.get_mut(idx) {
                tab.sleeping = true;
                labels.push(tab.label.clone());
            }
        }
        labels
    };
    for label in labels_to_sleep {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.close();
        }
    }
}

pub fn sleep_idle_tabs(app: &AppHandle) {
    let (enabled, timeout_mins, allowlist) = {
        let state = app.state::<AppState>();
        let s = lock(&state.settings);
        (
            s.sleeping_tabs_enabled,
            i64::from(s.sleep_timeout_minutes.clamp(1, 1440)),
            s.sleep_allowlist.clone(),
        )
    };
    if !enabled {
        return;
    }
    let cutoff = crate::db::now_secs() - timeout_mins * 60;
    let (labels_to_sleep, snapshot) = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        let active_id = tabs.active.clone();
        let mut labels = Vec::new();
        for tab in &mut tabs.items {
            if Some(&tab.id) != active_id.as_ref()
                && !tab.sleeping
                && !tab.pinned
                && !tab.is_private
                && !tab.url.starts_with("tb://")
                && tab.last_active_at <= cutoff
                && !crate::adblock::is_host_allowlisted(Some(&tab.url), &allowlist)
            {
                tab.sleeping = true;
                labels.push(tab.label.clone());
            }
        }
        if labels.is_empty() {
            return;
        }
        (labels, tabs.snapshot())
    };
    for label in labels_to_sleep {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.close();
        }
    }
    let _ = app.emit_to("shell", "browser-state", snapshot);
}

pub fn select_tab(app: &AppHandle, tab_id: &str) -> Result<BrowserState, String> {
    let (wake_tab, top, label) = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        let Some(pos) = tabs.items.iter().position(|tab| tab.id == tab_id) else {
            return Err("탭을 찾지 못했습니다.".into());
        };
        let wake = if tabs.items[pos].sleeping {
            tabs.items[pos].sleeping = false;
            tabs.items[pos].loading = !tabs.items[pos].url.starts_with("tb://");
            Some(tabs.items[pos].clone())
        } else {
            None
        };
        if wake.is_none() {
            tabs.active = Some(tab_id.to_string());
            tabs.active_workspace = tabs.items[pos].workspace_id.clone();
            tabs.items[pos].last_active_at = crate::db::now_secs();
        }
        let label = tabs.items[pos].label.clone();
        (wake, tabs.top, label)
    };
    let woke = wake_tab.is_some();
    if let Some(tab) = wake_tab {
        create_webview(app, top, &tab)?;
        if let Some(webview) = app.get_webview(&tab.label) {
            let _ = apply_zoom(&webview, tab.zoom);
            schedule_scroll_restore(&webview, tab.scroll_x, tab.scroll_y);
        }
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        if let Some(ws) = tabs
            .items
            .iter_mut()
            .find(|item| item.id == tab_id)
            .map(|item| {
                item.last_active_at = crate::db::now_secs();
                item.workspace_id.clone()
            })
        {
            tabs.active = Some(tab_id.to_string());
            tabs.active_workspace = ws;
        }
    }
    if woke {
        let reveal_app = app.clone();
        let reveal_id = tab_id.to_string();
        let reveal_label = label.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(std::time::Duration::from_millis(750)).await;
            let state = reveal_app.state::<AppState>();
            let should_show = lock(&state.tabs).active.as_deref() == Some(&reveal_id);
            if should_show {
                show_only_focus(&reveal_app, &reveal_label, true);
            }
        });
    } else {
        show_only(app, &label);
    }
    let state = app.state::<AppState>();
    let tabs = lock(&state.tabs);
    persist_session(app, &tabs);
    let snapshot = tabs.snapshot();
    drop(tabs);
    sleep_inactive_tabs(app, 6);
    let _ = app.emit_to("shell", "browser-state", snapshot.clone());
    Ok(snapshot)
}

/// Moves a tab before `before_id`, or to the end when it is absent.
fn reorder_tabs_in(tabs: &mut Tabs, tab_id: &str, before_id: Option<&str>) -> bool {
    let Some(from) = tabs.items.iter().position(|tab| tab.id == tab_id) else {
        return false;
    };
    let tab = tabs.items.remove(from);
    let index = before_id
        .filter(|id| *id != tab_id)
        .and_then(|id| tabs.items.iter().position(|item| item.id == id))
        .unwrap_or(tabs.items.len());
    tabs.items.insert(index, tab);
    true
}

pub fn reorder_tab(
    app: &AppHandle,
    tab_id: &str,
    before_id: Option<&str>,
) -> Result<BrowserState, String> {
    let state = app.state::<AppState>();
    let mut tabs = lock(&state.tabs);
    if !reorder_tabs_in(&mut tabs, tab_id, before_id) {
        return Err("탭을 찾지 못했습니다.".into());
    }
    let snapshot = tabs.snapshot();
    persist_session(app, &tabs);
    drop(tabs);
    let _ = app.emit_to("shell", "browser-state", snapshot.clone());
    Ok(snapshot)
}

fn assign_tab_workspace(tabs: &mut Tabs, tab_id: &str, workspace_id: &str) -> Result<(), String> {
    if !tabs
        .workspaces
        .iter()
        .any(|workspace| workspace.id == workspace_id)
    {
        return Err("작업공간을 찾지 못했습니다.".into());
    }
    let Some(tab) = tabs.items.iter_mut().find(|tab| tab.id == tab_id) else {
        return Err("탭을 찾지 못했습니다.".into());
    };
    tab.workspace_id = workspace_id.to_string();
    Ok(())
}

pub fn move_tab_to_workspace(
    app: &AppHandle,
    tab_id: &str,
    workspace_id: &str,
) -> Result<BrowserState, String> {
    {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        assign_tab_workspace(&mut tabs, tab_id, workspace_id)?;
    }
    select_tab(app, tab_id)
}

fn create_workspace_in(tabs: &mut Tabs, name: &str, now_secs: i64) -> Result<String, String> {
    let name: String = name.trim().chars().take(80).collect();
    if name.is_empty() {
        return Err("작업공간 이름을 입력하세요.".into());
    }
    if tabs
        .workspaces
        .iter()
        .any(|item| item.name.eq_ignore_ascii_case(&name))
    {
        return Err("같은 이름의 작업공간이 있습니다.".into());
    }
    let base = format!("ws-{now_secs}-{}", tabs.workspaces.len());
    let mut id = base.clone();
    let mut suffix = 1_u32;
    while tabs.workspaces.iter().any(|workspace| workspace.id == id) {
        id = format!("{base}-{suffix}");
        suffix = suffix.wrapping_add(1);
    }
    tabs.workspaces.push(Workspace {
        id: id.clone(),
        name,
    });
    tabs.active_workspace = id.clone();
    Ok(id)
}

fn switch_workspace_in(tabs: &mut Tabs, workspace_id: &str) -> Result<Option<String>, String> {
    if !tabs.workspaces.iter().any(|item| item.id == workspace_id) {
        return Err("작업공간을 찾지 못했습니다.".into());
    }
    tabs.active_workspace = workspace_id.to_string();
    let tab_id = tabs
        .items
        .iter()
        .rev()
        .find(|tab| tab.workspace_id == workspace_id)
        .map(|tab| tab.id.clone());
    tabs.active = tab_id.clone();
    Ok(tab_id)
}

fn rename_workspace_in(
    tabs: &mut Tabs,
    workspace_id: &str,
    name: &str,
) -> Result<BrowserState, String> {
    let name: String = name.trim().chars().take(80).collect();
    if name.is_empty() {
        return Err("작업공간 이름을 입력하세요.".into());
    }
    if tabs
        .workspaces
        .iter()
        .any(|item| item.id != workspace_id && item.name.eq_ignore_ascii_case(&name))
    {
        return Err("같은 이름의 작업공간이 있습니다.".into());
    }
    let Some(workspace) = tabs
        .workspaces
        .iter_mut()
        .find(|item| item.id == workspace_id)
    else {
        return Err("작업공간을 찾지 못했습니다.".into());
    };
    workspace.name = name;
    Ok(tabs.snapshot())
}

fn delete_workspace_in(
    tabs: &mut Tabs,
    workspace_id: &str,
) -> Result<(Option<String>, bool, bool, BrowserState), String> {
    if workspace_id == "default" {
        return Err("기본 작업공간은 삭제할 수 없습니다.".into());
    }
    let Some(index) = tabs
        .workspaces
        .iter()
        .position(|item| item.id == workspace_id)
    else {
        return Err("작업공간을 찾지 못했습니다.".into());
    };
    tabs.workspaces.remove(index);
    for tab in &mut tabs.items {
        if tab.workspace_id == workspace_id {
            tab.workspace_id = "default".into();
        }
    }
    let was_active = tabs.active_workspace == workspace_id;
    if was_active {
        tabs.active_workspace = "default".into();
    }
    let active = if was_active {
        tabs.items
            .iter()
            .rev()
            .find(|tab| tab.workspace_id == "default")
            .map(|tab| tab.id.clone())
    } else {
        None
    };
    if let Some(id) = &active {
        tabs.active = Some(id.clone());
    }
    let create_tab = was_active && active.is_none();
    Ok((active, create_tab, was_active, tabs.snapshot()))
}

pub fn create_workspace(app: &AppHandle, name: &str) -> Result<BrowserState, String> {
    {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        create_workspace_in(&mut tabs, name, crate::db::now_secs())?;
        persist_session(app, &tabs);
    }
    let home = crate::effective_home_url(app);
    let snapshot = open_tab(app, &home)?;
    let new_label = {
        let state = app.state::<AppState>();
        let tabs = lock(&state.tabs);
        tabs.active_tab().map(|tab| tab.label.clone())
    };
    if let Some(label) = new_label {
        show_only_focus(app, &label, false);
    }
    Ok(snapshot)
}

pub fn switch_workspace(app: &AppHandle, workspace_id: &str) -> Result<BrowserState, String> {
    let active = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        let tab_id = switch_workspace_in(&mut tabs, workspace_id)?;
        persist_session(app, &tabs);
        tab_id
    };
    let Some(tab_id) = active else {
        let home = crate::effective_home_url(app);
        let snapshot = open_tab(app, &home)?;
        let new_label = {
            let state = app.state::<AppState>();
            let tabs = lock(&state.tabs);
            tabs.active_tab().map(|tab| tab.label.clone())
        };
        if let Some(label) = new_label {
            show_only_focus(app, &label, false);
        }
        return Ok(snapshot);
    };
    select_tab(app, &tab_id)
}

pub fn rename_workspace(
    app: &AppHandle,
    workspace_id: &str,
    name: &str,
) -> Result<BrowserState, String> {
    let state = app.state::<AppState>();
    let mut tabs = lock(&state.tabs);
    let snapshot = rename_workspace_in(&mut tabs, workspace_id, name)?;
    persist_session(app, &tabs);
    drop(tabs);
    sleep_inactive_tabs(app, 6);
    let _ = app.emit_to("shell", "browser-state", snapshot.clone());
    Ok(snapshot)
}

pub fn delete_workspace(app: &AppHandle, workspace_id: &str) -> Result<BrowserState, String> {
    let (activate, create_tab, _was_active, snapshot) = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        let res = delete_workspace_in(&mut tabs, workspace_id)?;
        persist_session(app, &tabs);
        res
    };
    if create_tab {
        let home = crate::effective_home_url(app);
        let opened = open_tab(app, &home)?;
        let new_label = {
            let state = app.state::<AppState>();
            let tabs = lock(&state.tabs);
            tabs.active_tab().map(|tab| tab.label.clone())
        };
        if let Some(label) = new_label {
            show_only_focus(app, &label, false);
        }
        return Ok(opened);
    }
    if let Some(id) = activate {
        return select_tab(app, &id);
    }
    let _ = app.emit_to("shell", "browser-state", snapshot.clone());
    Ok(snapshot)
}

pub fn navigate(app: &AppHandle, input: &str) -> Result<BrowserState, String> {
    let trimmed = input.trim();
    if let Some(kind) = trimmed.strip_prefix("tb://")
        && internal_page_title(kind, "ko").is_some()
        && kind != "start"
    {
        return open_internal_tab(app, kind);
    }
    let ui_language = lock(&app.state::<AppState>().settings).ui_language.clone();
    let search_engine = lock(&app.state::<AppState>().settings)
        .search_engine
        .clone();
    let is_start_target = trimmed == "tb://start" || trimmed == "fluo://start";
    let resolved = if is_start_target {
        "tb://start".to_string()
    } else {
        text::resolve_address(input, &search_engine)?
    };
    let is_search_query = !is_start_target
        && !trimmed.is_empty()
        && !trimmed.contains("://")
        && (resolved.starts_with("https://www.google.com/search?q=")
            || resolved.starts_with("https://www.bing.com/search?q=")
            || resolved.starts_with("https://duckduckgo.com/?q="));
    let (old_label_to_close, updated_tab, top, is_private) = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        let top = tabs.top;
        let active_id = tabs.active.clone();
        let needs_swap = tabs
            .items
            .iter()
            .find(|t| Some(&t.id) == active_id.as_ref())
            .map(|t| t.url.starts_with("tb://") != resolved.starts_with("tb://"))
            .unwrap_or(false);
        let next_label = if needs_swap {
            Some(tabs.alloc_webview_label())
        } else {
            None
        };
        let Some(tab) = tabs
            .items
            .iter_mut()
            .find(|tab| Some(&tab.id) == active_id.as_ref())
        else {
            drop(tabs);
            return open_tab(app, &resolved);
        };
        let old_label = tab.label.clone();
        if let Some(new_lbl) = next_label {
            tab.label = new_lbl;
        }
        tab.history.truncate(tab.cursor + 1);
        if tab.history.last().map(|entry| entry.url.as_str()) != Some(resolved.as_str()) {
            tab.history.push(HistoryEntry::new(&resolved));
            tab.scroll_x = 0.0;
            tab.scroll_y = 0.0;
        }
        tab.cursor = tab.history.len() - 1;
        tab.url = resolved.clone();
        if is_start_target {
            tab.loading = false;
            tab.title = internal_page_title("start", &ui_language)
                .unwrap_or("Fluo")
                .to_string();
        } else {
            tab.loading = true;
        }
        (
            if old_label != tab.label {
                Some(old_label)
            } else {
                None
            },
            tab.clone(),
            top,
            tab.is_private,
        )
    };
    if is_search_query && !is_private {
        let state = app.state::<AppState>();
        let db = lock(&state.db);
        let _ = crate::db::record_search_history(&db, trimmed, &search_engine);
    }

    if let Some(old_label) = old_label_to_close {
        create_webview(app, top, &updated_tab)?;
        show_only_focus(app, &updated_tab.label, !is_start_target);
        if let Some(old_wv) = app.get_webview(&old_label) {
            let _ = old_wv.close();
        }
    } else if !is_start_target && let Some(webview) = app.get_webview(&updated_tab.label) {
        let _ = webview.eval("window.__tbPageFadeOut?.();");
        let url = url::Url::parse(&resolved).map_err(|error| error.to_string())?;
        webview
            .navigate(url)
            .map_err(|error| format!("이동하지 못했습니다: {error}"))?;
        let _ = webview.show();
        let _ = webview.set_focus();
    }
    let _ = sync_shell_bounds(app);
    let state = app.state::<AppState>();
    let tabs = lock(&state.tabs);
    persist_session(app, &tabs);
    let snapshot = tabs.snapshot();
    drop(tabs);
    sleep_inactive_tabs(app, 6);
    let _ = app.emit_to("shell", "browser-state", snapshot.clone());
    Ok(snapshot)
}

fn go_to_history(app: &AppHandle, delta: isize) -> Result<BrowserState, String> {
    let ui_language = lock(&app.state::<AppState>().settings).ui_language.clone();
    let (old_label_to_close, updated_tab, top, target) = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        let top = tabs.top;
        let active_id = tabs.active.clone();
        let Some(pos) = tabs
            .items
            .iter()
            .position(|tab| Some(&tab.id) == active_id.as_ref())
        else {
            return Err("활성 탭이 없습니다.".into());
        };
        let next = tabs.items[pos].cursor as isize + delta;
        if next < 0 || next as usize >= tabs.items[pos].history.len() {
            return Ok(tabs.snapshot());
        }
        let prev_is_internal = tabs.items[pos].url.starts_with("tb://");
        let target_url = tabs.items[pos].history[next as usize].url.clone();
        let next_is_internal = target_url.starts_with("tb://");
        let new_label = if prev_is_internal != next_is_internal {
            Some(tabs.alloc_webview_label())
        } else {
            None
        };
        let tab = &mut tabs.items[pos];
        let old_label = tab.label.clone();
        if let Some(lbl) = new_label {
            tab.label = lbl;
        }
        if let Some(current) = tab.history.get_mut(tab.cursor) {
            current.scroll_x = tab.scroll_x;
            current.scroll_y = tab.scroll_y;
        }
        tab.cursor = next as usize;
        let entry = tab.history[tab.cursor].clone();
        tab.url = entry.url.clone();
        if let Some(kind) = entry.url.strip_prefix("tb://") {
            tab.loading = false;
            tab.title = internal_page_title(kind, &ui_language)
                .unwrap_or("Fluo")
                .to_string();
        } else {
            tab.loading = true;
        }
        tab.scroll_x = entry.scroll_x;
        tab.scroll_y = entry.scroll_y;
        (
            if old_label != tab.label {
                Some(old_label)
            } else {
                None
            },
            tab.clone(),
            top,
            entry.url,
        )
    };

    if let Some(old_label) = old_label_to_close {
        create_webview(app, top, &updated_tab)?;
        show_only_focus(app, &updated_tab.label, !target.starts_with("tb://"));
        if let Some(old_wv) = app.get_webview(&old_label) {
            let _ = old_wv.close();
        }
    } else if !target.starts_with("tb://")
        && let Some(webview) = app.get_webview(&updated_tab.label)
    {
        let url = url::Url::parse(&target).map_err(|error| error.to_string())?;
        webview
            .navigate(url)
            .map_err(|error| format!("이동하지 못했습니다: {error}"))?;
    }
    let state = app.state::<AppState>();
    let tabs = lock(&state.tabs);
    persist_session(app, &tabs);
    let snapshot = tabs.snapshot();
    drop(tabs);
    sleep_inactive_tabs(app, 6);
    let _ = app.emit_to("shell", "browser-state", snapshot.clone());
    Ok(snapshot)
}

pub fn go_back(app: &AppHandle) -> Result<BrowserState, String> {
    go_to_history(app, -1)
}

pub fn go_forward(app: &AppHandle) -> Result<BrowserState, String> {
    go_to_history(app, 1)
}

pub fn jump_history_delta(app: &AppHandle, delta: isize) -> Result<BrowserState, String> {
    go_to_history(app, delta)
}

pub fn active_tab_history(app: &AppHandle) -> Result<(usize, Vec<String>), String> {
    let state = app.state::<AppState>();
    let tabs = lock(&state.tabs);
    let Some(tab) = tabs.active_tab() else {
        return Ok((0, Vec::new()));
    };
    let urls = tab.history.iter().map(|h| h.url.clone()).collect();
    Ok((tab.cursor, urls))
}

pub fn reload(app: &AppHandle) -> Result<BrowserState, String> {
    let state = app.state::<AppState>();
    let tabs = lock(&state.tabs);
    if let Some(tab) = tabs.active_tab()
        && let Some(webview) = app.get_webview(&tab.label)
    {
        let _ = webview.reload();
    }
    Ok(tabs.snapshot())
}

pub fn stop_loading(app: &AppHandle) -> Result<BrowserState, String> {
    let state = app.state::<AppState>();
    let mut tabs = lock(&state.tabs);
    let active_id = tabs.active.clone();
    if let Some(tab) = active_id.and_then(|id| tabs.items.iter_mut().find(|tab| tab.id == id)) {
        if let Some(webview) = app.get_webview(&tab.label) {
            let _ = webview.eval("window.stop();");
        }
        tab.loading = false;
    }
    let snapshot = tabs.snapshot();
    persist_session(app, &tabs);
    drop(tabs);
    let _ = app.emit_to("shell", "browser-state", snapshot.clone());
    Ok(snapshot)
}

pub fn reopen_closed_tab(app: &AppHandle) -> Result<BrowserState, String> {
    let entry = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        tabs.closed.pop()
    };
    match entry {
        Some(entry) => open_tab(app, &entry.url),
        None => Err("닫은 탭이 없습니다.".into()),
    }
}

fn apply_zoom(webview: &Webview, zoom: f64) -> Result<(), String> {
    webview
        .with_webview(move |platform| {
            let result = (|| -> windows::core::Result<()> {
                // SAFETY: called on the UI thread with the live controller provided by Tauri.
                unsafe { platform.controller().SetZoomFactor(zoom) }
            })();
            if let Err(error) = result {
                eprintln!("확대/축소를 적용하지 못했습니다: {error}");
            }
        })
        .map_err(|error| format!("확대/축소를 적용하지 못했습니다: {error}"))
}

pub fn set_zoom(app: &AppHandle, factor: f64) -> Result<BrowserState, String> {
    let zoom = clamp_zoom(factor);
    let (label, snapshot) = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        let active_id = tabs.active.clone();
        let Some(tab) = active_id.and_then(|id| tabs.items.iter_mut().find(|tab| tab.id == id))
        else {
            return Err("활성 탭이 없습니다.".into());
        };
        tab.zoom = zoom;
        (tab.label.clone(), tabs.snapshot())
    };

    if let Some(webview) = app.get_webview(&label) {
        apply_zoom(&webview, zoom)?;
    }

    let _ = app.emit_to("shell", "browser-state", snapshot.clone());
    Ok(snapshot)
}

pub fn print_active(app: &AppHandle) -> Result<(), String> {
    active_webview(app)?
        .eval("window.print()")
        .map_err(|error| format!("인쇄 대화상자를 열지 못했습니다: {error}"))
}

pub async fn find_in_page(
    app: &AppHandle,
    query: &str,
    action: &str,
) -> Result<FindResult, String> {
    let action = match action {
        "find" | "next" | "prev" | "clear" => action,
        _ => return Err("지원하지 않는 페이지 찾기 동작입니다.".into()),
    };
    let webview = active_webview(app)?;
    let script = format!(
        "({})({}, {})",
        FIND_IN_PAGE_SCRIPT,
        serde_json::to_string(query).map_err(|error| error.to_string())?,
        serde_json::to_string(action).map_err(|error| error.to_string())?
    );
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = std::sync::Mutex::new(Some(sender));
    webview
        .eval_with_callback(&script, move |result| {
            if let Some(sender) = sender
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .take()
            {
                let _ = sender.send(result);
            }
        })
        .map_err(|error| format!("페이지 찾기 스크립트를 실행하지 못했습니다: {error}"))?;
    let raw = tokio::time::timeout(Duration::from_secs(5), receiver)
        .await
        .map_err(|_| "페이지 찾기 응답 시간이 초과되었습니다.".to_string())?
        .map_err(|_| "페이지 찾기 결과를 받지 못했습니다.".to_string())?;
    let mut value = serde_json::from_str::<serde_json::Value>(&raw)
        .unwrap_or_else(|_| serde_json::Value::String(raw));
    if let Some(encoded) = value.as_str() {
        value = serde_json::from_str(encoded).unwrap_or(value);
    }
    serde_json::from_value::<FindResult>(value)
        .map_err(|error| format!("페이지 찾기 결과를 해석하지 못했습니다: {error}"))
}

pub fn toggle_fullscreen(app: &AppHandle) -> Result<bool, String> {
    let window = window(app)?;
    let fullscreen = window
        .is_fullscreen()
        .map_err(|error| format!("전체 화면 상태를 읽지 못했습니다: {error}"))?;
    window
        .set_fullscreen(!fullscreen)
        .map_err(|error| format!("전체 화면 전환에 실패했습니다: {error}"))?;
    Ok(!fullscreen)
}

pub fn window_action(app: &AppHandle, action: &str) -> Result<(), String> {
    let window = window(app)?;
    match action {
        "minimize" => window
            .minimize()
            .map_err(|error| format!("창을 최소화하지 못했습니다: {error}")),
        "toggle-maximize" => {
            let maximized = window
                .is_maximized()
                .map_err(|error| format!("창 상태를 읽지 못했습니다: {error}"))?;
            if maximized {
                window
                    .unmaximize()
                    .map_err(|error| format!("창 크기를 복원하지 못했습니다: {error}"))
            } else {
                window
                    .maximize()
                    .map_err(|error| format!("창을 최대화하지 못했습니다: {error}"))
            }
        }
        "close" => window
            .close()
            .map_err(|error| format!("창을 닫지 못했습니다: {error}")),
        "start-dragging" => window
            .start_dragging()
            .map_err(|error| format!("창을 이동하지 못했습니다: {error}")),
        _ => Err("알 수 없는 창 동작입니다.".into()),
    }
}

pub fn set_window_title(app: &AppHandle, title: &str) -> Result<(), String> {
    let window = window(app)?;
    let ui_language = lock(&app.state::<AppState>().settings).ui_language.clone();
    let base_title = default_window_title(&ui_language);
    let trimmed = title.trim();
    let next_title = if trimmed.is_empty() {
        base_title.to_string()
    } else {
        format!("{trimmed} — {base_title}")
    };
    window
        .set_title(&next_title)
        .map_err(|error| format!("창 이름을 변경하지 못했습니다: {error}"))
}

pub fn sync_ui_language_to_tabs(app: &AppHandle, ui_language: &str) -> Result<(), String> {
    let state = app.state::<AppState>();
    let (labels, snapshot, changed) = {
        let mut tabs = lock(&state.tabs);
        let mut changed = false;
        for tab in &mut tabs.items {
            if let Some(kind) = tab.url.strip_prefix("tb://")
                && let Some(new_title) = internal_page_title(kind, ui_language)
                && tab.title != new_title
            {
                tab.title = new_title.to_string();
                changed = true;
            }
        }
        let labels: Vec<String> = tabs.items.iter().map(|tab| tab.label.clone()).collect();
        (labels, tabs.snapshot(), changed)
    };
    if changed {
        let _ = app.emit_to("shell", "browser-state", snapshot);
    }
    let lang_json = serde_json::to_string(ui_language).unwrap_or_else(|_| "\"ko\"".into());
    let script = format!("window.__tbSetUiLang?.({lang_json});");
    for label in labels {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.eval(&script);
        }
    }
    if let Some(win) = app.get_window("main")
        && let Ok(current_title) = win.title()
        && matches!(
            current_title.as_str(),
            "Fluo" | "번역 브라우저" | "Translate Browser" | "翻訳ブラウザ" | "翻译浏览器"
        )
    {
        let _ = win.set_title(default_window_title(ui_language));
    }
    Ok(())
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct OverlayAnchor {
    pub left: f64,
    pub top: f64,
    pub right: f64,
    pub bottom: f64,
}

pub fn open_overlay(
    app: &AppHandle,
    kind: &str,
    anchor: Option<OverlayAnchor>,
    tab_id: Option<String>,
) -> Result<(), String> {
    let kind = match kind {
        "menu" | "adblock" | "site-security" | "bookmark-edit" | "nav-history" | "translate"
        | "workspace" | "tab-menu" | "chrome-menu" | "page-menu" | "tab-search"
        | "task-manager" | "bookmarks" | "glossary" | "history" | "visits" | "downloads"
        | "settings" | "feedback" | "rename-window" | "export-project" | "onboarding" => kind,
        _ => return Err("알 수 없는 브라우저 오버레이입니다.".into()),
    };

    let chrome_top = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        tabs.overlay_open = true;
        tabs.overlay_blur = !matches!(kind, "chrome-menu" | "page-menu" | "tab-menu");
        tabs.top
    };
    sync_shell_bounds(app)?;
    let anchor_json = serde_json::to_string(&anchor).map_err(|error| error.to_string())?;
    let tab_id_json = serde_json::to_string(&tab_id).map_err(|error| error.to_string())?;
    let script = format!(
        "window.__SET_APP_OVERLAY_KIND__?.({}, {}, {}, {});",
        serde_json::to_string(kind).map_err(|error| error.to_string())?,
        anchor_json,
        tab_id_json,
        chrome_top
    );
    if let Some(shell) = app.get_webview("shell") {
        shell.eval(script).map_err(|error| error.to_string())?;
        let _ = shell.set_focus();
    }
    Ok(())
}

pub fn close_overlay(app: &AppHandle) -> Result<(), String> {
    {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        tabs.overlay_open = false;
        tabs.overlay_blur = false;
    }
    let active_webview = active_webview(app).ok();
    if let Some(shell) = app.get_webview("shell") {
        let _ = shell.eval("window.__CLEAR_APP_OVERLAY__?.();");
    }
    let _ = sync_shell_bounds(app);
    if let Some(webview) = active_webview {
        let _ = webview.set_focus();
        let _ = sync_shell_bounds(app);
    }
    let _ = app.emit_to("shell", "overlay-closed", ());
    Ok(())
}

pub fn open_webview_devtools(app: &AppHandle) {
    let Ok(webview) = active_webview(app) else {
        return;
    };
    #[cfg(windows)]
    {
        let _ = webview.with_webview(|platform| unsafe {
            if let Ok(core) = platform.controller().CoreWebView2() {
                let _ = core.OpenDevToolsWindow();
            }
        });
    }
}

pub fn run_page_context_action(
    app: &AppHandle,
    action: &str,
    text: Option<String>,
    x: Option<f64>,
    y: Option<f64>,
) -> Result<(), String> {
    if action == "inspect" {
        open_webview_devtools(app);
        return Ok(());
    }
    let webview = active_webview(app)?;
    if let Some(tts_cmd) = action.strip_prefix("tts-") {
        let rate_num = text
            .as_deref()
            .and_then(|s| s.parse::<f64>().ok())
            .unwrap_or(1.0);
        let cmd_json = serde_json::to_string(tts_cmd).map_err(|e| e.to_string())?;
        return webview
            .eval(format!("window.__tbTtsControl?.({cmd_json}, {rate_num});"))
            .map_err(|error| error.to_string());
    }
    let action_json = serde_json::to_string(action).map_err(|e| e.to_string())?;
    let text_json = serde_json::to_string(&text).map_err(|e| e.to_string())?;
    let x_json = serde_json::to_string(&x).map_err(|e| e.to_string())?;
    let y_json = serde_json::to_string(&y).map_err(|e| e.to_string())?;
    webview
        .eval(format!(
            "window.__tbRunContextAction?.({action_json}, {text_json}, {x_json}, {y_json});"
        ))
        .map_err(|error| error.to_string())
}

pub fn sync_ui_theme_to_tabs(
    app: &AppHandle,
    is_dark: Option<bool>,
    accent: &str,
    tinted: bool,
) -> Result<(), String> {
    let labels: Vec<String> = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        tabs.ui_is_dark = is_dark;
        tabs.ui_accent = accent.to_string();
        tabs.ui_tinted = tinted;
        tabs.items.iter().map(|tab| tab.label.clone()).collect()
    };
    let resolved_dark = effective_is_dark(app);
    let bg_color = theme_bg_color(resolved_dark);
    if let Some(main_win) = app.get_window("main") {
        let _ = main_win.set_background_color(Some(bg_color));
    }
    let payload = serde_json::json!({
        "isDark": is_dark,
        "accent": accent,
        "tinted": tinted,
    });
    let script = format!("window.__tbSetUiTheme?.({payload});");
    for label in labels {
        if let Some(webview) = app.get_webview(&label) {
            let _ = webview.set_background_color(Some(bg_color));
            let _ = webview.eval(&script);
        }
    }
    Ok(())
}

pub fn eval_on_active_tab(app: &AppHandle, script: &str) -> Result<(), String> {
    let webview = active_webview(app)?;
    webview.eval(script).map_err(|error| error.to_string())
}

pub fn layout(app: &AppHandle, top: f64) -> Result<(), String> {
    let top = top.clamp(38.0, 360.0);
    let active_label = {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        tabs.top = top;
        tabs.active_tab().map(|tab| tab.label.clone())
    };
    if let Some(label) = active_label
        && let Some(webview) = app.get_webview(&label)
    {
        let _ = webview.eval(format!("window.__tbSetTopBarHeight?.({top:.1});"));
    }
    let _ = sync_shell_bounds(app);
    Ok(())
}

pub fn active_webview(app: &AppHandle) -> Result<Webview, String> {
    let state = app.state::<AppState>();
    let tabs = lock(&state.tabs);
    let tab = tabs
        .active_tab()
        .ok_or_else(|| "활성 탭이 없습니다.".to_string())?;
    app.get_webview(&tab.label)
        .ok_or_else(|| "브라우저 WebView를 찾지 못했습니다.".to_string())
}

fn webview_for_page(app: &AppHandle, tab_id: &str, page_url: &str) -> Result<Webview, String> {
    let state = app.state::<AppState>();
    let tabs = lock(&state.tabs);
    let tab = tabs
        .active_tab()
        .ok_or_else(|| "활성 탭이 없습니다.".to_string())?;
    if tab.id != tab_id || tab.url != page_url {
        return Err("페이지가 변경되어 현재 번역을 적용하지 않았습니다.".into());
    }
    app.get_webview(&tab.label)
        .ok_or_else(|| "브라우저 WebView를 찾지 못했습니다.".to_string())
}

async fn evaluate_count(webview: &Webview, script: &str) -> Result<usize, String> {
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = std::sync::Mutex::new(Some(sender));
    webview
        .eval_with_callback(script, move |result| {
            if let Some(sender) = sender
                .lock()
                .unwrap_or_else(|error| error.into_inner())
                .take()
            {
                let _ = sender.send(result);
            }
        })
        .map_err(|error| format!("페이지 번역 스크립트를 실행하지 못했습니다: {error}"))?;
    let raw = tokio::time::timeout(Duration::from_secs(10), receiver)
        .await
        .map_err(|_| "페이지 번역 스크립트 응답 시간이 초과되었습니다.".to_string())?
        .map_err(|_| "페이지 번역 스크립트 결과를 받지 못했습니다.".to_string())?;

    let value = serde_json::from_str::<serde_json::Value>(&raw)
        .unwrap_or_else(|_| serde_json::Value::String(raw));
    let value = if let Some(encoded) = value.as_str() {
        serde_json::from_str::<serde_json::Value>(encoded).unwrap_or(value)
    } else {
        value
    };
    value
        .as_u64()
        .map(|count| count as usize)
        .ok_or_else(|| "페이지 번역 스크립트 결과가 올바르지 않습니다.".into())
}

pub async fn sync_reading_position(app: &AppHandle) -> Result<(), String> {
    let (tab_id, label, page_url) = {
        let state = app.state::<AppState>();
        let tabs = lock(&state.tabs);
        let tab = tabs
            .active_tab()
            .ok_or_else(|| "활성 탭이 없습니다.".to_string())?;
        (tab.id.clone(), tab.label.clone(), tab.url.clone())
    };
    let webview = app
        .get_webview(&label)
        .ok_or_else(|| "브라우저 WebView를 찾지 못했습니다.".to_string())?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = std::sync::Mutex::new(Some(sender));
    webview
        .eval_with_callback(
            "JSON.stringify({x:window.scrollX||0,y:window.scrollY||0})",
            move |result| {
                if let Some(sender) = sender
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take()
                {
                    let _ = sender.send(result);
                }
            },
        )
        .map_err(|error| format!("읽기 위치를 읽지 못했습니다: {error}"))?;
    let raw = tokio::time::timeout(Duration::from_secs(3), receiver)
        .await
        .map_err(|_| "읽기 위치 조회 시간이 초과되었습니다.".to_string())?
        .map_err(|_| "읽기 위치 결과를 받지 못했습니다.".to_string())?;
    let mut value = serde_json::from_str::<serde_json::Value>(&raw)
        .unwrap_or_else(|_| serde_json::Value::String(raw));
    if let Some(encoded) = value.as_str() {
        value = serde_json::from_str(encoded).unwrap_or(value);
    }
    let position = |key: &str| {
        value
            .get(key)
            .and_then(serde_json::Value::as_f64)
            .filter(|value| value.is_finite() && *value >= 0.0)
            .unwrap_or(0.0)
    };
    let (scroll_x, scroll_y) = (position("x"), position("y"));

    let state = app.state::<AppState>();
    let mut tabs = lock(&state.tabs);
    let Some(tab) = tabs.items.iter_mut().find(|tab| tab.id == tab_id) else {
        return Ok(());
    };
    if tab.url != page_url
        || ((tab.scroll_x - scroll_x).abs() < 1.0 && (tab.scroll_y - scroll_y).abs() < 1.0)
    {
        return Ok(());
    }
    tab.scroll_x = scroll_x;
    tab.scroll_y = scroll_y;
    if let Some(entry) = tab.history.get_mut(tab.cursor)
        && entry.url == tab.url
    {
        entry.scroll_x = scroll_x;
        entry.scroll_y = scroll_y;
    }
    persist_session(app, &tabs);
    Ok(())
}

pub async fn apply_inline_translations(
    app: &AppHandle,
    tab_id: &str,
    page_url: &str,
    translations: &[InlineTranslation],
    bilingual: bool,
) -> Result<usize, String> {
    if translations.is_empty() {
        return Ok(0);
    }
    let webview = webview_for_page(app, tab_id, page_url)?;
    let (font_scale, line_height, ui_language) = {
        let state = app.state::<AppState>();
        let settings = lock(&state.settings);
        (
            settings.translation_font_scale,
            settings.translation_line_height,
            settings.ui_language.clone(),
        )
    };
    let payload = serde_json::to_string(translations)
        .map_err(|error| format!("페이지 번역 데이터를 만들지 못했습니다: {error}"))?;
    let ui_lang_json = serde_json::to_string(&ui_language).unwrap_or_else(|_| "\"ko\"".into());
    let script = INLINE_TRANSLATE_SCRIPT
        .replace("__PAYLOAD__", &payload)
        .replace("__FONT_SCALE__", &format!("{font_scale:.2}"))
        .replace("__LINE_HEIGHT__", &format!("{line_height:.2}"))
        .replace("__BILINGUAL__", if bilingual { "true" } else { "false" })
        .replace("__UI_LANG__", &ui_lang_json);
    let count = evaluate_count(&webview, &script).await?;
    let _ = webview.eval("window.__tbSetDynamicTranslate?.(true);");
    Ok(count)
}

pub async fn pick_region_article(app: &AppHandle) -> Result<Article, String> {
    let (tab_id, webview) = {
        let state = app.state::<AppState>();
        let tabs = lock(&state.tabs);
        let tab = tabs
            .active_tab()
            .ok_or_else(|| "활성 탭이 없습니다.".to_string())?;
        let webview = app
            .get_webview(&tab.label)
            .ok_or_else(|| "브라우저 WebView를 찾지 못했습니다.".to_string())?;
        (tab.id.clone(), webview)
    };

    let _ = webview.set_focus();
    webview
        .eval(REGION_PICKER_SCRIPT)
        .map_err(|error| format!("영역 선택 모드를 시작하지 못했습니다: {error}"))?;

    for _ in 0..400 {
        tokio::time::sleep(Duration::from_millis(150)).await;
        let still_active = {
            let state = app.state::<AppState>();
            let tabs = lock(&state.tabs);
            tabs.active.as_deref() == Some(tab_id.as_str())
        };
        if !still_active {
            let _ = webview.eval("window.__tbRegionPickerCleanup?.();");
            return Err("탭이 변경되어 영역 선택을 취소했습니다.".into());
        }
        let (sender, receiver) = tokio::sync::oneshot::channel();
        let sender = std::sync::Mutex::new(Some(sender));
        if webview
            .eval_with_callback("window.__tbRegionPick?.status || 'idle'", move |result| {
                if let Some(sender) = sender
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take()
                {
                    let _ = sender.send(result);
                }
            })
            .is_err()
        {
            return Err("영역 선택 상태를 확인하지 못했습니다.".into());
        }
        let Ok(Ok(raw)) = tokio::time::timeout(Duration::from_secs(2), receiver).await else {
            continue;
        };
        let status = serde_json::from_str::<String>(&raw).unwrap_or(raw);
        match status.as_str() {
            "picked" => return extract_article(app).await,
            "cancelled" => return Err("영역 선택을 취소했습니다.".into()),
            _ => continue,
        }
    }
    let _ = webview.eval("window.__tbRegionPickerCleanup?.();");
    Err("영역 선택 시간이 초과되었습니다.".into())
}

pub fn cancel_region_picker(app: &AppHandle) -> Result<(), String> {
    if let Ok(webview) = active_webview(app) {
        let _ = webview.eval(
            "window.__tbRegionPick = { status: 'cancelled' }; window.__tbRegionPickerCleanup?.();",
        );
    }
    Ok(())
}

pub async fn poll_page_action(app: &AppHandle) -> Result<Option<serde_json::Value>, String> {
    let webview = active_webview(app)?;
    let (sender, receiver) = tokio::sync::oneshot::channel();
    let sender = std::sync::Mutex::new(Some(sender));
    webview
        .eval_with_callback(
            "(() => { const a = window.__tbPendingAction || null; window.__tbPendingAction = null; if (a) return JSON.stringify(a); const meta = window.__tbReadPageMeta ? window.__tbReadPageMeta() : null; return JSON.stringify(meta ? { type: 'page-meta', ...meta } : null); })()",
            move |result| {
                if let Some(sender) = sender
                    .lock()
                    .unwrap_or_else(|error| error.into_inner())
                    .take()
                {
                    let _ = sender.send(result);
                }
            },
        )
        .map_err(|error| error.to_string())?;
    let raw = tokio::time::timeout(Duration::from_secs(2), receiver)
        .await
        .map_err(|_| "페이지 작업 조회 시간이 초과되었습니다.".to_string())?
        .map_err(|_| "페이지 작업 결과를 받지 못했습니다.".to_string())?;
    let mut value =
        serde_json::from_str::<serde_json::Value>(&raw).unwrap_or(serde_json::Value::Null);
    if let Some(encoded) = value.as_str() {
        value = serde_json::from_str(encoded).unwrap_or(serde_json::Value::Null);
    }
    if value.is_null() {
        Ok(None)
    } else {
        Ok(Some(value))
    }
}

pub fn show_quick_translation(app: &AppHandle, text: &str, is_error: bool) -> Result<(), String> {
    let webview = active_webview(app)?;
    let script = format!(
        "window.__tbShowQuickResult?.({}, {});",
        serde_json::to_string(text).map_err(|error| error.to_string())?,
        if is_error { "true" } else { "false" }
    );
    webview.eval(&script).map_err(|error| error.to_string())
}

pub async fn restore_inline_translations(
    app: &AppHandle,
    tab_id: &str,
    _page_url: &str,
) -> Result<usize, String> {
    let webview = {
        let state = app.state::<AppState>();
        let tabs = lock(&state.tabs);
        let tab = tabs
            .items
            .iter()
            .find(|tab| tab.id == tab_id)
            .ok_or_else(|| "복원할 탭을 찾지 못했습니다.".to_string())?;
        app.get_webview(&tab.label)
            .ok_or_else(|| "브라우저 WebView를 찾지 못했습니다.".to_string())?
    };
    let _ = webview.eval("window.__tbSetDynamicTranslate?.(false);");
    evaluate_count(&webview, INLINE_RESTORE_SCRIPT).await
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ViewportOrder {
    pub seq: u64,
    pub keys: Vec<String>,
}

pub async fn get_viewport_inline_order(app: &AppHandle) -> Result<ViewportOrder, String> {
    let webview = active_webview(app)?;
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
    webview
        .eval_with_callback(
            "(() => (window.__tbGetViewportOrder ? window.__tbGetViewportOrder() : { seq: 0, keys: [] }))()",
            move |result| {
                let _ = sender.send(result);
            },
        )
        .map_err(|error| format!("뷰포트 순서 조회 실패: {error}"))?;
    let raw = tokio::time::timeout(Duration::from_secs(4), receiver.recv())
        .await
        .map_err(|_| "뷰포트 순서 조회 시간이 초과되었습니다.".to_string())?
        .ok_or_else(|| "뷰포트 순서 응답을 받지 못했습니다.".to_string())?;
    let mut val: serde_json::Value = serde_json::from_str(&raw).unwrap_or(serde_json::Value::Null);
    if let Some(s) = val.as_str() {
        val = serde_json::from_str(s).unwrap_or(serde_json::Value::Null);
    }
    Ok(serde_json::from_value(val).unwrap_or_default())
}

pub async fn extract_article(app: &AppHandle) -> Result<Article, String> {
    let webview = active_webview(app)?;
    let (sender, mut receiver) = tokio::sync::mpsc::unbounded_channel::<String>();
    webview
        .eval_with_callback(EXTRACT_SCRIPT, move |result| {
            let _ = sender.send(result);
        })
        .map_err(|error| format!("본문 추출 스크립트를 실행하지 못했습니다: {error}"))?;

    let raw = tokio::time::timeout(Duration::from_secs(20), receiver.recv())
        .await
        .map_err(|_| "본문 추출 시간이 초과되었습니다.".to_string())?
        .ok_or_else(|| "본문 추출 결과를 받지 못했습니다.".to_string())?;

    let article = parse_article(&raw)?;

    {
        let state = app.state::<AppState>();
        let mut tabs = lock(&state.tabs);
        let active_id = tabs.active.clone();
        if let Some(tab) = tabs
            .items
            .iter_mut()
            .find(|tab| Some(&tab.id) == active_id.as_ref())
        {
            tab.url = article.url.clone();
            if !article.title.is_empty() {
                tab.title = article.title.clone();
            }
        }
        let snapshot = tabs.snapshot();
        persist_session(app, &tabs);
        drop(tabs);
        let _ = app.emit_to("shell", "browser-state", snapshot);

        if let Ok(page_key) = text::normalize_page_url(&article.url) {
            let page_id = text::page_id(&page_key);
            let json = serde_json::to_string(&article)
                .map_err(|error| format!("본문 스냅샷을 만들지 못했습니다: {error}"))?;
            let db = lock(&state.db);
            crate::db::save_snapshot(&db, &page_id, &article.url, &article.title, &json)?;
        }
    }

    Ok(article)
}

#[derive(serde::Deserialize)]
struct RawArticle {
    #[serde(default)]
    error: Option<String>,
    #[serde(default)]
    title: String,
    #[serde(default)]
    url: String,
    #[serde(default)]
    site: String,
    #[serde(default)]
    lang: Option<String>,
    #[serde(default)]
    blocks: Vec<RawBlock>,
}

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawBlock {
    #[serde(default)]
    kind: String,
    #[serde(default)]
    doc_index: Option<usize>,
    #[serde(default)]
    level: Option<u8>,
    #[serde(default)]
    inline_key: Option<String>,
    #[serde(default)]
    text: String,
    #[serde(default)]
    rows: Option<Vec<Vec<String>>>,
}

fn parse_article(raw: &str) -> Result<Article, String> {
    let mut value: serde_json::Value = serde_json::from_str(raw)
        .map_err(|error| format!("본문 추출 결과를 해석하지 못했습니다: {error}"))?;
    // `eval_with_callback` serializes the JS result once; tolerate an extra string layer.
    if let Some(text) = value.as_str() {
        value = serde_json::from_str(text)
            .map_err(|error| format!("본문 추출 결과를 해석하지 못했습니다: {error}"))?;
    }
    let raw_article: RawArticle = serde_json::from_value(value)
        .map_err(|error| format!("본문 추출 형식이 올바르지 않습니다: {error}"))?;
    if let Some(error) = raw_article.error {
        return Err(format!("페이지에서 본문을 추출하지 못했습니다: {error}"));
    }

    let mut blocks = Vec::new();
    for (index, block) in raw_article.blocks.into_iter().enumerate() {
        if block.text.trim().is_empty() {
            continue;
        }
        let id = text::paragraph_id(&block.text, index);
        blocks.push(Block {
            id,
            kind: block.kind,
            doc_index: block.doc_index,
            inline_key: block.inline_key,
            level: block.level,
            text: block.text,
            rows: block.rows,
        });
        if blocks.len() >= 1500 {
            break;
        }
    }

    if blocks.is_empty() {
        return Err("본문으로 사용할 수 있는 문단을 찾지 못했습니다.".into());
    }

    Ok(Article {
        title: if raw_article.title.is_empty() {
            raw_article.site.clone()
        } else {
            raw_article.title
        },
        url: raw_article.url,
        site: raw_article.site,
        lang: raw_article.lang,
        blocks,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registering_tabs_keeps_state_and_session_entries_unique() {
        let mut tabs = Tabs::default();
        let first = tabs.register_tab("https://one.example/");
        let second = tabs.register_tab("https://two.example/");
        tabs.active = Some(second.id.clone());

        assert_eq!(tabs.items.len(), 2);
        assert_eq!(
            tabs.items.iter().filter(|tab| tab.id == first.id).count(),
            1
        );
        assert_eq!(
            tabs.items.iter().filter(|tab| tab.id == second.id).count(),
            1
        );
        let state = tabs.snapshot();
        assert_eq!(state.tabs.len(), 2);
        assert_eq!(state.active_tab.as_deref(), Some(second.id.as_str()));
    }

    #[test]
    fn stale_page_finish_does_not_clear_current_navigation_loading() {
        let mut tab = Tab::new(1, "https://new.example/");

        assert!(!finish_page_load(&mut tab, "https://old.example/"));
        assert!(tab.loading);

        assert!(finish_page_load(&mut tab, "https://new.example/"));
        assert!(!tab.loading);
    }

    #[test]
    fn moves_a_tab_only_to_an_existing_workspace() {
        let mut tabs = Tabs::default();
        tabs.workspaces.push(Workspace {
            id: "research".into(),
            name: "Research".into(),
        });
        tabs.items.push(Tab::new(1, "https://example.org/"));

        assign_tab_workspace(&mut tabs, "1", "research").unwrap();
        assert_eq!(tabs.items[0].workspace_id, "research");
        assert!(assign_tab_workspace(&mut tabs, "1", "missing").is_err());
    }

    #[test]
    fn closed_tabs_are_bounded_and_pop_newest_first() {
        let mut closed = ClosedTabs::default();
        for index in 0..(MAX_CLOSED_TABS + 5) {
            closed.push(format!("https://example.org/{index}"), String::new());
        }
        assert_eq!(closed.len(), MAX_CLOSED_TABS);
        assert_eq!(
            closed.pop().map(|entry| entry.url),
            Some(format!("https://example.org/{}", MAX_CLOSED_TABS + 4))
        );
        closed.push(String::new(), "blank".into());
        assert_eq!(closed.len(), MAX_CLOSED_TABS - 1);
        closed.push("https://example.org/latest".into(), String::new());
        assert_eq!(closed.len(), MAX_CLOSED_TABS);
    }

    #[test]
    fn zoom_is_clamped_to_browser_like_bounds() {
        assert_eq!(clamp_zoom(1.0), 1.0);
        assert_eq!(clamp_zoom(0.0), MIN_ZOOM);
        assert_eq!(clamp_zoom(10.0), MAX_ZOOM);
        assert_eq!(clamp_zoom(f64::NAN), 1.0);
    }

    #[test]
    fn tabs_can_be_reordered_before_another_tab_or_to_the_end() {
        let mut tabs = Tabs::default();
        let first = tabs.register_tab("https://one.example/");
        let second = tabs.register_tab("https://two.example/");
        let third = tabs.register_tab("https://three.example/");

        assert!(reorder_tabs_in(&mut tabs, &third.id, Some(&first.id)));
        let order: Vec<_> = tabs.items.iter().map(|tab| tab.id.clone()).collect();
        assert_eq!(
            order,
            vec![third.id.clone(), first.id.clone(), second.id.clone()]
        );

        assert!(reorder_tabs_in(&mut tabs, &third.id, None));
        let order: Vec<_> = tabs.items.iter().map(|tab| tab.id.clone()).collect();
        assert_eq!(
            order,
            vec![first.id.clone(), second.id.clone(), third.id.clone()]
        );

        assert!(!reorder_tabs_in(&mut tabs, "missing", None));
    }

    #[test]
    fn workspace_create_rename_switch_and_delete_preserve_invariants() {
        let mut tabs = Tabs::default();
        let ws1 = create_workspace_in(&mut tabs, "Research", 100).unwrap();
        assert_eq!(tabs.active_workspace, ws1);
        assert_eq!(tabs.workspaces.len(), 2);

        // Rejects empty & case-insensitive duplicates.
        assert!(create_workspace_in(&mut tabs, "   ", 101).is_err());
        assert!(create_workspace_in(&mut tabs, "research", 102).is_err());

        // Truncates long names cleanly to 80 chars.
        let long_name = "a".repeat(100);
        let ws2 = create_workspace_in(&mut tabs, &long_name, 103).unwrap();
        assert_eq!(
            tabs.workspaces
                .iter()
                .find(|w| w.id == ws2)
                .unwrap()
                .name
                .chars()
                .count(),
            80
        );

        // Add tabs to default and ws1.
        tabs.active_workspace = "default".into();
        let t_def = tabs.register_tab("https://default.example/");
        tabs.active_workspace = ws1.clone();
        let t_ws1 = tabs.register_tab("https://ws1.example/");

        // Rename validates conflicts and updates name.
        assert!(rename_workspace_in(&mut tabs, &ws1, "   ").is_err());
        assert!(rename_workspace_in(&mut tabs, "missing", "Other").is_err());
        rename_workspace_in(&mut tabs, &ws1, "Wiki Reading").unwrap();
        assert_eq!(
            tabs.workspaces.iter().find(|w| w.id == ws1).unwrap().name,
            "Wiki Reading"
        );

        // Switching picks latest tab in workspace, or None if empty.
        assert_eq!(
            switch_workspace_in(&mut tabs, &ws1).unwrap(),
            Some(t_ws1.id.clone())
        );
        assert_eq!(switch_workspace_in(&mut tabs, &ws2).unwrap(), None);
        assert!(switch_workspace_in(&mut tabs, "missing").is_err());

        // Deleting default is forbidden.
        assert!(delete_workspace_in(&mut tabs, "default").is_err());

        // Deleting non-active ws1 moves its tabs to default while keeping ws2 active.
        let (act, create_tab, was_active, _) = delete_workspace_in(&mut tabs, &ws1).unwrap();
        assert!(!was_active);
        assert!(!create_tab);
        assert!(act.is_none());
        assert_eq!(tabs.active_workspace, ws2);
        assert_eq!(
            tabs.items
                .iter()
                .find(|t| t.id == t_ws1.id)
                .unwrap()
                .workspace_id,
            "default"
        );

        // Deleting active ws2 switches back to default and selects latest default tab.
        let (act2, create_tab2, was_active2, _) = delete_workspace_in(&mut tabs, &ws2).unwrap();
        assert!(was_active2);
        assert!(!create_tab2);
        assert_eq!(tabs.active_workspace, "default");
        assert!(act2 == Some(t_ws1.id) || act2 == Some(t_def.id));
    }

    #[test]
    fn workspace_snapshot_loads_valid_and_ignores_corrupted() {
        let mut tabs = Tabs::default();
        let valid = r#"{"workspaces":[{"id":"default","name":"기본"},{"id":"ws-1","name":"Work"}],"activeWorkspace":"ws-1"}"#;
        load_workspaces(&mut tabs, Some(valid));
        assert_eq!(tabs.workspaces.len(), 2);
        assert_eq!(tabs.active_workspace, "ws-1");

        // Missing default workspace should be ignored cleanly.
        let invalid = r#"{"workspaces":[{"id":"ws-2","name":"Only"}],"activeWorkspace":"ws-2"}"#;
        load_workspaces(&mut tabs, Some(invalid));
        assert_eq!(tabs.workspaces.len(), 2);
        assert_eq!(tabs.active_workspace, "ws-1");
    }
}
