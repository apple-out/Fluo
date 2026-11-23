import { Channel, invoke } from "@tauri-apps/api/core";

import type {
  AddressSuggestion,
  AppSettings,
  Article,
  AutofillProfile,
  Bookmark,
  BrowserState,
  CloudAccountRoot,
  ConnectionTest,
  CookieEntry,
  DetectedBrowserProfile,
  DownloadItem,
  ExternalImportSummary,
  FeedbackConfigStatus,
  FeedbackReportInput,
  FeedbackSubmitResult,
  FindResult,
  GlossaryEntry,
  HardwareProfile,
  HistoryItem,
  HyMt2ModelState,
  ImportSummary,
  ModelDownloadProgress,
  ModelInfo,
  SavedLogin,
  SearchHistoryItem,
  SegmentInput,
  SegmentResult,
  SettingsView,
  SyncConfig,
  TranslationContextPair,
  TranslationHistory,
  TranslationStreamEvent,
  UserScriptItem,
  ViewportOrder,
  WebExtensionInfo,
} from "./types";

export interface OverlayAnchor {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

export const api = {
  browserState: () => invoke<BrowserState>("browser_state"),
  newTab: (url?: string) =>
    invoke<BrowserState>("new_tab", { url: url ?? null }),
  newPrivateTab: (url?: string) =>
    invoke<BrowserState>("new_private_tab", { url: url ?? null }),
  closeTab: (tabId: string) => invoke<BrowserState>("close_tab", { tabId }),
  pinTab: (tabId: string) => invoke<BrowserState>("pin_tab", { tabId }),
  toggleMuteTab: (tabId: string) => invoke<BrowserState>("toggle_mute_tab", { tabId }),
  sleepTab: (tabId: string) => invoke<BrowserState>("sleep_tab", { tabId }),
  setWindowTitle: (title: string) => invoke<void>("set_window_title", { title }),
  closeOtherTabs: (tabId: string) => invoke<BrowserState>("close_other_tabs", { tabId }),
  closeTabsToRight: (tabId: string) => invoke<BrowserState>("close_tabs_to_right", { tabId }),
  selectTab: (tabId: string) => invoke<BrowserState>("select_tab", { tabId }),
  reorderTab: (tabId: string, beforeId: string | null) =>
    invoke<BrowserState>("reorder_tab", { tabId, beforeId }),
  moveTabToWorkspace: (tabId: string, workspaceId: string) =>
    invoke<BrowserState>("move_tab_to_workspace", { tabId, workspaceId }),
  navigate: (input: string) => invoke<BrowserState>("navigate", { input }),
  goBack: () => invoke<BrowserState>("go_back"),
  goForward: () => invoke<BrowserState>("go_forward"),
  jumpHistoryDelta: (delta: number) => invoke<BrowserState>("jump_history_delta", { delta }),
  activeTabHistory: () => invoke<[number, string[]]>("active_tab_history"),
  reload: () => invoke<BrowserState>("reload"),
  stopLoading: () => invoke<BrowserState>("stop_loading"),
  syncReadingPosition: () => invoke<void>("sync_reading_position"),
  openOverlay: (
    kind:
      | "menu"
      | "adblock"
      | "site-security"
      | "bookmark-edit"
      | "nav-history"
      | "translate"
      | "workspace"
      | "tab-menu"
      | "chrome-menu"
      | "page-menu"
      | "tab-search"
      | "task-manager"
      | "bookmarks"
      | "glossary"
      | "history"
      | "visits"
      | "downloads"
      | "settings"
      | "feedback"
      | "rename-window"
      | "export-project"
      | "onboarding",
    anchor?: OverlayAnchor | null,
    tabId?: string | null,
  ) => invoke<void>("open_overlay", { kind, anchor: anchor ?? null, tabId: tabId ?? null }),
  pollPageAction: () =>
    invoke<
      | { type: "quick-translate"; text: string }
      | { type: "add-glossary"; source: string; translation: string; doNotTranslate: boolean }
      | { type: "retranslate"; key: string }
      | { type: "edit"; key: string; translation: string }
      | { type: "report-translation"; key: string; source?: string; translation?: string }
      | { type: "page-meta"; bg: string; isDark: boolean; scrollState: "up" | "down" }
      | {
          type: "page-contextmenu";
          x?: number;
          y?: number;
          linkUrl?: string;
          linkText?: string;
          imageUrl?: string;
          selectedText?: string;
          isEditable?: boolean;
          hasMedia?: boolean;
        }
      | null
    >("poll_page_action"),
  showQuickTranslation: (text: string, isError: boolean) =>
    invoke<void>("show_quick_translation", { text, isError }),
  runPageContextAction: (
    action:
      | "quick-translate"
      | "add-glossary"
      | "copy"
      | "cut"
      | "paste"
      | "select-all"
      | "inspect"
      | "tts-toggle"
      | "tts-stop"
      | "tts-next"
      | "tts-prev"
      | "tts-rate"
      | "pip"
      | "scroll-top"
      | "scroll-bottom"
      | "media-rate"
      | "toggle-dark-page"
      | "toggle-reader-mode"
      | "show-card",
    text?: string,
    x?: number,
    y?: number,
  ) => invoke<void>("run_page_context_action", { action, text, x, y }),
  syncUiThemeToTabs: (isDark: boolean | null, accent: string, tinted: boolean) =>
    invoke<void>("sync_ui_theme_to_tabs", { isDark, accent, tinted }),
  savePageTranslationOverride: (
    pageUrl: string,
    sourceText: string,
    translation: string,
    targetLanguage?: string,
  ) =>
    invoke<void>("save_page_translation_override", {
      pageUrl,
      sourceText,
      translation,
      targetLanguage: targetLanguage ?? null,
    }),
  openInternalTab: (
    kind: "start" | "bookmarks" | "visits" | "downloads" | "glossary" | "history" | "settings",
  ) => invoke<BrowserState>("open_internal_tab", { kind }),
  closeOverlay: () => invoke<void>("close_overlay"),
  setShellExpanded: (expanded: boolean) => invoke<void>("set_shell_expanded", { expanded }),
  layoutContent: (top: number) => invoke<void>("layout_content", { top }),
  extractArticle: () => invoke<Article>("extract_article"),
  pickRegionArticle: () => invoke<Article>("pick_region_article"),
  cancelRegionPicker: () => invoke<void>("cancel_region_picker"),
  loadSavedPageTranslations: (
    pageUrl: string,
    segments: SegmentInput[],
    targetLanguage?: string,
  ) =>
    invoke<SegmentResult[]>("load_saved_page_translations", {
      pageUrl,
      segments,
      targetLanguage: targetLanguage ?? null,
    }),
  applyInlineTranslations: (
    tabId: string,
    pageUrl: string,
    translations: Array<{
      key: string;
      translation: string;
      uncertain?: boolean;
      notes?: string[];
    }>,
    bilingual = false,
  ) =>
    invoke<number>("apply_inline_translations", {
      tabId,
      pageUrl,
      translations,
      bilingual,
    }),
  restoreInlineTranslations: (tabId: string, pageUrl: string) =>
    invoke<number>("restore_inline_translations", { tabId, pageUrl }),

  loadSettings: () => invoke<SettingsView>("load_settings"),
  setUiLanguage: (uiLanguage: string, syncTargetLanguage?: boolean) =>
    invoke<SettingsView>("set_ui_language", {
      uiLanguage,
      syncTargetLanguage: syncTargetLanguage ?? null,
    }),
  completeOnboarding: (uiLanguage?: string, targetLanguage?: string) =>
    invoke<SettingsView>("complete_onboarding", {
      uiLanguage: uiLanguage ?? null,
      targetLanguage: targetLanguage ?? null,
    }),
  setTranslationPrefs: (style?: string, targetLanguage?: string) =>
    invoke<SettingsView>("set_translation_prefs", {
      style: style ?? null,
      targetLanguage: targetLanguage ?? null,
    }),
  saveSettings: (
    settings: AppSettings,
    apiKey?: string,
    bridgeToken?: string,
  ) =>
    invoke<SettingsView>("save_settings", {
      settings,
      apiKey: apiKey ?? null,
      bridgeToken: bridgeToken ?? null,
    }),
  listModels: (
    settings: AppSettings,
    apiKey?: string,
    bridgeToken?: string,
  ) =>
    invoke<ModelInfo[]>("list_models", {
      settings,
      apiKey: apiKey ?? null,
      bridgeToken: bridgeToken ?? null,
    }),
  testConnection: (
    settings: AppSettings,
    apiKey?: string,
    bridgeToken?: string,
  ) =>
    invoke<ConnectionTest>("test_connection", {
      settings,
      apiKey: apiKey ?? null,
      bridgeToken: bridgeToken ?? null,
    }),

  translateBlocks: (
    segments: SegmentInput[],
    style?: string,
    force?: boolean,
    model?: string,
    pageUrl?: string,
    pageTitle?: string,
    targetLanguage?: string,
    priorContext?: TranslationContextPair[],
    styleSample?: string[],
  ) =>
    invoke<SegmentResult[]>("translate_blocks", {
      segments,
      style: style ?? null,
      force: force ?? null,
      model: model ?? null,
      pageUrl: pageUrl ?? null,
      pageTitle: pageTitle ?? null,
      targetLanguage: targetLanguage ?? null,
      priorContext: priorContext ?? null,
      styleSample: styleSample ?? null,
    }),
  translateBlocksStream: (
    segments: SegmentInput[],
    onEvent: (event: TranslationStreamEvent) => void,
    style?: string,
    force?: boolean,
    model?: string,
    pageUrl?: string,
    pageTitle?: string,
    targetLanguage?: string,
    priorContext?: TranslationContextPair[],
    styleSample?: string[],
  ) => {
    const channel = new Channel<TranslationStreamEvent>();
    channel.onmessage = onEvent;
    return invoke<SegmentResult[]>("translate_blocks_stream", {
      segments,
      style: style ?? null,
      force: force ?? null,
      model: model ?? null,
      pageUrl: pageUrl ?? null,
      pageTitle: pageTitle ?? null,
      targetLanguage: targetLanguage ?? null,
      priorContext: priorContext ?? null,
      styleSample: styleSample ?? null,
      onEvent: channel,
    });
  },
  getViewportInlineOrder: () => invoke<ViewportOrder>("get_viewport_inline_order"),
  detectHyMt2Hardware: () => invoke<HardwareProfile>("detect_hy_mt2_hardware"),
  listHyMt2Models: () => invoke<HyMt2ModelState[]>("list_hy_mt2_models"),
  downloadHyMt2Model: (
    modelId: string,
    onProgress: (progress: ModelDownloadProgress) => void,
  ) => {
    const channel = new Channel<ModelDownloadProgress>();
    channel.onmessage = onProgress;
    return invoke<HyMt2ModelState>("download_hy_mt2_model", {
      modelId,
      onProgress: channel,
    });
  },
  cancelHyMt2Download: (modelId: string) =>
    invoke<boolean>("cancel_hy_mt2_download", { modelId }),
  deleteHyMt2Model: (modelId: string) =>
    invoke<void>("delete_hy_mt2_model", { modelId }),
  unloadHyMt2Model: () => invoke<boolean>("unload_hy_mt2_model"),
  getTesterDiagnostics: () => invoke<string>("get_tester_diagnostics"),
  saveTesterDiagnostics: () => invoke<string>("save_tester_diagnostics"),
  getFeedbackConfig: () => invoke<FeedbackConfigStatus>("get_feedback_config"),
  submitFeedbackReport: (input: FeedbackReportInput) =>
    invoke<FeedbackSubmitResult>("submit_feedback_report", { input }),
  openFeedbackUrl: (url: string) => invoke<void>("open_feedback_url", { url }),
  saveFeedbackReportFile: (title: string, body: string) =>
    invoke<string>("save_feedback_report_file", { title, body }),
  cancelTranslation: () => invoke<void>("cancel_translation"),
  exportProject: (includeOriginal: boolean) =>
    invoke<string>("export_project", { includeOriginal }),

  listGlossary: () => invoke<GlossaryEntry[]>("list_glossary"),
  saveGlossary: (entry: GlossaryEntry) =>
    invoke<void>("save_glossary", { entry }),
  deleteGlossary: (id: string) => invoke<void>("delete_glossary", { id }),
  listBookmarks: () => invoke<Bookmark[]>("list_bookmarks"),
  saveBookmark: (bookmark: Bookmark) => invoke<void>("save_bookmark", { bookmark }),
  deleteBookmark: (id: string) => invoke<void>("delete_bookmark", { id }),
  renameBookmarkFolder: (oldName: string, newName: string) =>
    invoke<number>("rename_bookmark_folder", { oldName, newName }),
  listBookmarkFolders: () => invoke<string[]>("list_bookmark_folders"),
  createBookmarkFolder: (name: string) =>
    invoke<void>("create_bookmark_folder", { name }),
  deleteBookmarkFolder: (name: string) =>
    invoke<void>("delete_bookmark_folder", { name }),
  listHistory: (query = "", limit = 200) =>
    invoke<HistoryItem[]>("list_history", { query, limit }),
  deleteHistory: (id: number) => invoke<void>("delete_history", { id }),
  clearHistory: () => invoke<void>("clear_history"),
  listDownloads: (query = "", limit = 200) =>
    invoke<DownloadItem[]>("list_downloads", { query, limit }),
  deleteDownload: (id: number) => invoke<void>("delete_download", { id }),
  clearDownloads: () => invoke<void>("clear_downloads"),
  showDownloadInFolder: (path: string) =>
    invoke<void>("show_download_in_folder", { path }),
  findInPage: (query: string, action: "find" | "next" | "prev" | "clear") =>
    invoke<FindResult>("find_in_page", { query, action }),
  addressSuggestions: (query: string) =>
    invoke<AddressSuggestion[]>("address_suggestions", { query }),
  reopenClosedTab: () => invoke<BrowserState>("reopen_closed_tab"),
  setZoom: (factor: number) => invoke<BrowserState>("set_zoom", { factor }),
  printPage: () => invoke<void>("print_page"),
  toggleFullscreen: () => invoke<boolean>("toggle_fullscreen"),
  windowAction: (action: "minimize" | "toggle-maximize" | "close" | "start-dragging") =>
    invoke<void>("window_action", { action }),
  exportAppData: () => invoke<string>("export_app_data"),
  importAppData: (payload: string) =>
    invoke<ImportSummary>("import_app_data", { payload }),
  createWorkspace: (name: string) => invoke<BrowserState>("create_workspace", { name }),
  switchWorkspace: (workspaceId: string) => invoke<BrowserState>("switch_workspace", { workspaceId }),
  renameWorkspace: (workspaceId: string, name: string) => invoke<BrowserState>("rename_workspace", { workspaceId, name }),
  deleteWorkspace: (workspaceId: string) => invoke<BrowserState>("delete_workspace", { workspaceId }),
  listTranslationHistory: (query = "") =>
    invoke<TranslationHistory[]>("list_translation_history", { query }),

  listSearchHistory: (query = "", limit = 300) =>
    invoke<SearchHistoryItem[]>("list_search_history", { query, limit }),
  deleteSearchHistory: (id: number) => invoke<void>("delete_search_history", { id }),
  clearSearchHistory: () => invoke<void>("clear_search_history"),

  listSavedLogins: (origin?: string) =>
    invoke<SavedLogin[]>("list_saved_logins", { origin: origin ?? null }),
  saveLogin: (id: string, origin: string, username: string, password: string) =>
    invoke<void>("save_login", { id, origin, username, password }),
  deleteLogin: (id: string) => invoke<void>("delete_login", { id }),
  autofillSavedLogin: (origin: string, username?: string) =>
    invoke<void>("autofill_saved_login", { origin, username: username ?? null }),

  loadAutofillProfile: () => invoke<AutofillProfile>("load_autofill_profile"),
  saveAutofillProfile: (profile: AutofillProfile) =>
    invoke<AutofillProfile>("save_autofill_profile", { profile }),
  autofillProfileActiveTab: () => invoke<void>("autofill_profile_active_tab"),

  loadSyncConfig: () => invoke<SyncConfig>("load_sync_config"),
  saveSyncConfig: (config: SyncConfig) => invoke<SyncConfig>("save_sync_config", { config }),
  runSyncNow: () => invoke<SyncConfig>("run_sync_now"),
  detectCloudAccounts: () => invoke<CloudAccountRoot[]>("detect_cloud_accounts"),
  connectSyncAccount: (
    provider: string,
    email: string,
    displayName: string,
    syncDir?: string,
  ) =>
    invoke<SyncConfig>("connect_sync_account", {
      provider,
      email,
      displayName,
      syncDir: syncDir ?? null,
    }),
  disconnectSyncAccount: () => invoke<SyncConfig>("disconnect_sync_account"),
  startOauthLogin: (provider: "google" | "microsoft") =>
    invoke<void>("start_oauth_login", { provider }),

  startCodexOauthLogin: () => invoke<void>("start_codex_oauth_login"),
  importLocalCodexAuth: () =>
    invoke<{ view: SettingsView; source: string }>("import_local_codex_auth"),
  clearCodexAuth: () => invoke<SettingsView>("clear_codex_auth"),

  detectExternalBrowsers: () => invoke<DetectedBrowserProfile[]>("detect_external_browsers"),
  importExternalBrowser: (
    browserId: string,
    includeBookmarks: boolean,
    includeHistory: boolean,
  ) =>
    invoke<ExternalImportSummary>("import_external_browser", {
      browserId,
      includeBookmarks,
      includeHistory,
    }),
  importPasswordsCsv: (csvText: string) => invoke<number>("import_passwords_csv", { csvText }),

  listCookies: (uri: string) => invoke<CookieEntry[]>("list_cookies", { uri }),
  deleteCookie: (name: string, domain: string, path: string) =>
    invoke<void>("delete_cookie", { name, domain, path }),
  clearBrowsingData: (scope: "cookies" | "cache" | "all") =>
    invoke<void>("clear_browsing_data", { scope }),

  listUserScripts: () => invoke<UserScriptItem[]>("list_user_scripts"),
  saveUserScript: (item: UserScriptItem) => invoke<void>("save_user_script", { item }),
  deleteUserScript: (id: string) => invoke<void>("delete_user_script", { id }),

  listWebExtensions: () => invoke<WebExtensionInfo[]>("list_web_extensions"),
  addWebExtension: (folderPath: string) =>
    invoke<WebExtensionInfo>("add_web_extension", { folderPath }),
  toggleOrRemoveWebExtension: (extId: string, remove: boolean, enabled: boolean) =>
    invoke<void>("toggle_or_remove_web_extension", { extId, remove, enabled }),

  toggleAdblockSite: (url: string) => invoke<SettingsView>("toggle_adblock_site", { url }),
  allowPopupSite: (host: string) => invoke<SettingsView>("allow_popup_site", { host }),
};

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return String(error);
}
