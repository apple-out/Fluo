<script lang="ts">
  import { onMount, tick } from "svelte";
  import { emit, listen, type UnlistenFn } from "@tauri-apps/api/event";

  import GlossaryPanel from "$lib/components/GlossaryPanel.svelte";
  import BookmarkPanel from "$lib/components/BookmarkPanel.svelte";
  import BookmarkBar from "$lib/components/BookmarkBar.svelte";
  import DownloadsPanel from "$lib/components/DownloadsPanel.svelte";
  import FeedbackPanel from "$lib/components/FeedbackPanel.svelte";
  import HistoryPanel from "$lib/components/HistoryPanel.svelte";
  import OnboardingModal from "$lib/components/OnboardingModal.svelte";
  import SettingsPanel from "$lib/components/SettingsPanel.svelte";
  import TabBar from "$lib/components/TabBar.svelte";
  import TranslationHistoryPanel from "$lib/components/TranslationHistoryPanel.svelte";
  import { api, errorMessage, type OverlayAnchor } from "$lib/api";
  import { getUiLang, setUiLang, t, UI_LANGUAGES } from "$lib/i18n.svelte";
  import { normalizeBookmarkUrl } from "$lib/browser-utils";
  import {
    defaultSettings,
    emptyGlossaryEntry,
    type AddressSuggestion,
    type AppSettings,
    type Article,
    type Block,
    type Bookmark,
    type BrowserState,
    type FeedbackDraft,
    type FindResult,
    type HardwareProfile,
    type HistoryItem,
    type HyMt2ModelState,
    type ModelDownloadProgress,
    type ModelInfo,
    type SegmentResult,
    type SettingsView,
    type TranslationContextPair,
    type UiLanguage,
  } from "$lib/types";

  type Status = { kind: "info" | "success" | "error"; text: string };
  type InternalPageKind = "start" | "bookmarks" | "visits" | "downloads" | "glossary" | "history" | "settings";
  type OverlayKind =
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
    | "feedback"
    | "rename-window"
    | "export-project"
    | "onboarding"
    | Exclude<InternalPageKind, "start">;

  type PopupBlockedNotice = {
    tabId: string;
    url: string;
    sourceOrigin: string;
  };

  type SaveLoginPrompt = {
    origin: string;
    username: string;
    password: string;
  };

  type PageMenuContext = {
    x: number;
    y: number;
    linkUrl: string;
    linkText: string;
    imageUrl: string;
    selectedText: string;
    isEditable: boolean;
    hasMedia: boolean;
  };

  const initialOverlay = typeof window === "undefined"
    ? null
    : (window as Window & { __APP_OVERLAY_KIND__?: OverlayKind }).__APP_OVERLAY_KIND__ ?? null;

  const initialAnchor = typeof window === "undefined"
    ? null
    : (window as Window & { __APP_OVERLAY_ANCHOR__?: OverlayAnchor | null }).__APP_OVERLAY_ANCHOR__ ?? null;

  const initialTabMenuId = typeof window === "undefined"
    ? null
    : (window as Window & { __APP_OVERLAY_TAB_ID__?: string | null }).__APP_OVERLAY_TAB_ID__ ?? null;

  const initialChromeTop = typeof window === "undefined"
    ? 84
    : (window as Window & { __APP_OVERLAY_CHROME_TOP__?: number }).__APP_OVERLAY_CHROME_TOP__ ?? 84;

  const initialPage = typeof window === "undefined"
    ? null
    : (window as Window & { __APP_PAGE_KIND__?: InternalPageKind }).__APP_PAGE_KIND__ ?? null;

  let browser = $state<BrowserState>({
    tabs: [],
    activeTab: null,
    canGoBack: false,
    canGoForward: false,
    workspaces: [{ id: "default", name: "기본 작업공간" }],
    activeWorkspace: "default",
  });
  let address = $state("");
  let settingsView = $state<SettingsView | null>(null);
  let article = $state<Article | null>(null);
  let translations = $state<Record<string, SegmentResult>>({});
  let errors = $state<Record<string, string>>({});
  let translating = $state(false);
  let manualTranslateCancel = false;
  let inlineMode = $state(false);
  let progress = $state({ done: 0, total: 0 });
  let status = $state<Status | null>(null);
  let overlayKind = $state<OverlayKind | null>(initialOverlay);
  let overlayAnchor = $state<OverlayAnchor | null>(initialAnchor);
  let tabMenuTargetId = $state<string | null>(initialTabMenuId);
  let overlayChromeTop = $state<number>(initialChromeTop);
  let pageKind = $state<InternalPageKind | null>(initialPage);
  let feedbackDraft = $state<FeedbackDraft | null>(null);
  let workspaceMode = $state<"list" | "create" | "rename" | "delete">("list");
  let workspaceDraftName = $state("");
  let workspaceBusy = $state(false);
  let renameWindowDraft = $state("");
  let onboardingTriggered = false;
  let exporting = $state(false);

  function workspaceLabel(ws: { id: string; name: string }): string {
    return ws.id === "default" ? t("common.defaultWorkspace") : ws.name;
  }

  async function quickSwitchUiLanguage(lang: UiLanguage) {
    setUiLang(lang);
    try {
      const updated = await api.setUiLanguage(lang);
      onSettingsSaved(updated);
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }
  const VALID_TRANSLATION_STYLES = new Set(["auto", "natural", "novel", "literal", "polished"]);
  function readSavedStyle(): string {
    try {
      const saved = localStorage.getItem("tb-last-style")?.trim().toLowerCase() ?? "";
      return VALID_TRANSLATION_STYLES.has(saved) ? saved : "auto";
    } catch {
      return "auto";
    }
  }
  function saveLocalStyle(style: string) {
    try {
      localStorage.setItem("tb-last-style", style);
    } catch {
      // Ignore storage failures.
    }
  }

  let availableModels = $state<ModelInfo[]>([]);
  let selectedModel = $state("");
  let selectedTargetLanguage = $state("한국어");
  let selectedStyle = $state(readSavedStyle());
  let detectedAutoStyle = $state<string | null>(null);
  let bilingualAll = $state(false);
  let loadingModels = $state(false);
  let modelRequestId = 0;
  let overlayHyMt2Models = $state<HyMt2ModelState[]>([]);
  let overlayHyMt2Hw = $state<HardwareProfile | null>(null);
  let overlayDownloadProgress = $state<ModelDownloadProgress | null>(null);
  let overlayDownloadingId = $state<string | null>(null);

  const targetLanguagePresets = [
    "한국어",
    "English",
    "日本語",
    "中文(简体)",
    "中文(繁體)",
    "Español",
    "Français",
    "Deutsch",
    "Русский",
    "Português",
    "Italiano",
    "Tiếng Việt",
    "ไทย",
    "Bahasa Indonesia",
    "العربية",
  ];

  let headerEl = $state<HTMLElement | null>(null);
  let addressInput = $state<HTMLInputElement | null>(null);
  let addressFocused = $state(false);
  let justFocusedAddress = false;
  let startQuery = $state("");
  let recentVisits = $state<HistoryItem[]>([]);
  type QuickShortcut = { id: string; title: string; url: string };
  const DEFAULT_QUICK_SHORTCUTS: QuickShortcut[] = [
    { id: "google", title: "Google", url: "https://www.google.com/" },
    { id: "wikipedia", title: "Wikipedia", url: "https://en.wikipedia.org/" },
    { id: "github", title: "GitHub", url: "https://github.com/" },
    { id: "youtube", title: "YouTube", url: "https://www.youtube.com/" },
  ];
  let quickShortcuts = $state<QuickShortcut[]>(DEFAULT_QUICK_SHORTCUTS);
  let showAddShortcutForm = $state(false);
  let newShortcutTitle = $state("");
  let newShortcutUrl = $state("");
  let autoTranslateHosts = $state<string[]>([]);
  let autoTranslatePrompt = $state<{ lang: string; host: string } | null>(null);
  let ttsActive = $state(false);
  let ttsProgress = $state<{ index: number; total: number }>({ index: 0, total: 0 });
  let audibleTabs = $state<Record<string, boolean>>({});
  let navHistoryEntries = $state<Array<{ delta: number; url: string }>>([]);
  let bmDraftId = $state<string | null>(null);
  let bmDraftTitle = $state("");
  let bmDraftUrl = $state("");
  let bmDraftFolder = $state("");
  let bmDraftFavorite = $state(false);
  let bmDraftCreatedAt = $state(0);

  function loadLocalBrowserExtras() {
    try {
      const rawShortcuts = localStorage.getItem("fluo-custom-shortcuts");
      if (rawShortcuts) {
        const parsed = JSON.parse(rawShortcuts);
        const legacyIds = ["google", "wikipedia", "namu", "github", "nhk", "syosetu"].join(",");
        if (
          Array.isArray(parsed) &&
          parsed.length > 0 &&
          parsed.map((item: { id?: string }) => item?.id ?? "").join(",") !== legacyIds
        ) {
          quickShortcuts = parsed;
        }
      }
      const rawAutoHosts = localStorage.getItem("fluo-auto-translate-hosts");
      if (rawAutoHosts) {
        const parsed = JSON.parse(rawAutoHosts);
        if (Array.isArray(parsed)) autoTranslateHosts = parsed;
      }
    } catch {
      // Ignore storage errors.
    }
  }

  function addCustomQuickShortcut() {
    let rawUrl = newShortcutUrl.trim();
    if (!rawUrl) return;
    if (!rawUrl.startsWith("http://") && !rawUrl.startsWith("https://")) {
      rawUrl = `https://${rawUrl}`;
    }
    const title = newShortcutTitle.trim() || rawUrl.replace(/^https?:\/\//, "").split("/")[0];
    quickShortcuts = [...quickShortcuts, { id: crypto.randomUUID(), title, url: rawUrl }];
    try {
      localStorage.setItem("fluo-custom-shortcuts", JSON.stringify(quickShortcuts));
    } catch {}
    newShortcutTitle = "";
    newShortcutUrl = "";
    showAddShortcutForm = false;
  }

  function removeCustomQuickShortcut(id: string) {
    quickShortcuts = quickShortcuts.filter((s) => s.id !== id);
    try {
      localStorage.setItem("fluo-custom-shortcuts", JSON.stringify(quickShortcuts));
    } catch {}
  }

  function toggleAutoTranslateHost(host: string) {
    const clean = host.trim().toLowerCase();
    if (!clean) return;
    if (autoTranslateHosts.includes(clean)) {
      autoTranslateHosts = autoTranslateHosts.filter((h) => h !== clean);
    } else {
      autoTranslateHosts = [...autoTranslateHosts, clean];
      autoTranslatePrompt = null;
      if (!inlineMode && !translating) {
        void translateCurrentPage(false);
      }
    }
    try {
      localStorage.setItem("fluo-auto-translate-hosts", JSON.stringify(autoTranslateHosts));
    } catch {}
  }

  function evalQuickMath(raw: string): string | null {
    const expr = (raw.startsWith("=") ? raw.slice(1) : raw).trim();
    if (expr.length < 3 || expr.length > 48) return null;
    if (!/^[0-9.+\-*/()% \t]+$/.test(expr)) return null;
    if (!/[+\-*/]/.test(expr) || !/\d/.test(expr)) return null;
    const compact = expr.replace(/\s+/g, "");
    if (/[+\-*/%.]{2,}/.test(compact)) return null;
    try {
      const fn = new Function(`return (${compact});`);
      const val = fn();
      if (typeof val === "number" && Number.isFinite(val)) {
        return Number.isInteger(val) ? String(val) : String(Number(val.toFixed(6)));
      }
    } catch {}
    return null;
  }

  function trLocal(koText: string, enText: string, jaText: string, zhText: string): string {
    const lang = getUiLang();
    if (lang === "en") return enText;
    if (lang === "ja") return jaText;
    if (lang === "zh-CN") return zhText;
    return koText;
  }
  let headerHeight = $state(88);
  let bookmarks = $state<Bookmark[]>([]);
  let bookmarkBarVisible = $state(false);
  let compactChrome = $state(false);
  let scrollCollapsed = $state(false);
  let scrollProgress = $state(0);
  let targetScrollProgress = 0;
  let scrollSpringRaf = 0;
  let glassTinted = $state(false);
  let accentColor = $state("#684cff");
  let themeAutoSync = $state(true);
  let systemPrefersDark = $state(false);
  let popupBlockedNotice = $state<PopupBlockedNotice | null>(null);
  let saveLoginPrompt = $state<SaveLoginPrompt | null>(null);
  let pageMenuContext = $state<PageMenuContext | null>(null);
  let pageBg = $state<string | null>(null);
  let pageIsDark = $state<boolean | null>(null);
  let tabSearchQuery = $state("");
  let suggestions = $state<AddressSuggestion[]>([]);
  let suggestionIndex = $state(-1);
  let findBarOpen = $state(false);
  let collapseRatio = $derived(
    compactChrome ? 1 : findBarOpen ? 0 : scrollProgress,
  );
  let effectiveCompact = $derived(collapseRatio > 0.55);
  let findQuery = $state("");
  let findResult = $state<FindResult>({ total: 0, current: 0 });
  let findInput = $state<HTMLInputElement | null>(null);
  let pickingRegion = $state(false);
  let suggestionTimer: ReturnType<typeof setTimeout> | undefined;
  let suggestionRequestId = 0;

  let statusTimer: ReturnType<typeof setTimeout> | undefined;
  let layoutFrame = 0;
  let syncingReadingPosition = false;

  let settings = $derived<AppSettings>(settingsView?.settings ?? defaultSettings());
  let resolvedTheme = $derived(
    settings.theme === "system" ? (systemPrefersDark ? "dark" : "light") : settings.theme,
  );
  let effectivePageDark = $derived(
    settings.theme === "dark"
      ? true
      : settings.theme === "light" || settings.theme === "sepia"
        ? false
        : themeAutoSync && pageIsDark !== null
          ? pageIsDark
          : systemPrefersDark,
  );
  let translationModel = $derived(
    selectedModel ||
      settings.model ||
      (settings.provider === "hyMt2"
        ? "Hy-MT2-1.8B-Q4_K_M.gguf"
        : settings.provider === "codexLegacy"
          ? "gpt-5.4"
          : ""),
  );
  let activeTab = $derived(
    browser.tabs.find((tab) => tab.id === browser.activeTab) ?? null,
  );
  let visibleTabs = $derived(browser.tabs.filter((tab) => tab.workspaceId === browser.activeWorkspace));
  let secure = $derived(activeTab?.url.startsWith("https://") ?? false);
  let isCurrentBookmarked = $derived.by(() => {
    if (!activeTab) return false;
    try {
      const normalized = normalizeBookmarkUrl(activeTab.url);
      return bookmarks.some((b) => {
        try {
          return normalizeBookmarkUrl(b.url) === normalized;
        } catch {
          return false;
        }
      });
    } catch {
      return false;
    }
  });
  let filteredSearchTabs = $derived.by(() => {
    const q = tabSearchQuery.trim().toLowerCase();
    if (!q) return browser.tabs;
    return browser.tabs.filter(
      (t) => t.title.toLowerCase().includes(q) || t.url.toLowerCase().includes(q),
    );
  });
  let pendingBlocks = $derived(
    article
      ? article.blocks.filter(
          (block) =>
            block.kind !== "code" &&
            block.kind !== "table" &&
            block.text.trim().length > 0 &&
            !translations[block.id],
        ).length
      : 0,
  );

  function notify(kind: Status["kind"], text: string) {
    status = { kind, text };
    clearTimeout(statusTimer);
    statusTimer = setTimeout(
      () => {
        status = null;
      },
      kind === "error" ? 9_000 : 4_500,
    );
  }

  function applyBrowser(next: BrowserState) {
    const previousUrl = browser.tabs.find((item) => item.id === browser.activeTab)?.url;
    const nextTab = next.tabs.find((item) => item.id === next.activeTab);
    const tabOrUrlChanged =
      Boolean(browser.activeTab) &&
      (next.activeTab !== browser.activeTab || (nextTab?.url && previousUrl !== nextTab.url));
    if (tabOrUrlChanged) {
      autoTranslatePrompt = null;
      if (ttsActive) {
        ttsActive = false;
        ttsProgress = { index: 0, total: 0 };
      }
    }
    if (
      inlineMode &&
      browser.activeTab &&
      previousUrl &&
      tabOrUrlChanged
    ) {
      const previousTabId = browser.activeTab;
      const previousPageUrl = article?.url ?? previousUrl;
      if (translating) {
        manualTranslateCancel = true;
        void api.cancelTranslation().catch(() => {});
      }
      // Closing the translated tab destroys its WebView, so restoring is meaningless.
      const previousStillOpen = next.tabs.some((item) => item.id === previousTabId);
      if (previousStillOpen && previousTabId && previousPageUrl) {
        void api
          .restoreInlineTranslations(previousTabId, previousPageUrl)
          .catch((cause) => notify("error", errorMessage(cause)));
      }
      clearArticle();
    }
    browser = next;
    if (nextTab && !addressFocused) {
      address = nextTab.url === "tb://start" ? "" : nextTab.url;
    }
  }

  function clearArticle() {
    article = null;
    translations = {};
    errors = {};
    progress = { done: 0, total: 0 };
    inlineMode = false;
    autoTranslatePrompt = null;
  }

  async function restoreInlinePage() {
    if (!inlineMode || !activeTab) return;
    await api.restoreInlineTranslations(
      activeTab.id,
      article?.url ?? activeTab.url,
    );
    inlineMode = false;
  }

  /// Navigation must continue even when the current page can no longer be
  /// reached (crash, closed WebView, replaced document).
  async function restoreInlineBeforeLeaving() {
    try {
      await restoreInlinePage();
    } catch {
      inlineMode = false;
    }
  }

  async function updateLayout() {
    // Do not shrink the shell webview while a full-window overlay menu or address suggestions are open.
    if (!headerEl || overlayKind || (addressFocused && suggestions.length > 0)) return;
    cancelAnimationFrame(layoutFrame);
    layoutFrame = requestAnimationFrame(() => {
      if (overlayKind || (addressFocused && suggestions.length > 0)) return;
      const rect = headerEl?.getBoundingClientRect();
      const bottom = Math.max(
        38,
        Math.round(rect ? rect.bottom : headerHeight),
      );
      headerHeight = bottom;
      api.layoutContent(bottom).catch(() => {});
    });
  }

  let layoutAnimTimer: ReturnType<typeof setTimeout> | undefined;
  let layoutSettledTimer: ReturnType<typeof setTimeout> | undefined;
  function animateLayout(durationMs = 280) {
    clearTimeout(layoutAnimTimer);
    if (!effectiveCompact && !overlayKind && !(addressFocused && suggestions.length > 0)) {
      const targetHeight = bookmarkBarVisible ? 124 : 96;
      headerHeight = targetHeight;
      api.layoutContent(targetHeight).catch(() => {});
    }
    layoutAnimTimer = setTimeout(() => {
      updateLayout();
    }, durationMs);
  }
  function scheduleSettledLayout(delayMs = 120) {
    clearTimeout(layoutSettledTimer);
    layoutSettledTimer = setTimeout(() => {
      void updateLayout();
    }, delayMs);
  }

  async function navigate(input?: string) {
    const target = (input ?? address).trim();
    suggestions = [];
    suggestionIndex = -1;
    if (!target) {
      addressInput?.focus();
      return;
    }
    try {
      if (translating) await api.cancelTranslation();
      await restoreInlineBeforeLeaving();
      clearArticle();
      applyBrowser(await api.navigate(target));
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function newTab() {
    try {
      scrollCollapsed = false;
      setTargetScrollProgress(0);
      if (!overlayKind) {
        if (translating) await api.cancelTranslation();
        await restoreInlineBeforeLeaving();
        await api.syncReadingPosition().catch(() => {});
        clearArticle();
      }
      applyBrowser(await api.newTab());
      if (overlayKind) {
        await closeOverlay();
      } else {
        address = "";
        await tick();
        addressInput?.focus();
        updateLayout();
      }
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function closeTab(id: string) {
    const closingActive = id === browser.activeTab;
    try {
      if (closingActive && translating) await api.cancelTranslation();
      if (audibleTabs[id]) {
        const nextAudible = { ...audibleTabs };
        delete nextAudible[id];
        audibleTabs = nextAudible;
      }
      applyBrowser(await api.closeTab(id));
      if (closingActive) clearArticle();
      updateLayout();
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function newPrivateTab(url?: string) {
    try {
      if (!overlayKind) {
        if (translating) await api.cancelTranslation();
        await restoreInlineBeforeLeaving();
        await api.syncReadingPosition().catch(() => {});
        clearArticle();
      }
      applyBrowser(await api.newPrivateTab(url));
      if (overlayKind) {
        await closeOverlay();
      } else {
        updateLayout();
      }
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function pinTab(id: string) {
    try {
      applyBrowser(await api.pinTab(id));
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function toggleMuteTab(id: string) {
    try {
      applyBrowser(await api.toggleMuteTab(id));
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function sleepTab(id: string) {
    try {
      if (audibleTabs[id]) {
        const nextAudible = { ...audibleTabs };
        delete nextAudible[id];
        audibleTabs = nextAudible;
      }
      applyBrowser(await api.sleepTab(id));
      notify("info", t("toast.tabSlept"));
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  function stepScrollSpring() {
    scrollSpringRaf = 0;
    const diff = targetScrollProgress - scrollProgress;
    if (Math.abs(diff) < 0.004) {
      scrollProgress = targetScrollProgress;
      scrollCollapsed = scrollProgress > 0.55;
      if (scrollProgress <= 0.01 || scrollProgress >= 0.99) {
        void updateLayout();
      }
      return;
    }
    scrollProgress = Number((scrollProgress + diff * 0.24).toFixed(4));
    scrollCollapsed = scrollProgress > 0.55;
    scrollSpringRaf = requestAnimationFrame(stepScrollSpring);
  }

  function setTargetScrollProgress(next: number) {
    const clamped = Math.max(0, Math.min(1, next));
    if (
      clamped < 0.15 &&
      clamped < scrollProgress &&
      !overlayKind &&
      !(addressFocused && suggestions.length > 0)
    ) {
      const expandHeight = bookmarkBarVisible ? 122 : 94;
      if (headerHeight < expandHeight) {
        headerHeight = expandHeight;
        api.layoutContent(expandHeight).catch(() => {});
      }
    }
    targetScrollProgress = clamped;
    if (!scrollSpringRaf && typeof requestAnimationFrame !== "undefined") {
      scrollSpringRaf = requestAnimationFrame(stepScrollSpring);
    }
  }

  function toggleCompactChrome() {
    compactChrome = !compactChrome;
    if (!compactChrome) {
      scrollCollapsed = false;
      setTargetScrollProgress(0);
    } else {
      setTargetScrollProgress(1);
    }
    try {
      localStorage.setItem("compact-chrome", String(compactChrome));
    } catch {
      // Ignore storage errors.
    }
    animateLayout(350);
  }

  function toggleGlassTint() {
    glassTinted = !glassTinted;
    try {
      localStorage.setItem("glass-tinted", String(glassTinted));
    } catch {
      // Ignore storage errors.
    }
    notify(
      "info",
      glassTinted ? t("toast.glassTinted") : t("toast.glassClear"),
    );
  }

  async function toggleAdBlock() {
    const nextEnabled = !settings.adBlockEnabled;
    try {
      const updated = await api.saveSettings({
        ...settings,
        adBlockEnabled: nextEnabled,
      });
      onSettingsSaved(updated);
      notify(
        "info",
        nextEnabled ? t("toast.adblockOn") : t("toast.adblockOff"),
      );
      if (activeTab && !activeTab.url.startsWith("tb://")) {
        void reload();
      }
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  function stripTrackingParams(rawUrl: string): string {
    try {
      const parsed = new URL(rawUrl);
      const trackKeys = new Set([
        "fbclid",
        "gclid",
        "yclid",
        "msclkid",
        "igshid",
        "mc_cid",
        "mc_eid",
        "_ga",
        "_gl",
        "ref_src",
        "si",
      ]);
      for (const key of [...parsed.searchParams.keys()]) {
        const lower = key.toLowerCase();
        if (lower.startsWith("utm_") || trackKeys.has(lower)) {
          parsed.searchParams.delete(key);
        }
      }
      return parsed.toString();
    } catch {
      return rawUrl;
    }
  }

  async function copyCurrentUrl() {
    if (!activeTab?.url) return;
    try {
      await navigator.clipboard.writeText(activeTab.url);
      notify("success", t("chrome.urlCopied"));
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function copyCleanCurrentUrl(targetUrl?: string) {
    const raw = targetUrl ?? activeTab?.url;
    if (!raw) return;
    try {
      const cleaned = stripTrackingParams(raw);
      await navigator.clipboard.writeText(cleaned);
      notify(
        "success",
        trLocal(
          "추적 파라미터가 제거된 깨끗한 주소를 복사했습니다.",
          "Copied clean URL without tracking parameters.",
          "追跡パラメータを除去したURLをコピーしました。",
          "已复制移除追踪参数的干净网址。",
        ),
      );
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function deduplicateWorkspaceTabs() {
    const seen = new Set<string>();
    const activeId = browser.activeTab;
    const wsTabs = browser.tabs.filter((t) => t.workspaceId === browser.activeWorkspace);
    if (activeId) {
      const cur = wsTabs.find((t) => t.id === activeId);
      if (cur) seen.add(stripTrackingParams(cur.url));
    }
    let closedCount = 0;
    for (const tab of wsTabs) {
      if (tab.id === activeId || tab.pinned) continue;
      const key = stripTrackingParams(tab.url);
      if (seen.has(key)) {
        await closeTab(tab.id);
        closedCount++;
      } else {
        seen.add(key);
      }
    }
    notify(
      "info",
      closedCount > 0
        ? trLocal(`중복 탭 ${closedCount}개를 정리했습니다.`, `Closed ${closedCount} duplicate tab(s).`, `重複タブを${closedCount}件整理しました。`, `已清理 ${closedCount} 个重复标签页。`)
        : trLocal("중복된 탭이 없습니다.", "No duplicate tabs found.", "重複しているタブはありません。", "没有发现重复的标签页。"),
    );
  }

  async function addCurrentPageToReadingList() {
    if (!activeTab?.url || activeTab.url.startsWith("tb://")) return;
    try {
      const normalized = normalizeBookmarkUrl(activeTab.url);
      const now = Math.floor(Date.now() / 1000);
      const folderName = trLocal("나중에 읽기", "Reading List", "あとで読む", "稍后阅读");
      await api.saveBookmark({
        id: crypto.randomUUID(),
        url: normalized,
        title: activeTab.title || normalized,
        folder: folderName,
        favorite: false,
        createdAt: now,
        updatedAt: now,
      });
      await loadBookmarks();
      notify(
        "success",
        trLocal(
          "📑 '나중에 읽기' 목록에 저장했습니다.",
          "📑 Saved to Reading List.",
          "📑 「あとで読む」に保存しました。",
          "📑 已保存至“稍后阅读”。",
        ),
      );
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function summarizeCurrentPage() {
    const model = translationModel.trim();
    if (!model) {
      notify("error", t("toast.quickNeedModel"));
      return;
    }
    try {
      const cardTitle = trLocal("✨ AI 페이지 핵심 요약", "✨ AI Page Summary", "✨ AI ページ要点要約", "✨ AI 页面核心摘要");
      await api.runPageContextAction("show-card", cardTitle);
      const extracted = article ?? (await api.extractArticle());
      const topBlocks = extracted.blocks
        .filter((b) => b.kind !== "code" && b.text.replace(/\[\[img\d+\]\]/g, "").trim().length >= 24)
        .slice(0, 5);
      if (topBlocks.length === 0) {
        await api.showQuickTranslation(
          trLocal(
            "요약할 본문 텍스트를 찾지 못했습니다.",
            "No summarizable text found on this page.",
            "要約できる本文が見つかりませんでした。",
            "未找到可摘要的正文文本。",
          ),
          true,
        );
        return;
      }
      const targetLang = (selectedTargetLanguage || settings.targetLanguage).trim() || "한국어";
      const res = await api.translateBlocks(
        topBlocks.map((b, i) => ({
          id: `sum-${i}`,
          text: b.text.replace(/\[\[img\d+\]\]/g, "").replace(/\[\[\d+\|([^\]]+)\]\]/g, "$1").slice(0, 320),
        })),
        "polished",
        false,
        model,
        extracted.url,
        extracted.title,
        targetLang,
      );
      const bullets = res
        .map((r) => r.translation.trim())
        .filter(Boolean)
        .map((line) => `• ${line}`)
        .join("\n\n");
      await api.showQuickTranslation(bullets || t("toast.quickEmpty"), false);
    } catch (cause) {
      await api.showQuickTranslation(errorMessage(cause), true);
    }
  }

  async function bookmarkAllTabs() {
    try {
      const state = await api.browserState();
      applyBrowser(state);
      const currentBookmarks = await api.listBookmarks();
      const existing = new Set<string>();
      for (const item of currentBookmarks) {
        try {
          existing.add(normalizeBookmarkUrl(item.url));
        } catch {
          // Skip invalid URLs.
        }
      }
      const tabsToSave = state.tabs.filter((t) => t.workspaceId === state.activeWorkspace);
      const now = Math.floor(Date.now() / 1000);
      let added = 0;
      for (const tab of tabsToSave) {
        try {
          const normalized = normalizeBookmarkUrl(tab.url);
          if (existing.has(normalized)) continue;
          existing.add(normalized);
          await api.saveBookmark({
            id: crypto.randomUUID(),
            url: normalized,
            title: tab.title || normalized,
            folder: "",
            favorite: false,
            createdAt: now,
            updatedAt: now,
          });
          added++;
        } catch {
          // Ignore non-http(s) tabs such as tb:// internal pages.
        }
      }
      await loadBookmarks();
      notify(
        added > 0 ? "success" : "info",
        added > 0 ? t("toast.bookmarkAllResult", { count: added }) : t("toast.bookmarkAllNone"),
      );
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  function renameWindow() {
    void openOverlay("rename-window");
  }

  async function submitRenameWindow() {
    const title = renameWindowDraft;
    try {
      await api.setWindowTitle(title);
      notify(
        "success",
        title.trim() ? t("toast.windowRenamed", { title: title.trim() }) : t("toast.windowReset"),
      );
      await closeOverlay();
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function closeOtherTabs(id: string) {
    try {
      if (translating && id !== browser.activeTab) await api.cancelTranslation();
      applyBrowser(await api.closeOtherTabs(id));
      updateLayout();
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function closeTabsToRight(id: string) {
    try {
      applyBrowser(await api.closeTabsToRight(id));
      updateLayout();
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function selectTab(id: string) {
    if (id === browser.activeTab) return;
    try {
      scrollCollapsed = false;
      setTargetScrollProgress(0);
      if (translating) await api.cancelTranslation();
      await restoreInlineBeforeLeaving();
      await api.syncReadingPosition().catch(() => {});
      clearArticle();
      applyBrowser(await api.selectTab(id));
      await tick();
      updateLayout();
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function moveTab(tabId: string, workspaceId: string) {
    try {
      if (translating) await api.cancelTranslation();
      await restoreInlineBeforeLeaving();
      await api.syncReadingPosition().catch(() => {});
      clearArticle();
      applyBrowser(await api.moveTabToWorkspace(tabId, workspaceId));
      updateLayout();
      notify("success", t("toast.tabMoved"));
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function switchWorkspace(id: string) {
    try {
      if (translating) await api.cancelTranslation();
      await restoreInlineBeforeLeaving();
      await api.syncReadingPosition().catch(() => {});
      clearArticle();
      applyBrowser(await api.switchWorkspace(id));
      await closeOverlay();
    } catch (cause) { notify("error", errorMessage(cause)); }
  }

  async function startWorkspaceCreate() {
    workspaceMode = "create";
    workspaceDraftName = "";
    await tick();
    void updateLayout();
  }

  async function startWorkspaceRename() {
    const workspace = browser.workspaces.find((item) => item.id === browser.activeWorkspace);
    if (!workspace) return;
    workspaceMode = "rename";
    workspaceDraftName = workspaceLabel(workspace);
    await tick();
    void updateLayout();
  }

  async function startWorkspaceDelete() {
    const workspace = browser.workspaces.find((item) => item.id === browser.activeWorkspace);
    if (!workspace || workspace.id === "default") {
      notify("info", t("ws.cannotDeleteDefault"));
      return;
    }
    workspaceMode = "delete";
    await tick();
    void updateLayout();
  }

  async function submitCreateWorkspace() {
    const name = workspaceDraftName.trim();
    if (!name || workspaceBusy) return;
    workspaceBusy = true;
    try {
      if (translating) await api.cancelTranslation();
      await restoreInlineBeforeLeaving();
      await api.syncReadingPosition().catch(() => {});
      clearArticle();
      applyBrowser(await api.createWorkspace(name));
      await closeOverlay();
    } catch (cause) {
      notify("error", errorMessage(cause));
    } finally {
      workspaceBusy = false;
    }
  }

  async function submitRenameWorkspace() {
    const workspace = browser.workspaces.find((item) => item.id === browser.activeWorkspace);
    const name = workspaceDraftName.trim();
    if (!workspace || !name || workspaceBusy) return;
    workspaceBusy = true;
    try {
      applyBrowser(await api.renameWorkspace(workspace.id, name));
      await closeOverlay();
    } catch (cause) {
      notify("error", errorMessage(cause));
    } finally {
      workspaceBusy = false;
    }
  }

  async function submitDeleteWorkspace() {
    const workspace = browser.workspaces.find((item) => item.id === browser.activeWorkspace);
    if (!workspace || workspace.id === "default") {
      notify("info", t("ws.cannotDeleteDefault"));
      return;
    }
    if (workspaceBusy) return;
    workspaceBusy = true;
    try {
      if (translating) await api.cancelTranslation();
      await restoreInlineBeforeLeaving();
      await api.syncReadingPosition().catch(() => {});
      clearArticle();
      applyBrowser(await api.deleteWorkspace(workspace.id));
      await closeOverlay();
    } catch (cause) {
      notify("error", errorMessage(cause));
    } finally {
      workspaceBusy = false;
    }
  }

  async function goBack() {
    try {
      if (translating) await api.cancelTranslation();
      await restoreInlineBeforeLeaving();
      clearArticle();
      applyBrowser(await api.goBack());
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function goForward() {
    try {
      if (translating) await api.cancelTranslation();
      await restoreInlineBeforeLeaving();
      clearArticle();
      applyBrowser(await api.goForward());
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function reload() {
    try {
      if (translating) await api.cancelTranslation();
      await restoreInlineBeforeLeaving();
      clearArticle();
      applyBrowser(await api.reload());
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function stopLoading() {
    try { applyBrowser(await api.stopLoading()); }
    catch (cause) { notify("error", errorMessage(cause)); }
  }

  function goHome() {
    void navigate("tb://start");
  }

  async function reorderTab(tabId: string, beforeId: string | null) {
    if (tabId === beforeId) return;
    try {
      applyBrowser(await api.reorderTab(tabId, beforeId));
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function loadBookmarks() {
    try {
      bookmarks = await api.listBookmarks();
    } catch {
      // The bar is non-critical; keep the last known list on failure.
    }
  }

  function toggleBookmarkBar() {
    bookmarkBarVisible = !bookmarkBarVisible;
    try {
      localStorage.setItem("bookmark-bar", bookmarkBarVisible ? "true" : "false");
    } catch {
      // Storage may be unavailable; the toggle still works this session.
    }
    updateLayout();
  }

  async function setZoom(factor: number) {
    try {
      applyBrowser(await api.setZoom(factor));
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function adjustZoom(direction: 1 | -1) {
    const current = activeTab?.zoom ?? 1;
    const next = Math.round((current + direction * 0.1) * 100) / 100;
    await setZoom(next);
  }

  async function printPage() {
    try {
      await api.printPage();
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function toggleFullscreen() {
    try {
      await api.toggleFullscreen();
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function duplicateTab(id: string) {
    const tab = browser.tabs.find((item) => item.id === id);
    if (!tab) return;
    try {
      if (translating) await api.cancelTranslation();
      await restoreInlineBeforeLeaving();
      clearArticle();
      applyBrowser(await api.newTab(tab.url));
      updateLayout();
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function reopenClosedTab() {
    try {
      applyBrowser(await api.reopenClosedTab());
      updateLayout();
      notify("success", t("toast.tabReopened"));
    } catch (cause) {
      notify("info", errorMessage(cause));
    }
  }

  async function exportAppData() {
    try {
      const path = await api.exportAppData();
      notify("success", t("toast.exportedData", { path }));
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function refreshSuggestions() {
    const query = address.trim();
    const requestId = ++suggestionRequestId;
    if (!query || !addressFocused) {
      suggestions = [];
      suggestionIndex = -1;
      return;
    }
    const localExtras: AddressSuggestion[] = [];
    const mathVal = evalQuickMath(query);
    if (mathVal !== null) {
      localExtras.push({
        kind: "calc",
        title: `= ${mathVal}`,
        url: mathVal,
      });
    }
    const low = query.toLowerCase();
    for (const tab of browser.tabs) {
      if (tab.id === browser.activeTab || tab.url === "tb://start") continue;
      if (tab.title.toLowerCase().includes(low) || tab.url.toLowerCase().includes(low)) {
        localExtras.push({
          kind: "tab",
          title: tab.title || tab.url,
          url: `tab://${tab.id}`,
        });
      }
      if (localExtras.length >= 3) break;
    }
    try {
      const items = await api.addressSuggestions(query);
      if (requestId !== suggestionRequestId) return;
      const merged = [...localExtras, ...items];
      suggestions = merged;
      if (suggestionIndex >= merged.length) suggestionIndex = -1;
    } catch {
      if (requestId === suggestionRequestId) suggestions = localExtras;
    }
  }

  function onAddressInput() {
    suggestionIndex = -1;
    clearTimeout(suggestionTimer);
    suggestionTimer = setTimeout(() => void refreshSuggestions(), 140);
  }

  function chooseSuggestion(item: AddressSuggestion) {
    suggestions = [];
    suggestionIndex = -1;
    addressInput?.blur();
    if (item.url.startsWith("tab://")) {
      const tabId = item.url.slice(6);
      void selectTab(tabId);
      return;
    }
    if (item.kind === "calc") {
      void navigator.clipboard.writeText(item.url).catch(() => {});
      address = item.url;
      notify("success", `${item.title} (${trLocal("클립보드 복사됨", "Copied", "コピー済み", "已复制")})`);
      return;
    }
    void navigate(item.url);
  }

  function onAddressKeydown(event: KeyboardEvent) {
    if (event.key === "ArrowDown" && suggestions.length > 0) {
      event.preventDefault();
      suggestionIndex = (suggestionIndex + 1) % suggestions.length;
    } else if (event.key === "ArrowUp" && suggestions.length > 0) {
      event.preventDefault();
      suggestionIndex =
        (suggestionIndex - 1 + suggestions.length) % suggestions.length;
    } else if (event.key === "Enter" && suggestionIndex >= 0) {
      event.preventDefault();
      const item = suggestions[suggestionIndex];
      if (item) chooseSuggestion(item);
    } else if (event.key === "Escape") {
      suggestions = [];
      suggestionIndex = -1;
    }
  }

  function needsTranslation(raw: string): boolean {
    const stripped = raw
      .replace(/\[\[img\d+\]\]/g, "")
      .replace(/\[\[\d+\s*[|:]/g, "")
      .replace(/\]\]/g, "")
      .replace(/\{[0-9A-Fa-f\s]{2,}\}/g, "")
      .trim();
    return /\p{L}/u.test(stripped);
  }

  function translatable(blocks: Block[]): Block[] {
    return blocks.filter(
      (block) =>
        block.kind !== "code" &&
        block.kind !== "table" &&
        needsTranslation(block.text),
    );
  }

  async function translateCurrentPage(force = false) {
    if (translating) return;
    try {
      if (inlineMode && !force && article) {
        await translateAll(false);
        return;
      }
      if (inlineMode) {
        await restoreInlinePage();
        clearArticle();
      }
      const extracted = await api.extractArticle();
      article = extracted;
      translations = {};
      errors = {};
      inlineMode = true;
      await tick();
      if (force) notify("info", t("toast.retranslateBlocks", { count: extracted.blocks.length }));
      await translateAll(force);
    } catch (cause) {
      inlineMode = false;
      article = null;
      notify("error", errorMessage(cause));
    }
  }

  async function pickRegionAndTranslate() {
    if (translating) return;
    if (pickingRegion) {
      pickingRegion = false;
      await api.cancelRegionPicker().catch(() => {});
      return;
    }
    try {
      if (inlineMode) {
        await restoreInlinePage();
        clearArticle();
      }
      pickingRegion = true;
      notify("info", t("toast.pickRegionGuide"));
      const extracted = await api.pickRegionArticle();
      pickingRegion = false;
      article = extracted;
      translations = {};
      errors = {};
      inlineMode = true;
      await tick();
      await translateAll(false);
    } catch (cause) {
      pickingRegion = false;
      const msg = errorMessage(cause);
      if (msg.includes("취소")) notify("info", msg);
      else notify("error", msg);
    }
  }

  async function applySavedTranslationIfAvailable(tabId: string, pageUrl: string) {
    if (translating || inlineMode) return;
    const current = activeTab;
    if (!current || current.id !== tabId || current.isPrivate || current.url !== pageUrl) return;
    try {
      const extracted = await api.extractArticle();
      if (activeTab?.id !== tabId || activeTab?.url !== pageUrl || inlineMode || translating) return;
      const queue = translatable(extracted.blocks);
      if (queue.length === 0) return;
      const saved = await api.loadSavedPageTranslations(
        extracted.url,
        queue.map((block) => ({ id: block.id, text: block.text })),
        selectedTargetLanguage || settings.targetLanguage,
      );
      if (saved.length === 0) return;
      const blockById = new Map(extracted.blocks.map((block) => [block.id, block]));
      const nextTranslations: Record<string, SegmentResult> = {};
      const inlineItems: Array<{ key: string; translation: string }> = [];
      for (const item of saved) {
        nextTranslations[item.id] = item;
        const inlineKey = blockById.get(item.id)?.inlineKey;
        if (inlineKey) inlineItems.push({ key: inlineKey, translation: item.translation });
      }
      if (inlineItems.length === 0) return;
      await api.applyInlineTranslations(tabId, extracted.url, inlineItems);
      article = extracted;
      translations = nextTranslations;
      errors = {};
      inlineMode = true;
      notify("info", t("toast.loadedInlineCount", { count: inlineItems.length }));
    } catch {
      // Auto-applying saved translations is best-effort.
    }
  }

  function collectStyleSample(blocks: Block[] | undefined): string[] | undefined {
    if (!blocks || blocks.length === 0) return undefined;
    const ordered = blocks
      .map((block, idx) => ({ block, order: block.docIndex ?? idx }))
      .filter(({ block }) => block.kind !== "code" && block.text.trim().length > 0)
      .sort((a, b) => a.order - b.order)
      .map(({ block }) => block.text.trim());
    if (ordered.length === 0) return undefined;
    if (ordered.length <= 20) return ordered;
    const picked: string[] = ordered.slice(0, 12);
    const rest = ordered.slice(12);
    const step = Math.max(1, Math.floor(rest.length / 8));
    for (let i = 0; i < rest.length && picked.length < 20; i += step) {
      picked.push(rest[i]);
    }
    return picked;
  }

  function persistTranslationStyle(nextStyle: string) {
    const clean = VALID_TRANSLATION_STYLES.has(nextStyle) ? nextStyle : "auto";
    selectedStyle = clean;
    saveLocalStyle(clean);
    void api
      .setTranslationPrefs(clean)
      .then((view) => {
        settingsView = view;
      })
      .catch(() => {});
  }

  function persistTranslationLanguage(nextLang: string) {
    const clean = nextLang.trim();
    if (!clean) return;
    selectedTargetLanguage = clean;
    void api
      .setTranslationPrefs(undefined, clean)
      .then((view) => {
        settingsView = view;
      })
      .catch(() => {});
  }

  function collectPriorContext(
    allBlocks: Block[],
    targetBlocks: Block[],
    translationMap: Record<string, SegmentResult>,
  ): TranslationContextPair[] {
    if (allBlocks.length === 0 || targetBlocks.length === 0) return [];
    const targetIds = new Set(targetBlocks.map((b) => b.id));
    const docOrdered = allBlocks
      .map((block, idx) => ({ block, order: block.docIndex ?? idx }))
      .filter(({ block }) => block.kind !== "code")
      .sort((a, b) => a.order - b.order)
      .map(({ block }) => block);

    let earliestIdx = docOrdered.length;
    for (let i = 0; i < docOrdered.length; i++) {
      if (targetIds.has(docOrdered[i].id)) {
        earliestIdx = i;
        break;
      }
    }
    if (earliestIdx === docOrdered.length) earliestIdx = 0;

    const pairs: TranslationContextPair[] = [];
    const seenSources = new Set<string>();
    const appendPair = (block: Block, allowUntranslated: boolean) => {
      const src = block.text.trim();
      if (!src || seenSources.has(src)) return;
      const done = translationMap[block.id]?.translation?.trim();
      if (!done && !allowUntranslated) return;
      seenSources.add(src);
      pairs.push(done ? { source: src, translation: done } : { source: src, translation: null });
    };

    const neighborStart = Math.max(0, earliestIdx - 4);
    for (let i = 0; i < neighborStart && pairs.length < 8; i++) {
      const b = docOrdered[i];
      if (!targetIds.has(b.id) && translationMap[b.id]?.translation) {
        appendPair(b, false);
      }
    }
    for (let i = neighborStart; i < earliestIdx; i++) {
      const b = docOrdered[i];
      if (!targetIds.has(b.id)) {
        appendPair(b, earliestIdx - i <= 2);
      }
    }
    return pairs.slice(-14);
  }

  async function translateAll(force = false) {
    const sourceArticle = article;
    if (!sourceArticle || translating) return;
    const model = translationModel.trim();
    if (!model) {
      notify("error", t("toast.needModelSelect"));
      return;
    }
    const sourceTabId = activeTab?.id;
    const applyInline = inlineMode && !!sourceTabId;
    const queue = translatable(sourceArticle.blocks).filter(
      (block) => force || !translations[block.id],
    );
    if (queue.length === 0) {
      notify("info", t("toast.noTranslateSegments"));
      return;
    }

    const isHyMt2 = settings.provider === "hyMt2";
    if (isHyMt2) {
      try {
        const list = await api.listHyMt2Models();
        if (!list.some((m) => m.installed)) {
          notify("info", t("toast.noLocalModel"));
          void openOverlay("translate");
          return;
        }
      } catch {
        // Proceed cleanly to surface exact backend error if listing fails.
      }
    }

    const targetLang = (selectedTargetLanguage || settings.targetLanguage).trim() || "한국어";
    const activeStyle = (selectedStyle || settings.style || "auto").trim();
    const styleSample = activeStyle === "auto" ? collectStyleSample(sourceArticle.blocks) : undefined;
    const uniqueQueue: Block[] = [];
    const duplicatesByKey = new Map<string, Block[]>();
    for (const block of queue) {
      const key = block.text.replace(/\s+/g, " ").trim();
      const existing = duplicatesByKey.get(key);
      if (existing) {
        existing.push(block);
      } else {
        duplicatesByKey.set(key, [block]);
        uniqueQueue.push(block);
      }
    }

    translating = true;
    manualTranslateCancel = false;
    detectedAutoStyle = null;
    errors = {};
    progress = { done: 0, total: uniqueQueue.length };
    let uncertainTotal = 0;
    let missingTotal = 0;
    let batchFailureCount = 0;
    let lastError = "";
    const blockById = new Map(sourceArticle.blocks.map((block) => [block.id, block]));
    const completedIds = new Set<string>();
    let pendingQueue = [...uniqueQueue];
    let pendingInlineQueue: Array<{
      key: string;
      translation: string;
      uncertain?: boolean;
      notes?: string[];
    }> = [];
    let inlineFlushTimer: ReturnType<typeof setTimeout> | undefined;
    let nextTranslations = { ...translations };
    let mismatchNotified = false;
    let lastViewportSeq = -1;

    async function flushInlineUpdates() {
      if (!applyInline || !sourceTabId || !inlineMode || pendingInlineQueue.length === 0) {
        pendingInlineQueue = [];
        return;
      }
      const toApply = pendingInlineQueue;
      pendingInlineQueue = [];
      try {
        const applied = await api.applyInlineTranslations(
          sourceTabId,
          sourceArticle!.url,
          toApply,
          bilingualAll,
        );
        if (applied < toApply.length && !mismatchNotified) {
          mismatchNotified = true;
          notify("info", t("toast.inlinePartialApplied", { count: toApply.length - applied }));
        }
      } catch {
        // Best effort when page navigates mid-flush.
      }
    }

    function scheduleInlineFlush() {
      if (inlineFlushTimer !== undefined) return;
      inlineFlushTimer = setTimeout(() => {
        inlineFlushTimer = undefined;
        void flushInlineUpdates();
      }, 16);
    }

    function applyCompletedResult(result: SegmentResult, countForUnresolved?: Set<string>) {
      if (article !== sourceArticle) return;
      completedIds.add(result.id);
      countForUnresolved?.delete(result.id);
      if (result.uncertain) uncertainTotal += 1;
      const repBlock = blockById.get(result.id);
      const key = repBlock ? repBlock.text.replace(/\s+/g, " ").trim() : "";
      const targets = key ? (duplicatesByKey.get(key) ?? []) : [];
      if (targets.length === 0 && repBlock) targets.push(repBlock);
      for (const target of targets) {
        nextTranslations[target.id] = { ...result, id: target.id };
        if (target.inlineKey) {
          pendingInlineQueue.push({
            key: target.inlineKey,
            translation: result.translation,
            uncertain: result.uncertain,
            notes: result.notes,
          });
        }
      }
      translations = { ...nextTranslations };
      progress = { done: completedIds.size, total: uniqueQueue.length };
      scheduleInlineFlush();
    }

    async function reprioritizeQueue(): Promise<number> {
      if (!applyInline) return lastViewportSeq;
      try {
        const order = await api.getViewportInlineOrder();
        if (order.keys.length > 0 && order.seq !== lastViewportSeq) {
          const rank = new Map<string, number>();
          order.keys.forEach((k, idx) => rank.set(k, idx));
          pendingQueue.sort((a, b) => {
            const ra = a.inlineKey ? (rank.get(a.inlineKey) ?? 999_999) : 999_999;
            const rb = b.inlineKey ? (rank.get(b.inlineKey) ?? 999_999) : 999_999;
            return ra - rb;
          });
          lastViewportSeq = order.seq;
        }
      } catch {
        // Keep current queue order if webview order query is unavailable.
      }
      return lastViewportSeq;
    }

    try {
      let chunkIndex = 0;
      while (
        pendingQueue.length > 0 &&
        article === sourceArticle &&
        !manualTranslateCancel &&
        (!sourceTabId || activeTab?.id === sourceTabId)
      ) {
        const chunkSeq = await reprioritizeQueue();
        const maxCount = isHyMt2 ? (chunkIndex === 0 ? 8 : 32) : chunkIndex === 0 ? 6 : 36;
        const maxChars = isHyMt2 ? (chunkIndex === 0 ? 1_800 : 6_400) : 7_500;
        const chunk: Block[] = [];
        let chars = 0;
        for (const candidate of pendingQueue) {
          if (completedIds.has(candidate.id)) continue;
          const len = candidate.text.length;
          if (chunk.length > 0 && (chunk.length >= maxCount || chars + len > maxChars)) break;
          chunk.push(candidate);
          chars += len;
        }
        if (chunk.length === 0) break;
        chunkIndex += 1;
        const chunkPriorContext = collectPriorContext(sourceArticle.blocks, chunk, nextTranslations);

        const chunkIds = new Set(chunk.map((b) => b.id));
        const unresolved = new Set(chunk.map((b) => b.id));
        let reprioritizeAbort = false;
        let scrollWatchTimer: ReturnType<typeof setInterval> | undefined;

        if (isHyMt2 && applyInline && pendingQueue.length > chunk.length) {
          scrollWatchTimer = setInterval(async () => {
            if (reprioritizeAbort || manualTranslateCancel) return;
            try {
              const order = await api.getViewportInlineOrder();
              if (!order.keys.length || order.seq === chunkSeq) return;
              const hotKeys = new Set(order.keys.slice(0, 12));
              const remainingChunkVisible = chunk.some(
                (b) => !completedIds.has(b.id) && b.inlineKey && hotKeys.has(b.inlineKey),
              );
              const unseenVisible = pendingQueue.some(
                (b) =>
                  !completedIds.has(b.id) &&
                  !chunkIds.has(b.id) &&
                  b.inlineKey &&
                  hotKeys.has(b.inlineKey),
              );
              if (!remainingChunkVisible && unseenVisible) {
                reprioritizeAbort = true;
                void api.cancelTranslation().catch(() => {});
              }
            } catch {
              // Ignore transient scroll polling failures.
            }
          }, 480);
        }

        try {
          const results = await api.translateBlocksStream(
            chunk.map((block) => ({ id: block.id, text: block.text })),
            (event) => {
              if (event.type === "started") {
                detectedAutoStyle = event.detectedStyle ?? null;
              } else if (event.type === "segment" && !completedIds.has(event.result.id)) {
                applyCompletedResult(event.result, unresolved);
              }
            },
            activeStyle,
            force,
            model,
            sourceArticle.url,
            sourceArticle.title,
            targetLang,
            chunkPriorContext,
            styleSample,
          );
          if (article !== sourceArticle) break;
          for (const result of results) {
            if (!completedIds.has(result.id)) {
              applyCompletedResult(result, unresolved);
            }
          }
          if (unresolved.size > 0) {
            missingTotal += unresolved.size;
            const nextErrors = { ...errors };
            for (const id of unresolved) {
              completedIds.add(id);
              nextErrors[id] = t("toast.segmentMissing");
            }
            errors = nextErrors;
          }
          pendingQueue = pendingQueue.filter((b) => !completedIds.has(b.id));
        } catch (cause) {
          const message = errorMessage(cause);
          if (message.includes("취소")) {
            if (reprioritizeAbort && !manualTranslateCancel) {
              pendingQueue = pendingQueue.filter((b) => !completedIds.has(b.id));
              continue;
            }
            notify("info", message);
            break;
          }
          for (const b of chunk) {
            completedIds.add(b.id);
          }
          pendingQueue = pendingQueue.filter((b) => !completedIds.has(b.id));
          const fatal =
            message.includes("API 키") ||
            message.includes("인증") ||
            message.includes("한도") ||
            message.includes("모델") ||
            message.includes("다운로드");
          const nextErrors = { ...errors };
          for (const block of chunk) {
            nextErrors[block.id] = message;
          }
          errors = nextErrors;
          batchFailureCount += 1;
          lastError = message;
          if (fatal) {
            notify("error", message);
            break;
          }
        } finally {
          if (scrollWatchTimer !== undefined) {
            clearInterval(scrollWatchTimer);
          }
        }
        progress = {
          done: completedIds.size,
          total: uniqueQueue.length,
        };
      }
      if (inlineFlushTimer !== undefined) {
        clearTimeout(inlineFlushTimer);
        inlineFlushTimer = undefined;
      }
      await flushInlineUpdates();
    } finally {
      if (inlineFlushTimer !== undefined) {
        clearTimeout(inlineFlushTimer);
      }
      translating = false;
    }

    if (article !== sourceArticle) return;
    if (batchFailureCount > 0 && lastError) {
      notify("error", t("toast.batchFailed", { count: batchFailureCount, error: lastError }));
    } else if (missingTotal > 0) {
      notify("error", t("toast.missingSegments", { count: missingTotal }));
    } else if (uncertainTotal > 0) {
      notify("info", t("toast.uncertainSegments", { count: uncertainTotal }));
    } else if (progress.done >= progress.total) {
      const styleNote = activeStyle === "auto" && detectedAutoStyle ? ` (${detectedAutoStyle})` : "";
      notify("success", t("toast.translationDoneFull", { target: targetLang, styleNote, count: progress.total }));
    }
  }

  async function cancelTranslation() {
    manualTranslateCancel = true;
    try {
      await api.cancelTranslation();
      notify("info", t("toast.cancellingTranslation"));
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function restoreOriginalPage() {
    try {
      if (translating) await api.cancelTranslation();
      await restoreInlinePage();
      clearArticle();
      notify("success", t("toast.restoredOriginal"));
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function loadTranslationModels() {
    if (loadingModels) return;
    const requestId = ++modelRequestId;
    loadingModels = true;
    try {
      const result = await api.listModels({ ...settings });
      if (requestId !== modelRequestId) return;
      availableModels = result;
      if (!selectedModel) {
        selectedModel = result.find((model) => model.name === settings.model)?.name
          ?? result[0]?.name
          ?? "";
      }
      notify("success", t("toast.modelsLoaded", { count: result.length }));
    } catch (cause) {
      if (requestId === modelRequestId) notify("error", errorMessage(cause));
    } finally {
      if (requestId === modelRequestId) loadingModels = false;
    }
  }

  function exportProject() {
    if (exporting) return;
    void openOverlay("export-project");
  }

  async function runExportProject(includeOriginal: boolean) {
    if (exporting) return;
    exporting = true;
    try {
      const path = await api.exportProject(includeOriginal);
      notify("success", t("toast.projectExported", { path }));
      await closeOverlay();
    } catch (cause) {
      notify("error", errorMessage(cause));
    } finally {
      exporting = false;
    }
  }

  async function openSettings() {
    await openInternalTab("settings");
  }

  function openFeedback(draft?: Partial<FeedbackDraft>) {
    feedbackDraft = {
      category: draft?.category ?? "bug",
      summary: draft?.summary ?? "",
      description: draft?.description ?? "",
      pageUrl: draft?.pageUrl ?? (activeTab?.url?.startsWith("http") ? activeTab.url : ""),
      sourceText: draft?.sourceText ?? "",
      translatedText: draft?.translatedText ?? "",
    };
    void openOverlay("feedback");
  }

  async function openInternalTab(kind: InternalPageKind) {
    try {
      if (overlayKind) await closeOverlay();
      applyBrowser(await api.openInternalTab(kind));
      updateLayout();
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function openOverlay(kind: OverlayKind, anchor?: OverlayAnchor | null, tabId?: string | null) {
    overlayKind = kind;
    overlayAnchor = anchor ?? null;
    if (tabId !== undefined) tabMenuTargetId = tabId ?? null;
    overlayChromeTop = headerHeight;
    if (kind === "tab-search") tabSearchQuery = "";
    if (kind === "workspace") {
      workspaceMode = "list";
      workspaceDraftName = "";
      workspaceBusy = false;
    }
    if (kind === "rename-window") {
      renameWindowDraft = "";
    }
    if (kind === "translate" && settings.provider === "hyMt2") {
      void refreshOverlayHyMt2State();
    }
    try {
      await api.openOverlay(kind, anchor ?? null, tabId ?? null);
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  function anchorOf(event: MouseEvent): OverlayAnchor {
    const r = (event.currentTarget as HTMLElement).getBoundingClientRect();
    return { left: r.left, top: r.top, right: r.right, bottom: r.bottom };
  }

  function popupPositionStyle(align: "left" | "right", width: number): string {
    if (!overlayAnchor) return "";
    const top = Math.round(overlayAnchor.bottom + 4);
    if (align === "left") {
      const left = Math.max(8, Math.min(Math.round(overlayAnchor.left), window.innerWidth - width - 8));
      return `position:fixed;left:${left}px;top:${top}px;`;
    }
    const right = Math.max(8, Math.round(window.innerWidth - overlayAnchor.right));
    return `position:fixed;right:${right}px;top:${top}px;`;
  }

  function pageMenuPositionStyle(width = 258, approxHeight = 360): string {
    const cx = pageMenuContext?.x ?? overlayAnchor?.left ?? 120;
    const cy = pageMenuContext?.y ?? overlayAnchor?.top ?? 120;
    const left = Math.max(8, Math.min(Math.round(cx), window.innerWidth - width - 10));
    const top = Math.max(8, Math.min(Math.round(cy), window.innerHeight - approxHeight - 10));
    return `position:fixed;left:${left}px;top:${top}px;`;
  }

  async function pasteFromClipboardToPage() {
    try {
      const text = await navigator.clipboard.readText();
      await api.runPageContextAction("paste", text);
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function translateAndReplaceInInput(sourceText: string, outLang: string) {
    const model = translationModel.trim();
    if (!model) {
      notify("error", t("toast.quickNeedModel"));
      return;
    }
    try {
      notify("info", trLocal(`${outLang}로 역번역 중…`, `Translating to ${outLang}…`, `${outLang}に翻訳中…`, `正在翻译为 ${outLang}…`));
      const res = await api.translateBlocks(
        [{ id: "outbound-input", text: sourceText }],
        "natural",
        true,
        model,
        activeTab?.url,
        activeTab?.title,
        outLang,
      );
      const translated = res[0]?.translation?.trim();
      if (translated) {
        await api.runPageContextAction("paste", translated);
        notify("success", trLocal("번역된 문장으로 입력창에 바로 치환했습니다.", "Replaced selection with translation.", "翻訳文で入力欄を置換しました。", "已用译文替换输入框选中内容。"));
      }
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function closeOverlay() {
    if (overlayKind) {
      overlayKind = null;
      overlayAnchor = null;
      tabMenuTargetId = null;
    }
    workspaceMode = "list";
    workspaceDraftName = "";
    workspaceBusy = false;
    feedbackDraft = null;
    try {
      await api.closeOverlay();
      await tick();
      updateLayout();
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function loadOverlayData(kind: OverlayKind) {
    try {
      try {
        bookmarkBarVisible = localStorage.getItem("bookmark-bar") === "true";
        compactChrome = localStorage.getItem("compact-chrome") === "true";
        glassTinted = localStorage.getItem("glass-tinted") === "true";
        accentColor = localStorage.getItem("accent-color") || "#684cff";
        themeAutoSync = localStorage.getItem("theme-auto-sync") !== "false";
      } catch {
        // Ignore storage errors.
      }
      if (kind === "settings" || kind === "translate") {
        applySettings(await api.loadSettings());
      }
      if (kind === "translate") {
        void refreshOverlayHyMt2State();
      }
      if (
        kind === "bookmarks" ||
        kind === "bookmark-edit" ||
        kind === "site-security" ||
        kind === "workspace" ||
        kind === "tab-menu" ||
        kind === "chrome-menu" ||
        kind === "page-menu" ||
        kind === "tab-search" ||
        kind === "task-manager" ||
        kind === "menu" ||
        kind === "feedback"
      ) {
        applyBrowser(await api.browserState());
      }
      if (kind === "bookmark-edit") {
        await loadBookmarks();
        const curUrl = activeTab?.url ?? "";
        let found: Bookmark | undefined;
        try {
          const norm = normalizeBookmarkUrl(curUrl);
          found = bookmarks.find((b) => {
            try { return normalizeBookmarkUrl(b.url) === norm; } catch { return false; }
          });
        } catch {
          found = bookmarks.find((b) => b.url === curUrl);
        }
        if (found) {
          bmDraftId = found.id;
          bmDraftTitle = found.title;
          bmDraftUrl = found.url;
          bmDraftFolder = found.folder;
          bmDraftFavorite = found.favorite;
          bmDraftCreatedAt = found.createdAt;
        } else {
          bmDraftId = null;
          bmDraftTitle = activeTab?.title || curUrl;
          bmDraftUrl = curUrl;
          bmDraftFolder = "";
          bmDraftFavorite = false;
          bmDraftCreatedAt = 0;
        }
      }
      if (kind === "feedback" && !feedbackDraft) {
        feedbackDraft = {
          category: "bug",
          pageUrl: activeTab?.url?.startsWith("http") ? activeTab.url : "",
        };
      }
      if (kind === "tab-search") {
        tabSearchQuery = "";
      }
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function refreshOverlayHyMt2State() {
    try {
      const [hw, list] = await Promise.all([api.detectHyMt2Hardware(), api.listHyMt2Models()]);
      overlayHyMt2Hw = hw;
      overlayHyMt2Models = list;
      const installed = list.filter((m) => m.installed);
      if (installed.length > 0) {
        availableModels = installed.map((m) => ({
          name: m.id,
          displayName: `${m.displayName} (${(m.installedBytes / (1024 ** 3)).toFixed(2)} GB)`,
        }));
        if (!selectedModel || !installed.some((m) => m.id === selectedModel)) {
          selectedModel = installed.find((m) => m.recommended)?.id ?? installed[0].id;
        }
      }
    } catch {
      // Best-effort overlay refresh.
    }
  }

  async function downloadRecommendedInOverlay() {
    if (overlayDownloadingId) return;
    const targetId = overlayHyMt2Hw?.recommendedModelId || "Hy-MT2-1.8B-Q4_K_M.gguf";
    overlayDownloadingId = targetId;
    overlayDownloadProgress = {
      modelId: targetId,
      status: "starting",
      downloadedBytes: 0,
      totalBytes: 0,
      percent: 0,
      speedMbps: 0,
      message: "다운로드 준비 중…",
    };
    try {
      const done = await api.downloadHyMt2Model(targetId, (p) => {
        overlayDownloadProgress = p;
      });
      selectedModel = done.id;
      await refreshOverlayHyMt2State();
      notify("success", t("toast.modelDownloadDone", { name: done.displayName }));
    } catch (cause) {
      const msg = errorMessage(cause);
      if (msg.includes("취소") || msg.toLowerCase().includes("cancel")) {
        notify("info", t("toast.modelDownloadPaused"));
      } else {
        notify("error", msg);
      }
    } finally {
      overlayDownloadingId = null;
      overlayDownloadProgress = null;
    }
  }

  async function cancelOverlayDownload() {
    if (!overlayDownloadingId) return;
    try {
      await api.cancelHyMt2Download(overlayDownloadingId);
    } catch {
      // Ignore cancel race errors.
    }
  }

  async function triggerTranslateFromOverlay(
    action: "translate" | "retranslate" | "restore" | "region" | "cancel",
  ) {
    await closeOverlay();
    switch (action) {
      case "translate": void translateCurrentPage(false); break;
      case "retranslate": void translateCurrentPage(true); break;
      case "restore": void restoreOriginalPage(); break;
      case "region": void pickRegionAndTranslate(); break;
      case "cancel": void cancelTranslation(); break;
    }
  }

  async function navigateFromOverlay(url: string) {
    try {
      applyBrowser(await api.navigate(url));
      await closeOverlay();
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  function applySettings(view: SettingsView) {
    setUiLang(view.settings.uiLanguage);
    const providerChanged =
      settingsView !== null &&
      (settingsView.settings.provider !== view.settings.provider ||
        settingsView.settings.baseUrl !== view.settings.baseUrl ||
        settingsView.settings.model !== view.settings.model);
    settingsView = view;
    selectedModel = view.settings.model;
    selectedTargetLanguage = view.settings.targetLanguage || "한국어";
    selectedStyle = VALID_TRANSLATION_STYLES.has(view.settings.style)
      ? view.settings.style
      : "auto";
    saveLocalStyle(selectedStyle);
    if (providerChanged) {
      availableModels = [];
      modelRequestId++;
      loadingModels = false;
    }
  }

  function onSettingsSaved(view: SettingsView) {
    applySettings(view);
    void emit("settings-updated", view).catch(() => {});
  }

  async function openFindBar() {
    if (overlayKind) await closeOverlay();
    findBarOpen = true;
    await tick();
    updateLayout();
    findInput?.focus();
    findInput?.select();
    if (findQuery.trim()) {
      await runFind("find");
    }
  }

  async function closeFindBar() {
    if (!findBarOpen) return;
    findBarOpen = false;
    findResult = { total: 0, current: 0 };
    await tick();
    updateLayout();
    await api.findInPage("", "clear").catch(() => {});
  }

  async function runFind(action: "find" | "next" | "prev" | "clear") {
    try {
      findResult = await api.findInPage(findQuery, action);
    } catch (cause) {
      if (action !== "clear") notify("error", errorMessage(cause));
    }
  }

  function onKeydown(event: KeyboardEvent) {
    if (event.key === "Escape" && event.shiftKey && !event.ctrlKey && !event.altKey) {
      event.preventDefault();
      void openOverlay("task-manager");
      return;
    }
    if (event.key === "Escape") {
      if (overlayKind) void closeOverlay();
      else if (findBarOpen) void closeFindBar();
      return;
    }
    if (event.key === "F5") {
      event.preventDefault();
      void reload();
      return;
    }
    if (event.key === "F12") {
      event.preventDefault();
      void api.runPageContextAction("inspect");
      return;
    }
    if (event.key === "F11") {
      event.preventDefault();
      void toggleFullscreen();
      return;
    }
    if (event.altKey && !event.ctrlKey && !event.metaKey) {
      if (event.key === "ArrowLeft") { event.preventDefault(); void goBack(); }
      else if (event.key === "ArrowRight") { event.preventDefault(); void goForward(); }
      else if (event.key === "Home") { event.preventDefault(); goHome(); }
      return;
    }
    if (!event.ctrlKey || event.altKey || event.metaKey) return;
    if (event.key === "PageUp" || event.key === "PageDown") {
      event.preventDefault();
      const tabs = visibleTabs;
      if (tabs.length > 1) {
        const current = tabs.findIndex((tab) => tab.id === browser.activeTab);
        const step = event.key === "PageDown" ? 1 : -1;
        const target = tabs[(current + step + tabs.length) % tabs.length];
        if (target) void selectTab(target.id);
      }
      return;
    }
    if (event.key === "Delete" && event.shiftKey) {
      event.preventDefault();
      void openInternalTab("settings");
      return;
    }
    const key = event.key.toLowerCase();
    if (key === "l") {
      event.preventDefault();
      addressInput?.focus();
      addressInput?.select();
    } else if (key === "t" && event.shiftKey) {
      event.preventDefault();
      void reopenClosedTab();
    } else if (key === "t") {
      event.preventDefault();
      void newTab();
    } else if (key === "w") {
      if (browser.activeTab) {
        event.preventDefault();
        void closeTab(browser.activeTab);
      }
    } else if (key === "r") {
      event.preventDefault();
      void reload();
    } else if (key === "d" && event.shiftKey) {
      event.preventDefault();
      void bookmarkAllTabs();
    } else if (key === "d") {
      event.preventDefault();
      void saveCurrentBookmark();
    } else if (key === "a" && event.shiftKey) {
      event.preventDefault();
      void openOverlay("tab-search");
    } else if (key === "m" && !event.shiftKey) {
      if (browser.activeTab) {
        event.preventDefault();
        void toggleMuteTab(browser.activeTab);
      }
    } else if (key === "f") {
      event.preventDefault();
      void openFindBar();
    } else if (key === "h" && !event.shiftKey) {
      event.preventDefault();
      void openInternalTab("visits");
    } else if (key === "j") {
      event.preventDefault();
      void openInternalTab("downloads");
    } else if (key === "o" && event.shiftKey) {
      event.preventDefault();
      void openInternalTab("bookmarks");
    } else if (key === "p" && event.shiftKey) {
      event.preventDefault();
      void newPrivateTab();
    } else if (key === "p") {
      event.preventDefault();
      void printPage();
    } else if (key === "b" && event.shiftKey) {
      event.preventDefault();
      toggleBookmarkBar();
    } else if (key === "+" || key === "=") {
      event.preventDefault();
      void adjustZoom(1);
    } else if (key === "-") {
      event.preventDefault();
      void adjustZoom(-1);
    } else if (key === "0") {
      event.preventDefault();
      void setZoom(1);
    } else if (/^[1-9]$/.test(key)) {
      const num = Number(key);
      const tabs = visibleTabs;
      if (tabs.length > 0) {
        event.preventDefault();
        const target = num === 9 ? tabs[tabs.length - 1] : tabs[num - 1];
        if (target) void selectTab(target.id);
      }
    }
  }

  async function saveCurrentBookmark() {
    if (!activeTab) return;
    await openOverlay("bookmark-edit");
  }

  async function submitBookmarkDraft() {
    const cleanUrl = bmDraftUrl.trim();
    if (!cleanUrl) return;
    try {
      const normalized = cleanUrl.startsWith("tb://") ? cleanUrl : normalizeBookmarkUrl(cleanUrl);
      const now = Math.floor(Date.now() / 1000);
      await api.saveBookmark({
        id: bmDraftId ?? crypto.randomUUID(),
        url: normalized,
        title: bmDraftTitle.trim() || normalized,
        folder: bmDraftFolder.trim(),
        favorite: bmDraftFavorite,
        createdAt: bmDraftCreatedAt || now,
        updatedAt: now,
      });
      await loadBookmarks();
      notify("success", t("toast.bookmarkSaved"));
      await closeOverlay();
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  async function deleteBookmarkDraft() {
    if (!bmDraftId) return;
    try {
      await api.deleteBookmark(bmDraftId);
      await loadBookmarks();
      await closeOverlay();
    } catch (cause) {
      notify("error", errorMessage(cause));
    }
  }

  onMount(() => {
    loadLocalBrowserExtras();
    try {
      glassTinted = localStorage.getItem("glass-tinted") === "true";
      accentColor = localStorage.getItem("accent-color") || "#684cff";
      themeAutoSync = localStorage.getItem("theme-auto-sync") !== "false";
    } catch {
      // Ignore storage errors.
    }
    if (pageKind) {
      requestAnimationFrame(() => {
        try {
          (
            window as Window & {
              chrome?: { webview?: { postMessage?: (msg: string) => void } };
            }
          ).chrome?.webview?.postMessage?.('{"type":"internal-ready"}');
        } catch {
          // Ignore outside WebView2 host.
        }
      });
      void api.loadSettings().then((view) => {
        applySettings(view);
      }).catch(() => {});
      void api.browserState().then(applyBrowser).catch(() => {});
      if (pageKind === "start") {
        void loadBookmarks();
        void api.listHistory("", 12).then((items) => {
          recentVisits = items;
        }).catch(() => {});
      }
      let unlistenPageSettings: UnlistenFn | undefined;
      let unlistenPagePrefs: UnlistenFn | undefined;
      listen<SettingsView>("settings-updated", (event) => {
        applySettings(event.payload);
      }).then((dispose) => {
        unlistenPageSettings = dispose;
      });
      listen<{ glassTinted?: boolean; accentColor?: string; themeAutoSync?: boolean }>(
        "ui-prefs-updated",
        (event) => {
          if (typeof event.payload?.glassTinted === "boolean") {
            glassTinted = event.payload.glassTinted;
          }
          if (typeof event.payload?.accentColor === "string" && event.payload.accentColor) {
            accentColor = event.payload.accentColor;
          }
          if (typeof event.payload?.themeAutoSync === "boolean") {
            themeAutoSync = event.payload.themeAutoSync;
          }
        },
      ).then((dispose) => {
        unlistenPagePrefs = dispose;
      });
      return () => {
        unlistenPageSettings?.();
        unlistenPagePrefs?.();
        clearTimeout(statusTimer);
      };
    }

    const handlePageAction = async (action: {
      type?: string;
      text?: string;
      source?: string;
      translation?: string;
      doNotTranslate?: boolean;
      key?: string;
    }) => {
      if (!action?.type) return;
      const targetLang = (selectedTargetLanguage || settings.targetLanguage).trim() || "한국어";
      const activeStyle = (selectedStyle || settings.style || "auto").trim();
      const autoSample = activeStyle === "auto" ? collectStyleSample(article?.blocks) : undefined;
      if (action.type === "quick-translate" && action.text) {
        const model = translationModel.trim();
        if (!model) {
          await api.showQuickTranslation(t("toast.quickNeedModel"), true);
          return;
        }
        try {
          const res = await api.translateBlocks(
            [{ id: "quick", text: action.text }],
            activeStyle,
            false,
            model,
            activeTab?.url,
            activeTab?.title,
            targetLang,
            undefined,
            autoSample,
          );
          await api.showQuickTranslation(res[0]?.translation || t("toast.quickEmpty"), false);
        } catch (cause) {
          await api.showQuickTranslation(errorMessage(cause), true);
        }
      } else if (action.type === "add-glossary" && action.source) {
        const entry = {
          ...emptyGlossaryEntry(),
          id: crypto.randomUUID(),
          source: action.source,
          translation: action.translation ?? action.source,
          doNotTranslate: Boolean(action.doNotTranslate),
        };
        await api.saveGlossary(entry);
        notify("success", t("toast.glossaryAdded", { source: action.source }));
      } else if (action.type === "retranslate" && action.key) {
        const targetBlock = article?.blocks.find((b) => b.inlineKey === action.key);
        const model = translationModel.trim();
        if (!targetBlock || !article || !model || !activeTab) return;
        const priorContext = collectPriorContext(article.blocks, [targetBlock], translations);
        const res = await api.translateBlocks(
          [{ id: targetBlock.id, text: targetBlock.text }],
          activeStyle,
          true,
          model,
          article.url,
          article.title,
          targetLang,
          priorContext,
          autoSample,
        );
        if (res[0]) {
          translations = { ...translations, [targetBlock.id]: res[0] };
          await api.applyInlineTranslations(
            activeTab.id,
            article.url,
            [
              {
                key: action.key,
                translation: res[0].translation,
                uncertain: res[0].uncertain,
                notes: res[0].notes,
              },
            ],
            bilingualAll,
          );
          notify("success", t("toast.segmentRetranslated"));
        }
      } else if (action.type === "edit" && action.key) {
        const targetBlock = article?.blocks.find((b) => b.inlineKey === action.key);
        if (!targetBlock || !article) return;
        const edited = action.translation ?? "";
        await api.savePageTranslationOverride(
          article.url,
          targetBlock.text,
          edited,
          targetLang,
        );
        const prev = translations[targetBlock.id];
        if (prev) {
          translations = {
            ...translations,
            [targetBlock.id]: { ...prev, translation: edited, uncertain: false, notes: [] },
          };
        }
        notify("success", t("toast.segmentEdited"));
      } else if (action.type === "report-translation") {
        const targetBlock = action.key
          ? article?.blocks.find((b) => b.inlineKey === action.key)
          : undefined;
        const srcText = action.source?.trim() || targetBlock?.text || "";
        const trText =
          action.translation?.trim() ||
          (targetBlock ? translations[targetBlock.id]?.translation : "") ||
          "";
        openFeedback({
          category: "translation",
          pageUrl: article?.url || (activeTab?.url?.startsWith("http") ? activeTab.url : ""),
          sourceText: srcText,
          translatedText: trText,
        });
      }
    };

    const overlayWindow = window as Window & {
      __SET_APP_OVERLAY_KIND__?: (
        kind: OverlayKind | null,
        anchor?: OverlayAnchor | null,
        tabId?: string | null,
        chromeTop?: number | null,
      ) => void;
      __CLEAR_APP_OVERLAY__?: () => void;
      __ON_BROWSER_MESSAGE__?: (msg: {
        type?: string;
        p?: number;
        x?: number;
        y?: number;
        bg?: string;
        isDark?: boolean;
        linkUrl?: string;
        linkText?: string;
        imageUrl?: string;
        selectedText?: string;
        isEditable?: boolean;
        hasMedia?: boolean;
        origin?: string;
        username?: string;
        password?: string;
        text?: string;
        source?: string;
        translation?: string;
        doNotTranslate?: boolean;
        key?: string;
        lang?: string;
        url?: string;
        playing?: boolean;
        paused?: boolean;
        index?: number;
        total?: number;
        tabId?: string;
      }) => void;
    };
    overlayWindow.__ON_BROWSER_MESSAGE__ = (msg) => {
      if (!msg) return;
      if (
        msg.type === "quick-translate" ||
        msg.type === "add-glossary" ||
        msg.type === "retranslate" ||
        msg.type === "edit" ||
        msg.type === "report-translation"
      ) {
        void handlePageAction(msg).catch((e) => notify("error", errorMessage(e)));
        return;
      }
      if (msg.type === "dynamic-blocks-added") {
        if (inlineMode && !translating) {
          void translateCurrentPage(false);
        }
        return;
      }
      if (msg.type === "tts-state") {
        ttsActive = Boolean(msg.playing);
        ttsProgress = { index: msg.index ?? 0, total: msg.total ?? 0 };
        void tick().then(() => updateLayout());
        return;
      }
      if (msg.type === "media-audio-state") {
        const tid = msg.tabId || browser.activeTab;
        if (tid) {
          audibleTabs = { ...audibleTabs, [tid]: Boolean(msg.playing) };
        }
        return;
      }
      if (msg.type === "page-language-detected" && msg.lang) {
        const curUrl = activeTab?.url ?? msg.url ?? "";
        if (!curUrl.startsWith("http") || inlineMode || translating) return;
        const targetRaw = (selectedTargetLanguage || settings.targetLanguage || "한국어").toLowerCase();
        const targetCode = targetRaw.includes("한국") || targetRaw === "ko"
          ? "ko"
          : targetRaw.includes("日本") || targetRaw === "ja"
            ? "ja"
            : targetRaw.includes("中文") || targetRaw.startsWith("zh")
              ? "zh"
              : "en";
        if (msg.lang === targetCode || msg.lang === "unknown") {
          autoTranslatePrompt = null;
          return;
        }
        let host = "";
        try {
          host = new URL(curUrl).hostname.toLowerCase();
        } catch {}
        if (host && autoTranslateHosts.includes(host)) {
          autoTranslatePrompt = null;
          void translateCurrentPage(false);
        } else if (host) {
          autoTranslatePrompt = { lang: msg.lang.toUpperCase(), host };
          void tick().then(() => updateLayout());
        }
        return;
      }
      if (msg.type === "page-contextmenu") {
        const cx = typeof msg.x === "number" ? msg.x : 120;
        const cy = typeof msg.y === "number" ? msg.y : 120;
        pageMenuContext = {
          x: cx,
          y: cy,
          linkUrl: msg.linkUrl ?? "",
          linkText: msg.linkText ?? "",
          imageUrl: msg.imageUrl ?? "",
          selectedText: msg.selectedText ?? "",
          isEditable: Boolean(msg.isEditable),
          hasMedia: Boolean(msg.hasMedia),
        };
        overlayAnchor = { left: cx, top: cy, right: cx, bottom: cy };
        overlayChromeTop = headerHeight;
        overlayKind = "page-menu";
        return;
      }
      if (msg.type === "save-login" && msg.origin && msg.username && msg.password) {
        saveLoginPrompt = {
          origin: msg.origin,
          username: msg.username,
          password: msg.password,
        };
        void tick().then(() => updateLayout());
        return;
      }
      if (msg.type === "request-autofill-login" && msg.origin) {
        void api.autofillSavedLogin(msg.origin, msg.username).catch((e) => notify("error", errorMessage(e)));
        return;
      }
      if (msg.type === "request-autofill-profile") {
        void api.autofillProfileActiveTab().catch((e) => notify("error", errorMessage(e)));
        return;
      }
      if (msg.type !== "scroll") return;
      if (msg.bg) pageBg = msg.bg;
      if (typeof msg.isDark === "boolean") pageIsDark = msg.isDark;
      if (typeof msg.p === "number" && Number.isFinite(msg.p)) {
        if (!addressFocused && !findBarOpen && !overlayKind && !compactChrome) {
          setTargetScrollProgress(msg.p);
        }
      }
    };
    overlayWindow.__SET_APP_OVERLAY_KIND__ = (kind, anchor, tabId, chromeTop) => {
      overlayKind = kind;
      overlayAnchor = anchor ?? null;
      if (tabId !== undefined) tabMenuTargetId = tabId ?? null;
      if (typeof chromeTop === "number" && Number.isFinite(chromeTop)) {
        overlayChromeTop = chromeTop;
      }
      if (kind) void loadOverlayData(kind);
    };
    overlayWindow.__CLEAR_APP_OVERLAY__ = () => {
      overlayKind = null;
      overlayAnchor = null;
      tabMenuTargetId = null;
      feedbackDraft = null;
    };

    let unlisten: UnlistenFn | undefined;
    let unlistenSettings: UnlistenFn | undefined;
    let unlistenShortcut: UnlistenFn | undefined;
    let unlistenDownload: UnlistenFn | undefined;
    let unlistenOverlay: UnlistenFn | undefined;
    let unlistenPopupBlocked: UnlistenFn | undefined;
    let resizeTimer: ReturnType<typeof setTimeout> | undefined;
    let readingPositionTimer: ReturnType<typeof setInterval> | undefined;
    let observer: ResizeObserver | undefined;
    const colorSchemeQuery =
      typeof window !== "undefined" && window.matchMedia
        ? window.matchMedia("(prefers-color-scheme: dark)")
        : null;
    systemPrefersDark = colorSchemeQuery?.matches ?? false;
    const onColorSchemeChange = (e: MediaQueryListEvent) => {
      systemPrefersDark = e.matches;
    };
    colorSchemeQuery?.addEventListener("change", onColorSchemeChange);

    listen<PopupBlockedNotice>("popup-blocked", (event) => {
      popupBlockedNotice = event.payload;
      void tick().then(() => updateLayout());
    }).then((dispose) => {
      unlistenPopupBlocked = dispose;
    });

    listen<BrowserState>("browser-state", (event) => {
      applyBrowser(event.payload);
    }).then((dispose) => {
      unlisten = dispose;
    });

    listen<SettingsView>("settings-updated", (event) => {
      applySettings(event.payload);
    }).then((dispose) => {
      unlistenSettings = dispose;
    });

    listen<string>("browser-shortcut", (event) => {
      switch (event.payload) {
        case "new-tab": void newTab(); break;
        case "new-private-tab": void newPrivateTab(); break;
        case "close-tab": if (browser.activeTab) void closeTab(browser.activeTab); break;
        case "reopen-closed-tab": void reopenClosedTab(); break;
        case "focus-address": addressInput?.focus(); addressInput?.select(); break;
        case "reload": void reload(); break;
        case "bookmark": void saveCurrentBookmark(); break;
        case "bookmark-all-tabs": void bookmarkAllTabs(); break;
        case "tab-search": void openOverlay("tab-search"); break;
        case "task-manager": void openOverlay("task-manager"); break;
        case "mute-tab": if (browser.activeTab) void toggleMuteTab(browser.activeTab); break;
        case "find-in-page": void openFindBar(); break;
        case "downloads": void openInternalTab("downloads"); break;
        case "open-history": void openInternalTab("visits"); break;
        case "open-bookmarks": void openInternalTab("bookmarks"); break;
        case "open-settings": void openInternalTab("settings"); break;
        case "home": goHome(); break;
        case "back": void goBack(); break;
        case "forward": void goForward(); break;
        case "toggle-bookmark-bar": toggleBookmarkBar(); break;
        case "print": void printPage(); break;
        case "zoom-in": void adjustZoom(1); break;
        case "zoom-out": void adjustZoom(-1); break;
        case "zoom-reset": void setZoom(1); break;
        case "fullscreen": void toggleFullscreen(); break;
        case "next-tab":
        case "previous-tab": {
          const tabs = visibleTabs;
          if (tabs.length > 1) {
            const current = tabs.findIndex((tab) => tab.id === browser.activeTab);
            const step = event.payload === "next-tab" ? 1 : -1;
            const target = tabs[(current + step + tabs.length) % tabs.length];
            if (target) void selectTab(target.id);
          }
          break;
        }
        default: {
          if (event.payload.startsWith("tab-")) {
            const num = Number(event.payload.slice(4));
            const tabs = visibleTabs;
            if (num >= 1 && num <= 9 && tabs.length > 0) {
              const target = num === 9 ? tabs[tabs.length - 1] : tabs[num - 1];
              if (target) void selectTab(target.id);
            }
          }
          break;
        }
      }
    }).then((dispose) => {
      unlistenShortcut = dispose;
    });

    let unlistenSavedTranslation: UnlistenFn | undefined;
    listen<{ tabId: string; url: string }>("saved-translation-available", (event) => {
      void applySavedTranslationIfAvailable(event.payload.tabId, event.payload.url);
    }).then((dispose) => {
      unlistenSavedTranslation = dispose;
    });

    let unlistenTranslateAction: UnlistenFn | undefined;
    listen<{
      action: "translate" | "retranslate" | "restore" | "region" | "cancel" | "sync";
      model?: string;
      targetLanguage?: string;
      style?: string;
      bilingualAll?: boolean;
    }>("translate-popup-action", (event) => {
      if (event.payload.model !== undefined) selectedModel = event.payload.model;
      if (event.payload.targetLanguage !== undefined) {
        selectedTargetLanguage = event.payload.targetLanguage;
      }
      if (event.payload.style !== undefined) {
        selectedStyle = VALID_TRANSLATION_STYLES.has(event.payload.style)
          ? event.payload.style
          : "auto";
        saveLocalStyle(selectedStyle);
      }
      if (event.payload.bilingualAll !== undefined) {
        bilingualAll = event.payload.bilingualAll;
      }
      switch (event.payload.action) {
        case "translate": void translateCurrentPage(false); break;
        case "retranslate": void translateCurrentPage(true); break;
        case "restore": void restoreOriginalPage(); break;
        case "region": void pickRegionAndTranslate(); break;
        case "cancel": void cancelTranslation(); break;
      }
    }).then((dispose) => {
      unlistenTranslateAction = dispose;
    });

    listen<{ state: "started" | "finished"; fileName: string; success: boolean }>(
      "download-status",
      (event) => {
        const { state, fileName, success } = event.payload;
        if (state === "started") notify("info", t("toast.downloadStarted", { name: fileName }));
        else if (success) notify("success", t("toast.downloadDone", { name: fileName }));
        else notify("error", t("toast.downloadFailed", { name: fileName }));
      },
    ).then((dispose) => {
      unlistenDownload = dispose;
    });

    let unlistenUiPrefs: UnlistenFn | undefined;
    listen<{ glassTinted?: boolean; accentColor?: string; themeAutoSync?: boolean }>(
      "ui-prefs-updated",
      (event) => {
        if (typeof event.payload?.glassTinted === "boolean") {
          glassTinted = event.payload.glassTinted;
        }
        if (typeof event.payload?.accentColor === "string" && event.payload.accentColor) {
          accentColor = event.payload.accentColor;
        }
        if (typeof event.payload?.themeAutoSync === "boolean") {
          themeAutoSync = event.payload.themeAutoSync;
        }
      },
    ).then((dispose) => {
      unlistenUiPrefs = dispose;
    });

    listen("overlay-closed", () => {
      try {
        bookmarkBarVisible = localStorage.getItem("bookmark-bar") === "true";
        compactChrome = localStorage.getItem("compact-chrome") === "true";
        glassTinted = localStorage.getItem("glass-tinted") === "true";
        accentColor = localStorage.getItem("accent-color") || "#684cff";
        themeAutoSync = localStorage.getItem("theme-auto-sync") !== "false";
      } catch {
        // Keep the current visibility when storage is unavailable.
      }
      void api.browserState().then(applyBrowser).catch(() => {});
      void loadBookmarks();
      updateLayout();
    }).then((dispose) => {
      unlistenOverlay = dispose;
    });

    api
      .browserState()
      .then(async (state) => {
        applyBrowser(state);
        updateLayout();
      })
      .catch((cause) => notify("error", errorMessage(cause)));

    try {
      bookmarkBarVisible = localStorage.getItem("bookmark-bar") === "true";
      compactChrome = localStorage.getItem("compact-chrome") === "true";
      glassTinted = localStorage.getItem("glass-tinted") === "true";
      accentColor = localStorage.getItem("accent-color") || "#684cff";
      themeAutoSync = localStorage.getItem("theme-auto-sync") !== "false";
      if (compactChrome) scrollProgress = 1;
    } catch {
      bookmarkBarVisible = false;
      compactChrome = false;
      glassTinted = false;
    }
    void loadBookmarks();

    const onFocus = () => void loadBookmarks();
    window.addEventListener("focus", onFocus);

    api
      .loadSettings()
      .then((view) => {
        applySettings(view);
        if (!view.settings.onboardingCompleted && !onboardingTriggered) {
          onboardingTriggered = true;
          void openOverlay("onboarding");
        }
      })
      .catch((cause) => notify("error", errorMessage(cause)));

    if (headerEl) {
      observer = new ResizeObserver(() => {
        if (!scrollSpringRaf && Math.abs(targetScrollProgress - scrollProgress) <= 0.01) {
          scheduleSettledLayout(140);
        }
      });
      observer.observe(headerEl);
    }
    updateLayout();
    readingPositionTimer = setInterval(() => {
      if (
        syncingReadingPosition ||
        document.hidden ||
        !activeTab ||
        activeTab.loading ||
        activeTab.url.startsWith("tb://")
      ) {
        return;
      }
      syncingReadingPosition = true;
      void api.syncReadingPosition().catch(() => {}).finally(() => {
        syncingReadingPosition = false;
      });
    }, 5_000);

    let pollingPageAction = false;
    const pageActionTimer = setInterval(() => {
      if (
        pollingPageAction ||
        document.hidden ||
        !activeTab ||
        activeTab.loading ||
        activeTab.url.startsWith("tb://")
      ) {
        return;
      }
      pollingPageAction = true;
      void api
        .pollPageAction()
        .then(async (action) => {
          if (!action) return;
          if (action.type === "page-contextmenu") {
            const cx = typeof action.x === "number" ? action.x : 120;
            const cy = typeof action.y === "number" ? action.y : 120;
            pageMenuContext = {
              x: cx,
              y: cy,
              linkUrl: action.linkUrl ?? "",
              linkText: action.linkText ?? "",
              imageUrl: action.imageUrl ?? "",
              selectedText: action.selectedText ?? "",
              isEditable: Boolean(action.isEditable),
              hasMedia: Boolean(action.hasMedia),
            };
            void openOverlay("page-menu", { left: cx, top: cy, right: cx, bottom: cy });
            return;
          }
          if (action.type === "page-meta") {
            if (action.bg) pageBg = action.bg;
            pageIsDark = action.isDark;
            if (!addressFocused && !findBarOpen && !overlayKind && !compactChrome) {
              setTargetScrollProgress(action.scrollState === "down" ? 1 : 0);
            }
            return;
          }
          await handlePageAction(action);
        })
        .catch(() => {})
        .finally(() => {
          pollingPageAction = false;
        });
    }, 2_500);

    let unlistenTabMenuAction: UnlistenFn | undefined;
    listen<{
      action:
        | "pin"
        | "mute"
        | "sleep"
        | "duplicate"
        | "reopen"
        | "close-others"
        | "close-right"
        | "close"
        | "move"
        | "new-tab";
      tabId: string;
      workspaceId?: string;
    }>("tab-menu-action", (event) => {
      const { action, tabId, workspaceId } = event.payload;
      switch (action) {
        case "new-tab": void newTab(); break;
        case "pin": void pinTab(tabId); break;
        case "mute": void toggleMuteTab(tabId); break;
        case "sleep": void sleepTab(tabId); break;
        case "duplicate": void duplicateTab(tabId); break;
        case "reopen": void reopenClosedTab(); break;
        case "close-others": void closeOtherTabs(tabId); break;
        case "close-right": void closeTabsToRight(tabId); break;
        case "close": void closeTab(tabId); break;
        case "move": if (workspaceId) void moveTab(tabId, workspaceId); break;
      }
    }).then((dispose) => {
      unlistenTabMenuAction = dispose;
    });

    const onResize = () => {
      clearTimeout(resizeTimer);
      resizeTimer = setTimeout(() => updateLayout(), 120);
    };
    window.addEventListener("resize", onResize);
    window.addEventListener("keydown", onKeydown);

    return () => {
      unlisten?.();
      unlistenSettings?.();
      unlistenShortcut?.();
      unlistenDownload?.();
      unlistenOverlay?.();
      unlistenSavedTranslation?.();
      unlistenTranslateAction?.();
      unlistenTabMenuAction?.();
      unlistenUiPrefs?.();
      unlistenPopupBlocked?.();
      colorSchemeQuery?.removeEventListener("change", onColorSchemeChange);
      observer?.disconnect();
      window.removeEventListener("resize", onResize);
      window.removeEventListener("keydown", onKeydown);
      window.removeEventListener("focus", onFocus);
      clearTimeout(resizeTimer);
      clearTimeout(layoutAnimTimer);
      clearTimeout(layoutSettledTimer);
      clearInterval(readingPositionTimer);
      clearInterval(pageActionTimer);
      clearTimeout(statusTimer);
      cancelAnimationFrame(layoutFrame);
      cancelAnimationFrame(scrollSpringRaf);
    };
  });

  $effect(() => {
    if (typeof document !== "undefined") {
      document.documentElement.dataset.theme = resolvedTheme;
      document.documentElement.style.setProperty("--custom-accent", accentColor || "#684cff");
      if (glassTinted) document.documentElement.dataset.tinted = "true";
      else delete document.documentElement.dataset.tinted;
      if (overlayKind) document.documentElement.dataset.overlay = "true";
      else delete document.documentElement.dataset.overlay;
    }
    if (!pageKind) {
      const forcedDark =
        settings.theme === "dark"
          ? true
          : settings.theme === "light" || settings.theme === "sepia"
            ? false
            : themeAutoSync
              ? null
              : systemPrefersDark;
      void api.syncUiThemeToTabs(forcedDark, accentColor || "#684cff", glassTinted).catch(() => {});
    }
  });

  $effect(() => {
    const showSuggestions = Boolean(!pageKind && addressFocused && suggestions.length > 0);
    void api.setShellExpanded(showSuggestions).then(() => {
      if (!showSuggestions && !overlayKind) updateLayout();
    }).catch(() => {});
  });
</script>

<svelte:head>
  <title>{overlayKind ? t("chrome.menuAria") : t("app.name")}</title>
</svelte:head>

{#if pageKind}
  <div class="internal-page-root" class:tinted={glassTinted}>
    {#if pageKind === "start"}
      <div class="start-page">
        <div class="start-hero">
          <h1 class="start-logo">Fluo</h1>
          <p class="start-sub">{t("app.tagline")}</p>
        </div>
        <form
          class="start-search"
          onsubmit={(event) => {
            event.preventDefault();
            const q = startQuery.trim();
            if (q) void api.navigate(q);
          }}
        >
          <input
            type="text"
            bind:value={startQuery}
            placeholder={t("chrome.addressPlaceholder")}
            spellcheck="false"
            autocomplete="off"
          />
          <button type="submit">{t("common.search")}</button>
        </form>
        <section class="start-sec">
          <div style="display:flex;align-items:center;justify-content:space-between;margin-bottom:8px;">
            <h2 style="margin:0;">{trLocal("빠른 실행", "Quick Launch", "クイック起動", "快速访问")}</h2>
            <button
              type="button"
              class="action"
              style="height:26px;padding:0 10px;font-size:12px;"
              onclick={() => (showAddShortcutForm = !showAddShortcutForm)}
            >
              {showAddShortcutForm ? t("common.cancel") : `+ ${t("common.add")}`}
            </button>
          </div>
          {#if showAddShortcutForm}
            <form
              style="display:flex;gap:8px;margin-bottom:10px;"
              onsubmit={(e) => {
                e.preventDefault();
                addCustomQuickShortcut();
              }}
            >
              <input
                type="text"
                bind:value={newShortcutTitle}
                placeholder={trLocal("이름", "Name", "名前", "名称")}
                style="flex:1;height:32px;padding:0 10px;border-radius:10px;border:1px solid var(--line);"
              />
              <input
                type="text"
                bind:value={newShortcutUrl}
                placeholder="https://example.com"
                style="flex:2;height:32px;padding:0 10px;border-radius:10px;border:1px solid var(--line);"
              />
              <button type="submit" class="action primary-liquid" style="height:32px;padding:0 14px;">
                {t("common.save")}
              </button>
            </form>
          {/if}
          <div class="start-grid">
            {#each quickShortcuts as item (item.id)}
              <div class="start-card" style="position:relative;display:flex;align-items:center;justify-content:space-between;">
                <button
                  type="button"
                  style="all:unset;cursor:pointer;flex:1;overflow:hidden;text-overflow:ellipsis;white-space:nowrap;font-weight:600;"
                  onclick={() => void api.navigate(item.url)}
                >
                  {item.title}
                </button>
                <button
                  type="button"
                  title={t("common.delete")}
                  style="all:unset;cursor:pointer;opacity:0.55;padding:0 4px;font-size:14px;"
                  onclick={(e) => {
                    e.stopPropagation();
                    removeCustomQuickShortcut(item.id);
                  }}
                >
                  ×
                </button>
              </div>
            {/each}
          </div>
        </section>
        {#if bookmarks.length > 0}
          <section class="start-sec">
            <h2>{trLocal("북마크", "Bookmarks", "ブックマーク", "书签")}</h2>
            <div class="start-grid">
              {#each bookmarks.slice(0, 8) as bm (bm.id)}
                <button type="button" class="start-card" onclick={() => void api.navigate(bm.url)}>
                  <span class="start-card-title">{bm.title || bm.url}</span>
                  <span class="start-card-sub">{bm.folder || ""}</span>
                </button>
              {/each}
            </div>
          </section>
        {/if}
        {#if recentVisits.length > 0}
          <section class="start-sec">
            <h2>{trLocal("최근 방문", "Recent Visits", "最近の履歴", "最近访问")}</h2>
            <div class="start-list">
              {#each recentVisits.slice(0, 6) as item (item.id)}
                <button type="button" class="start-row" onclick={() => void api.navigate(item.url)}>
                  <span class="start-card-title">{item.title || item.url}</span>
                  <span class="start-card-sub">{item.url}</span>
                </button>
              {/each}
            </div>
          </section>
        {/if}
      </div>
    {:else if pageKind === "settings" && settingsView}
      <SettingsPanel
        embedded={true}
        view={settingsView}
        onclose={() => {}}
        onsaved={onSettingsSaved}
        onnotify={notify}
      />
    {:else if pageKind === "glossary"}
      <GlossaryPanel embedded={true} onclose={() => {}} onnotify={notify} />
    {:else if pageKind === "bookmarks"}
      <BookmarkPanel
        embedded={true}
        current={activeTab}
        onclose={() => {}}
        onnavigate={(url) => void api.navigate(url)}
        onnotify={notify}
      />
    {:else if pageKind === "history"}
      <TranslationHistoryPanel
        embedded={true}
        onclose={() => {}}
        onnavigate={(url) => void api.navigate(url)}
        onnotify={notify}
      />
    {:else if pageKind === "visits"}
      <HistoryPanel
        embedded={true}
        onclose={() => {}}
        onnavigate={(url) => void api.navigate(url)}
        onnotify={notify}
      />
    {:else if pageKind === "downloads"}
      <DownloadsPanel embedded={true} onclose={() => {}} onnotify={notify} />
    {/if}
    {#if status}
      <div class="overlay-notice" class:error={status.kind === "error"} class:success={status.kind === "success"}>
        <span>{status.text}</span>
        <button type="button" aria-label={t("common.close")} onclick={() => (status = null)}>×</button>
      </div>
    {/if}
  </div>
{:else}
<div
  class="app"
  class:page-dark={effectivePageDark}
  class:page-light={!effectivePageDark}
  class:tinted={glassTinted}
  style:--page-bg={pageBg || ""}
  style:--custom-accent={accentColor || "#684cff"}
>
  <!-- svelte-ignore a11y_no_noninteractive_element_interactions -->
  <div
    class="chrome-shell"
    bind:this={headerEl}
    role="region"
    aria-label={t("app.name")}
    onmousedown={(event) => {
      if (event.button !== 0 || event.detail >= 2) return;
      const target = event.target as HTMLElement | null;
      if (
        target?.closest(
          "button, input, select, textarea, a, .tab, .workspace-select-btn, .address, .find-bar, .prompt-banner, .window-controls",
        )
      ) {
        return;
      }
      const startX = event.screenX;
      const startY = event.screenY;
      let started = false;
      const cleanup = () => {
        window.removeEventListener("mousemove", onMove, true);
        window.removeEventListener("mouseup", cleanup, true);
        window.removeEventListener("blur", cleanup, true);
      };
      const onMove = (moveEv: MouseEvent) => {
        if (started) return;
        if (
          Math.abs(moveEv.screenX - startX) >= 5 ||
          Math.abs(moveEv.screenY - startY) >= 5
        ) {
          started = true;
          cleanup();
          void api.windowAction("start-dragging");
        }
      };
      window.addEventListener("mousemove", onMove, true);
      window.addEventListener("mouseup", cleanup, true);
      window.addEventListener("blur", cleanup, true);
    }}
    ondblclick={(event) => {
      const target = event.target as HTMLElement | null;
      if (
        target?.closest(
          "button, input, select, textarea, a, .tab, .workspace-select-btn, .address, .find-bar, .prompt-banner, .window-controls",
        )
      ) {
        return;
      }
      void api.windowAction("toggle-maximize");
    }}
    oncontextmenu={(event) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea, .tab")) return;
      event.preventDefault();
      void openOverlay("chrome-menu", {
        left: event.clientX,
        top: event.clientY,
        right: event.clientX,
        bottom: event.clientY,
      });
    }}
  >
    <div class="window-controls" role="group" aria-label={t("chromeMenu.aria")}>
      <button
        class="win-btn"
        type="button"
        aria-label={t("chrome.minimize")}
        title={t("chrome.minimize")}
        onclick={() => void api.windowAction("minimize")}
      >
        <svg viewBox="0 0 12 12" aria-hidden="true"><path d="M2.2 6h7.6" /></svg>
      </button>
      <button
        class="win-btn"
        type="button"
        aria-label={t("chrome.maximizeRestore")}
        title={t("chrome.maximizeRestore")}
        onclick={() => void api.windowAction("toggle-maximize")}
      >
        <svg viewBox="0 0 12 12" aria-hidden="true"><rect x="2.3" y="2.3" width="7.4" height="7.4" rx="1.4" /></svg>
      </button>
      <button
        class="win-btn close"
        type="button"
        aria-label={t("common.close")}
        title={t("chrome.close")}
        onclick={() => void api.windowAction("close")}
      >
        <svg viewBox="0 0 12 12" aria-hidden="true"><path d="m2.8 2.8 6.4 6.4M9.2 2.8 2.8 9.2" /></svg>
      </button>
    </div>
    <header
      class="chrome"
      class:compact={effectiveCompact}
      style:--cp={collapseRatio.toFixed(4)}
      style:--tabbar-full-h={bookmarkBarVisible ? "68px" : "40px"}
    >
      <div class="tabbar-collapse-wrap" class:collapsed={effectiveCompact}>
        <div class="tabbar-collapse-inner">
          <TabBar
            tabs={visibleTabs}
            activeTab={browser.activeTab}
            onselect={selectTab}
            onmovetab={moveTab}
            onclose={closeTab}
            onnew={newTab}
            onreorder={reorderTab}
            workspaces={browser.workspaces}
            activeWorkspace={browser.activeWorkspace}
            onopenworkspacemenu={(anchor) => void openOverlay("workspace", anchor)}
            onopentabmenu={(tabId, anchor) => {
              void openOverlay("tab-menu", anchor, tabId);
            }}
            onmutetab={(tabId) => void toggleMuteTab(tabId)}
            onopentabsearch={(anchor) => void openOverlay("tab-search", anchor)}
            {audibleTabs}
          />
          {#if bookmarkBarVisible}
            <BookmarkBar
              {bookmarks}
              onnavigate={(url) => void navigate(url)}
              onopen={() => void openInternalTab("bookmarks")}
            />
          {/if}
        </div>
      </div>

      <div class="toolbar">
        <div class="nav-buttons">
          <button class="nav" type="button" aria-label={t("chrome.home")} title={t("chrome.homeTitle")} onclick={goHome}>
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="m2.5 7 5.5-4.5L13.5 7v6.5h-4v-4h-3v4h-4z" /></svg>
          </button>
          <button
            class="nav"
            type="button"
            aria-label={t("chrome.back")}
            title={t("chrome.backTitle")}
            disabled={!browser.canGoBack}
            onclick={goBack}
            oncontextmenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              if (!browser.canGoBack) return;
              const anchor = anchorOf(e);
              void api.activeTabHistory().then(([cursor, urls]) => {
                const list: Array<{ delta: number; url: string }> = [];
                for (let i = cursor - 1; i >= 0 && list.length < 10; i--) {
                  list.push({ delta: i - cursor, url: urls[i] });
                }
                if (list.length > 0) {
                  navHistoryEntries = list;
                  void openOverlay("nav-history", anchor);
                }
              });
            }}
          >
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="M10 3 5 8l5 5" /></svg>
          </button>
          <button
            class="nav"
            type="button"
            aria-label={t("chrome.forward")}
            title={t("chrome.forwardTitle")}
            disabled={!browser.canGoForward}
            onclick={goForward}
            oncontextmenu={(e) => {
              e.preventDefault();
              e.stopPropagation();
              if (!browser.canGoForward) return;
              const anchor = anchorOf(e);
              void api.activeTabHistory().then(([cursor, urls]) => {
                const list: Array<{ delta: number; url: string }> = [];
                for (let i = cursor + 1; i < urls.length && list.length < 10; i++) {
                  list.push({ delta: i - cursor, url: urls[i] });
                }
                if (list.length > 0) {
                  navHistoryEntries = list;
                  void openOverlay("nav-history", anchor);
                }
              });
            }}
          >
            <svg viewBox="0 0 16 16" aria-hidden="true"><path d="m6 3 5 5-5 5" /></svg>
          </button>
          {#if activeTab?.loading}
          <button class="nav" type="button" aria-label={t("chrome.stopLoading")} title={t("chrome.stopLoading")} onclick={stopLoading}>
            <svg viewBox="0 0 16 16" aria-hidden="true"><rect x="4" y="4" width="8" height="8" rx="1" /></svg>
          </button>
          {:else}
          <button
            class="nav"
            type="button"
            aria-label={t("chrome.reload")}
            title={t("chrome.reloadTitle")}
            onclick={reload}
          >
            <svg viewBox="0 0 16 16" aria-hidden="true">
              <path d="M13 8a5 5 0 1 1-1.5-3.6M13 3v3.2h-3.2" />
            </svg>
          </button>
          {/if}
        </div>

        <form
          class="address"
          class:focused={addressFocused}
          class:compact={effectiveCompact}
          onsubmit={(event) => {
            event.preventDefault();
            addressInput?.blur();
            void navigate();
          }}
        >
          {#if activeTab}
            <button
              class="address-tab-chip"
              class:visible={effectiveCompact && !addressFocused}
              type="button"
              title={t("chrome.tabChipTitle")}
              onclick={(e) => void openOverlay("tab-search", anchorOf(e))}
            >
              <span class="chip-dot"></span>
              <span class="chip-title">{activeTab.title || activeTab.url}</span>
              <span class="chip-count">{visibleTabs.length}</span>
            </button>
          {/if}
          <button
            class="scheme"
            class:insecure={!secure && !activeTab?.url.startsWith("tb://")}
            type="button"
            aria-label={trLocal("사이트 보안 정보", "Site security", "サイトセキュリティ", "站点安全信息")}
            title={trLocal("사이트 보안 정보 보기", "View site security info", "サイトセキュリティを表示", "查看站点安全信息")}
            onclick={(e) => void openOverlay("site-security", anchorOf(e))}
          >
            {#if secure}
              <svg viewBox="0 0 16 16" aria-hidden="true">
                <path d="M4.5 7V5.5a3.5 3.5 0 0 1 7 0V7" />
                <rect x="3.5" y="7" width="9" height="6" rx="1.4" />
              </svg>
            {:else}
              <svg viewBox="0 0 16 16" aria-hidden="true">
                <circle cx="8" cy="8" r="5.4" />
                <path d="M2.8 8h10.4M8 2.6c1.7 1.6 2.5 3.4 2.5 5.4S9.7 11.8 8 13.4C6.3 11.8 5.5 10 5.5 8S6.3 4.2 8 2.6Z" />
              </svg>
            {/if}
          </button>
          <input
            bind:this={addressInput}
            bind:value={address}
            onfocus={(event) => {
              addressFocused = true;
              justFocusedAddress = true;
              (event.currentTarget as HTMLInputElement).select();
              void refreshSuggestions();
            }}
            onmouseup={(event) => {
              if (justFocusedAddress) {
                justFocusedAddress = false;
                event.preventDefault();
                (event.currentTarget as HTMLInputElement).select();
              }
            }}
            onblur={() => {
              addressFocused = false;
              justFocusedAddress = false;
              suggestions = [];
              suggestionIndex = -1;
            }}
            oninput={onAddressInput}
            onkeydown={onAddressKeydown}
            spellcheck="false"
            autocomplete="off"
            placeholder={t("chrome.addressPlaceholder")}
            aria-label={t("chrome.address")}
          />
          {#if activeTab?.loading}
            <span class="address-spinner" aria-label={t("chrome.loading")}></span>
          {/if}
          <button
            class="omnibox-btn shield"
            class:adblock-on={settings.adBlockEnabled}
            class:adblock-off={!settings.adBlockEnabled}
            type="button"
            aria-label={t("chrome.adblockAria")}
            aria-haspopup="dialog"
            title={settings.adBlockEnabled
              ? t("chrome.adblockOnTitle")
              : t("chrome.adblockOffTitle")}
            onclick={(e) => void openOverlay("adblock", anchorOf(e))}
          >
            🛡
          </button>
          {#if activeTab && !activeTab.url.startsWith("tb://")}
            <button
              class="omnibox-btn"
              type="button"
              aria-label={t("chrome.copyUrl")}
              title={t("chrome.copyUrlTitle")}
              onclick={copyCurrentUrl}
            >
              🔗
            </button>
          {/if}
          <button
            class="omnibox-btn star"
            class:bookmarked={isCurrentBookmarked}
            type="button"
            aria-label={t("chrome.bookmarkSave")}
            title={isCurrentBookmarked ? t("chrome.bookmarkSaved") : t("chrome.bookmarkSaveTitle")}
            onclick={(e) => void openOverlay("bookmark-edit", anchorOf(e))}
            disabled={!activeTab}
          >
            {isCurrentBookmarked ? "★" : "☆"}
          </button>
          {#if addressFocused && suggestions.length > 0}
            <ul class="address-suggestions" role="listbox" aria-label={t("chrome.addressSuggestions")}>
              {#each suggestions as item, index (`${item.kind}-${item.url}-${index}`)}
                <li role="option" aria-selected={index === suggestionIndex}>
                  <button
                    type="button"
                    class:active={index === suggestionIndex}
                    onmousedown={(event) => event.preventDefault()}
                    onclick={() => chooseSuggestion(item)}
                  >
                    <span class="suggestion-kind" aria-hidden="true">
                      {item.kind === "bookmark" ? "★" : item.kind === "search" ? "🔍" : item.kind === "calc" ? "＝" : item.kind === "tab" ? "⇥" : "◷"}
                    </span>
                    <span class="suggestion-title">{item.title || item.url}</span>
                    <span class="suggestion-url">
                      {item.kind === "search"
                        ? t("chrome.previousSearch")
                        : item.kind === "tab"
                          ? trLocal("열린 탭으로 전환", "Switch to Tab", "タブに切り替え", "切换到标签页")
                          : item.url}
                    </span>
                  </button>
                </li>
              {/each}
            </ul>
          {/if}
        </form>

        <div class="actions">
          <button
            class="action primary-liquid"
            class:busy={translating}
            type="button"
            title={t("chrome.translateOptions")}
            aria-haspopup="dialog"
            onclick={(e) => void openOverlay("translate", anchorOf(e))}
          >
            {#if translating}
              <span class="btn-spinner" aria-hidden="true"></span>
              <span>{t("chrome.translateWorking", { done: progress.done, total: progress.total })}</span>
            {:else if inlineMode && pendingBlocks === 0}
              <span>{t("chrome.translatedBtn")}</span>
            {:else}
              <span>{t("chrome.translateBtn")}</span>
            {/if}
          </button>
          {#if inlineMode && !translating}
            <button
              class="action secondary-liquid"
              type="button"
              title={t("chrome.restoreOriginalTitle")}
              onclick={restoreOriginalPage}
            >
              {t("chrome.restoreOriginal")}
            </button>
          {/if}

          {#if translating}
            <button
              class="action"
              type="button"
              title={t("chrome.cancelRemainingTitle")}
              onclick={cancelTranslation}
            >
              {t("common.cancel")}
            </button>
          {/if}

          {#if activeTab && Math.abs(activeTab.zoom - 1) > 0.001}
            <button
              class="action zoom-indicator"
              type="button"
              title={t("chrome.zoomResetTitle")}
              onclick={() => void setZoom(1)}
            >
              {Math.round(activeTab.zoom * 100)}%
            </button>
          {/if}
          <button
            class="action icon"
            class:tinted-active={glassTinted}
            type="button"
            aria-label={t("chrome.tintToggleAria")}
            title={glassTinted ? t("chrome.tintOnTitle") : t("chrome.tintOffTitle")}
            onclick={toggleGlassTint}
          >
            ◐
          </button>
          <button
            class="action icon"
            type="button"
            aria-label={t("chrome.compactToggleAria")}
            title={effectiveCompact ? t("chrome.compactExpandTitle") : t("chrome.compactShrinkTitle")}
            onclick={() => {
              if (scrollProgress > 0.1 && !compactChrome) {
                scrollCollapsed = false;
                setTargetScrollProgress(0);
              } else {
                toggleCompactChrome();
              }
            }}
          >
            {effectiveCompact ? "▾" : "▴"}
          </button>
          <button class="action icon" type="button" aria-label={t("chrome.menuAria")} aria-haspopup="dialog" title={t("chrome.menuAria")} onclick={(e) => void openOverlay("menu", anchorOf(e))}>⋯</button>
          <button class="action icon" type="button" aria-label={t("chrome.settings")} title={t("chrome.settings")} onclick={openSettings}>
            <svg viewBox="0 0 16 16" aria-hidden="true">
              <circle cx="8" cy="8" r="2.2" />
              <path d="M8 1.8v1.6M8 12.6v1.6M1.8 8h1.6M12.6 8h1.6M3.6 3.6l1.1 1.1M11.3 11.3l1.1 1.1M12.4 3.6l-1.1 1.1M4.7 11.3l-1.1 1.1" />
            </svg>
          </button>
        </div>
      </div>

      {#if autoTranslatePrompt && !inlineMode && !translating}
        <div class="prompt-banner" role="region" aria-label="Auto Translate">
          <span class="prompt-text">
            🌐 {trLocal("외국어 페이지가 감지되었습니다", "Foreign language page detected", "外国語ページを検出しました", "检测到外语页面")}
            <strong>({autoTranslatePrompt.lang})</strong>
          </span>
          <div class="prompt-actions">
            <button
              type="button"
              class="prompt-btn primary"
              onclick={() => {
                autoTranslatePrompt = null;
                void tick().then(() => updateLayout());
                void translateCurrentPage(false);
              }}
            >
              {t("chrome.translateBtn")}
            </button>
            <button
              type="button"
              class="prompt-btn"
              onclick={() => {
                const h = autoTranslatePrompt?.host ?? "";
                autoTranslatePrompt = null;
                void tick().then(() => updateLayout());
                toggleAutoTranslateHost(h);
              }}
            >
              {trLocal("이 사이트 항상 번역", "Always translate site", "このサイトを常に翻訳", "始终翻译此站点")}
            </button>
            <button
              type="button"
              class="prompt-close"
              aria-label={t("common.close")}
              onclick={() => {
                autoTranslatePrompt = null;
                void tick().then(() => updateLayout());
              }}
            >
              ×
            </button>
          </div>
        </div>
      {/if}

      {#if ttsActive}
        <div class="prompt-banner" role="region" aria-label="Read Aloud">
          <span class="prompt-text">
            🔊 {trLocal("소리 내어 읽기", "Read Aloud", "音声読み上げ", "语音朗读")}
            {#if ttsProgress.total > 0}
              <strong>({ttsProgress.index}/{ttsProgress.total})</strong>
            {/if}
          </span>
          <div class="prompt-actions">
            <button type="button" class="prompt-btn" onclick={() => void api.runPageContextAction("tts-prev")}>⏮</button>
            <button type="button" class="prompt-btn primary" onclick={() => void api.runPageContextAction("tts-toggle")}>⏯</button>
            <button type="button" class="prompt-btn" onclick={() => void api.runPageContextAction("tts-next")}>⏭</button>
            <button type="button" class="prompt-btn" onclick={() => void api.runPageContextAction("tts-rate", "1.25")}>1.25x</button>
            <button
              type="button"
              class="prompt-close"
              aria-label={t("common.close")}
              onclick={() => {
                ttsActive = false;
                void api.runPageContextAction("tts-stop");
                void tick().then(() => updateLayout());
              }}
            >
              ×
            </button>
          </div>
        </div>
      {/if}

      {#if popupBlockedNotice}
        <div class="prompt-banner" role="alert">
          <span class="prompt-text">
            {t("banner.popupBlocked")} <strong>{popupBlockedNotice.sourceOrigin || popupBlockedNotice.url}</strong>
          </span>
          <div class="prompt-actions">
            <button
              type="button"
              class="prompt-btn"
              onclick={() => {
                const targetUrl = popupBlockedNotice?.url;
                popupBlockedNotice = null;
                void tick().then(() => updateLayout());
                if (targetUrl) void api.newTab(targetUrl).then(applyBrowser);
              }}
            >
              {t("banner.allowOnce")}
            </button>
            <button
              type="button"
              class="prompt-btn primary"
              onclick={() => {
                const notice = popupBlockedNotice;
                popupBlockedNotice = null;
                void tick().then(() => updateLayout());
                if (notice) {
                  void api.allowPopupSite(notice.sourceOrigin || notice.url).then((view) => {
                    onSettingsSaved(view);
                    void api.newTab(notice.url).then(applyBrowser);
                  });
                }
              }}
            >
              {t("banner.allowAlways")}
            </button>
            <button
              type="button"
              class="prompt-close"
              aria-label={t("common.close")}
              onclick={() => {
                popupBlockedNotice = null;
                void tick().then(() => updateLayout());
              }}
            >
              ×
            </button>
          </div>
        </div>
      {/if}

      {#if saveLoginPrompt}
        <div class="prompt-banner" role="region" aria-label={t("banner.savePasswordAria")}>
          <span class="prompt-text">
            {t("banner.savePasswordQuestion")} <strong>{saveLoginPrompt.origin}</strong> ({saveLoginPrompt.username})
          </span>
          <div class="prompt-actions">
            <button
              type="button"
              class="prompt-btn primary"
              onclick={() => {
                const promptData = saveLoginPrompt;
                saveLoginPrompt = null;
                void tick().then(() => updateLayout());
                if (promptData) {
                  void api
                    .saveLogin(crypto.randomUUID(), promptData.origin, promptData.username, promptData.password)
                    .then(() => notify("success", t("banner.savedLoginToast", { origin: promptData.origin })))
                    .catch((e) => notify("error", errorMessage(e)));
                }
              }}
            >
              {t("common.save")}
            </button>
            <button
              type="button"
              class="prompt-close"
              aria-label={t("banner.doNotSave")}
              onclick={() => {
                saveLoginPrompt = null;
                void tick().then(() => updateLayout());
              }}
            >
              ×
            </button>
          </div>
        </div>
      {/if}

      {#if findBarOpen}
        <div class="find-bar" role="search" aria-label={t("find.aria")}>
          <input
            bind:this={findInput}
            type="search"
            placeholder={t("find.placeholder")}
            bind:value={findQuery}
            oninput={() => void runFind("find")}
            onkeydown={(event) => {
              if (event.key === "Enter") {
                event.preventDefault();
                void runFind(event.shiftKey ? "prev" : "next");
              } else if (event.key === "Escape") {
                event.preventDefault();
                void closeFindBar();
              }
            }}
          />
          <span class="find-count">
            {#if findQuery.trim()}
              {findResult.total > 0 ? `${findResult.current}/${findResult.total}` : t("find.noResults")}
            {/if}
          </span>
          <button
            class="action"
            type="button"
            aria-label={t("find.prev")}
            title={t("find.prevTitle")}
            disabled={findResult.total === 0}
            onclick={() => void runFind("prev")}
          >
            ▲
          </button>
          <button
            class="action"
            type="button"
            aria-label={t("find.next")}
            title={t("find.nextTitle")}
            disabled={findResult.total === 0}
            onclick={() => void runFind("next")}
          >
            ▼
          </button>
          <button
            class="action icon"
            type="button"
            aria-label={t("find.close")}
            title={t("find.closeTitle")}
            onclick={() => void closeFindBar()}
          >
            ×
          </button>
        </div>
      {/if}

      {#if status}
        <div class="status" class:error={status.kind === "error"} class:success={status.kind === "success"}>
          <span class="status-text">{status.text}</span>
          <button class="status-close" type="button" aria-label={t("common.close")} onclick={() => (status = null)}>
            ×
          </button>
        </div>
      {/if}
    </header>
  </div>

  {#if addressFocused && suggestions.length > 0 && !overlayKind}
    <div
      class="suggestions-backdrop"
      role="presentation"
      onmousedown={() => addressInput?.blur()}
    ></div>
  {/if}

  {#if overlayKind}
  <div
    class="overlay-root"
    role="presentation"
    oncontextmenu={(event) => {
      const target = event.target as HTMLElement | null;
      if (target?.closest("input, textarea")) return;
      event.preventDefault();
      if (target?.classList.contains("overlay-menu-dismiss")) {
        if (event.clientY < overlayChromeTop) {
          overlayKind = "chrome-menu";
          overlayAnchor = {
            left: event.clientX,
            top: event.clientY,
            right: event.clientX,
            bottom: event.clientY,
          };
          tabMenuTargetId = null;
        } else {
          pageMenuContext = {
            x: event.clientX,
            y: event.clientY,
            linkUrl: "",
            linkText: "",
            imageUrl: "",
            selectedText: pageMenuContext?.selectedText ?? "",
            isEditable: false,
            hasMedia: false,
          };
          overlayAnchor = {
            left: event.clientX,
            top: event.clientY,
            right: event.clientX,
            bottom: event.clientY,
          };
          overlayKind = "page-menu";
        }
      }
    }}
  >
    {#if overlayKind === "menu"}
      <div
        class="overlay-menu-dismiss"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) void closeOverlay();
        }}
      >
        <nav class="overlay-menu" aria-label={t("chrome.menuAria")} style={popupPositionStyle("right", 268)}>
          <button onclick={() => void newTab()}>
            <span>{t("menu.newTab")}</span><kbd>Ctrl+T</kbd>
          </button>
          <button onclick={() => void newPrivateTab()}>
            <span>{t("menu.newPrivateTab")}</span><kbd>Ctrl+Shift+P</kbd>
          </button>
          <button onclick={() => { overlayKind = "tab-search"; tabSearchQuery = ""; }}>
            <span>{t("menu.tabSearch")}</span><kbd>Ctrl+Shift+A</kbd>
          </button>
          <button onclick={() => { overlayKind = "task-manager"; }}>
            <span>{t("menu.taskManager")}</span><kbd>Shift+Esc</kbd>
          </button>
          <button onclick={() => void openFindBar()}>
            <span>{t("menu.findInPage")}</span><kbd>Ctrl+F</kbd>
          </button>
          <div class="menu-divider"></div>
          <div class="menu-zoom-row" role="group" aria-label={t("menu.zoomGroup")}>
            <span class="menu-zoom-label">{t("menu.zoomLabel")}</span>
            <div class="menu-zoom-controls">
              <button type="button" aria-label={t("menu.zoomOut")} onclick={() => void adjustZoom(-1)}>−</button>
              <button type="button" class="zoom-pct" title={t("menu.zoomReset")} onclick={() => void setZoom(1)}>
                {Math.round((activeTab?.zoom ?? 1) * 100)}%
              </button>
              <button type="button" aria-label={t("menu.zoomIn")} onclick={() => void adjustZoom(1)}>+</button>
              <button type="button" title={t("menu.fullscreen")} onclick={() => { void toggleFullscreen(); void closeOverlay(); }}>⛶</button>
            </div>
          </div>
          <div class="menu-divider"></div>
          <button onclick={() => void openInternalTab("bookmarks")}>
            <span>{t("menu.bookmarks")}</span><kbd>Ctrl+Shift+O</kbd>
          </button>
          <button onclick={() => void openInternalTab("visits")}>
            <span>{t("menu.visits")}</span><kbd>Ctrl+H</kbd>
          </button>
          <button onclick={() => void openInternalTab("downloads")}>
            <span>{t("menu.downloads")}</span><kbd>Ctrl+J</kbd>
          </button>
          <button onclick={() => void openInternalTab("glossary")}>
            <span>{t("menu.glossary")}</span>
          </button>
          <button onclick={() => void openInternalTab("history")}>
            <span>{t("menu.translationHistory")}</span>
          </button>
          <div class="menu-divider"></div>
          <button onclick={() => { void reopenClosedTab().then(() => closeOverlay()); }}>
            <span>{t("menu.reopenClosed")}</span><kbd>Ctrl+Shift+T</kbd>
          </button>
          <button onclick={() => { toggleBookmarkBar(); void closeOverlay(); }}>
            <span>{bookmarkBarVisible ? t("menu.bookmarkBarHide") : t("menu.bookmarkBarShow")}</span><kbd>Ctrl+Shift+B</kbd>
          </button>
          <button onclick={() => { toggleCompactChrome(); void closeOverlay(); }}>
            <span>{compactChrome ? t("menu.compactOff") : t("menu.compactOn")}</span>
          </button>
          <button onclick={() => { toggleGlassTint(); void closeOverlay(); }}>
            <span>{glassTinted ? t("menu.tintOff") : t("menu.tintOn")}</span>
          </button>
          <button onclick={() => { void toggleAdBlock().then(() => closeOverlay()); }}>
            <span>{settings.adBlockEnabled ? t("menu.adblockOff") : t("menu.adblockOn")}</span>
          </button>
          <button onclick={() => void printPage()}>
            <span>{t("menu.print")}</span><kbd>Ctrl+P</kbd>
          </button>
          <div class="menu-divider"></div>
          <div class="menu-lang-row" role="group" aria-label={t("common.uiLanguage")}>
            <span class="menu-lang-label">{t("common.uiLanguage")}</span>
            <div class="menu-lang-pills">
              {#each UI_LANGUAGES as lang (lang.code)}
                <button
                  type="button"
                  class="menu-lang-pill"
                  class:selected={getUiLang() === lang.code}
                  title={lang.nativeName}
                  onclick={() => void quickSwitchUiLanguage(lang.code)}
                >
                  {lang.shortLabel}
                </button>
              {/each}
            </div>
          </div>
          <button onclick={() => void openInternalTab("settings")}>
            <span>{t("menu.settings")}</span>
          </button>
          <button onclick={() => openFeedback({ category: "bug" })}>
            <span>{t("menu.feedback")}</span>
          </button>
          <div class="menu-divider"></div>
          <button onclick={() => { void exportAppData(); }}>{t("menu.exportAll")}</button>
          <button onclick={() => { exportProject(); }}>{t("menu.exportPage")}</button>
        </nav>
      </div>
    {:else if overlayKind === "nav-history"}
      <div
        class="overlay-menu-dismiss"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) void closeOverlay();
        }}
      >
        <nav class="overlay-menu overlay-chrome-menu" aria-label="Tab History" style={popupPositionStyle("left", 280)}>
          {#each navHistoryEntries as item (item.delta)}
            <button
              onclick={() => {
                void api.jumpHistoryDelta(item.delta).then((s) => {
                  applyBrowser(s);
                  void closeOverlay();
                });
              }}
            >
              <span style="overflow:hidden;text-overflow:ellipsis;white-space:nowrap;">{item.url}</span>
              <kbd>{item.delta > 0 ? `+${item.delta}` : item.delta}</kbd>
            </button>
          {/each}
        </nav>
      </div>
    {:else if overlayKind === "site-security"}
      {@const curUrl = activeTab?.url ?? ""}
      {@const isInternalUrl = curUrl.startsWith("tb://")}
      {@const isHttpsSite = curUrl.startsWith("https://")}
      {@const curHost = (() => { try { return curUrl ? new URL(curUrl).hostname.toLowerCase() : ""; } catch { return ""; } })()}
      {@const siteAllowlisted = Boolean(curHost && settings.adBlockAllowlist.some((e) => curHost === e || curHost.endsWith(`.${e}`)))}
      <div
        class="overlay-menu-dismiss"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) void closeOverlay();
        }}
      >
        <div class="overlay-translate" role="dialog" aria-label={trLocal("사이트 보안 정보", "Site Security", "サイトセキュリティ", "站点安全信息")} style={popupPositionStyle("left", 300)}>
          <div class="tp-header">
            <strong>{isInternalUrl ? "Fluo" : (curHost || curUrl)}</strong>
            <button type="button" class="tp-close" aria-label={t("common.close")} onclick={() => void closeOverlay()}>×</button>
          </div>
          <p class="action-modal-desc" style="margin: 8px 0;">
            {#if isInternalUrl}
              {trLocal("Fluo 로컬 내부 페이지입니다.", "Fluo internal browser page.", "Fluo のローカルページです。", "这是 Fluo 本地页面。")}
            {:else if isHttpsSite}
              🔒 {trLocal("HTTPS 암호화로 안전하게 연결되었습니다.", "Connection is encrypted via HTTPS.", "HTTPS で暗号化された安全な接続です。", "已通过 HTTPS 安全加密连接。")}
            {:else}
              ⚠️ {trLocal("암호화되지 않은 HTTP 연결입니다.", "Connection is not secure (HTTP).", "暗号化されていない HTTP 接続です。", "未加密的 HTTP 连接（不安全）。")}
            {/if}
          </p>
          <div class="tp-actions">
            {#if curHost && !isInternalUrl}
              <button
                type="button"
                onclick={() => {
                  void api.toggleAdblockSite(curUrl).then((view) => {
                    onSettingsSaved(view);
                    void closeOverlay();
                    void reload();
                  }).catch((e) => notify("error", errorMessage(e)));
                }}
              >
                {siteAllowlisted ? t("adblock.reenableSite") : t("adblock.allowSite", { host: curHost })}
              </button>
              <button
                type="button"
                onclick={() => {
                  toggleAutoTranslateHost(curHost);
                  void closeOverlay();
                }}
              >
                {autoTranslateHosts.includes(curHost)
                  ? `✓ ${trLocal("이 사이트 항상 자동 번역 켜짐", "Always translate enabled", "常に自動翻訳：オン", "已开启始终自动翻译")}`
                  : trLocal("이 사이트 항상 자동 번역", "Always translate this site", "このサイトを常に自動翻訳", "始终自动翻译此站点")}
              </button>
            {/if}
            {#if curUrl}
              <button type="button" onclick={() => { void copyCurrentUrl(); void closeOverlay(); }}>{t("common.copy")}</button>
            {/if}
          </div>
        </div>
      </div>
    {:else if overlayKind === "bookmark-edit"}
      <div
        class="overlay-menu-dismiss"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) void closeOverlay();
        }}
      >
        <div
          class="overlay-translate"
          role="dialog"
          aria-label={trLocal("북마크 편집", "Edit Bookmark", "ブックマーク編集", "编辑书签")}
          style={popupPositionStyle("right", 300)}
        >
          <form
            style="display:grid;gap:10px;"
            onsubmit={(event) => {
              event.preventDefault();
              void submitBookmarkDraft();
            }}
          >
            <div class="tp-header">
              <strong>{bmDraftId ? trLocal("북마크 편집", "Edit Bookmark", "ブックマーク編集", "编辑书签") : trLocal("북마크 추가", "Add Bookmark", "ブックマーク追加", "添加书签")}</strong>
              <button type="button" class="tp-close" aria-label={t("common.close")} onclick={() => void closeOverlay()}>×</button>
            </div>
            <label class="tp-field">
              <span>{trLocal("이름", "Name", "名前", "名称")}</span>
              <input type="text" bind:value={bmDraftTitle} />
            </label>
            <label class="tp-field">
              <span>URL</span>
              <input type="text" bind:value={bmDraftUrl} />
            </label>
            <label class="tp-field">
              <span>{trLocal("폴더", "Folder", "フォルダ", "文件夹")}</span>
              <input type="text" list="bm-existing-folders" bind:value={bmDraftFolder} placeholder="Default" />
              <datalist id="bm-existing-folders">
                {#each [...new Set(bookmarks.map((b) => b.folder).filter(Boolean))] as folderName (folderName)}
                  <option value={folderName}></option>
                {/each}
              </datalist>
            </label>
            <label class="tp-check">
              <input type="checkbox" bind:checked={bmDraftFavorite} />
              <span>★ {trLocal("즐겨찾기 고정", "Pin to Favorites", "お気に入りに固定", "固定到收藏夹")}</span>
            </label>
            <div class="tp-actions">
              <button class="primary" type="submit">{t("common.save")}</button>
              {#if bmDraftId}
                <button type="button" onclick={() => void deleteBookmarkDraft()}>{t("common.delete")}</button>
              {/if}
              <button type="button" onclick={() => void closeOverlay()}>{t("common.cancel")}</button>
            </div>
          </form>
        </div>
      </div>
    {:else if overlayKind === "adblock"}
      {@const currentHost = (() => {
        try {
          return activeTab?.url ? new URL(activeTab.url).hostname.toLowerCase() : "";
        } catch {
          return "";
        }
      })()}
      {@const siteAllowlisted = Boolean(
        currentHost &&
          settings.adBlockAllowlist.some(
            (entry) => currentHost === entry || currentHost.endsWith(`.${entry}`),
          ),
      )}
      <div
        class="overlay-menu-dismiss"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) void closeOverlay();
        }}
      >
        <nav class="overlay-menu overlay-chrome-menu" aria-label={t("adblock.aria")} style={popupPositionStyle("right", 282)}>
          <button onclick={() => { void toggleAdBlock().then(() => closeOverlay()); }}>
            <span>{settings.adBlockEnabled ? t("adblock.toggleOff") : t("adblock.toggleOn")}</span>
            <kbd>{settings.adBlockEnabled ? "ON" : "OFF"}</kbd>
          </button>
          {#if currentHost}
            <button
              onclick={() => {
                if (!activeTab?.url) return;
                void api
                  .toggleAdblockSite(activeTab.url)
                  .then((view) => {
                    onSettingsSaved(view);
                    notify(
                      "info",
                      siteAllowlisted
                        ? t("toast.adblockSiteReenabled", { host: currentHost })
                        : t("toast.adblockSiteAllowlisted", { host: currentHost }),
                    );
                    void closeOverlay();
                    void reload();
                  })
                  .catch((e) => notify("error", errorMessage(e)));
              }}
            >
              <span>{siteAllowlisted ? t("adblock.reenableSite") : t("adblock.allowSite", { host: currentHost })}</span>
            </button>
          {/if}
          <div class="menu-divider"></div>
          <button onclick={() => void openInternalTab("settings")}>
            <span>{t("adblock.moreSettings")}</span>
            <kbd>{settings.trackingPrevention}</kbd>
          </button>
        </nav>
      </div>
    {:else if overlayKind === "chrome-menu"}
      <div
        class="overlay-menu-dismiss"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) void closeOverlay();
        }}
      >
        <nav class="overlay-menu overlay-chrome-menu" aria-label={t("chromeMenu.aria")} style={popupPositionStyle("left", 258)}>
          <button onclick={() => { void api.windowAction("toggle-maximize"); void closeOverlay(); }}>
            <span>{t("chromeMenu.toggleMax")}</span>
          </button>
          <button onclick={() => { void api.windowAction("minimize"); void closeOverlay(); }}>
            <span>{t("chromeMenu.minimize")}</span>
          </button>
          <div class="menu-divider"></div>
          <button onclick={() => { void newTab(); }}>
            <span>{t("menu.newTab")}</span><kbd>Ctrl+T</kbd>
          </button>
          <button onclick={() => { void newPrivateTab(); }}>
            <span>{t("menu.newPrivateTab")}</span><kbd>Ctrl+Shift+P</kbd>
          </button>
          <button onclick={() => { void reopenClosedTab().then(() => closeOverlay()); }}>
            <span>{t("menu.reopenClosed")}</span><kbd>Ctrl+Shift+T</kbd>
          </button>
          <button onclick={() => { void bookmarkAllTabs().then(() => closeOverlay()); }}>
            <span>{t("chromeMenu.bookmarkAll")}</span><kbd>Ctrl+Shift+D</kbd>
          </button>
          <button onclick={() => renameWindow()}>
            <span>{t("chromeMenu.renameWindow")}</span>
          </button>
          <div class="menu-divider"></div>
          <button onclick={() => { toggleCompactChrome(); void closeOverlay(); }}>
            <span>{compactChrome ? t("menu.compactOff") : t("menu.compactOn")}</span>
          </button>
          <button onclick={() => { toggleGlassTint(); void closeOverlay(); }}>
            <span>{glassTinted ? t("menu.tintOff") : t("menu.tintOn")}</span>
          </button>
          <button onclick={() => { toggleBookmarkBar(); void closeOverlay(); }}>
            <span>{bookmarkBarVisible ? t("menu.bookmarkBarHide") : t("menu.bookmarkBarShow")}</span><kbd>Ctrl+Shift+B</kbd>
          </button>
          <button onclick={() => { overlayKind = "tab-search"; tabSearchQuery = ""; }}>
            <span>{t("menu.tabSearch")}</span><kbd>Ctrl+Shift+A</kbd>
          </button>
          <div class="menu-divider"></div>
          <button onclick={() => { overlayKind = "task-manager"; }}>
            <span>{t("menu.taskManager")}</span><kbd>Shift+Esc</kbd>
          </button>
          <button onclick={() => void openFindBar()}>
            <span>{t("menu.findInPage")}</span><kbd>Ctrl+F</kbd>
          </button>
          <div class="menu-divider"></div>
          <button onclick={() => { void api.windowAction("close"); }}>
            <span>{t("common.close")}</span><kbd>Alt+F4</kbd>
          </button>
        </nav>
      </div>
    {:else if overlayKind === "page-menu"}
      <div
        class="overlay-menu-dismiss"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) void closeOverlay();
        }}
      >
        <nav
          class="overlay-menu overlay-chrome-menu overlay-page-menu"
          aria-label={t("pageMenu.aria")}
          style={pageMenuPositionStyle(264, 390)}
        >
          <div class="page-menu-nav-row" role="group" aria-label={t("pageMenu.navGroup")}>
            <button
              type="button"
              class="pm-nav-btn"
              title={t("chrome.backTitle")}
              disabled={!browser.canGoBack}
              onclick={() => { void goBack(); void closeOverlay(); }}
            >
              ←
            </button>
            <button
              type="button"
              class="pm-nav-btn"
              title={t("chrome.forwardTitle")}
              disabled={!browser.canGoForward}
              onclick={() => { void goForward(); void closeOverlay(); }}
            >
              →
            </button>
            <button
              type="button"
              class="pm-nav-btn"
              title={t("chrome.reloadTitle")}
              onclick={() => { void reload(); void closeOverlay(); }}
            >
              ↻
            </button>
            <button
              type="button"
              class="pm-nav-btn"
              title={isCurrentBookmarked ? t("chrome.bookmarkSaved") : t("chrome.bookmarkSaveTitle")}
              onclick={() => { void saveCurrentBookmark(); void closeOverlay(); }}
            >
              {isCurrentBookmarked ? "★" : "☆"}
            </button>
          </div>
          <div class="menu-divider"></div>

          {#if pageMenuContext?.selectedText}
            {@const selText = pageMenuContext.selectedText}
            <button
              onclick={() => {
                const cx = pageMenuContext?.x ?? 160;
                const cy = pageMenuContext?.y ?? 160;
                void closeOverlay().then(() => api.runPageContextAction("quick-translate", selText, cx, cy));
              }}
            >
              <span>{t("pageMenu.quickTranslate")}</span>
              <kbd>“{selText.slice(0, 10)}{selText.length > 10 ? "…" : ""}”</kbd>
            </button>
            <button
              onclick={() => {
                const cx = pageMenuContext?.x ?? 160;
                const cy = pageMenuContext?.y ?? 160;
                void closeOverlay().then(() => api.runPageContextAction("add-glossary", selText, cx, cy));
              }}
            >
              <span>{t("pageMenu.addGlossary")}</span>
            </button>
            <button
              onclick={() => {
                void navigator.clipboard.writeText(selText);
                notify("info", t("pageMenu.copiedSel"));
                void closeOverlay();
              }}
            >
              <span>{t("common.copy")}</span><kbd>Ctrl+C</kbd>
            </button>
            <button
              onclick={() => {
                const q = encodeURIComponent(selText);
                const searchUrl =
                  settings.searchEngine === "bing"
                    ? `https://www.bing.com/search?q=${q}`
                    : settings.searchEngine === "duckduckgo"
                      ? `https://duckduckgo.com/?q=${q}`
                      : `https://www.google.com/search?q=${q}`;
                void api.newTab(searchUrl).then((s) => {
                  applyBrowser(s);
                  void closeOverlay();
                });
              }}
            >
              <span>{t("pageMenu.searchWeb", { text: `${selText.slice(0, 12)}${selText.length > 12 ? "…" : ""}` })}</span>
            </button>
            <div class="menu-divider"></div>
          {/if}

          {#if pageMenuContext?.linkUrl}
            {@const targetLink = pageMenuContext.linkUrl}
            <button
              onclick={() => {
                void api.newTab(targetLink).then((s) => {
                  applyBrowser(s);
                  void closeOverlay();
                });
              }}
            >
              <span>{t("pageMenu.openLinkTab")}</span>
            </button>
            <button
              onclick={() => {
                void newPrivateTab(targetLink);
              }}
            >
              <span>{t("pageMenu.openLinkPrivate")}</span>
            </button>
            <button
              onclick={() => {
                void navigator.clipboard.writeText(targetLink);
                notify("success", t("pageMenu.copiedLink"));
                void closeOverlay();
              }}
            >
              <span>{t("pageMenu.copyLink")}</span>
            </button>
            <div class="menu-divider"></div>
          {/if}

          {#if pageMenuContext?.imageUrl}
            {@const targetImg = pageMenuContext.imageUrl}
            <button
              onclick={() => {
                void api.newTab(targetImg).then((s) => {
                  applyBrowser(s);
                  void closeOverlay();
                });
              }}
            >
              <span>{t("pageMenu.openImgTab")}</span>
            </button>
            <button
              onclick={() => {
                void navigator.clipboard.writeText(targetImg);
                notify("success", t("pageMenu.copiedImg"));
                void closeOverlay();
              }}
            >
              <span>{t("pageMenu.copyImg")}</span>
            </button>
            <div class="menu-divider"></div>
          {/if}

          {#if pageMenuContext?.isEditable}
            {#if pageMenuContext?.selectedText}
              {@const editSel = pageMenuContext.selectedText}
              <button
                onclick={() => {
                  void closeOverlay().then(() => translateAndReplaceInInput(editSel, "English"));
                }}
              >
                <span>⇄ {trLocal("영어로 역번역해 입력창 치환", "Translate to English & Replace", "英語に翻訳して置換", "翻译为英语并替换")}</span>
              </button>
              <button
                onclick={() => {
                  void closeOverlay().then(() => translateAndReplaceInInput(editSel, "日本語"));
                }}
              >
                <span>⇄ {trLocal("일본어로 역번역해 입력창 치환", "Translate to Japanese & Replace", "日本語に翻訳して置換", "翻译为日语并替换")}</span>
              </button>
            {/if}
            <button
              onclick={() => {
                void closeOverlay().then(() => pasteFromClipboardToPage());
              }}
            >
              <span>{t("pageMenu.paste")}</span><kbd>Ctrl+V</kbd>
            </button>
            <button
              onclick={() => {
                void closeOverlay().then(() => api.runPageContextAction("cut"));
              }}
            >
              <span>{t("pageMenu.cut")}</span><kbd>Ctrl+X</kbd>
            </button>
            <button
              onclick={() => {
                void closeOverlay().then(() => api.runPageContextAction("select-all"));
              }}
            >
              <span>{t("pageMenu.selectAll")}</span><kbd>Ctrl+A</kbd>
            </button>
            <div class="menu-divider"></div>
          {/if}

          <button onclick={() => void triggerTranslateFromOverlay("translate")}>
            <span>{t("pageMenu.translatePage", { lang: selectedTargetLanguage || settings.targetLanguage })}</span>
          </button>
          <button onclick={() => void triggerTranslateFromOverlay("region")}>
            <span>{t("pageMenu.regionTranslate")}</span>
          </button>
          <button
            onclick={() => {
              void closeOverlay().then(() => summarizeCurrentPage());
            }}
          >
            <span>✨ {trLocal("AI 페이지 핵심 요약", "AI Page Summary (TL;DR)", "AI ページ要点要約", "AI 页面核心摘要")}</span>
          </button>
          {#if inlineMode}
            <button onclick={() => void triggerTranslateFromOverlay("restore")}>
              <span>{t("pageMenu.restoreOriginal")}</span>
            </button>
          {/if}
          <div class="menu-divider"></div>
          <button onclick={() => void openFindBar()}>
            <span>{t("pageMenu.findDots")}</span><kbd>Ctrl+F</kbd>
          </button>
          <button
            onclick={() => {
              void addCurrentPageToReadingList();
              void closeOverlay();
            }}
          >
            <span>📑 {trLocal("나중에 읽기에 추가", "Add to Reading List", "あとで読むに追加", "添加到稍后阅读")}</span>
          </button>
          <button
            onclick={() => {
              void closeOverlay().then(() => api.runPageContextAction("toggle-reader-mode"));
            }}
          >
            <span>📖 {trLocal("집중 읽기 모드 (방해 요소 숨김)", "Distraction-Free Reader Mode", "集中リーダーモード切替", "沉浸阅读模式 (隐藏干扰)")}</span>
          </button>
          <button
            onclick={() => {
              void copyCleanCurrentUrl();
              void closeOverlay();
            }}
          >
            <span>✨ {trLocal("주소 복사 (추적 파라미터 제거)", "Copy Clean URL (No Trackers)", "クリーンURLをコピー (追跡除去)", "复制干净网址 (移除追踪)")}</span>
          </button>
          <button
            onclick={() => {
              void closeOverlay().then(() => {
                ttsActive = true;
                return api.runPageContextAction("tts-toggle");
              });
            }}
          >
            <span>🔊 {trLocal("소리 내어 읽기 (TTS)", "Read Aloud (TTS)", "音声読み上げ (TTS)", "语音朗读 (TTS)")}</span>
          </button>
          {#if pageMenuContext?.hasMedia}
            <button
              onclick={() => {
                void closeOverlay().then(() => api.runPageContextAction("pip"));
              }}
            >
              <span>📺 {trLocal("동영상 PIP (화면 속 화면)", "Picture-in-Picture (PiP)", "ピクチャ・イン・ピクチャ (PiP)", "画中画模式 (PiP)")}</span>
            </button>
            <button
              onclick={() => {
                void closeOverlay().then(() => api.runPageContextAction("media-rate", "cycle"));
              }}
            >
              <span>⏩ {trLocal("동영상 배속 전환 (1.0x~2.0x)", "Cycle Video Speed (1.0x–2.0x)", "動画再生速度を切替 (1.0x〜2.0x)", "切换视频倍速 (1.0x–2.0x)")}</span>
            </button>
          {/if}
          <button
            onclick={() => {
              void closeOverlay().then(() => api.runPageContextAction("toggle-dark-page"));
            }}
          >
            <span>🌙 {trLocal("웹페이지 야간 다크 반전", "Smart Dark Page Mode", "ページのダーク反転切替", "网页夜间深色反色")}</span>
          </button>
          <button
            onclick={() => {
              void closeOverlay().then(() => api.runPageContextAction("scroll-top"));
            }}
          >
            <span>↑ {trLocal("페이지 맨 위로 이동", "Scroll to Top", "ページの先頭へ移動", "回到页面顶部")}</span>
          </button>
          <button
            onclick={() => {
              void closeOverlay().then(() => printPage());
            }}
          >
            <span>{t("pageMenu.printDots")}</span><kbd>Ctrl+P</kbd>
          </button>
          <button
            onclick={() => {
              void closeOverlay().then(() => api.runPageContextAction("inspect"));
            }}
          >
            <span>{trLocal("요소 검사 (개발자 도구)", "Inspect Element", "要素を調査", "检查元素")}</span><kbd>F12</kbd>
          </button>
        </nav>
      </div>
    {:else if overlayKind === "tab-menu"}
      {@const menuTab = browser.tabs.find((item) => item.id === tabMenuTargetId)}
      <div
        class="overlay-menu-dismiss"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) void closeOverlay();
        }}
      >
        <nav class="overlay-menu overlay-chrome-menu" aria-label={t("tabbar.tabMenu")} style={popupPositionStyle("left", 228)}>
          {#if menuTab}
            <button onclick={() => { void pinTab(menuTab.id).then(() => closeOverlay()); }}>
              <span>{menuTab.pinned ? t("tabMenu.unpin") : t("tabMenu.pin")}</span>
            </button>
            <button onclick={() => { void toggleMuteTab(menuTab.id).then(() => closeOverlay()); }}>
              <span>{menuTab.muted ? t("tabMenu.unmute") : t("tabMenu.mute")}</span><kbd>Ctrl+M</kbd>
            </button>
            {#if menuTab.id !== browser.activeTab && !menuTab.sleeping}
              <button onclick={() => { void sleepTab(menuTab.id).then(() => closeOverlay()); }}>
                <span>{t("tabMenu.sleep")}</span>
              </button>
            {/if}
            <button onclick={() => { void duplicateTab(menuTab.id).then(() => closeOverlay()); }}>
              <span>{t("tabMenu.duplicate")}</span>
            </button>
            <button onclick={() => { void copyCleanCurrentUrl(menuTab.url).then(() => closeOverlay()); }}>
              <span>✨ {trLocal("이 탭 깨끗한 주소 복사", "Copy Clean Tab URL", "このタブのクリーンURLをコピー", "复制此标签页干净网址")}</span>
            </button>
            <button onclick={() => { void deduplicateWorkspaceTabs().then(() => closeOverlay()); }}>
              <span>🧹 {trLocal("중복 탭 정리", "Close Duplicate Tabs", "重複タブを整理", "清理重复标签页")}</span>
            </button>
            <button onclick={() => { void reopenClosedTab().then(() => closeOverlay()); }}>
              <span>{t("menu.reopenClosed")}</span><kbd>Ctrl+Shift+T</kbd>
            </button>
            <div class="menu-divider"></div>
            <button onclick={() => { void closeOtherTabs(menuTab.id).then(() => closeOverlay()); }}>
              <span>{t("tabMenu.closeOthers")}</span>
            </button>
            <button onclick={() => { void closeTabsToRight(menuTab.id).then(() => closeOverlay()); }}>
              <span>{t("tabMenu.closeRight")}</span>
            </button>
            <button onclick={() => { void closeTab(menuTab.id).then(() => closeOverlay()); }}>
              <span>{t("tabbar.closeTab")}</span><kbd>Ctrl+W</kbd>
            </button>
            {#if browser.workspaces.length > 1}
              <div class="menu-divider"></div>
              {#each browser.workspaces.filter((w) => w.id !== browser.activeWorkspace) as workspace (workspace.id)}
                <button onclick={() => { void moveTab(menuTab.id, workspace.id).then(() => closeOverlay()); }}>
                  <span>→ {workspaceLabel(workspace)}</span>
                </button>
              {/each}
            {/if}
          {/if}
        </nav>
      </div>
    {:else if overlayKind === "tab-search"}
      <div
        class="overlay-menu-dismiss"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) void closeOverlay();
        }}
      >
        <div class="overlay-glass-modal tab-search-modal" role="dialog" aria-label={t("tabbar.searchTabs")}>
          <div class="gm-header">
            <strong>{t("tabSearch.title", { count: browser.tabs.length })}</strong>
            <button type="button" class="tp-close" aria-label={t("common.close")} onclick={() => void closeOverlay()}>×</button>
          </div>
          <input
            class="gm-search"
            type="search"
            placeholder={t("tabSearch.placeholder")}
            bind:value={tabSearchQuery}
          />
          <div class="gm-list">
            {#each filteredSearchTabs as tab (tab.id)}
              {@const tabWs = browser.workspaces.find((w) => w.id === tab.workspaceId)}
              <div class="gm-item" class:active={tab.id === browser.activeTab}>
                <button
                  type="button"
                  class="gm-item-main"
                  onclick={() => {
                    void selectTab(tab.id).then(() => closeOverlay());
                  }}
                >
                  <span class="gm-item-title">
                    {tab.isPrivate ? "🕶 " : ""}{tab.muted ? "🔇 " : ""}{tab.sleeping ? "💤 " : ""}{tab.title || tab.url}
                  </span>
                  <span class="gm-item-sub">
                    {tabWs ? workspaceLabel(tabWs) : t("common.defaultWorkspace")} · {tab.url}
                  </span>
                </button>
                <button
                  type="button"
                  class="gm-item-close"
                  title={t("tabbar.closeTab")}
                  onclick={() => void closeTab(tab.id)}
                >
                  ×
                </button>
              </div>
            {:else}
              <p class="gm-empty">{t("tabSearch.empty")}</p>
            {/each}
          </div>
        </div>
      </div>
    {:else if overlayKind === "task-manager"}
      <div
        class="overlay-menu-dismiss"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) void closeOverlay();
        }}
      >
        <div class="overlay-glass-modal task-manager-modal" role="dialog" aria-label={t("menu.taskManager")}>
          <div class="gm-header">
            <strong>{t("taskMgr.title", { count: browser.tabs.length })}</strong>
            <button type="button" class="tp-close" aria-label={t("common.close")} onclick={() => void closeOverlay()}>×</button>
          </div>
          <div class="gm-list">
            {#each browser.tabs as tab (tab.id)}
              <div class="gm-item" class:active={tab.id === browser.activeTab}>
                <div class="gm-item-main static">
                  <span class="gm-item-title">
                    {tab.title || tab.url}
                  </span>
                  <span class="gm-item-sub">
                    {tab.id === browser.activeTab ? t("taskMgr.active") : tab.sleeping ? t("taskMgr.sleeping") : t("taskMgr.idle")}
                    {tab.muted ? t("taskMgr.mutedBadge") : ""}
                    {tab.isPrivate ? t("taskMgr.privateBadge") : ""}
                    · {tab.url}
                  </span>
                </div>
                <div class="gm-item-actions">
                  {#if tab.id !== browser.activeTab}
                    <button type="button" onclick={() => void selectTab(tab.id).then(() => closeOverlay())}>{t("taskMgr.switch")}</button>
                    {#if !tab.sleeping}
                      <button type="button" onclick={() => void sleepTab(tab.id)}>{t("taskMgr.sleep")}</button>
                    {/if}
                  {/if}
                  <button type="button" onclick={() => void toggleMuteTab(tab.id)}>
                    {tab.muted ? t("taskMgr.unmute") : t("taskMgr.mute")}
                  </button>
                  <button type="button" class="danger" onclick={() => void closeTab(tab.id)}>{t("taskMgr.kill")}</button>
                </div>
              </div>
            {/each}
          </div>
        </div>
      </div>
    {:else if overlayKind === "workspace"}
      {@const activeWs = browser.workspaces.find((w) => w.id === browser.activeWorkspace)}
      <div
        class="overlay-menu-dismiss left"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) void closeOverlay();
        }}
      >
        <nav class="overlay-menu overlay-workspace" aria-label={t("ws.menuAria")} style={popupPositionStyle("left", 252)}>
          {#each browser.workspaces as workspace (workspace.id)}
            {@const count = browser.tabs.filter((tab) => tab.workspaceId === workspace.id).length}
            <button
              class:active-item={workspace.id === browser.activeWorkspace}
              disabled={workspaceBusy}
              onclick={() => void switchWorkspace(workspace.id)}
            >
              <span>{workspace.id === browser.activeWorkspace ? "✓ " : ""}{workspaceLabel(workspace)}</span>
              <span class="ws-count">{t("ws.tabCount", { count })}</span>
            </button>
          {/each}
          <div class="menu-divider"></div>
          {#if workspaceMode === "list"}
            <button type="button" onclick={() => void startWorkspaceCreate()}>{t("ws.add")}</button>
            <button type="button" onclick={() => void startWorkspaceRename()}>{t("ws.rename")}</button>
            {#if activeWs && activeWs.id !== "default"}
              <button type="button" class="danger" onclick={() => void startWorkspaceDelete()}>{t("ws.delete")}</button>
            {/if}
          {:else if workspaceMode === "create" || workspaceMode === "rename"}
            <form
              class="ws-inline-form"
              onsubmit={(event) => {
                event.preventDefault();
                if (workspaceMode === "create") void submitCreateWorkspace();
                else void submitRenameWorkspace();
              }}
            >
              <input
                type="text"
                bind:value={workspaceDraftName}
                placeholder={workspaceMode === "create" ? t("ws.newPlaceholder") : t("ws.renamePlaceholder")}
                disabled={workspaceBusy}
              />
              <div class="ws-form-actions">
                <button type="submit" class="ws-submit" disabled={workspaceBusy || !workspaceDraftName.trim()}>
                  {workspaceMode === "create" ? t("ws.createBtn") : t("common.save")}
                </button>
                <button type="button" onclick={() => (workspaceMode = "list")} disabled={workspaceBusy}>
                  {t("common.cancel")}
                </button>
              </div>
            </form>
          {:else if workspaceMode === "delete" && activeWs}
            <div class="ws-confirm-box">
              <p>{t("ws.deleteConfirm", { name: workspaceLabel(activeWs) })}</p>
              <div class="ws-form-actions">
                <button type="button" class="ws-danger-btn" onclick={() => void submitDeleteWorkspace()} disabled={workspaceBusy}>
                  {t("common.confirmDelete")}
                </button>
                <button type="button" onclick={() => (workspaceMode = "list")} disabled={workspaceBusy}>
                  {t("common.cancel")}
                </button>
              </div>
            </div>
          {/if}
        </nav>
      </div>
    {:else if overlayKind === "translate"}
      <div
        class="overlay-menu-dismiss"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) void closeOverlay();
        }}
      >
        <div class="overlay-translate" role="dialog" aria-label={t("translate.title")} style={popupPositionStyle("right", 280)}>
          <div class="tp-header">
            <strong>{t("translate.title")}</strong>
            <button type="button" class="tp-close" aria-label={t("common.close")} onclick={() => void closeOverlay()}>×</button>
          </div>
          <label class="tp-field">
            <span>{t("translate.modelLabel")}</span>
            <div class="tp-row">
              <select
                value={translationModel}
                onchange={(event) => (selectedModel = event.currentTarget.value)}
              >
                {#if translationModel && !availableModels.some((model) => model.name === translationModel)}
                  <option value={translationModel}>{translationModel}</option>
                {/if}
                {#each availableModels as model (model.name)}
                  <option value={model.name}>{model.displayName || model.name}</option>
                {/each}
                {#if !translationModel && availableModels.length === 0}
                  <option value="" disabled>{t("translate.modelUnset")}</option>
                {/if}
              </select>
              <button type="button" onclick={loadTranslationModels} disabled={loadingModels}>
                {loadingModels ? "…" : t("translate.modelListBtn")}
              </button>
              {#if settings.provider === "hyMt2"}
                <button type="button" onclick={() => void openSettings()} title={t("translate.manageTitle")}>
                  {t("translate.manageBtn")}
                </button>
              {/if}
            </div>
          </label>
          <label class="tp-field">
            <span>{t("translate.targetLangLabel")}</span>
            <input
              type="text"
              list="translate-lang-presets"
              placeholder={t("translate.targetLangPlaceholder")}
              bind:value={selectedTargetLanguage}
              onchange={() => persistTranslationLanguage(selectedTargetLanguage)}
            />
            <datalist id="translate-lang-presets">
              {#each targetLanguagePresets as lang (lang)}
                <option value={lang}></option>
              {/each}
            </datalist>
          </label>
          <label class="tp-field">
            <span>{t("translate.styleLabel")}</span>
            <select
              bind:value={selectedStyle}
              onchange={() => persistTranslationStyle(selectedStyle)}
            >
              <option value="auto">{t("translate.styleAuto")}</option>
              <option value="natural">{t("translate.styleNatural")}</option>
              <option value="novel">{t("translate.styleNovel")}</option>
              <option value="literal">{t("translate.styleLiteral")}</option>
              <option value="polished">{t("translate.stylePolished")}</option>
            </select>
            {#if selectedStyle === "auto"}
              <span class="tp-style-hint">
                {detectedAutoStyle
                  ? t("translate.styleDetected", { style: detectedAutoStyle })
                  : t("translate.styleAutoHint")}
              </span>
            {/if}
          </label>
          <label class="tp-check">
            <input type="checkbox" bind:checked={bilingualAll} />
            <span>{t("translate.bilingual")}</span>
          </label>
          {#if settings.provider === "hyMt2" && overlayHyMt2Models.length > 0 && !overlayHyMt2Models.some((m) => m.installed)}
            {@const recModel = overlayHyMt2Models.find((m) => m.recommended) ?? overlayHyMt2Models[0]}
            <div class="tp-setup">
              <div class="tp-setup-title">{t("translate.firstSetupTitle")}</div>
              <div class="tp-setup-sub">
                {t("translate.firstSetupSub", { name: recModel.displayName, gb: (recModel.expectedBytes / (1024 ** 3)).toFixed(2) })}
              </div>
              {#if overlayDownloadProgress}
                <div class="tp-prog-bar">
                  <div class="tp-prog-fill" style={`width:${overlayDownloadProgress.percent.toFixed(1)}%`}></div>
                </div>
                <div class="tp-setup-row">
                  <span>{overlayDownloadProgress.percent.toFixed(1)}% · {overlayDownloadProgress.speedMbps.toFixed(1)} MB/s</span>
                  <button type="button" onclick={() => void cancelOverlayDownload()}>{t("translate.pause")}</button>
                </div>
              {:else}
                <button
                  type="button"
                  class="tp-setup-btn"
                  onclick={() => void downloadRecommendedInOverlay()}
                >
                  {t("translate.downloadRec", { action: recModel.partialBytes > 0 ? t("translate.resumeDownload") : t("translate.startDownload") })}
                </button>
              {/if}
            </div>
          {/if}
          <div class="tp-actions">
            <button
              class="primary"
              type="button"
              onclick={() => void triggerTranslateFromOverlay("translate")}
              disabled={translating}
            >
              {t("translate.runPage")}
            </button>
            <button
              type="button"
              onclick={() => void triggerTranslateFromOverlay("region")}
              disabled={translating}
            >
              {t("translate.runRegion")}
            </button>
            {#if inlineMode}
              <button
                type="button"
                onclick={() => void triggerTranslateFromOverlay("retranslate")}
                disabled={translating}
              >
                {t("translate.retranslate")}
              </button>
              <button
                type="button"
                onclick={() => void triggerTranslateFromOverlay("restore")}
                disabled={translating}
              >
                {t("translate.restore")}
              </button>
            {/if}
            {#if translating}
              <button type="button" onclick={() => void triggerTranslateFromOverlay("cancel")}>
                {t("translate.stop")}
              </button>
            {/if}
            <button
              type="button"
              onclick={() => {
                void closeOverlay().then(() => summarizeCurrentPage());
              }}
              disabled={translating}
            >
              ✨ {trLocal("AI 페이지 핵심 요약", "AI Page Summary (TL;DR)", "AI ページ要点要約", "AI 页面核心摘要")}
            </button>
            <button
              type="button"
              onclick={() => {
                void closeOverlay().then(() => {
                  ttsActive = true;
                  return api.runPageContextAction("tts-toggle");
                });
              }}
            >
              🔊 {trLocal("소리 내어 읽기 (TTS)", "Read Aloud (TTS)", "音声読み上げ (TTS)", "语音朗读 (TTS)")}
            </button>
            <button
              type="button"
              class="tp-report-btn"
              onclick={() => openFeedback({ category: "translation" })}
            >
              {t("translate.reportBtn")}
            </button>
          </div>
        </div>
      </div>
    {:else if overlayKind === "rename-window"}
      <div
        class="overlay-menu-dismiss"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) void closeOverlay();
        }}
      >
        <div
          class="overlay-glass-modal action-modal"
          role="dialog"
          aria-label={t("modal.renameWindowTitle")}
        >
          <form
            class="action-modal-form"
            onsubmit={(event) => {
              event.preventDefault();
              void submitRenameWindow();
            }}
          >
            <div class="gm-header">
              <strong>{t("modal.renameWindowTitle")}</strong>
              <button type="button" class="tp-close" aria-label={t("common.close")} onclick={() => void closeOverlay()}>×</button>
            </div>
            <p class="action-modal-desc">{t("modal.renameWindowHint")}</p>
            <input
              class="gm-search"
              type="text"
              placeholder={t("modal.renameWindowPh")}
              bind:value={renameWindowDraft}
            />
            <div class="action-modal-foot">
              <button type="button" class="action-btn" onclick={() => void closeOverlay()}>{t("common.cancel")}</button>
              <button type="submit" class="action-btn primary">{t("modal.applyBtn")}</button>
            </div>
          </form>
        </div>
      </div>
    {:else if overlayKind === "export-project"}
      <div
        class="overlay-menu-dismiss"
        role="presentation"
        onclick={(event) => {
          if (event.target === event.currentTarget) void closeOverlay();
        }}
      >
        <div class="overlay-glass-modal action-modal" role="dialog" aria-label={t("modal.exportProjectTitle")}>
          <div class="gm-header">
            <strong>{t("modal.exportProjectTitle")}</strong>
            <button type="button" class="tp-close" aria-label={t("common.close")} onclick={() => void closeOverlay()}>×</button>
          </div>
          <p class="action-modal-desc">{t("modal.exportProjectDesc")}</p>
          <div class="action-modal-foot">
            <button type="button" class="action-btn" onclick={() => void closeOverlay()} disabled={exporting}>{t("common.cancel")}</button>
            <button type="button" class="action-btn" onclick={() => void runExportProject(false)} disabled={exporting}>{t("modal.excludeOriginalBtn")}</button>
            <button type="button" class="action-btn primary" onclick={() => void runExportProject(true)} disabled={exporting}>{t("modal.includeOriginalBtn")}</button>
          </div>
        </div>
      </div>
    {:else if overlayKind === "onboarding"}
      <OnboardingModal
        firstRun={!settings.onboardingCompleted}
        currentTargetLanguage={selectedTargetLanguage || settings.targetLanguage}
        onlanguagechange={async (lang) => {
          setUiLang(lang);
          try {
            const updated = await api.setUiLanguage(lang, true);
            onSettingsSaved(updated);
          } catch (cause) {
            notify("error", errorMessage(cause));
          }
        }}
        oncomplete={async (targetLang) => {
          try {
            const updated = await api.completeOnboarding(getUiLang(), targetLang);
            onSettingsSaved(updated);
            await closeOverlay();
          } catch (cause) {
            notify("error", errorMessage(cause));
          }
        }}
        onclose={() => {
          if (!settings.onboardingCompleted) {
            void api
              .completeOnboarding(getUiLang(), selectedTargetLanguage || settings.targetLanguage)
              .then((updated) => onSettingsSaved(updated))
              .catch(() => {})
              .finally(() => void closeOverlay());
          } else {
            void closeOverlay();
          }
        }}
      />
    {:else if overlayKind === "feedback"}
      <FeedbackPanel
        initialDraft={feedbackDraft}
        currentUrl={activeTab?.url ?? ""}
        onclose={closeOverlay}
        onnotify={notify}
      />
    {:else if overlayKind === "settings" && settingsView}
      <SettingsPanel
        view={settingsView}
        onclose={closeOverlay}
        onsaved={onSettingsSaved}
        onnotify={notify}
      />
    {:else if overlayKind === "glossary"}
      <GlossaryPanel onclose={closeOverlay} onnotify={notify} />
    {:else if overlayKind === "bookmarks"}
      <BookmarkPanel
        current={activeTab}
        onclose={closeOverlay}
        onnavigate={(url) => void navigateFromOverlay(url)}
        onnotify={notify}
      />
    {:else if overlayKind === "history"}
      <TranslationHistoryPanel
        onclose={closeOverlay}
        onnavigate={(url) => void navigateFromOverlay(url)}
        onnotify={notify}
      />
    {:else if overlayKind === "visits"}
      <HistoryPanel
        onclose={closeOverlay}
        onnavigate={(url) => void navigateFromOverlay(url)}
        onnotify={notify}
      />
    {:else if overlayKind === "downloads"}
      <DownloadsPanel onclose={closeOverlay} onnotify={notify} />
    {/if}

    {#if status}
      <div class="overlay-notice" class:error={status.kind === "error"} class:success={status.kind === "success"}>
        <span>{status.text}</span>
        <button type="button" aria-label={t("common.close")} onclick={() => (status = null)}>×</button>
      </div>
    {/if}
  </div>
  {/if}

  <main class="content"><div class="browser-placeholder"></div></main>
</div>
{/if}

<style>
  :global(:root) {
    --surface: #ffffff;
    --surface-2: #edf0f8;
    --surface-hover: rgba(255, 255, 255, 0.85);
    --surface-strong: rgba(226, 232, 245, 0.92);
    --border: rgba(15, 23, 42, 0.22);
    --border-strong: rgba(15, 23, 42, 0.34);
    --text: #0f1422;
    --text-muted: #2d364d;
    --text-soft: #54607a;
    --text-halo: 0 1px 2px rgba(255, 255, 255, 0.92), 0 0 8px rgba(255, 255, 255, 0.65);
    --custom-accent: #684cff;
    --accent: var(--custom-accent, #684cff);
    --accent-soft: color-mix(in srgb, var(--accent) 18%, transparent);
    --ok: #15803d;
    --ok-soft: rgba(22, 163, 74, 0.14);
    --err: #b42318;
    --err-soft: rgba(220, 38, 38, 0.14);
    --chrome-height: 88px;
    --glass-backdrop: blur(26px) saturate(2.1) brightness(1.04);
    --scroll-thumb: rgba(15, 23, 42, 0.18);
    --scroll-thumb-hover: rgba(15, 23, 42, 0.32);
    --scroll-border: rgba(255, 255, 255, 0.82);
    --glass-rim-grad: linear-gradient(
      145deg,
      rgba(255, 255, 255, 0.96) 0%,
      rgba(255, 255, 255, 0.42) 45%,
      rgba(15, 23, 42, 0.22) 100%
    );
    --glass-shell-bg:
      radial-gradient(circle at 14% 0%, color-mix(in srgb, var(--accent) 14%, transparent), transparent 46%),
      linear-gradient(180deg, #eff2f8 0%, #f5f7fb 100%);
    --glass-island:
      linear-gradient(180deg, rgba(255, 255, 255, 0.84) 0%, rgba(244, 248, 255, 0.78) 100%);
    --glass-border: rgba(15, 23, 42, 0.22);
    --glass-border-strong: rgba(15, 23, 42, 0.32);
    --glass-highlight: rgba(255, 255, 255, 0.96);
    --glass-inner-edge: rgba(15, 23, 42, 0.10);
    --glass-pill:
      linear-gradient(180deg, rgba(255, 255, 255, 0.78) 0%, rgba(244, 247, 254, 0.68) 100%);
    --glass-pill-active:
      linear-gradient(180deg, rgba(255, 255, 255, 0.94) 0%, rgba(248, 250, 255, 0.86) 100%);
    --glass-tab:
      linear-gradient(180deg, rgba(255, 255, 255, 0.70) 0%, rgba(242, 246, 254, 0.58) 100%);
    --glass-bevel:
      inset 0 1px 0 var(--glass-highlight),
      inset 0 -1px 0 var(--glass-inner-edge),
      0 2px 8px rgba(15, 23, 42, 0.08);
    --glass-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.85),
      0 12px 30px rgba(15, 23, 42, 0.18);
    --primary-grad:
      linear-gradient(
        135deg,
        color-mix(in srgb, var(--accent) 88%, transparent) 0%,
        color-mix(in srgb, var(--accent) 78%, rgba(15, 23, 42, 0.24)) 100%
      );
    --primary-bevel:
      inset 0 1px 0 rgba(255, 255, 255, 0.65),
      inset 0 -1px 0 rgba(15, 23, 42, 0.16);
    --primary-glow: var(--primary-bevel);
    --secondary-grad:
      linear-gradient(135deg, rgba(45, 190, 185, 0.86) 0%, rgba(36, 165, 172, 0.78) 100%);
  }

  :global(:root[data-theme="dark"]),
  .app.page-dark {
    --surface: #1c1f2b;
    --surface-2: #222533;
    --surface-hover: rgba(255, 255, 255, 0.18);
    --surface-strong: rgba(255, 255, 255, 0.26);
    --border: rgba(255, 255, 255, 0.22);
    --border-strong: rgba(255, 255, 255, 0.34);
    --text: #f8faff;
    --text-muted: #dce3f2;
    --text-soft: #a8b3cb;
    --text-halo: 0 1px 3px rgba(0, 0, 0, 0.88), 0 0 8px rgba(0, 0, 0, 0.62);
    --accent: color-mix(in srgb, var(--custom-accent, #684cff) 76%, #ffffff);
    --accent-soft: color-mix(in srgb, var(--accent) 28%, transparent);
    --glass-backdrop: blur(26px) saturate(2.05) contrast(1.05);
    --scroll-thumb: rgba(255, 255, 255, 0.22);
    --scroll-thumb-hover: rgba(255, 255, 255, 0.38);
    --scroll-border: rgba(255, 255, 255, 0.24);
    --glass-shell-bg:
      radial-gradient(circle at 14% 0%, color-mix(in srgb, var(--accent) 14%, transparent), transparent 48%),
      linear-gradient(180deg, #161923 0%, #1c202c 100%);
    --glass-rim-grad: linear-gradient(
      145deg,
      rgba(255, 255, 255, 0.48) 0%,
      rgba(255, 255, 255, 0.16) 48%,
      rgba(255, 255, 255, 0.28) 100%
    );
    --glass-island:
      linear-gradient(180deg, rgba(22, 27, 42, 0.84) 0%, rgba(14, 18, 28, 0.80) 100%);
    --glass-border: rgba(255, 255, 255, 0.24);
    --glass-border-strong: rgba(255, 255, 255, 0.36);
    --glass-highlight: rgba(255, 255, 255, 0.38);
    --glass-inner-edge: rgba(0, 0, 0, 0.32);
    --glass-pill:
      linear-gradient(180deg, rgba(26, 31, 46, 0.78) 0%, rgba(16, 20, 32, 0.70) 100%);
    --glass-pill-active:
      linear-gradient(180deg, rgba(36, 43, 64, 0.92) 0%, rgba(24, 30, 48, 0.84) 100%);
    --glass-tab:
      linear-gradient(180deg, rgba(24, 29, 44, 0.70) 0%, rgba(15, 20, 32, 0.62) 100%);
    --glass-bevel:
      inset 0 1px 0 var(--glass-highlight),
      inset 0 -1px 0 var(--glass-inner-edge),
      0 2px 8px rgba(0, 0, 0, 0.35);
    --glass-shadow:
      inset 0 1px 0 rgba(255, 255, 255, 0.22),
      0 14px 34px rgba(0, 0, 0, 0.56);
  }

  .app.tinted,
  .internal-page-root.tinted,
  :global(:root[data-tinted="true"]) {
    --glass-backdrop: none;
    --glass-island: linear-gradient(180deg, #f3f5fb 0%, #e8ecf7 100%);
    --glass-pill: linear-gradient(180deg, #f5f7fc 0%, #eaeff9 100%);
    --glass-pill-active: linear-gradient(180deg, #ffffff 0%, #f1f4fc 100%);
    --glass-tab: linear-gradient(180deg, #eaedf7 0%, #dfe5f2 100%);
    --glass-border: rgba(30, 41, 59, 0.26);
    --glass-border-strong: rgba(30, 41, 59, 0.38);
    --glass-inner-edge: rgba(30, 41, 59, 0.08);
    --scroll-thumb: rgba(30, 41, 59, 0.24);
    --scroll-thumb-hover: rgba(30, 41, 59, 0.42);
    --scroll-border: rgba(255, 255, 255, 0.6);
  }

  :global(:root[data-theme="dark"]) .app.tinted,
  :global(:root[data-theme="dark"]) .internal-page-root.tinted,
  :global(:root[data-theme="dark"][data-tinted="true"]),
  .app.page-dark.tinted {
    --glass-backdrop: none;
    --glass-island: linear-gradient(180deg, #272b3a 0%, #1f2330 100%);
    --glass-pill: linear-gradient(180deg, #2d3244 0%, #242938 100%);
    --glass-pill-active: linear-gradient(180deg, #363c52 0%, #2c3144 100%);
    --glass-tab: linear-gradient(180deg, #232736 0%, #1c202c 100%);
    --glass-border: rgba(0, 0, 0, 0.62);
    --glass-border-strong: rgba(0, 0, 0, 0.78);
    --glass-inner-edge: rgba(0, 0, 0, 0.34);
    --scroll-thumb: rgba(255, 255, 255, 0.24);
    --scroll-thumb-hover: rgba(255, 255, 255, 0.42);
    --scroll-border: rgba(255, 255, 255, 0.14);
  }

  :global(:root[data-theme="sepia"]) {
    --surface: #fffaf0;
    --surface-2: #f4ecdc;
    --surface-hover: rgba(255, 250, 240, 0.75);
    --surface-strong: #e5dac4;
    --border: rgba(68, 52, 34, 0.22);
    --border-strong: rgba(68, 52, 34, 0.34);
    --text: #352d23;
    --text-muted: #655a4c;
    --text-soft: #8e8170;
    --accent: var(--custom-accent, #7757a5);
    --accent-soft: color-mix(in srgb, var(--accent) 16%, transparent);
    --glass-shell-bg:
      radial-gradient(circle at 14% 0%, color-mix(in srgb, var(--accent) 18%, transparent), transparent 48%),
      linear-gradient(180deg, #f9f3e6 0%, #f2e8d5 100%);
    --glass-island:
      linear-gradient(180deg, rgba(255, 250, 240, 0.34) 0%, rgba(244, 234, 214, 0.24) 100%);
    --glass-pill:
      linear-gradient(180deg, rgba(255, 250, 240, 0.18) 0%, rgba(245, 236, 216, 0.10) 100%);
    --glass-pill-active:
      linear-gradient(180deg, rgba(255, 252, 244, 0.38) 0%, rgba(248, 240, 222, 0.24) 100%);
    --glass-tab:
      linear-gradient(180deg, rgba(255, 248, 234, 0.12) 0%, rgba(240, 229, 206, 0.06) 100%);
    --glass-border: rgba(68, 52, 34, 0.20);
    --glass-border-strong: rgba(68, 52, 34, 0.30);
    --glass-highlight: rgba(255, 255, 255, 0.90);
    --glass-inner-edge: rgba(68, 52, 34, 0.08);
    --scroll-thumb: rgba(68, 52, 34, 0.18);
    --scroll-thumb-hover: rgba(68, 52, 34, 0.34);
    --scroll-border: rgba(255, 255, 255, 0.72);
  }

  :global(html),
  :global(body) {
    height: 100%;
    margin: 0;
    overflow: hidden;
    background: transparent;
  }

  :global(body) {
    color: var(--text);
    font-family:
      "Pretendard", system-ui, -apple-system, "Segoe UI", "Malgun Gothic",
      "Apple SD Gothic Neo", sans-serif;
    font-size: 14px;
    -webkit-font-smoothing: antialiased;
  }

  :global(html[data-overlay="true"]),
  :global(html[data-overlay="true"] body) {
    background: transparent;
  }

  :global(*, *::before, *::after) {
    box-sizing: border-box;
  }

  :global(*::-webkit-scrollbar) {
    width: 9px;
    height: 9px;
    background: transparent;
  }

  :global(*::-webkit-scrollbar-track) {
    background: transparent;
    margin: 4px;
  }

  :global(*::-webkit-scrollbar-thumb) {
    border-radius: 999px;
    background: var(--scroll-thumb, rgba(15, 23, 42, 0.16));
    border: 2px solid transparent;
    background-clip: padding-box;
    box-shadow: inset 0 0 0 1px var(--scroll-border, rgba(255, 255, 255, 0.78));
  }

  :global(*::-webkit-scrollbar-thumb:hover) {
    background: color-mix(in srgb, var(--accent, #684cff) 44%, var(--scroll-thumb-hover, rgba(15, 23, 42, 0.3)));
    background-clip: padding-box;
  }

  :global(*::-webkit-scrollbar-corner) {
    background: transparent;
  }

  :global(button) {
    font-family: inherit;
  }

  .app {
    display: flex;
    flex-direction: column;
    height: 100vh;
    background: transparent;
  }

  .chrome-shell {
    position: relative;
    z-index: 40;
    flex: 0 0 auto;
    padding: 5px 10px 6px;
    background: linear-gradient(
      180deg,
      color-mix(in srgb, var(--surface) 44%, transparent) 0%,
      color-mix(in srgb, var(--surface) 24%, transparent) 78%,
      transparent 100%
    );
    text-shadow: var(--text-halo, none);
    transition: padding 0.42s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .chrome {
    position: relative;
    overflow: visible;
    border-radius: 20px;
    background: transparent;
    border: 0;
    box-shadow: none;
    -webkit-user-select: none;
    user-select: none;
    padding: 0;
  }

  .chrome.compact {
    padding: 0;
  }

  .tabbar-collapse-wrap {
    height: calc((1 - var(--cp, 0)) * var(--tabbar-full-h, 40px));
    overflow: hidden;
    will-change: height;
  }

  .tabbar-collapse-wrap.collapsed {
    pointer-events: none;
  }

  .tabbar-collapse-inner {
    min-height: 0;
    overflow: hidden;
    opacity: calc(1 - var(--cp, 0) * 1.15);
    transform: translate3d(0, calc(var(--cp, 0) * -10px), 0) scale(calc(1 - var(--cp, 0) * 0.025));
    transform-origin: top center;
    will-change: opacity, transform;
  }

  .suggestions-backdrop {
    position: fixed;
    z-index: 25;
    inset: 0;
    background: transparent;
  }

  @keyframes liquidPopIn {
    0% {
      opacity: 0;
      transform: translate3d(0, -12px, 0) scale(0.9);
    }
    68% {
      opacity: 1;
      transform: translate3d(0, 2px, 0) scale(1.02);
    }
    100% {
      opacity: 1;
      transform: translate3d(0, 0, 0) scale(1);
    }
  }

  @keyframes modalSpringIn {
    0% {
      opacity: 0;
      transform: translate3d(-50%, -14px, 0) scale(0.9);
    }
    70% {
      opacity: 1;
      transform: translate3d(-50%, 2px, 0) scale(1.015);
    }
    100% {
      opacity: 1;
      transform: translate3d(-50%, 0, 0) scale(1);
    }
  }

  @keyframes barSlideDown {
    0% {
      opacity: 0;
      transform: translate3d(0, -10px, 0) scale(0.94);
    }
    72% {
      opacity: 1;
      transform: translate3d(0, 1px, 0) scale(1.008);
    }
    100% {
      opacity: 1;
      transform: translate3d(0, 0, 0) scale(1);
    }
  }

  @keyframes pageFadeIn {
    0% {
      opacity: 0;
      transform: translate3d(0, 12px, 0) scale(0.985);
    }
    100% {
      opacity: 1;
      transform: translate3d(0, 0, 0) scale(1);
    }
  }

  @keyframes fadeBackdrop {
    0% { opacity: 0; }
    100% { opacity: 1; }
  }

  .chrome::before {
    display: none;
  }

  .overlay-root {
    position: fixed;
    z-index: 120;
    inset: 0;
    overflow: hidden;
    color: var(--text);
    background: transparent;
    font-family: "Pretendard", system-ui, -apple-system, "Segoe UI", "Malgun Gothic", sans-serif;
  }

  .overlay-menu-dismiss {
    position: fixed;
    z-index: 1;
    inset: 0;
    background: transparent;
    animation: fadeBackdrop 0.24s ease;
  }

  .overlay-menu {
    position: absolute;
    top: 48px;
    right: 12px;
    display: grid;
    width: min(268px, calc(100vw - 24px));
    gap: 3px;
    padding: 8px;
    border: 1px solid var(--glass-border-strong);
    border-radius: 18px;
    background: var(--glass-island);
    background-size: 120px 120px, 100% 100%;
    box-shadow: var(--glass-shadow);
    -webkit-backdrop-filter: var(--glass-backdrop);
    backdrop-filter: var(--glass-backdrop);
    transform-origin: top center;
    animation: liquidPopIn 0.3s cubic-bezier(0.2, 1.24, 0.32, 1);
    transition:
      left 0.24s cubic-bezier(0.2, 1.2, 0.32, 1),
      top 0.24s cubic-bezier(0.2, 1.2, 0.32, 1);
  }

  .overlay-menu button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    min-height: 34px;
    padding: 0 12px;
    border: 1px solid transparent;
    border-radius: 12px;
    background: transparent;
    color: var(--text);
    text-align: left;
    font: inherit;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
    transition:
      background 0.16s cubic-bezier(0.22, 1, 0.36, 1),
      border-color 0.16s cubic-bezier(0.22, 1, 0.36, 1),
      transform 0.16s cubic-bezier(0.22, 1, 0.36, 1);
  }

  .overlay-menu button:hover {
    background: var(--glass-pill-active);
    border-color: var(--glass-border);
    transform: translateX(2px);
  }

  .overlay-menu button:active {
    transform: scale(0.98);
  }

  .overlay-menu kbd {
    display: inline-flex;
    align-items: center;
    padding: 2px 7px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--glass-pill);
    font-family: ui-monospace, "Cascadia Mono", "Consolas", monospace;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text);
    opacity: 0.9;
    flex: 0 0 auto;
  }

  .menu-zoom-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 3px 6px 3px 12px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .menu-zoom-controls {
    display: inline-flex;
    align-items: center;
    gap: 3px;
  }

  .menu-zoom-controls button {
    min-height: 26px;
    padding: 0 8px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: var(--glass-pill);
    font-size: 11.5px;
    justify-content: center;
  }

  .menu-zoom-controls button.zoom-pct {
    min-width: 48px;
    font-weight: 600;
  }

  .overlay-glass-modal {
    position: fixed;
    top: 64px;
    left: 50%;
    transform: translateX(-50%);
    width: min(560px, calc(100vw - 32px));
    max-height: min(520px, calc(100vh - 96px));
    display: flex;
    flex-direction: column;
    gap: 10px;
    padding: 16px;
    border: 1px solid var(--glass-border-strong);
    border-radius: 20px;
    background: var(--glass-island);
    background-size: 120px 120px, 100% 100%;
    box-shadow: var(--glass-shadow);
    -webkit-backdrop-filter: var(--glass-backdrop);
    backdrop-filter: var(--glass-backdrop);
    color: var(--text);
    animation: modalSpringIn 0.34s cubic-bezier(0.2, 1.2, 0.32, 1);
  }

  .gm-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 13.5px;
  }

  .gm-search {
    width: 100%;
    height: 36px;
    padding: 0 14px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: var(--glass-pill-active);
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
    outline: none;
  }

  .gm-search:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  .gm-list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    overflow-y: auto;
    padding-right: 2px;
  }

  .gm-item {
    display: flex;
    align-items: center;
    gap: 8px;
    padding: 7px 10px;
    border: 1px solid var(--glass-border);
    border-radius: 14px;
    background: var(--glass-pill);
  }

  .gm-item.active {
    background: var(--glass-pill-active);
    border-color: var(--accent);
  }

  .gm-item-main {
    flex: 1;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 2px;
    border: 0;
    background: transparent;
    color: var(--text);
    text-align: left;
    padding: 0;
    cursor: pointer;
  }

  .gm-item-main.static {
    cursor: default;
  }

  .gm-item-title {
    font-size: 12.5px;
    font-weight: 600;
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .gm-item-sub {
    font-size: 11px;
    color: var(--text-soft);
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .gm-item-close {
    width: 26px;
    height: 26px;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-muted);
    font-size: 15px;
    cursor: pointer;
  }

  .gm-item-close:hover {
    background: var(--surface-strong);
    color: var(--text);
  }

  .gm-item-actions {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 0 0 auto;
  }

  .gm-item-actions button {
    height: 26px;
    padding: 0 9px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: var(--glass-pill-active);
    color: var(--text);
    font-size: 11px;
    cursor: pointer;
  }

  .gm-item-actions button.danger {
    color: var(--err);
  }

  .gm-empty {
    margin: 14px 0;
    text-align: center;
    font-size: 12px;
    color: var(--text-soft);
  }

  .overlay-notice {
    position: fixed;
    z-index: 80;
    top: 12px;
    left: 50%;
    display: flex;
    align-items: center;
    gap: 12px;
    max-width: min(560px, calc(100vw - 32px));
    padding: 9px 14px;
    border: 1px solid var(--glass-border-strong);
    border-radius: 999px;
    background: var(--glass-island);
    color: var(--text-muted);
    box-shadow: var(--glass-bevel);
    font-size: 12px;
    transform: translateX(-50%);
    -webkit-backdrop-filter: var(--glass-backdrop);
    backdrop-filter: var(--glass-backdrop);
  }

  .overlay-notice.error { color: var(--err); }
  .overlay-notice.success { color: var(--ok); }
  .overlay-notice button { border: 0; background: transparent; color: inherit; font-size: 16px; cursor: pointer; }

  .toolbar {
    position: relative;
    z-index: 2;
    contain: layout style;
    display: flex;
    align-items: center;
    gap: 7px;
    width: 100%;
    max-width: 100%;
    margin: 0 auto;
    height: 38px;
    padding: 0 calc(6px + var(--cp, 0) * 114px) 0 6px;
    background: transparent;
  }

  .address-tab-chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 0;
    height: 24px;
    padding: 0;
    margin-right: 0;
    border: 1px solid transparent;
    border-radius: 999px;
    background: var(--glass-pill);
    color: var(--text);
    font: inherit;
    font-size: 11.5px;
    font-weight: 600;
    opacity: 0;
    overflow: hidden;
    white-space: nowrap;
    pointer-events: none;
    cursor: pointer;
    flex: 0 0 auto;
    transition:
      max-width 0.22s cubic-bezier(0.22, 1, 0.36, 1),
      padding 0.22s cubic-bezier(0.22, 1, 0.36, 1),
      margin-right 0.22s cubic-bezier(0.22, 1, 0.36, 1),
      opacity 0.18s ease,
      border-color 0.18s ease;
  }

  .address-tab-chip.visible {
    max-width: 172px;
    padding: 0 9px;
    margin-right: 3px;
    border-color: var(--glass-border);
    opacity: 1;
    pointer-events: auto;
  }

  .address-tab-chip .chip-dot {
    width: 6px;
    height: 6px;
    border-radius: 50%;
    background: linear-gradient(135deg, var(--accent), color-mix(in srgb, var(--accent) 60%, #3ecfcf));
    flex: 0 0 auto;
  }

  .address-tab-chip .chip-title {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .address-tab-chip .chip-count {
    padding: 1px 5px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 10px;
    font-weight: 700;
  }

  .nav::before,
  .address::before,
  .action::before,
  .overlay-menu::before,
  .overlay-translate::before,
  .overlay-glass-modal::before {
    content: "";
    position: absolute;
    inset: 0;
    padding: 1px;
    border-radius: inherit;
    background: var(--glass-rim-grad);
    -webkit-mask:
      linear-gradient(#fff 0 0) content-box,
      linear-gradient(#fff 0 0);
    mask:
      linear-gradient(#fff 0 0) content-box,
      linear-gradient(#fff 0 0);
    -webkit-mask-composite: xor;
    mask-composite: exclude;
    pointer-events: none;
    opacity: 0.54;
  }

  .nav,
  .action {
    position: relative;
  }

  .nav-buttons {
    display: flex;
    gap: 4px;
    flex: 0 0 auto;
  }

  .nav {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    padding: 0;
    border: 1px solid var(--glass-border-strong);
    border-radius: 999px;
    background: var(--glass-pill);
    -webkit-backdrop-filter: var(--glass-backdrop);
    backdrop-filter: var(--glass-backdrop);
    box-shadow: var(--glass-bevel);
    color: var(--text-muted);
    cursor: pointer;
    transition:
      background 0.2s cubic-bezier(0.2, 1.24, 0.32, 1),
      color 0.2s ease,
      transform 0.24s cubic-bezier(0.2, 1.28, 0.36, 1),
      box-shadow 0.22s ease;
  }

  .nav:hover:not(:disabled) {
    background: var(--glass-pill-active);
    color: var(--text);
    transform: translateY(-2px) scale(1.07);
    box-shadow: var(--glass-bevel);
  }

  .nav:active:not(:disabled) {
    transform: translateY(0) scale(0.90);
  }

  .nav:disabled {
    opacity: 0.38;
    cursor: default;
  }

  .nav svg,
  .action.icon svg,
  .scheme svg {
    width: 15px;
    height: 15px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.6;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .address {
    display: flex;
    align-items: center;
    gap: 7px;
    flex: 1;
    min-width: 160px;
    position: relative;
    z-index: 60;
    height: 32px;
    padding: 0 6px 0 12px;
    border: 1px solid var(--glass-border-strong);
    border-radius: 999px;
    background: var(--glass-pill);
    -webkit-backdrop-filter: var(--glass-backdrop);
    backdrop-filter: var(--glass-backdrop);
    box-shadow: var(--glass-bevel);
    transition:
      border-color 0.24s cubic-bezier(0.2, 1.24, 0.32, 1),
      background 0.24s ease,
      box-shadow 0.26s cubic-bezier(0.2, 1.24, 0.32, 1),
      transform 0.26s cubic-bezier(0.2, 1.24, 0.32, 1);
  }

  .address.focused {
    border-color: var(--accent);
    background: var(--glass-pill-active);
    transform: translateY(-0.5px);
    box-shadow:
      0 0 0 2.5px var(--accent-soft),
      var(--glass-bevel);
  }

  .scheme {
    display: grid;
    place-items: center;
    flex: 0 0 auto;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
  }

  .scheme:hover {
    background: var(--surface-strong);
    color: var(--text);
  }

  .scheme.insecure {
    color: #d97706;
  }

  .start-page {
    max-width: 820px;
    height: 100%;
    margin: 0 auto;
    padding: 40px 24px 36px;
    overflow-y: auto;
    display: grid;
    gap: 22px;
    align-content: start;
  }

  .start-hero { display: grid; gap: 4px; }
  .start-logo {
    margin: 0;
    font-size: 40px;
    font-weight: 800;
    background: linear-gradient(135deg, #5b8cff, #7c3aed, #0ea5e9);
    -webkit-background-clip: text;
    background-clip: text;
    -webkit-text-fill-color: transparent;
  }
  .start-sub { margin: 0; color: var(--text-soft); font-size: 13px; }
  .start-search {
    display: flex;
    gap: 8px;
    padding: 6px;
    border-radius: 999px;
    border: 1px solid var(--glass-border-strong);
    background: var(--glass-pill);
  }
  .start-search input {
    flex: 1;
    border: 0;
    background: transparent;
    padding: 8px 14px;
    color: var(--text);
    font-size: 14px;
    outline: none;
  }
  .start-search button {
    border: 0;
    border-radius: 999px;
    padding: 8px 18px;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
    cursor: pointer;
  }
  .start-sec { display: grid; gap: 10px; }
  .start-sec h2 { margin: 0; font-size: 12px; color: var(--text-soft); text-transform: uppercase; letter-spacing: 0.04em; }
  .start-grid {
    display: grid;
    grid-template-columns: repeat(auto-fill, minmax(150px, 1fr));
    gap: 10px;
  }
  .start-card {
    text-align: left;
    padding: 12px 14px;
    border-radius: 14px;
    border: 1px solid var(--glass-border);
    background: var(--glass-pill);
    color: var(--text);
    font-weight: 600;
    cursor: pointer;
    display: grid;
    gap: 4px;
  }
  .start-card:hover { border-color: var(--accent); }
  .start-list { display: grid; gap: 6px; }
  .start-row {
    text-align: left;
    padding: 10px 14px;
    border-radius: 12px;
    border: 1px solid var(--glass-border);
    background: var(--glass-pill);
    color: var(--text);
    cursor: pointer;
    display: flex;
    justify-content: space-between;
    gap: 12px;
  }
  .start-card-title { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 13px; }
  .start-card-sub { overflow: hidden; text-overflow: ellipsis; white-space: nowrap; font-size: 11px; color: var(--text-soft); }

  .address input {
    flex: 1;
    min-width: 0;
    height: 100%;
    padding: 0;
    border: none;
    background: transparent;
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
    outline: none;
  }

  .address input::placeholder {
    color: var(--text-soft);
  }

  .omnibox-btn {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-muted);
    font-size: 12px;
    cursor: pointer;
    flex: 0 0 auto;
    transition: background 0.14s ease, color 0.14s ease;
  }

  .omnibox-btn:hover:not(:disabled) {
    background: var(--surface-strong);
    color: var(--text);
  }

  .omnibox-btn.star.bookmarked {
    color: var(--accent);
  }

  .omnibox-btn.shield.adblock-on {
    color: var(--accent);
    opacity: 0.92;
  }

  .omnibox-btn.shield.adblock-off {
    opacity: 0.38;
    filter: grayscale(1);
  }

  .action.icon.tinted-active {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }

  .window-controls {
    position: fixed;
    top: 8px;
    right: 12px;
    z-index: 95;
    display: inline-flex;
    align-items: center;
    gap: 2px;
    height: 30px;
    padding: 0 3px;
    border: 1px solid var(--glass-border-strong);
    border-radius: 999px;
    background: var(--glass-pill);
    -webkit-backdrop-filter: var(--glass-backdrop);
    backdrop-filter: var(--glass-backdrop);
    box-shadow: var(--glass-bevel);
  }

  .window-controls::before {
    content: "";
    position: absolute;
    inset: 0;
    padding: 1px;
    border-radius: inherit;
    background: var(--glass-rim-grad);
    -webkit-mask:
      linear-gradient(#fff 0 0) content-box,
      linear-gradient(#fff 0 0);
    mask:
      linear-gradient(#fff 0 0) content-box,
      linear-gradient(#fff 0 0);
    -webkit-mask-composite: xor;
    mask-composite: exclude;
    pointer-events: none;
    opacity: 0.54;
  }

  .win-btn {
    display: grid;
    place-items: center;
    width: 24px;
    height: 24px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    transition:
      background 0.16s ease,
      color 0.16s ease,
      transform 0.16s ease;
  }

  .win-btn svg {
    width: 11px;
    height: 11px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.5;
    stroke-linecap: round;
    stroke-linejoin: round;
  }

  .win-btn:hover {
    background: var(--glass-pill-active);
    color: var(--text);
    transform: scale(1.06);
  }

  .win-btn.close:hover {
    background: rgba(239, 68, 68, 0.88);
    color: #ffffff;
  }

  .address-spinner {
    width: 12px;
    height: 12px;
    flex: 0 0 auto;
    border: 2px solid var(--border-strong);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  .address-suggestions {
    position: absolute;
    top: calc(100% + 7px);
    left: 0;
    right: 0;
    z-index: 90;
    display: grid;
    gap: 3px;
    max-height: 320px;
    overflow-y: auto;
    margin: 0;
    padding: 7px;
    list-style: none;
    border: 1px solid var(--glass-border-strong);
    border-radius: 18px;
    background: var(--glass-island);
    background-size: 120px 120px, 100% 100%;
    box-shadow: var(--glass-shadow);
    -webkit-backdrop-filter: var(--glass-backdrop);
    backdrop-filter: var(--glass-backdrop);
    transform-origin: top center;
    animation: liquidPopIn 0.28s cubic-bezier(0.2, 1.24, 0.32, 1);
  }

  .address-suggestions button {
    display: grid;
    grid-template-columns: 18px minmax(0, 1fr);
    grid-template-rows: auto auto;
    align-items: center;
    width: 100%;
    padding: 6px 10px;
    border: 0;
    border-radius: 12px;
    background: transparent;
    color: var(--text);
    font: inherit;
    text-align: left;
    cursor: pointer;
    transition:
      background 0.16s cubic-bezier(0.22, 1, 0.36, 1),
      transform 0.16s cubic-bezier(0.22, 1, 0.36, 1);
  }

  .address-suggestions button:hover,
  .address-suggestions button.active {
    background: var(--glass-pill-active);
    transform: translateX(3px);
  }

  .suggestion-kind {
    grid-row: 1 / span 2;
    color: var(--accent);
    font-size: 11px;
  }

  .suggestion-title,
  .suggestion-url {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }

  .suggestion-title {
    font-size: 12.5px;
  }

  .suggestion-url {
    color: var(--text-soft);
    font-size: 11px;
  }

  .zoom-indicator {
    min-width: 42px;
    font-size: 11px;
    color: var(--accent);
  }

  .actions {
    display: flex;
    align-items: center;
    gap: 5px;
    flex: 0 0 auto;
  }

  .action {
    display: inline-flex;
    align-items: center;
    justify-content: center;
    gap: 5px;
    height: 30px;
    padding: 0 12px;
    border: 1px solid var(--glass-border-strong);
    border-radius: 999px;
    background: var(--glass-pill);
    -webkit-backdrop-filter: var(--glass-backdrop);
    backdrop-filter: var(--glass-backdrop);
    box-shadow: var(--glass-bevel);
    color: var(--text-muted);
    font-size: 12px;
    font-weight: 600;
    white-space: nowrap;
    cursor: pointer;
    transition:
      transform 0.24s cubic-bezier(0.2, 1.28, 0.36, 1),
      background 0.2s ease,
      box-shadow 0.22s ease,
      color 0.2s ease;
  }

  .action:hover:not(:disabled) {
    background: var(--glass-pill-active);
    color: var(--text);
    transform: translateY(-2px) scale(1.05);
    box-shadow: var(--glass-bevel);
  }

  .action:active:not(:disabled) {
    transform: translateY(0) scale(0.92);
  }

  .action.primary-liquid {
    border-color: rgba(255, 255, 255, 0.42);
    background: var(--primary-grad);
    -webkit-backdrop-filter: var(--glass-backdrop);
    backdrop-filter: var(--glass-backdrop);
    box-shadow: var(--primary-bevel);
    color: #ffffff;
  }

  .action.primary-liquid:hover:not(:disabled) {
    background: var(--primary-grad);
    filter: brightness(1.08);
    color: #ffffff;
    box-shadow: var(--primary-bevel);
  }

  .btn-spinner {
    width: 12px;
    height: 12px;
    flex: 0 0 auto;
    border: 2px solid rgba(255, 255, 255, 0.38);
    border-top-color: #ffffff;
    border-radius: 50%;
    animation: spin 0.75s linear infinite;
  }

  .action.secondary-liquid {
    border-color: rgba(255, 255, 255, 0.5);
    background: var(--secondary-grad);
    -webkit-backdrop-filter: var(--glass-backdrop);
    backdrop-filter: var(--glass-backdrop);
    box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.6);
    color: #ffffff;
  }

  .action.icon {
    width: 30px;
    padding: 0;
  }

  .action:disabled {
    opacity: 0.45;
    cursor: default;
  }

  .status {
    position: relative;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 10px;
    margin: 4px 6px 2px;
    padding: 6px 12px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: var(--glass-pill-active);
    color: var(--text-muted);
    font-size: 12px;
    line-height: 1.4;
    animation: barSlideDown 0.22s cubic-bezier(0.22, 1, 0.36, 1);
  }

  .status.error {
    background: var(--err-soft);
    color: var(--err);
  }

  .status.success {
    background: var(--ok-soft);
    color: var(--ok);
  }

  .status-text {
    flex: 1;
    min-width: 0;
    overflow-wrap: anywhere;
  }

  .status-close {
    flex: 0 0 auto;
    width: 22px;
    height: 22px;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: inherit;
    font-size: 15px;
    line-height: 1;
    cursor: pointer;
    opacity: 0.7;
  }

  .status-close:hover {
    opacity: 1;
    background: rgba(0, 0, 0, 0.06);
  }

  .content {
    position: relative;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    background: transparent;
  }

  .browser-placeholder {
    height: 100%;
    background: transparent;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .find-bar {
    position: relative;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 6px;
    padding: 4px 8px 3px;
    animation: barSlideDown 0.22s cubic-bezier(0.22, 1, 0.36, 1);
  }

  .find-bar input {
    flex: 1;
    max-width: 320px;
    height: 28px;
    padding: 0 12px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: var(--glass-pill-active);
    color: var(--text);
    font: inherit;
    font-size: 12px;
    outline: none;
  }

  .find-count {
    min-width: 56px;
    color: var(--text-muted);
    font-size: 11px;
    text-align: center;
  }

  .internal-page-root {
    width: 100%;
    height: 100vh;
    background: var(--glass-shell-bg);
    color: var(--text);
    overflow: hidden;
    animation: pageFadeIn 0.36s cubic-bezier(0.16, 1, 0.3, 1);
  }

  .prompt-banner {
    position: relative;
    z-index: 2;
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    max-width: 900px;
    margin: 5px auto 2px;
    padding: 5px 10px 5px 14px;
    border: 1px solid var(--glass-border-strong);
    border-radius: 999px;
    background: var(--glass-pill-active);
    -webkit-backdrop-filter: var(--glass-backdrop);
    backdrop-filter: var(--glass-backdrop);
    box-shadow: var(--glass-bevel);
    color: var(--text);
    font-size: 12px;
    animation: barSlideDown 0.2s cubic-bezier(0.22, 1, 0.36, 1);
  }
  .prompt-text {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .prompt-actions {
    display: flex;
    align-items: center;
    gap: 6px;
    flex: 0 0 auto;
  }
  .prompt-btn {
    height: 25px;
    padding: 0 11px;
    border: 1px solid var(--glass-border-strong);
    border-radius: 999px;
    background: var(--glass-pill-active);
    color: var(--text);
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
  }
  .prompt-btn.primary {
    background: var(--accent);
    border-color: transparent;
    color: #fff;
  }
  .prompt-close {
    width: 24px;
    height: 24px;
    border: 0;
    border-radius: 50%;
    background: transparent;
    color: var(--text-muted);
    font-size: 15px;
    cursor: pointer;
  }

  .overlay-menu-dismiss.left {
    justify-content: flex-start;
    padding: 38px 0 0 8px;
  }

  .overlay-workspace .active-item {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }

  .overlay-chrome-menu {
    width: 262px;
  }

  .overlay-page-menu {
    width: 274px;
  }

  .page-menu-nav-row {
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 5px;
    padding: 2px 2px 3px;
  }

  .overlay-menu button.pm-nav-btn {
    justify-content: center;
    min-height: 32px;
    padding: 0;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: var(--glass-pill);
    font-size: 14px;
    font-weight: 600;
  }

  .overlay-menu button.pm-nav-btn:hover:not(:disabled) {
    background: var(--glass-pill-active);
    transform: translateY(-1px);
  }

  .overlay-menu button.pm-nav-btn:disabled {
    opacity: 0.35;
    cursor: default;
  }

  .overlay-chrome-menu button {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
  }

  .overlay-chrome-menu kbd {
    display: inline-flex;
    align-items: center;
    padding: 2px 7px;
    border: 1px solid var(--border-strong);
    border-radius: 6px;
    background: var(--glass-pill);
    font-family: ui-monospace, "Cascadia Mono", "Consolas", monospace;
    font-size: 10.5px;
    font-weight: 600;
    letter-spacing: 0.02em;
    color: var(--text);
    opacity: 0.9;
    flex: 0 0 auto;
  }

  .menu-divider {
    height: 1px;
    margin: 4px 4px;
    background: var(--border);
  }

  .overlay-translate {
    width: 292px;
    padding: 15px;
    border: 1px solid var(--glass-border-strong);
    border-radius: 20px;
    background: var(--glass-island);
    background-size: 120px 120px, 100% 100%;
    color: var(--text);
    box-shadow: var(--glass-shadow);
    -webkit-backdrop-filter: var(--glass-backdrop);
    backdrop-filter: var(--glass-backdrop);
    display: flex;
    flex-direction: column;
    gap: 10px;
    transform-origin: top right;
    animation: liquidPopIn 0.3s cubic-bezier(0.2, 1.24, 0.32, 1);
  }

  .tp-header {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 13.5px;
  }

  .tp-close {
    width: 26px;
    height: 26px;
    border: 1px solid var(--glass-border);
    border-radius: 50%;
    background: var(--glass-pill);
    color: var(--text-muted);
    font-size: 15px;
    cursor: pointer;
  }

  .tp-field {
    display: grid;
    gap: 5px;
    font-size: 11px;
    color: var(--text-muted);
  }
  .tp-style-hint {
    font-size: 10.5px;
    color: var(--accent);
    opacity: 0.92;
    padding-left: 4px;
  }

  .tp-row {
    display: flex;
    gap: 6px;
  }

  .tp-field select,
  .tp-field input {
    width: 100%;
    min-width: 0;
    height: 32px;
    padding: 0 11px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: var(--glass-pill-active);
    color: var(--text);
    font: inherit;
    font-size: 12px;
    outline: none;
  }

  .tp-row button,
  .tp-actions button {
    height: 32px;
    padding: 0 12px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: var(--glass-pill-active);
    color: var(--text);
    font: inherit;
    font-size: 12px;
    font-weight: 500;
    cursor: pointer;
  }

  .tp-actions {
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 6px;
  }

  .tp-check {
    display: flex;
    align-items: center;
    gap: 7px;
    font-size: 12px;
    color: var(--text);
    cursor: pointer;
  }

  .tp-actions button.primary {
    grid-column: 1 / -1;
    background: var(--primary-grad);
    box-shadow: var(--primary-glow);
    color: #fff;
    border-color: rgba(255, 255, 255, 0.45);
    font-weight: 600;
  }

  .tp-actions button.tp-report-btn {
    grid-column: 1 / -1;
    height: 28px;
    font-size: 11.5px;
    opacity: 0.9;
  }

  .tp-setup {
    display: grid;
    gap: 6px;
    padding: 9px 10px;
    border-radius: 13px;
    border: 1px solid var(--glass-border-strong);
    background: var(--glass-pill);
  }

  .tp-setup-title {
    font-size: 11.5px;
    font-weight: 600;
    color: var(--text);
  }

  .tp-setup-sub {
    font-size: 11px;
    color: var(--text-muted);
  }

  .tp-setup-btn {
    height: 30px;
    padding: 0 11px;
    border: 1px solid rgba(255, 255, 255, 0.45);
    border-radius: 999px;
    background: var(--primary-grad);
    color: #fff;
    font: inherit;
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
  }

  .tp-setup-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    font-size: 11px;
    color: var(--text-muted);
  }

  .tp-setup-row button {
    height: 24px;
    padding: 0 9px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: var(--glass-pill-active);
    color: var(--text);
    font: inherit;
    font-size: 11px;
    cursor: pointer;
  }

  .tp-prog-bar {
    width: 100%;
    height: 6px;
    border-radius: 999px;
    background: rgba(120, 130, 160, 0.25);
    overflow: hidden;
  }

  .tp-prog-fill {
    height: 100%;
    border-radius: 999px;
    background: var(--primary-grad);
    transition: width 0.2s ease;
  }

  :global(.panel) {
    position: relative;
    border-radius: 24px !important;
    border: 1px solid var(--glass-border) !important;
    background: var(--glass-island) !important;
    box-shadow: var(--glass-shadow) !important;
    -webkit-backdrop-filter: var(--glass-backdrop);
    backdrop-filter: var(--glass-backdrop);
  }

  :global(.panel.embedded) {
    width: min(1060px, calc(100vw - 28px)) !important;
    margin: 94px auto 14px !important;
    height: calc(100vh - 108px) !important;
    border-radius: 24px !important;
    border: 1px solid var(--glass-border) !important;
    box-shadow: var(--glass-shadow) !important;
  }

  :global(.backdrop.embedded) {
    background: transparent !important;
  }

  :global(.panel input:not([type="checkbox"]):not([type="color"])),
  :global(.panel select) {
    border-radius: 999px !important;
    border-color: var(--glass-border) !important;
    background: var(--glass-pill-active) !important;
    color: var(--text) !important;
    padding-left: 12px !important;
    padding-right: 12px !important;
  }

  :global(.panel button:not(.tab-btn):not(.close):not(.color-swatch)) {
    border-radius: 999px !important;
    border-color: var(--glass-border) !important;
    background: var(--glass-pill) !important;
    color: var(--text) !important;
    transition: transform 0.16s ease, background 0.16s ease !important;
  }

  :global(.panel button:not(.tab-btn):not(.close):not(.color-swatch):hover:not(:disabled)) {
    background: var(--glass-pill-active) !important;
    transform: translateY(-1px);
  }

  :global(.panel button.primary) {
    background: var(--primary-grad) !important;
    box-shadow: var(--primary-glow) !important;
    border-color: rgba(255, 255, 255, 0.45) !important;
    color: #ffffff !important;
  }

  :global(.panel .settings-nav button.active),
  :global(.panel .folder-item.active) {
    background: var(--accent-soft) !important;
    border-color: var(--accent) !important;
    color: var(--accent) !important;
  }

  :global(.panel .panel-foot),
  :global(.panel .toolbar),
  :global(.panel .editor),
  :global(.panel .list li) {
    background: var(--glass-pill) !important;
    border-color: var(--glass-border) !important;
  }

  .menu-lang-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    padding: 4px 8px 4px 12px;
    font-size: 11.5px;
    color: var(--text-muted);
  }
  .menu-lang-pills {
    display: inline-flex;
    gap: 3px;
  }
  :global(.overlay-menu) .menu-lang-pill {
    width: auto;
    padding: 3px 6px;
    border-radius: 999px;
    border: 1px solid var(--glass-border);
    background: var(--glass-pill);
    color: var(--text);
    font-size: 10.5px;
    font-weight: 600;
    justify-content: center;
  }
  :global(.overlay-menu) .menu-lang-pill.selected {
    background: var(--accent-soft);
    border-color: var(--accent);
    color: var(--accent);
  }
  .ws-count {
    font-size: 10.5px;
    padding: 1px 6px;
    border-radius: 999px;
    background: var(--glass-pill);
    color: var(--text-muted);
  }
  .ws-inline-form,
  .ws-confirm-box {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 6px 8px;
  }
  .ws-inline-form input {
    width: 100%;
    height: 30px;
    padding: 0 10px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: var(--glass-pill-active);
    color: var(--text);
    font: inherit;
    font-size: 12px;
    outline: none;
  }
  .ws-inline-form input:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 2px var(--accent-soft);
  }
  .ws-confirm-box p {
    margin: 0;
    font-size: 11.5px;
    line-height: 1.4;
    color: var(--text-muted);
  }
  .ws-form-actions {
    display: flex;
    gap: 6px;
    justify-content: flex-end;
  }
  :global(.overlay-menu) .ws-form-actions button {
    width: auto;
    padding: 4px 10px;
    font-size: 11.5px;
    border: 1px solid var(--glass-border);
  }
  :global(.overlay-menu) .ws-submit {
    background: var(--primary-grad);
    color: #fff;
    border-color: transparent;
  }
  :global(.overlay-menu) .ws-danger-btn {
    background: color-mix(in srgb, #ef4444 18%, transparent);
    color: #ef4444;
    border-color: color-mix(in srgb, #ef4444 50%, var(--glass-border));
  }
  .action-modal {
    max-width: 420px;
  }
  .action-modal-form {
    display: flex;
    flex-direction: column;
    gap: 10px;
  }
  .action-modal-desc {
    margin: 0;
    font-size: 12.5px;
    line-height: 1.45;
    color: var(--text-muted);
  }
  .action-modal-foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    flex-wrap: wrap;
    margin-top: 4px;
  }
  .action-btn {
    height: 32px;
    padding: 0 14px;
    border: 1px solid var(--glass-border);
    border-radius: 999px;
    background: var(--glass-pill);
    color: var(--text);
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }
  .action-btn.primary {
    background: var(--primary-grad);
    border-color: transparent;
    color: #fff;
  }
</style>
