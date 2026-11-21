<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { emit, listen } from "@tauri-apps/api/event";

  import { api, errorMessage } from "$lib/api";
  import { getUiLang, setUiLang, t, UI_LANGUAGES } from "$lib/i18n.svelte";
  import type {
    AppSettings,
    AutofillProfile,
    CloudAccountRoot,
    ConnectionTest,
    CookieEntry,
    DetectedBrowserProfile,
    HardwareProfile,
    HyMt2ModelState,
    ModelDownloadProgress,
    ModelInfo,
    SavedLogin,
    SettingsView,
    SyncConfig,
    UiLanguage,
    UserScriptItem,
    WebExtensionInfo,
  } from "$lib/types";

  let {
    embedded = false,
    view,
    onclose,
    onsaved,
    onnotify,
  }: {
    embedded?: boolean;
    view: SettingsView;
    onclose: () => void;
    onsaved: (view: SettingsView) => void;
    onnotify: (kind: "info" | "success" | "error", text: string) => void;
  } = $props();

  let draft = $state<AppSettings>(untrack(() => ({ ...view.settings })));
  untrack(() => {
    setUiLang(view.settings.uiLanguage);
  });
  let apiKey = $state("");
  let bridgeToken = $state("");
  let hasApiKey = $state(untrack(() => view.hasApiKey));
  let hasBridgeToken = $state(untrack(() => view.hasBridgeToken));
  let hasCodexAuth = $state(untrack(() => view.hasCodexAuth));
  let codexAccountId = $state(untrack(() => view.codexAccountId ?? null));
  let codexEmail = $state(untrack(() => view.codexEmail ?? null));
  let codexBusy = $state(false);
  let models = $state<ModelInfo[]>([]);
  let loadingModels = $state(false);
  let hyHardware = $state<HardwareProfile | null>(null);
  let hyModels = $state<HyMt2ModelState[]>([]);
  let hyLoading = $state(false);
  let hyProgress = $state<Record<string, ModelDownloadProgress>>({});
  let testing = $state(false);
  let saving = $state(false);
  let testResult = $state<ConnectionTest | null>(null);
  let modelRequestId = 0;
  type SettingsSection =
    | "ai"
    | "profile"
    | "browser"
    | "privacy"
    | "performance"
    | "translation"
    | "appearance"
    | "data";
  let activeSection = $state<SettingsSection>("ai");
  type SubpageKind =
    | "adblock-exceptions"
    | "cookies"
    | "passwords"
    | "sleep-exceptions"
    | "extensions";
  type NavEntry = { section: SettingsSection; subpage: SubpageKind | null };
  let activeSubpage = $state<SubpageKind | null>(null);
  let navStack = $state<NavEntry[]>([{ section: "ai", subpage: null }]);
  let navIndex = $state(0);
  let lastNavAt = 0;
  let importingData = $state(false);
  let exportingData = $state(false);
  let dataFileInput = $state<HTMLInputElement | null>(null);
  let passwordCsvInput = $state<HTMLInputElement | null>(null);

  // P2 Browser/Profile/Privacy/Performance state
  let syncConfig = $state<SyncConfig | null>(null);
  let cloudAccounts = $state<CloudAccountRoot[]>([]);
  let authOpeningProvider = $state<"google" | "microsoft" | null>(null);
  let syncBusy = $state(false);
  let manualSyncProvider = $state<"google" | "microsoft" | "local">("local");
  let manualSyncIdentifier = $state("");
  let manualSyncDisplayName = $state("");
  let manualSyncDir = $state("");
  let confirmClearSiteScope = $state<null | "cookies" | "cache" | "all">(null);
  let confirmDeleteModelId = $state<string | null>(null);
  let newAdblockHost = $state("");
  let newPopupOrigin = $state("");
  let newSleepHost = $state("");
  let cookieUri = $state("");
  let cookies = $state<CookieEntry[]>([]);
  let cookiesBusy = $state(false);
  let savedLogins = $state<SavedLogin[]>([]);
  let revealedLoginIds = $state<Record<string, boolean>>({});
  let newLoginOrigin = $state("");
  let newLoginUser = $state("");
  let newLoginPass = $state("");
  let autofillProfile = $state<AutofillProfile>({
    name: "",
    email: "",
    phone: "",
    address: "",
    postalCode: "",
  });
  let externalBrowsers = $state<DetectedBrowserProfile[]>([]);
  let importBusy = $state(false);
  let webExtensions = $state<WebExtensionInfo[]>([]);
  let extensionFolderInput = $state("");
  let extensionBusy = $state(false);
  let userScripts = $state<UserScriptItem[]>([]);
  let newScriptName = $state("");
  let newScriptMatch = $state("*");
  let newScriptCode = $state("");
  let newScriptCss = $state("");

  function pushNav(sec: SettingsSection, sub: SubpageKind | null) {
    const cur = navStack[navIndex];
    if (cur && cur.section === sec && cur.subpage === sub) {
      activeSection = sec;
      activeSubpage = sub;
      return;
    }
    navStack = [...navStack.slice(0, navIndex + 1), { section: sec, subpage: sub }];
    navIndex = navStack.length - 1;
    activeSection = sec;
    activeSubpage = sub;
  }

  function selectSection(sec: SettingsSection) {
    pushNav(sec, null);
  }

  function openSubpage(sub: SubpageKind) {
    pushNav(activeSection, sub);
  }

  function closeSubpage() {
    if (
      navIndex > 0 &&
      navStack[navIndex - 1]?.section === activeSection &&
      navStack[navIndex - 1]?.subpage === null
    ) {
      navIndex -= 1;
      activeSection = navStack[navIndex].section;
      activeSubpage = navStack[navIndex].subpage;
    } else {
      pushNav(activeSection, null);
    }
  }

  function handleSettingsNav(direction: "back" | "forward") {
    const now = performance.now();
    if (now - lastNavAt < 80) return;
    lastNavAt = now;
    if (direction === "back") {
      if (navIndex > 0) {
        navIndex -= 1;
        activeSection = navStack[navIndex].section;
        activeSubpage = navStack[navIndex].subpage;
      } else {
        void api.goBack().catch(() => {});
      }
    } else if (navIndex + 1 < navStack.length) {
      navIndex += 1;
      activeSection = navStack[navIndex].section;
      activeSubpage = navStack[navIndex].subpage;
    } else {
      void api.goForward().catch(() => {});
    }
  }

  onMount(() => {
    let unlistenSync: (() => void) | undefined;
    let unlistenSyncErr: (() => void) | undefined;
    let unlistenCodexOk: (() => void) | undefined;
    let unlistenCodexErr: (() => void) | undefined;
    let unlistenSettingsUpdated: (() => void) | undefined;
    void listen<SettingsView>("settings-updated", (event) => {
      if (event.payload?.settings) {
        draft.style = event.payload.settings.style;
        draft.targetLanguage = event.payload.settings.targetLanguage;
        if (event.payload.settings.uiLanguage) {
          draft.uiLanguage = event.payload.settings.uiLanguage;
          setUiLang(event.payload.settings.uiLanguage);
        }
      }
    }).then((dispose) => {
      unlistenSettingsUpdated = dispose;
    });
    void listen<SyncConfig>("sync-config-updated", (event) => {
      syncConfig = event.payload;
      authOpeningProvider = null;
      syncBusy = false;
      void api.detectCloudAccounts().then((accs) => (cloudAccounts = accs)).catch(() => {});
      const provider =
        event.payload.provider === "google"
          ? "Google"
          : event.payload.provider === "microsoft"
            ? "Microsoft"
            : "Local";
      onnotify("success", t("set.syncConnectedToast", { provider, email: event.payload.email }));
    }).then((dispose) => {
      unlistenSync = dispose;
    });
    void listen<string>("sync-config-error", (event) => {
      authOpeningProvider = null;
      syncBusy = false;
      onnotify("error", event.payload);
    }).then((dispose) => {
      unlistenSyncErr = dispose;
    });
    void listen<SettingsView>("codex-auth-updated", (event) => {
      codexBusy = false;
      hasCodexAuth = event.payload.hasCodexAuth;
      codexAccountId = event.payload.codexAccountId ?? null;
      codexEmail = event.payload.codexEmail ?? null;
      onsaved(event.payload);
      onnotify(
        "success",
        `레거시 코덱스 연결 완료${codexEmail ? ` (${codexEmail})` : ""}`,
      );
      if (draft.provider === "codexLegacy") {
        void loadModels();
      }
    }).then((dispose) => {
      unlistenCodexOk = dispose;
    });
    void listen<string>("codex-auth-error", (event) => {
      codexBusy = false;
      onnotify("error", `레거시 코덱스 로그인 실패: ${event.payload}`);
    }).then((dispose) => {
      unlistenCodexErr = dispose;
    });

    const w = window as Window & {
      __tbSettingsNav?: (dir: "back" | "forward") => void;
    };
    w.__tbSettingsNav = (dir) => {
      handleSettingsNav(dir);
    };

    const suppressXButton = (e: MouseEvent | PointerEvent) => {
      if (e.button === 3 || e.button === 4) {
        e.preventDefault();
        e.stopPropagation();
      }
    };
    const onMouseUp = (e: MouseEvent) => {
      if (e.button === 3) {
        e.preventDefault();
        e.stopPropagation();
        handleSettingsNav("back");
      } else if (e.button === 4) {
        e.preventDefault();
        e.stopPropagation();
        handleSettingsNav("forward");
      }
    };
    const onKeyDown = (e: KeyboardEvent) => {
      if (e.altKey && e.key === "ArrowLeft") {
        e.preventDefault();
        handleSettingsNav("back");
      } else if (e.altKey && e.key === "ArrowRight") {
        e.preventDefault();
        handleSettingsNav("forward");
      }
    };

    window.addEventListener("pointerdown", suppressXButton, true);
    window.addEventListener("mousedown", suppressXButton, true);
    window.addEventListener("auxclick", suppressXButton, true);
    window.addEventListener("mouseup", onMouseUp, true);
    window.addEventListener("keydown", onKeyDown, true);

    return () => {
      unlistenSync?.();
      unlistenSyncErr?.();
      unlistenCodexOk?.();
      unlistenCodexErr?.();
      unlistenSettingsUpdated?.();
      if (w.__tbSettingsNav) delete w.__tbSettingsNav;
      window.removeEventListener("pointerdown", suppressXButton, true);
      window.removeEventListener("mousedown", suppressXButton, true);
      window.removeEventListener("auxclick", suppressXButton, true);
      window.removeEventListener("mouseup", onMouseUp, true);
      window.removeEventListener("keydown", onKeyDown, true);
    };
  });

  $effect(() => {
    const sec = activeSection;
    if (sec === "ai") {
      void refreshHyMt2State();
    } else if (sec === "profile") {
      if (!syncConfig) {
        void api
          .loadSyncConfig()
          .then((cfg) => {
            syncConfig = cfg;
          })
          .catch(() => {});
      }
      void api.detectCloudAccounts().then((accs) => (cloudAccounts = accs)).catch(() => {});
    } else if (sec === "privacy") {
      void api.listSavedLogins().then((rows) => (savedLogins = rows)).catch(() => {});
      void api.loadAutofillProfile().then((p) => (autofillProfile = p)).catch(() => {});
      void api.detectExternalBrowsers().then((b) => (externalBrowsers = b)).catch(() => {});
      void loadCookies();
    } else if (sec === "performance") {
      void api.listWebExtensions().then((x) => (webExtensions = x)).catch(() => {});
      void api.listUserScripts().then((s) => (userScripts = s)).catch(() => {});
    }
  });

  async function saveSync(next: SyncConfig) {
    syncConfig = next;
    try {
      syncConfig = await api.saveSyncConfig(next);
    } catch (e) {
      onnotify("error", errorMessage(e));
    }
  }

  async function startOauthAccountLogin(provider: "google" | "microsoft") {
    if (syncBusy) return;
    syncBusy = true;
    authOpeningProvider = provider;
    const providerLabel = provider === "google" ? "Google" : "Microsoft";
    try {
      await api.startOauthLogin(provider);
      onnotify("info", t("set.syncLoginOpenedToast", { provider: providerLabel }));
    } catch (e) {
      onnotify("error", errorMessage(e));
    } finally {
      syncBusy = false;
      authOpeningProvider = null;
    }
  }

  async function connectManualSync() {
    if (syncBusy) return;
    syncBusy = true;
    try {
      const id = manualSyncIdentifier.trim();
      const name = manualSyncDisplayName.trim() || id;
      const customDir = manualSyncDir.trim() || undefined;
      syncConfig = await api.connectSyncAccount(manualSyncProvider, id, name, customDir);
      const providerLabel =
        manualSyncProvider === "google"
          ? "Google"
          : manualSyncProvider === "microsoft"
            ? "Microsoft"
            : "Local";
      onnotify("success", t("set.syncConnectedToast", { provider: providerLabel, email: syncConfig.email }));
    } catch (e) {
      onnotify("error", errorMessage(e));
    } finally {
      syncBusy = false;
    }
  }

  async function disconnectAccountSync() {
    if (syncBusy) return;
    syncBusy = true;
    try {
      syncConfig = await api.disconnectSyncAccount();
      onnotify("info", t("set.syncDisconnectedToast"));
    } catch (e) {
      onnotify("error", errorMessage(e));
    } finally {
      syncBusy = false;
    }
  }

  async function syncNow() {
    if (!syncConfig || syncBusy) return;
    syncBusy = true;
    try {
      await api.saveSyncConfig(syncConfig);
      syncConfig = await api.runSyncNow();
      onnotify("success", t("set.syncMergedToast"));
    } catch (e) {
      onnotify("error", errorMessage(e));
    } finally {
      syncBusy = false;
    }
  }

  async function loadCookies() {
    cookiesBusy = true;
    try {
      cookies = await api.listCookies(cookieUri.trim());
    } catch (e) {
      onnotify("error", errorMessage(e));
    } finally {
      cookiesBusy = false;
    }
  }

  async function removeCookie(c: CookieEntry) {
    try {
      await api.deleteCookie(c.name, c.domain, c.path);
      cookies = cookies.filter(
        (item) => !(item.name === c.name && item.domain === c.domain && item.path === c.path),
      );
      onnotify("info", `쿠키(${c.name})를 삭제했습니다.`);
    } catch (e) {
      onnotify("error", errorMessage(e));
    }
  }

  async function clearSiteData(scope: "cookies" | "cache" | "all") {
    confirmClearSiteScope = null;
    const label =
      scope === "cookies"
        ? t("set.clearCookiesAll")
        : scope === "cache"
          ? t("set.clearCacheAll")
          : t("set.clearAllSiteData");
    try {
      await api.clearBrowsingData(scope);
      cookies = scope === "cache" ? cookies : [];
      onnotify("success", label);
    } catch (e) {
      onnotify("error", errorMessage(e));
    }
  }

  async function addLoginEntry() {
    const origin = newLoginOrigin.trim();
    const user = newLoginUser.trim();
    const pass = newLoginPass;
    if (!origin || !user || !pass) {
      onnotify("error", "사이트 주소(예: https://example.com), 아이디, 비밀번호를 입력하세요.");
      return;
    }
    try {
      await api.saveLogin(crypto.randomUUID(), origin, user, pass);
      savedLogins = await api.listSavedLogins();
      newLoginOrigin = "";
      newLoginUser = "";
      newLoginPass = "";
      onnotify("success", "암호화하여 로그인 정보를 저장했습니다.");
    } catch (e) {
      onnotify("error", errorMessage(e));
    }
  }

  async function removeLogin(id: string) {
    try {
      await api.deleteLogin(id);
      savedLogins = savedLogins.filter((l) => l.id !== id);
    } catch (e) {
      onnotify("error", errorMessage(e));
    }
  }

  async function saveProfileForm() {
    try {
      autofillProfile = await api.saveAutofillProfile(autofillProfile);
      onnotify("success", "자동 채우기 개인 정보를 저장했습니다.");
    } catch (e) {
      onnotify("error", errorMessage(e));
    }
  }

  async function importFromBrowser(b: DetectedBrowserProfile) {
    if (importBusy) return;
    importBusy = true;
    try {
      const res = await api.importExternalBrowser(b.id, b.hasBookmarks, b.hasHistory);
      onnotify("success", `${res.source}에서 북마크 ${res.bookmarks}개 · 방문 기록 ${res.history}개를 가져왔습니다.`);
    } catch (e) {
      onnotify("error", errorMessage(e));
    } finally {
      importBusy = false;
    }
  }

  async function importPasswordsFromCsv(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    try {
      const text = await file.text();
      const count = await api.importPasswordsCsv(text);
      savedLogins = await api.listSavedLogins();
      onnotify("success", `비밀번호 ${count}개를 암호화하여 가져왔습니다.`);
    } catch (e) {
      onnotify("error", errorMessage(e));
    } finally {
      input.value = "";
    }
  }

  async function installExtensionFolder() {
    const folder = extensionFolderInput.trim();
    if (!folder || extensionBusy) return;
    extensionBusy = true;
    try {
      await api.addWebExtension(folder);
      webExtensions = await api.listWebExtensions();
      extensionFolderInput = "";
      onnotify("success", "확장 프로그램을 설치했습니다.");
    } catch (e) {
      onnotify("error", errorMessage(e));
    } finally {
      extensionBusy = false;
    }
  }

  async function toggleExtension(ext: WebExtensionInfo, remove = false) {
    try {
      await api.toggleOrRemoveWebExtension(ext.id, remove, !ext.enabled);
      webExtensions = await api.listWebExtensions();
    } catch (e) {
      onnotify("error", errorMessage(e));
    }
  }

  async function addUserScript() {
    if (!newScriptName.trim() || (!newScriptCode.trim() && !newScriptCss.trim())) {
      onnotify("error", "스크립트 이름과 실행 코드 또는 CSS를 입력하세요.");
      return;
    }
    try {
      await api.saveUserScript({
        id: crypto.randomUUID(),
        name: newScriptName.trim(),
        matchPattern: newScriptMatch.trim() || "*",
        code: newScriptCode,
        css: newScriptCss,
        enabled: true,
        updatedAt: Math.floor(Date.now() / 1000),
      });
      userScripts = await api.listUserScripts();
      newScriptName = "";
      newScriptCode = "";
      newScriptCss = "";
      onnotify("success", "사용자 스크립트를 추가했습니다.");
    } catch (e) {
      onnotify("error", errorMessage(e));
    }
  }

  async function toggleUserScript(s: UserScriptItem) {
    try {
      await api.saveUserScript({ ...s, enabled: !s.enabled, updatedAt: Math.floor(Date.now() / 1000) });
      userScripts = await api.listUserScripts();
    } catch (e) {
      onnotify("error", errorMessage(e));
    }
  }

  async function removeUserScript(id: string) {
    try {
      await api.deleteUserScript(id);
      userScripts = userScripts.filter((s) => s.id !== id);
    } catch (e) {
      onnotify("error", errorMessage(e));
    }
  }

  function pushHost(list: string[], value: string): string[] {
    const clean = value.trim().toLowerCase();
    if (!clean || list.includes(clean)) return list;
    return [...list, clean];
  }
  let glassTinted = $state(
    typeof window !== "undefined" && localStorage.getItem("glass-tinted") === "true",
  );
  let accentColor = $state(
    (typeof window !== "undefined" && localStorage.getItem("accent-color")) || "#684cff",
  );
  let themeAutoSync = $state(
    typeof window === "undefined" || localStorage.getItem("theme-auto-sync") !== "false",
  );

  const accentPresets = [
    { name: "리퀴드 바이올렛", value: "#684cff" },
    { name: "오션 블루", value: "#2563eb" },
    { name: "아쿠아 시안", value: "#0891b2" },
    { name: "에메랄드", value: "#10b981" },
    { name: "로즈 핑크", value: "#ec4899" },
    { name: "코랄 오렌지", value: "#f97316" },
    { name: "골드 앰버", value: "#d97706" },
  ];

  function broadcastUiPrefs() {
    try {
      localStorage.setItem("glass-tinted", String(glassTinted));
      localStorage.setItem("accent-color", accentColor);
      localStorage.setItem("theme-auto-sync", String(themeAutoSync));
    } catch {
      // Ignore storage errors.
    }
    void emit("ui-prefs-updated", { glassTinted, accentColor, themeAutoSync }).catch(() => {});
  }

  function onToggleGlassTinted() {
    broadcastUiPrefs();
  }

  function setAccentColor(color: string) {
    accentColor = color;
    broadcastUiPrefs();
  }

  const providerLabels: Record<string, string> = {
    hyMt2: "Hy-MT2 온디바이스 (로컬 AI · 무료 오프라인)",
    googleGemini: "Google AI Studio (Gemini)",
    openAiCompatible: "OpenAI 공식 API / 호환 API",
    codexLegacy: "레거시 코덱스 (ChatGPT OAuth)",
    localBridge: "로컬 브리지",
  };

  async function refreshHyMt2State() {
    hyLoading = true;
    try {
      const [hw, list] = await Promise.all([
        api.detectHyMt2Hardware(),
        api.listHyMt2Models(),
      ]);
      hyHardware = hw;
      hyModels = list;
      if (draft.provider === "hyMt2" && !draft.model.trim()) {
        const installedRec = list.find((m) => m.installed && m.recommended) ?? list.find((m) => m.installed);
        draft.model = installedRec?.id ?? hw.recommendedModelId;
      }
    } catch {
      // Hardware probe errors surface smoothly on demand.
    } finally {
      hyLoading = false;
    }
  }

  function formatBytes(bytes: number): string {
    if (!bytes) return "0 MB";
    const gb = bytes / (1024 * 1024 * 1024);
    if (gb >= 1) return `${gb.toFixed(2)} GB`;
    return `${Math.max(1, Math.round(bytes / (1024 * 1024)))} MB`;
  }

  async function downloadHyModel(modelId: string) {
    hyProgress = {
      ...hyProgress,
      [modelId]: {
        modelId,
        status: "downloading",
        downloadedBytes: 0,
        totalBytes: hyModels.find((m) => m.id === modelId)?.expectedBytes ?? 0,
        percent: 0,
        speedMbps: 0,
        message: "연결 중…",
      },
    };
    try {
      const updated = await api.downloadHyMt2Model(modelId, (p) => {
        hyProgress = { ...hyProgress, [modelId]: p };
      });
      hyModels = await api.listHyMt2Models();
      draft.model = updated.id;
      const next = { ...hyProgress };
      delete next[modelId];
      hyProgress = next;
      onnotify("success", `${updated.displayName} 다운로드 및 무결성 검증을 완료했습니다.`);
    } catch (error) {
      const msg = errorMessage(error);
      const next = { ...hyProgress };
      delete next[modelId];
      hyProgress = next;
      hyModels = await api.listHyMt2Models().catch(() => hyModels);
      if (msg.includes("취소")) onnotify("info", msg);
      else onnotify("error", msg);
    }
  }

  async function cancelHyDownload(modelId: string) {
    try {
      await api.cancelHyMt2Download(modelId);
    } catch (error) {
      onnotify("error", errorMessage(error));
    }
  }

  async function deleteHyModel(modelId: string) {
    confirmDeleteModelId = null;
    try {
      await api.deleteHyMt2Model(modelId);
      hyModels = await api.listHyMt2Models();
      onnotify("info", `Deleted ${modelId}.`);
    } catch (error) {
      onnotify("error", errorMessage(error));
    }
  }

  async function changeUiLanguage(lang: UiLanguage) {
    draft.uiLanguage = lang;
    setUiLang(lang);
    try {
      const updated = await api.setUiLanguage(lang);
      draft = { ...updated.settings };
      setUiLang(updated.settings.uiLanguage);
      onsaved(updated);
    } catch (error) {
      onnotify("error", errorMessage(error));
    }
  }

  async function unloadHyModel() {
    try {
      const unloaded = await api.unloadHyMt2Model();
      hyModels = await api.listHyMt2Models();
      onnotify("info", unloaded ? "Hy-MT2 모델을 메모리(RAM/VRAM)에서 해제했습니다." : "현재 메모리에 적재된 모델이 없습니다.");
    } catch (error) {
      onnotify("error", errorMessage(error));
    }
  }

  const codexStarterModels = [
    { name: "gpt-5.4", displayName: "GPT-5.4 (ChatGPT OAuth)" },
    { name: "gpt-5.4-mini", displayName: "GPT-5.4 Mini" },
    { name: "gpt-6.1-sol", displayName: "GPT-6.1 Sol" },
    { name: "gpt-6.1-terra", displayName: "GPT-6.1 Terra" },
    { name: "gpt-5.3-codex", displayName: "GPT-5.3 Codex" },
    { name: "gpt-5.2-codex", displayName: "GPT-5.2 Codex" },
    { name: "gpt-5.1-codex", displayName: "GPT-5.1 Codex" },
    { name: "gpt-5.1-codex-mini", displayName: "GPT-5.1 Codex Mini" },
    { name: "gpt-5-codex", displayName: "GPT-5 Codex" },
  ];

  const codexModelOptions = $derived(
    draft.provider === "codexLegacy" && models.length === 0 ? codexStarterModels : models,
  );

  async function loadModels() {
    const requestId = ++modelRequestId;
    loadingModels = true;
    testResult = null;
    try {
      const result = await api.listModels(
        { ...draft },
        apiKey.trim() || undefined,
        bridgeToken.trim() || undefined,
      );
      if (requestId !== modelRequestId) return;
      models = result;
      const isUtilityCodex =
        draft.provider === "codexLegacy" &&
        draft.model.trim().toLowerCase().startsWith("codex-auto-review");
      if ((!draft.model || isUtilityCodex) && result.length > 0) {
        draft.model = result[0].name;
      }
      onnotify("success", `${providerLabels[draft.provider]} 모델 ${models.length}개를 불러왔습니다.`);
    } catch (error) {
      if (requestId === modelRequestId) onnotify("error", errorMessage(error));
    } finally {
      if (requestId === modelRequestId) loadingModels = false;
    }
  }

  async function persist(close: boolean): Promise<boolean> {
    saving = true;
    try {
      const result = await api.saveSettings(
        draft,
        apiKey.trim() ? apiKey.trim() : undefined,
        bridgeToken.trim() ? bridgeToken.trim() : undefined,
      );
      onsaved(result);
      hasApiKey = result.hasApiKey;
      hasBridgeToken = result.hasBridgeToken;
      hasCodexAuth = result.hasCodexAuth;
      codexAccountId = result.codexAccountId ?? null;
      codexEmail = result.codexEmail ?? null;
      apiKey = "";
      bridgeToken = "";
      testResult = null;
      if (close && !embedded) onclose();
      return true;
    } catch (error) {
      onnotify("error", errorMessage(error));
      return false;
    } finally {
      saving = false;
    }
  }

  async function saveAndClose() {
    if (await persist(true)) {
      onnotify("success", t("set.saveToast"));
    }
  }

  async function exportData() {
    if (exportingData) return;
    exportingData = true;
    try {
      const path = await api.exportAppData();
      onnotify("success", `데이터를 내보냈습니다: ${path}`);
    } catch (error) {
      onnotify("error", errorMessage(error));
    } finally {
      exportingData = false;
    }
  }

  async function importData(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    importingData = true;
    try {
      const text = await file.text();
      const summary = await api.importAppData(text);
      onnotify(
        "success",
        `가져오기 완료: 용어 ${summary.glossary}개 · 북마크 ${summary.bookmarks}개 · 방문 기록 ${summary.history}개`,
      );
    } catch (error) {
      onnotify("error", errorMessage(error));
    } finally {
      importingData = false;
      input.value = "";
    }
  }

  async function testConnection() {
    testing = true;
    testResult = null;
    try {
      testResult = await api.testConnection(
        { ...draft },
        apiKey.trim() || undefined,
        bridgeToken.trim() || undefined,
      );
      onnotify(
        testResult.ok ? "success" : "error",
        testResult.ok
          ? `연결 테스트 통과 (${testResult.model}). 설정은 저장 버튼을 눌러야 적용됩니다.`
          : testResult.message,
      );
    } catch (error) {
      onnotify("error", errorMessage(error));
    } finally {
      testing = false;
    }
  }

  async function clearApiKey() {
    saving = true;
    try {
      const result = await api.saveSettings(draft, "", undefined);
      onsaved(result);
      hasApiKey = result.hasApiKey;
      apiKey = "";
      onnotify("info", "저장된 API 키를 삭제했습니다.");
    } catch (error) {
      onnotify("error", errorMessage(error));
    } finally {
      saving = false;
    }
  }

  async function clearBridgeToken() {
    saving = true;
    try {
      const result = await api.saveSettings(draft, undefined, "");
      onsaved(result);
      hasBridgeToken = result.hasBridgeToken;
      bridgeToken = "";
      onnotify("info", "저장된 브리지 토큰을 삭제했습니다.");
    } catch (error) {
      onnotify("error", errorMessage(error));
    } finally {
      saving = false;
    }
  }

  async function loginCodexLegacy() {
    if (codexBusy) return;
    codexBusy = true;
    try {
      await api.startCodexOauthLogin();
      onnotify("info", "ChatGPT / 레거시 코덱스 로그인 창이 열렸습니다. 로그인 후 자동으로 연결됩니다.");
    } catch (error) {
      onnotify("error", errorMessage(error));
    } finally {
      codexBusy = false;
    }
  }

  async function importCodexSession() {
    if (codexBusy) return;
    codexBusy = true;
    try {
      const { view: nextView, source } = await api.importLocalCodexAuth();
      hasCodexAuth = nextView.hasCodexAuth;
      codexAccountId = nextView.codexAccountId ?? null;
      codexEmail = nextView.codexEmail ?? null;
      onsaved(nextView);
      onnotify(
        "success",
        `${source}에서 레거시 코덱스 세션을 불러왔습니다${codexEmail ? ` (${codexEmail})` : ""}.`,
      );
      if (draft.provider === "codexLegacy") {
        void loadModels();
      }
    } catch (error) {
      onnotify("error", errorMessage(error));
    } finally {
      codexBusy = false;
    }
  }

  async function disconnectCodexLegacy() {
    if (codexBusy) return;
    codexBusy = true;
    try {
      const nextView = await api.clearCodexAuth();
      hasCodexAuth = nextView.hasCodexAuth;
      codexAccountId = null;
      codexEmail = null;
      onsaved(nextView);
      onnotify("info", "레거시 코덱스 연결을 해제했습니다.");
    } catch (error) {
      onnotify("error", errorMessage(error));
    } finally {
      codexBusy = false;
    }
  }

  let exportingDiagnostics = $state(false);

  async function copyTesterDiagnostics() {
    if (exportingDiagnostics) return;
    exportingDiagnostics = true;
    try {
      const report = await api.getTesterDiagnostics();
      await navigator.clipboard.writeText(report);
      onnotify("success", "테스터 진단 리포트를 클립보드에 복사했습니다.");
    } catch (error) {
      onnotify("error", errorMessage(error));
    } finally {
      exportingDiagnostics = false;
    }
  }

  async function saveTesterDiagnostics() {
    if (exportingDiagnostics) return;
    exportingDiagnostics = true;
    try {
      const savedPath = await api.saveTesterDiagnostics();
      onnotify("success", `진단 리포트를 저장했습니다: ${savedPath}`);
    } catch (error) {
      onnotify("error", errorMessage(error));
    } finally {
      exportingDiagnostics = false;
    }
  }

  function onProviderChange() {
    modelRequestId++;
    loadingModels = false;
    models = [];
    testResult = null;
    if (draft.provider === "openAiCompatible") {
      if (
        !draft.baseUrl.trim() ||
        draft.baseUrl.includes("chatgpt.com/backend-api")
      ) {
        draft.baseUrl = "https://api.openai.com/v1";
      }
    } else if (draft.provider === "hyMt2") {
      void refreshHyMt2State();
      const installed = hyModels.find((m) => m.installed && m.recommended) ?? hyModels.find((m) => m.installed);
      if (!draft.model.trim() || !draft.model.toLowerCase().endsWith(".gguf")) {
        draft.model = installed?.id ?? hyHardware?.recommendedModelId ?? "Hy-MT2-1.8B-Q4_K_M.gguf";
      }
    } else if (draft.provider === "codexLegacy") {
      if (draft.baseUrl.trim() === "https://api.openai.com/v1") {
        draft.baseUrl = "";
      }
      if (!draft.model.trim()) {
        draft.model = "gpt-5.4";
      }
    }
  }
</script>

<div
  class="backdrop"
  class:embedded
  role="presentation"
  onclick={(event) => {
    if (!embedded && event.target === event.currentTarget) onclose();
  }}
>
  <div class="panel" class:embedded role="dialog" aria-modal={!embedded} aria-label={t("set.title")}>
    <header class="panel-head">
      <div>
        <h2>{t("set.title")}</h2>
        <p>{t("set.subtitle")}</p>
      </div>
      {#if !embedded}
        <button class="icon-button" type="button" aria-label={t("common.close")} onclick={onclose}>
          ×
        </button>
      {/if}
    </header>

    <div class="panel-body">
      <nav class="settings-nav" aria-label={t("set.navAria")}>
        <button class:active={activeSection === "profile"} onclick={() => selectSection("profile")}>{t("set.secProfile")}</button>
        <button class:active={activeSection === "browser"} onclick={() => selectSection("browser")}>{t("set.secBrowser")}</button>
        <button class:active={activeSection === "privacy"} onclick={() => selectSection("privacy")}>{t("set.secPrivacy")}</button>
        <button class:active={activeSection === "performance"} onclick={() => selectSection("performance")}>{t("set.secPerformance")}</button>
        <button class:active={activeSection === "ai"} onclick={() => selectSection("ai")}>{t("set.secAi")}</button>
        <button class:active={activeSection === "translation"} onclick={() => selectSection("translation")}>{t("set.secTranslation")}</button>
        <button class:active={activeSection === "appearance"} onclick={() => selectSection("appearance")}>{t("set.secAppearance")}</button>
        <button class:active={activeSection === "data"} onclick={() => selectSection("data")}>{t("set.secData")}</button>
      </nav>
      <div class="settings-content">
      {#if activeSection === "ai"}
      <section class="group">
        <h3>{t("set.aiGroup")}</h3>
        <label class="field">
          <span>{t("set.aiProvider")}</span>
          <select bind:value={draft.provider} onchange={onProviderChange}>
            {#each Object.entries(providerLabels) as [value, label]}
              <option {value}>{label}</option>
            {/each}
          </select>
        </label>

        {#if draft.provider === "googleGemini"}
          <label class="field">
            <span>Google AI Studio API 키</span>
            <div class="row">
              <input
                type="password"
                autocomplete="off"
                spellcheck="false"
                placeholder={hasApiKey ? "저장됨 · 변경하려면 새 키 입력" : "AIza..."}
                bind:value={apiKey}
              />
              {#if hasApiKey}
                <button class="ghost" type="button" onclick={clearApiKey} disabled={saving}>
                  키 삭제
                </button>
              {/if}
            </div>
            <small>키는 Windows DPAPI로 암호화되어 이 PC에만 저장됩니다.</small>
          </label>

          <label class="field">
            <span>모델</span>
            <div class="row">
              <input
                list="provider-model-list"
                spellcheck="false"
                placeholder="gemini-2.5-flash"
                bind:value={draft.model}
              />
              <datalist id="provider-model-list">
                {#each models as model}
                  <option value={model.name}>{model.displayName}</option>
                {/each}
              </datalist>
              <button
                class="ghost"
                type="button"
                onclick={loadModels}
                disabled={loadingModels || saving}
              >
                {loadingModels ? "불러오는 중…" : "모델 목록"}
              </button>
            </div>
            <small>입력한 키 또는 저장된 키로 조회합니다. 번역은 Gemini 네이티브 API(generateContent)로 요청하며, 400·429 같은 오류는 Provider가 보낸 사유와 함께 표시됩니다.</small>
          </label>
        {:else if draft.provider === "openAiCompatible"}
          <label class="field">
            <span>OpenAI API 기본 URL</span>
            <input
              spellcheck="false"
              placeholder="https://api.openai.com/v1"
              bind:value={draft.baseUrl}
            />
            <small>
              GPT 공식 연결은 https://api.openai.com/v1 을 사용하세요. OpenAI Platform API 키가 필요하며, 키는 Windows DPAPI로 보호됩니다. OpenAI 호환 로컬·원격 엔드포인트(`/v1`)도 지원됩니다.
            </small>
          </label>
          <label class="field">
            <span>OpenAI Platform API 키</span>
            <div class="row">
              <input
                type="password"
                autocomplete="off"
                spellcheck="false"
                placeholder={hasApiKey ? "저장됨 · 변경하려면 새 키 입력" : "sk-..."}
                bind:value={apiKey}
              />
              {#if hasApiKey}
                <button class="ghost" type="button" onclick={clearApiKey} disabled={saving}>
                  키 삭제
                </button>
              {/if}
            </div>
            <small>공식 api.openai.com 연결에는 API 키가 필수입니다. ChatGPT Plus/Pro 구독과 API 사용량 결제는 별도입니다.</small>
          </label>
          <label class="field">
            <span>GPT 모델</span>
            <div class="row">
              <input
                list="provider-model-list"
                spellcheck="false"
                placeholder="gpt-4.1-mini"
                bind:value={draft.model}
              />
              <datalist id="provider-model-list">
                {#each models as model}
                  <option value={model.name}>{model.displayName}</option>
                {/each}
              </datalist>
              <button class="ghost" type="button" onclick={loadModels} disabled={loadingModels || saving}>
                {loadingModels ? "불러오는 중…" : "모델 목록"}
              </button>
            </div>
            <small>계정에서 사용 가능한 모델을 조회합니다. 서버가 목록을 제공하지 않으면 모델 ID를 직접 입력하세요 (예: gpt-4.1-mini).</small>
          </label>
        {:else if draft.provider === "codexLegacy"}
          <div class="sync-status-card" style="margin-bottom:12px">
            <div class="sync-user">
              <div class="sync-avatar">{codexEmail ? codexEmail[0].toUpperCase() : "C"}</div>
              <div>
                <strong>{hasCodexAuth ? "레거시 코덱스 연결됨" : "레거시 코덱스 미연결"}</strong>
                <div class="sync-sub">
                  {#if hasCodexAuth}
                    {codexEmail || "ChatGPT OAuth 계정"}
                    {#if codexAccountId}
                      · 워크스페이스 {codexAccountId}
                    {/if}
                  {:else}
                    ChatGPT Plus/Pro OAuth 로그인 또는 기존 OpenCode/Codex 세션으로 연결하세요.
                  {/if}
                </div>
              </div>
            </div>
            <div class="sync-actions" style="flex-wrap:wrap">
              <button class="primary" type="button" onclick={loginCodexLegacy} disabled={codexBusy || saving}>
                {hasCodexAuth ? "다시 로그인" : "ChatGPT / Codex 로그인"}
              </button>
              <button class="ghost" type="button" onclick={importCodexSession} disabled={codexBusy || saving}>
                OpenCode / Codex 세션 불러오기
              </button>
              {#if hasCodexAuth}
                <button class="ghost" type="button" onclick={disconnectCodexLegacy} disabled={codexBusy || saving}>
                  연결 해제
                </button>
              {/if}
            </div>
          </div>

          <label class="field">
            <span>Codex 엔드포인트 URL (선택)</span>
            <input
              spellcheck="false"
              placeholder="https://chatgpt.com/backend-api/codex"
              bind:value={draft.baseUrl}
            />
            <small>비워 두면 공식 ChatGPT Codex 백엔드(https://chatgpt.com/backend-api/codex/responses)를 사용합니다. OAuth 토큰은 Windows DPAPI로 암호화 저장됩니다.</small>
          </label>

          <label class="field">
            <span>Codex 모델 선택</span>
            <div class="row">
              <select
                value={codexModelOptions.some((m) => m.name === draft.model) ? draft.model : ""}
                onchange={(e) => {
                  const picked = e.currentTarget.value;
                  if (picked) draft.model = picked;
                }}
              >
                {#if !codexModelOptions.some((m) => m.name === draft.model)}
                  <option value="">직접 입력 ({draft.model || "모델을 선택하세요"})</option>
                {/if}
                {#each codexModelOptions as model}
                  <option value={model.name}>
                    {model.displayName && model.displayName !== model.name
                      ? `${model.displayName} (${model.name})`
                      : model.name}
                  </option>
                {/each}
              </select>
              <button class="ghost" type="button" onclick={loadModels} disabled={loadingModels || saving}>
                {loadingModels ? "불러오는 중…" : "모델 목록"}
              </button>
            </div>
          </label>

          <label class="field">
            <span>Codex 모델 ID (직접 입력 가능)</span>
            <input
              list="codex-model-list"
              spellcheck="false"
              placeholder="gpt-5.4"
              bind:value={draft.model}
            />
            <datalist id="codex-model-list">
              {#each codexModelOptions as model}
                <option value={model.name}>{model.displayName}</option>
              {/each}
            </datalist>
            <small>위 드롭다운에서 모델을 고르거나 직접 입력하세요(예: gpt-5.4, gpt-6.1-sol, gpt-5.3-codex). 목록을 새로고침하려면 모델 목록을 누르세요.</small>
            <small style="display:block;margin-top:4px">{t("set.codexTermsNotice")}</small>
          </label>
        {:else if draft.provider === "hyMt2"}
          <div class="hy-panel">
            <div class="hy-hw-card">
              <div class="hy-hw-head">
                <div>
                  <strong>온디바이스 추론 하드웨어</strong>
                  <div class="hy-hw-sub">
                    {#if hyHardware}
                      {#if hyHardware.cpuName}
                        CPU {hyHardware.cpuName} ({hyHardware.cpuCores} 스레드) ·
                      {:else}
                        CPU {hyHardware.cpuCores} 스레드 ·
                      {/if}
                      RAM {hyHardware.totalRamMb} MB (여유 {hyHardware.availableRamMb} MB)
                      {#if hyHardware.gpuName}
                        · GPU {hyHardware.gpuName} (전용 {hyHardware.dedicatedVramMb} MB / 공유 {hyHardware.sharedVramMb} MB)
                      {/if}
                      {#if hyHardware.npuAvailable}
                        · NPU {hyHardware.npuName || "감지됨"}
                      {/if}
                    {:else}
                      하드웨어 프로파일을 분석하는 중…
                    {/if}
                  </div>
                </div>
                {#if hyHardware}
                  <span
                    class="hy-badge"
                    class:gpu={hyHardware.activeBackend.startsWith("vulkan") ||
                      hyHardware.activeBackend.startsWith("cuda")}
                  >
                    {hyHardware.activeBackend.startsWith("cuda")
                      ? "NVIDIA CUDA GPU 가속"
                      : hyHardware.activeBackend === "vulkan-nvidia"
                        ? "NVIDIA GeForce Vulkan 가속"
                        : hyHardware.activeBackend === "vulkan-amd-apu"
                          ? "AMD Radeon APU + Ryzen 하이브리드"
                          : hyHardware.activeBackend === "vulkan-amd"
                            ? "AMD Radeon Vulkan 가속"
                            : hyHardware.activeBackend === "vulkan-intel"
                              ? "Intel Arc Vulkan 가속"
                              : hyHardware.activeBackend === "vulkan-intel-igpu"
                                ? "Intel 내장 Vulkan 가속"
                                : hyHardware.activeBackend.startsWith("vulkan")
                                  ? "Vulkan GPU 가속"
                                  : "CPU 네이티브"}
                  </span>
                {/if}
              </div>
              {#if hyHardware}
                <div class="hy-rec">{hyHardware.recommendationReason}</div>
                {#if hyHardware.computeDecision}
                  <div class="hy-rec" style="margin-top:4px;opacity:0.88">{hyHardware.computeDecision}</div>
                {/if}
                {#if hyHardware.detectedGpus && hyHardware.detectedGpus.length > 1}
                  <div class="hy-hw-sub" style="margin-top:4px">
                    감지된 GPU({hyHardware.detectedGpus.length}개 · 외장 GPU 우선 자동 선택): {hyHardware.detectedGpus.join(" / ")}
                  </div>
                {/if}
              {/if}
            </div>

            <div class="hy-models">
              <div class="hy-models-head">
                <strong>GGUF 모델 관리 (AppLocalData\models)</strong>
                  <div class="row">
                    <button class="ghost" type="button" onclick={() => void unloadHyModel()} disabled={saving}>
                      {t("set.unload")}
                    </button>
                    <button class="ghost" type="button" onclick={() => void refreshHyMt2State()} disabled={hyLoading}>
                      {hyLoading ? t("set.checking") : t("set.refresh")}
                    </button>
                  </div>
                </div>

              {#each hyModels as m (m.id)}
                {@const prog = hyProgress[m.id]}
                <div class="hy-model-row" class:selected={draft.model === m.id}>
                  <div class="hy-model-main">
                    <div class="hy-model-title">
                      <button
                        type="button"
                        class="hy-pick-btn"
                        onclick={() => {
                          draft.model = m.id;
                          draft.hyMt2Tier = m.tier;
                        }}
                      >
                        <span class="hy-radio" class:checked={draft.model === m.id}></span>
                        <strong>{m.displayName}</strong>
                      </button>
                      {#if m.recommended}
                        <span class="hy-pill rec">{t("set.recommendedBadge")}</span>
                      {/if}
                      {#if m.loaded}
                        <span class="hy-pill live">{t("set.loadedBadge")}</span>
                      {:else if m.installed}
                        <span class="hy-pill ok">{t("set.installedBadge")}</span>
                      {/if}
                    </div>
                    <small>{m.description} · {formatBytes(m.expectedBytes)}</small>
                    {#if m.experimentalKernelNote}
                      <small class="hy-note">{m.experimentalKernelNote}</small>
                    {/if}
                    {#if prog}
                      <div class="hy-prog-wrap">
                        <div class="hy-prog-bar">
                          <div class="hy-prog-fill" style={`width:${Math.min(100, Math.max(2, prog.percent)).toFixed(1)}%`}></div>
                        </div>
                        <div class="hy-prog-meta">
                          <span>{prog.message || (prog.status === "verifying" ? "SHA256..." : t("set.download"))}</span>
                          <span>{prog.percent.toFixed(1)}% ({formatBytes(prog.downloadedBytes)} / {formatBytes(prog.totalBytes)}) · {prog.speedMbps.toFixed(2)} MB/s</span>
                        </div>
                      </div>
                    {/if}
                  </div>
                  <div class="hy-model-actions">
                    {#if prog}
                      <button class="ghost" type="button" onclick={() => void cancelHyDownload(m.id)}>
                        {t("set.pause")}
                      </button>
                    {:else if m.installed}
                      {#if confirmDeleteModelId === m.id}
                        <button class="primary" type="button" onclick={() => void deleteHyModel(m.id)} disabled={saving}>
                          {t("common.confirmDelete")}
                        </button>
                        <button class="ghost" type="button" onclick={() => (confirmDeleteModelId = null)}>
                          {t("common.cancel")}
                        </button>
                      {:else}
                        <button class="ghost" type="button" onclick={() => (confirmDeleteModelId = m.id)} disabled={saving}>
                          {t("common.delete")}
                        </button>
                      {/if}
                    {:else if m.downloadUrl}
                      <button class="primary" type="button" onclick={() => void downloadHyModel(m.id)}>
                        {m.partialBytes > 0 ? t("set.resumeDownload", { size: formatBytes(m.partialBytes) }) : t("set.download")}
                      </button>
                    {/if}
                  </div>
                </div>
              {/each}
            </div>

            <div class="grid">
              <label class="field">
                <span>{t("set.gpuLayersLabel")}</span>
                <input
                  type="number"
                  min="-1"
                  max="200"
                  bind:value={draft.hyMt2GpuLayers}
                />
              </label>

              <label class="field">
                <span>{t("set.idleUnloadLabel")}</span>
                <input
                  type="number"
                  min="0"
                  max="3600"
                  bind:value={draft.hyMt2IdleUnloadSecs}
                />
              </label>

              <label class="field">
                <span>{t("set.ctxLenLabel")}</span>
                <select bind:value={draft.hyMt2CtxSize}>
                  <option value={4096}>4,096</option>
                  <option value={8192}>8,192</option>
                  <option value={16384}>16,384</option>
                </select>
              </label>

              <label class="field">
                <span>{t("set.model")}</span>
                <input
                  spellcheck="false"
                  placeholder="Hy-MT2-1.8B-Q4_K_M.gguf"
                  bind:value={draft.model}
                />
                <small>다운로드한 모델 ID 또는 로컬 GGUF 경로를 사용합니다.</small>
              </label>
            </div>
          </div>
        {:else}
          <label class="field">
            <span>브리지 URL</span>
            <input
              spellcheck="false"
              placeholder="http://127.0.0.1:4096/v1"
              bind:value={draft.baseUrl}
            />
            <small>루프백 주소만 허용합니다. 서버가 /models를 지원하지 않으면 직접 입력하세요.</small>
          </label>
          <label class="field">
            <span>모델 이름</span>
            <div class="row">
              <input
                list="provider-model-list"
                spellcheck="false"
                placeholder="브리지 모델 ID"
                bind:value={draft.model}
              />
              <datalist id="provider-model-list">
                {#each models as model}
                  <option value={model.name}>{model.displayName}</option>
                {/each}
              </datalist>
              <button class="ghost" type="button" onclick={loadModels} disabled={loadingModels || saving}>
                {loadingModels ? "불러오는 중…" : "모델 목록"}
              </button>
            </div>
          </label>
          <label class="field">
            <span>인증 토큰</span>
            <div class="row">
              <input
                type="password"
                autocomplete="off"
                spellcheck="false"
                placeholder={hasBridgeToken ? "저장됨 · 변경하려면 새 토큰 입력" : "브리지 토큰"}
                bind:value={bridgeToken}
              />
              {#if hasBridgeToken}
                <button class="ghost" type="button" onclick={clearBridgeToken} disabled={saving}>
                  토큰 삭제
                </button>
              {/if}
            </div>
          </label>
        {/if}

        <div class="row end">
          <button class="ghost" type="button" onclick={testConnection} disabled={testing || saving}>
            {testing ? t("set.testing") : t("set.testConnection")}
          </button>
        </div>
        {#if testResult}
          <div class="test-result" class:ok={testResult.ok}>
            <strong>{testResult.ok ? t("set.testOk") : t("set.testFail")}</strong>
            <span>{testResult.message}</span>
          </div>
        {/if}
      </section>

      {:else if activeSection === "profile"}
      {#if syncConfig}
        {@const activeProviderLabel =
          syncConfig.provider === "google"
            ? t("set.syncConnectedGoogle")
            : syncConfig.provider === "microsoft"
              ? t("set.syncConnectedMs")
              : syncConfig.provider === "local"
                ? t("set.syncConnectedLocal")
                : t("set.syncUnlinked")}
        {@const connectedCloud = cloudAccounts.find((c) => c.provider === syncConfig?.provider)}
        <section class="group">
          <div class="profile-hero">
            <div
              class="avatar"
              class:google={syncConfig.provider === "google"}
              class:microsoft={syncConfig.provider === "microsoft"}
            >
              {syncConfig.provider === "google" ? "G" : syncConfig.provider === "microsoft" ? "M" : (syncConfig.displayName || "P").slice(0, 1).toUpperCase()}
            </div>
            <div class="profile-meta">
              <strong>{syncConfig.displayName || "Profile"}</strong>
              <span>{syncConfig.signedIn ? `${syncConfig.email} · ${activeProviderLabel}` : t("set.syncHintUnlinked")}</span>
              <small class:ok-text={syncConfig.signedIn}>
                {#if syncConfig.signedIn}
                  {connectedCloud?.available ? t("set.syncOnCloud") : t("set.syncOnLocal")}
                {:else}
                  {t("set.syncOff")}
                {/if}
                {#if syncConfig.lastSyncedAt}
                  {" "}{t("set.syncLast", { time: new Date(syncConfig.lastSyncedAt * 1000).toLocaleString() })}
                {/if}
              </small>
            </div>
            {#if syncConfig.signedIn}
              <div class="row">
                <button class="primary" type="button" onclick={syncNow} disabled={syncBusy}>
                  {syncBusy ? t("set.syncing") : t("set.syncNow")}
                </button>
                <button class="ghost" type="button" onclick={disconnectAccountSync} disabled={syncBusy}>
                  {t("set.logout")}
                </button>
              </div>
            {/if}
          </div>

          <h3>{t("set.syncProvidersHeading")}</h3>
          <div class="account-provider-grid">
            <button
              type="button"
              class="provider-card"
              class:selected={syncConfig.signedIn && syncConfig.provider === "google"}
              disabled={syncBusy}
              onclick={() => void startOauthAccountLogin("google")}
            >
              <span class="provider-badge google">G</span>
              <div>
                <strong>
                  {authOpeningProvider === "google"
                    ? t("set.syncOpeningGoogle")
                    : syncConfig.signedIn && syncConfig.provider === "google"
                      ? t("set.syncSwitchGoogle")
                      : t("set.syncSignInGoogle")}
                </strong>
                <small>
                  {cloudAccounts.find((c) => c.provider === "google")?.available
                    ? t("set.syncGoogleDriveAvail")
                    : t("set.syncGoogleNoDrive")}
                </small>
              </div>
            </button>
            <button
              type="button"
              class="provider-card"
              class:selected={syncConfig.signedIn && syncConfig.provider === "microsoft"}
              disabled={syncBusy}
              onclick={() => void startOauthAccountLogin("microsoft")}
            >
              <span class="provider-badge microsoft">M</span>
              <div>
                <strong>
                  {authOpeningProvider === "microsoft"
                    ? t("set.syncOpeningMs")
                    : syncConfig.signedIn && syncConfig.provider === "microsoft"
                      ? t("set.syncSwitchMs")
                      : t("set.syncSignInMs")}
                </strong>
                <small>
                  {cloudAccounts.find((c) => c.provider === "microsoft")?.available
                    ? t("set.syncMsOneDriveAvail")
                    : t("set.syncMsNoOneDrive")}
                </small>
              </div>
            </button>
          </div>
          <p class="note" style="margin:10px 0 0">
            {t("set.syncWindowHint")}
          </p>

          <details class="manual-sync-box" style="margin-top:12px">
            <summary style="cursor:pointer;font-size:12px;font-weight:600;color:var(--text-muted)">
              {t("set.syncManualSummary")}
            </summary>
            <div class="grid" style="margin-top:10px">
              <label class="field">
                <span>{t("set.syncManualProvider")}</span>
                <select bind:value={manualSyncProvider}>
                  <option value="local">{t("set.syncManualProvLocal")}</option>
                  <option value="google">Google (Google Drive)</option>
                  <option value="microsoft">Microsoft (OneDrive)</option>
                </select>
              </label>
              <label class="field">
                <span>{t("set.syncManualIdentifier")}</span>
                <input
                  spellcheck="false"
                  placeholder={manualSyncProvider === "local" ? "default-profile" : "name@example.com"}
                  bind:value={manualSyncIdentifier}
                />
              </label>
              <label class="field">
                <span>{t("set.syncManualName")}</span>
                <input placeholder="My PC" bind:value={manualSyncDisplayName} />
              </label>
              <label class="field">
                <span>{t("set.syncManualDir")}</span>
                <input spellcheck="false" placeholder="C:\Users\...\Sync" bind:value={manualSyncDir} />
              </label>
            </div>
            <div class="row end">
              <button
                class="ghost"
                type="button"
                disabled={syncBusy || !manualSyncIdentifier.trim()}
                onclick={connectManualSync}
              >
                {t("set.syncManualConnect")}
              </button>
            </div>
          </details>
        </section>

        <section class="group">
          <h3>{t("set.syncItemsHeading")}</h3>
          <p class="note">{t("set.syncSecurityNote")}</p>
          <div class="sync-list">
            {#each [
              ["syncBookmarks", t("set.syncBookmarks"), t("set.syncBookmarksDesc")],
              ["syncSettings", t("set.syncSettings"), t("set.syncSettingsDesc")],
              ["syncPersonalInfo", t("set.syncPersonalInfo"), t("set.syncPersonalInfoDesc")],
              ["syncHistory", t("set.syncHistory"), t("set.syncHistoryDesc")],
              ["syncOpenTabs", t("set.syncOpenTabs"), t("set.syncOpenTabsDesc")],
              ["syncExtensions", t("set.syncExtensions"), t("set.syncExtensionsDesc")],
              ["syncGlossary", t("set.syncGlossary"), t("set.syncGlossaryDesc")],
              ["syncWorkspaces", t("set.syncWorkspaces"), t("set.syncWorkspacesDesc")],
            ] as [key, title, desc] (key)}
              <div class="sync-row">
                <div>
                  <strong>{title}</strong>
                  <small>{desc}</small>
                </div>
                <button
                  type="button"
                  role="switch"
                  aria-checked={Boolean((syncConfig as unknown as Record<string, unknown>)[key])}
                  aria-label={title}
                  class="toggle-switch"
                  class:on={Boolean((syncConfig as unknown as Record<string, unknown>)[key])}
                  onclick={() => {
                    if (!syncConfig) return;
                    const next = { ...syncConfig, [key]: !(syncConfig as unknown as Record<string, boolean>)[key] };
                    void saveSync(next);
                  }}
                >
                  <span class="knob"></span>
                </button>
              </div>
            {/each}
          </div>
        </section>
      {/if}

      {:else if activeSection === "browser"}
        {#if activeSubpage === "adblock-exceptions"}
          <div class="subpage">
            <button type="button" class="subpage-back" onclick={closeSubpage}>
              ← {t("set.secBrowser")}
            </button>
            <section class="group">
              <h3>{t("set.adblockExTitle")}</h3>
              <p class="note">{t("set.adblockExNote")}</p>
              <div class="row">
                <input placeholder={t("set.adblockExPh")} bind:value={newAdblockHost} />
                <button
                  class="ghost"
                  type="button"
                  onclick={() => {
                    draft.adBlockAllowlist = pushHost(draft.adBlockAllowlist, newAdblockHost);
                    newAdblockHost = "";
                    void persist(false);
                  }}
                >
                  {t("set.addExceptionBtn")}
                </button>
              </div>
              <div class="compact-list" style="margin-top:12px">
                {#each draft.adBlockAllowlist as host, idx (`${idx}-${host}`)}
                  <div class="list-item">
                    <div class="list-meta"><strong>{host}</strong></div>
                    <button
                      class="ghost small-btn"
                      type="button"
                      onclick={() => {
                        draft.adBlockAllowlist = draft.adBlockAllowlist.filter((h) => h !== host);
                        void persist(false);
                      }}
                    >
                      {t("common.delete")}
                    </button>
                  </div>
                {/each}
                {#if draft.adBlockAllowlist.length === 0}
                  <small>{t("set.adblockExEmpty")}</small>
                {/if}
              </div>
            </section>
          </div>
        {:else}
          <section class="group">
            <h3>{t("set.searchStartGroup")}</h3>
            <label class="field"><span>{t("set.searchEngineLabel")}</span>
              <select bind:value={draft.searchEngine}>
                <option value="google">Google</option>
                <option value="bing">Bing</option>
                <option value="duckduckgo">DuckDuckGo</option>
              </select>
            </label>
            <label class="field"><span>{t("set.startupModeLabel")}</span>
              <select bind:value={draft.startupMode}>
                <option value="restore">{t("set.startupRestore")}</option>
                <option value="custom">{t("set.startupCustom")}</option>
                <option value="search">{t("set.startupSearch")}</option>
              </select>
            </label>
            <label class="field">
              <span>{t("set.homeUrlLabel")}</span>
              <input
                spellcheck="false"
                bind:value={draft.homeUrl}
                placeholder={t("set.homeUrlPh")}
              />
              <small>{t("set.homeUrlHelp")}</small>
            </label>
          </section>

          <section class="group">
            <h3>{t("set.adblockGroup")}</h3>
            <label class="check-field">
              <input
                type="checkbox"
                bind:checked={draft.adBlockEnabled}
                onchange={() => {
                  void persist(false).then((ok) => {
                    if (ok) {
                      onnotify(
                        "info",
                        draft.adBlockEnabled ? t("toast.adblockOn") : t("toast.adblockOff"),
                      );
                    }
                  });
                }}
              />
              <span>{t("set.adblockToggleLabel")}</span>
            </label>
            <button type="button" class="folder-nav-row" onclick={() => openSubpage("adblock-exceptions")}>
              <div>
                <strong>{t("set.adblockExceptionsTitle")}</strong>
                <small>{t("set.adblockExceptionsCount", { count: draft.adBlockAllowlist.length })}</small>
              </div>
              <span class="chevron">›</span>
            </button>
          </section>

          <section class="group">
            <h3>{t("set.popupGroup")}</h3>
            <label class="field"><span>{t("set.popupPolicyLabel")}</span>
              <select bind:value={draft.popupPolicy}>
                <option value="block">{t("set.popupBlock")}</option>
                <option value="block-all">{t("set.popupBlockAll")}</option>
                <option value="allow">{t("set.popupAllow")}</option>
              </select>
            </label>
            <div class="row">
              <input placeholder={t("set.popupAllowedPh")} bind:value={newPopupOrigin} />
              <button
                class="ghost"
                type="button"
                onclick={() => {
                  draft.popupAllowedOrigins = pushHost(draft.popupAllowedOrigins, newPopupOrigin);
                  newPopupOrigin = "";
                  void persist(false);
                }}
              >
                {t("set.popupAddBtn")}
              </button>
            </div>
            {#if draft.popupAllowedOrigins.length > 0}
              <div class="chip-list">
                {#each draft.popupAllowedOrigins as host, idx (`${idx}-${host}`)}
                  <span class="chip">
                    {host}
                    <button
                      type="button"
                      onclick={() => {
                        draft.popupAllowedOrigins = draft.popupAllowedOrigins.filter((h) => h !== host);
                        void persist(false);
                      }}
                    >×</button>
                  </span>
                {/each}
              </div>
            {/if}
          </section>
        {/if}

      {:else if activeSection === "privacy"}
        {#if activeSubpage === "cookies"}
          <div class="subpage">
            <button type="button" class="subpage-back" onclick={closeSubpage}>
              ← {t("set.secPrivacy")}
            </button>
            <section class="group">
              <h3>{t("set.cookiesManage")}</h3>
              <div class="row" style="margin-bottom:10px;flex-wrap:wrap">
                <button class="ghost" type="button" onclick={() => (confirmClearSiteScope = "cookies")}>{t("set.clearCookiesAll")}</button>
                <button class="ghost" type="button" onclick={() => (confirmClearSiteScope = "cache")}>{t("set.clearCacheAll")}</button>
                <button class="ghost" type="button" onclick={() => (confirmClearSiteScope = "all")}>{t("set.clearAllSiteData")}</button>
              </div>
              {#if confirmClearSiteScope}
                {@const confirmLabel =
                  confirmClearSiteScope === "cookies"
                    ? t("set.clearCookiesAll")
                    : confirmClearSiteScope === "cache"
                      ? t("set.clearCacheAll")
                      : t("set.clearAllSiteData")}
                <div class="inline-confirm-bar">
                  <span>{t("set.confirmClearCookies", { label: confirmLabel })}</span>
                  <div class="row">
                    <button
                      class="danger"
                      type="button"
                      onclick={() => confirmClearSiteScope && void clearSiteData(confirmClearSiteScope)}
                    >
                      {t("common.confirmDelete")}
                    </button>
                    <button class="ghost small-btn" type="button" onclick={() => (confirmClearSiteScope = null)}>
                      {t("common.cancel")}
                    </button>
                  </div>
                </div>
              {/if}
              <div class="row">
                <input placeholder={t("set.cookieFilterPh")} bind:value={cookieUri} />
                <button class="ghost" type="button" onclick={loadCookies} disabled={cookiesBusy}>
                  {cookiesBusy ? t("set.queryingBtn") : t("set.queryBtn")}
                </button>
              </div>
              <div class="compact-list">
                {#each cookies.slice(0, 60) as c, idx (`${idx}-${c.domain}-${c.name}-${c.path}`)}
                  <div class="list-item">
                    <div class="list-meta">
                      <strong>{c.name} <small>({c.domain}{c.path})</small></strong>
                      <small>{c.valuePreview} {c.httpOnly ? "· HttpOnly" : ""} {c.secure ? "· Secure" : ""}</small>
                    </div>
                    <button class="ghost small-btn" type="button" onclick={() => void removeCookie(c)}>{t("common.delete")}</button>
                  </div>
                {/each}
                {#if cookies.length === 0}
                  <small>{t("set.noCookies")}</small>
                {/if}
              </div>
            </section>
          </div>
        {:else if activeSubpage === "passwords"}
          <div class="subpage">
            <button type="button" class="subpage-back" onclick={closeSubpage}>
              ← {t("set.secPrivacy")}
            </button>
            <section class="group">
              <h3>{t("set.savedLoginsTitle")}</h3>
              <div class="grid">
                <input placeholder={t("set.loginOriginPh")} bind:value={newLoginOrigin} />
                <input placeholder={t("set.loginUserPh")} bind:value={newLoginUser} />
                <input type="password" placeholder={t("set.loginPassPh")} bind:value={newLoginPass} />
                <button class="ghost" type="button" onclick={addLoginEntry}>{t("set.addLoginBtn")}</button>
              </div>
              <div class="row" style="margin-top:8px">
                <button class="ghost" type="button" onclick={() => passwordCsvInput?.click()}>
                  {t("set.importCsvBtn")}
                </button>
                <input
                  class="hidden-file"
                  bind:this={passwordCsvInput}
                  type="file"
                  accept=".csv,text/csv"
                  onchange={importPasswordsFromCsv}
                />
              </div>
              <div class="compact-list">
                {#each savedLogins as item (item.id)}
                  <div class="list-item">
                    <div class="list-meta">
                      <strong>{item.origin}</strong>
                      <small>{item.username} · {revealedLoginIds[item.id] ? item.password : "••••••••"}</small>
                    </div>
                    <div class="row">
                      <button
                        class="ghost small-btn"
                        type="button"
                        onclick={() => {
                          revealedLoginIds = { ...revealedLoginIds, [item.id]: !revealedLoginIds[item.id] };
                        }}
                      >
                        {revealedLoginIds[item.id] ? t("set.hide") : t("set.show")}
                      </button>
                      <button class="ghost small-btn" type="button" onclick={() => void removeLogin(item.id)}>{t("common.delete")}</button>
                    </div>
                  </div>
                {/each}
                {#if savedLogins.length === 0}
                  <small>{t("set.noLogins")}</small>
                {/if}
              </div>
            </section>
            <section class="group">
              <h3>{t("set.autofillTitle")}</h3>
              <div class="grid">
                <input placeholder={t("set.autofillNamePh")} bind:value={autofillProfile.name} />
                <input placeholder={t("set.autofillEmailPh")} bind:value={autofillProfile.email} />
                <input placeholder={t("set.autofillPhonePh")} bind:value={autofillProfile.phone} />
                <input placeholder={t("set.autofillAddrPh")} bind:value={autofillProfile.address} />
                <input placeholder={t("set.autofillZipPh")} bind:value={autofillProfile.postalCode} />
                <button class="primary" type="button" onclick={saveProfileForm}>{t("set.saveProfileBtn")}</button>
              </div>
            </section>
          </div>
        {:else}
          <section class="group">
            <h3>{t("set.trackingGroup")}</h3>
            <div class="tracking-grid" role="radiogroup" aria-label={t("set.trackingGroup")}>
              {#each ([
                ["basic", t("set.trackingBasic"), t("set.trackingBasicDesc")],
                ["balanced", t("set.trackingBalanced"), t("set.trackingBalancedDesc")],
                ["strict", t("set.trackingStrict"), t("set.trackingStrictDesc")],
              ] as const) as [level, label, desc] (level)}
                <button
                  type="button"
                  role="radio"
                  aria-checked={draft.trackingPrevention === level}
                  class="tracking-card"
                  class:selected={draft.trackingPrevention === level}
                  onclick={() => {
                    draft.trackingPrevention = level;
                    void persist(false);
                  }}
                >
                  <div class="tracking-head">
                    <strong>{label}</strong>
                    {#if draft.trackingPrevention === level}
                      <span class="tracking-badge">{t("set.selectedBadge")}</span>
                    {/if}
                  </div>
                  <small>{desc}</small>
                </button>
              {/each}
            </div>
          </section>

          <section class="group">
            <h3>{t("set.vaultGroup")}</h3>
            <button type="button" class="folder-nav-row" onclick={() => openSubpage("cookies")}>
              <div>
                <strong>{t("set.cookiesManage")}</strong>
                <small>{t("set.cookiesManageDesc")}</small>
              </div>
              <span class="chevron">›</span>
            </button>
            <button type="button" class="folder-nav-row" onclick={() => openSubpage("passwords")}>
              <div>
                <strong>{t("set.passwordsManage")}</strong>
                <small>{t("set.passwordsManageDesc", { count: savedLogins.length })}</small>
              </div>
              <span class="chevron">›</span>
            </button>
          </section>

          <section class="group">
            <h3>{t("set.importBrowsers")}</h3>
            <div class="compact-list">
              {#each externalBrowsers as b (b.id)}
                <div class="list-item">
                  <div class="list-meta">
                    <strong>{b.label}</strong>
                    <small>{b.hasBookmarks ? t("set.extHasBm") : t("set.extNoBm")} · {b.hasHistory ? t("set.extHasHist") : t("set.extNoHist")}</small>
                  </div>
                  <button
                    class="ghost small-btn"
                    type="button"
                    disabled={importBusy || (!b.hasBookmarks && !b.hasHistory)}
                    onclick={() => void importFromBrowser(b)}
                  >
                    {importBusy ? t("set.importingBtn") : t("set.importBtn")}
                  </button>
                </div>
              {/each}
              {#if externalBrowsers.length === 0}
                <small>{t("set.extEmpty")}</small>
              {/if}
            </div>
          </section>
        {/if}

      {:else if activeSection === "performance"}
        {#if activeSubpage === "sleep-exceptions"}
          <div class="subpage">
            <button type="button" class="subpage-back" onclick={closeSubpage}>
              ← {t("set.secPerformance")}
            </button>
            <section class="group">
              <h3>{t("set.sleepExTitle")}</h3>
              <p class="note">{t("set.sleepExNote")}</p>
              <div class="row">
                <input placeholder={t("set.sleepExPh")} bind:value={newSleepHost} />
                <button
                  class="ghost"
                  type="button"
                  onclick={() => {
                    draft.sleepAllowlist = pushHost(draft.sleepAllowlist, newSleepHost);
                    newSleepHost = "";
                    void persist(false);
                  }}
                >
                  {t("set.addExceptionBtn")}
                </button>
              </div>
              <div class="compact-list" style="margin-top:12px">
                {#each draft.sleepAllowlist as host, idx (`${idx}-${host}`)}
                  <div class="list-item">
                    <div class="list-meta"><strong>{host}</strong></div>
                    <button
                      class="ghost small-btn"
                      type="button"
                      onclick={() => {
                        draft.sleepAllowlist = draft.sleepAllowlist.filter((h) => h !== host);
                        void persist(false);
                      }}
                    >
                      {t("common.delete")}
                    </button>
                  </div>
                {/each}
                {#if draft.sleepAllowlist.length === 0}
                  <small>{t("set.sleepExEmpty")}</small>
                {/if}
              </div>
            </section>
          </div>
        {:else if activeSubpage === "extensions"}
          <div class="subpage">
            <button type="button" class="subpage-back" onclick={closeSubpage}>
              ← {t("set.secPerformance")}
            </button>
            <section class="group">
              <h3>{t("set.extFolderTitle")}</h3>
              <div class="row" style="margin:8px 0">
                <input
                  spellcheck="false"
                  placeholder={t("set.extFolderPh")}
                  bind:value={extensionFolderInput}
                />
                <button class="ghost" type="button" onclick={installExtensionFolder} disabled={extensionBusy}>
                  {extensionBusy ? t("set.extInstalling") : t("set.extLoadBtn")}
                </button>
              </div>
              <div class="compact-list">
                {#each webExtensions as ext (ext.id)}
                  <div class="list-item">
                    <div class="list-meta">
                      <strong>{ext.name}</strong>
                      <small>{ext.enabled ? t("common.enabled") : t("common.disabled")} · {ext.id}</small>
                    </div>
                    <div class="row">
                      <button class="ghost small-btn" type="button" onclick={() => void toggleExtension(ext, false)}>
                        {ext.enabled ? t("common.disable") : t("common.enable")}
                      </button>
                      <button class="ghost small-btn" type="button" onclick={() => void toggleExtension(ext, true)}>{t("common.delete")}</button>
                    </div>
                  </div>
                {/each}
                {#if webExtensions.length === 0}
                  <small>{t("set.extNone")}</small>
                {/if}
              </div>
            </section>
            <section class="group">
              <h3>{t("set.userScriptsTitle")}</h3>
              <div class="grid">
                <input placeholder={t("set.scriptNamePh")} bind:value={newScriptName} />
                <input placeholder={t("set.scriptMatchPh")} bind:value={newScriptMatch} />
              </div>
              <div class="grid" style="margin-top:6px">
                <input placeholder={t("set.scriptJsPh")} bind:value={newScriptCode} />
                <input placeholder={t("set.scriptCssPh")} bind:value={newScriptCss} />
              </div>
              <div class="row end" style="margin-top:6px">
                <button class="ghost" type="button" onclick={addUserScript}>{t("set.scriptAddBtn")}</button>
              </div>
              <div class="compact-list">
                {#each userScripts as s (s.id)}
                  <div class="list-item">
                    <div class="list-meta">
                      <strong>{s.name} <small>({s.matchPattern})</small></strong>
                      <small>{s.enabled ? t("set.scriptRunning") : t("set.scriptStopped")}</small>
                    </div>
                    <div class="row">
                      <button class="ghost small-btn" type="button" onclick={() => void toggleUserScript(s)}>
                        {s.enabled ? t("common.disable") : t("common.enable")}
                      </button>
                      <button class="ghost small-btn" type="button" onclick={() => void removeUserScript(s.id)}>{t("common.delete")}</button>
                    </div>
                  </div>
                {/each}
              </div>
            </section>
          </div>
        {:else}
          <section class="group">
            <h3>{t("set.perfGroup")}</h3>
            <label class="check-field">
              <input type="checkbox" bind:checked={draft.hardwareAcceleration} />
              <span>{t("set.hwAccelLabel")}</span>
            </label>
            <label class="check-field">
              <input type="checkbox" bind:checked={draft.sleepingTabsEnabled} />
              <span>{t("set.sleepTabsLabel")}</span>
            </label>
            <div class="grid">
              <label class="field">
                <span>{t("set.sleepMinsLabel")}</span>
                <input type="number" min="1" max="1440" bind:value={draft.sleepTimeoutMinutes} />
              </label>
              <label class="field">
                <span>{t("set.maxAwakeLabel")}</span>
                <input type="number" min="1" max="50" bind:value={draft.maxAwakeTabs} />
              </label>
            </div>
            <button type="button" class="folder-nav-row" onclick={() => openSubpage("sleep-exceptions")}>
              <div>
                <strong>{t("set.sleepExManage")}</strong>
                <small>{t("set.sleepExCount", { count: draft.sleepAllowlist.length })}</small>
              </div>
              <span class="chevron">›</span>
            </button>
          </section>

          <section class="group">
            <h3>{t("set.extGroup")}</h3>
            <label class="check-field">
              <input type="checkbox" bind:checked={draft.browserExtensionsEnabled} />
              <span>{t("set.webExtEnable")}</span>
            </label>
            <button type="button" class="folder-nav-row" onclick={() => openSubpage("extensions")}>
              <div>
                <strong>{t("set.extManage")}</strong>
                <small>{t("set.extCountSummary", { ext: webExtensions.length, scripts: userScripts.length })}</small>
              </div>
              <span class="chevron">›</span>
            </button>
          </section>
        {/if}

      {:else if activeSection === "translation"}
      <section class="group">
        <h3>{t("set.trGroup")}</h3>
        <div class="grid">
          <label class="field">
            <span>{t("set.trTargetLang")}</span>
            <input bind:value={draft.targetLanguage} placeholder="한국어" />
          </label>
          <label class="field">
            <span>{t("set.trStyle")}</span>
            <select
              bind:value={draft.style}
              onchange={() => {
                void api
                  .setTranslationPrefs(draft.style)
                  .then((view) => onsaved(view))
                  .catch((e) => onnotify("error", errorMessage(e)));
              }}
            >
              <option value="auto">{t("translate.styleAuto")}</option>
              <option value="natural">{t("translate.styleNatural")}</option>
              <option value="novel">{t("translate.styleNovel")}</option>
              <option value="literal">{t("translate.styleLiteral")}</option>
              <option value="polished">{t("translate.stylePolished")}</option>
            </select>
          </label>
          <label class="field">
            <span>{t("set.trRateHour")}</span>
            <input type="number" min="0" max="10000" bind:value={draft.requestsPerHour} />
          </label>
          <label class="field">
            <span>{t("set.trRateDay")}</span>
            <input type="number" min="0" max="100000" bind:value={draft.requestsPerDay} />
          </label>
          <label class="field">
            <span>{t("set.trMinInterval")}</span>
            <input type="number" min="0" max="60000" step="50" bind:value={draft.minimumRequestIntervalMs} />
          </label>
          <label class="field">
            <span>{t("set.trFontScale")}</span>
            <input type="number" min="0.8" max="1.6" step="0.05" bind:value={draft.translationFontScale} />
          </label>
          <label class="field">
            <span>{t("set.trLineHeight")}</span>
            <input type="number" min="1.2" max="2.2" step="0.1" bind:value={draft.translationLineHeight} />
          </label>
        </div>
        <small>{t("set.trRateHelp")}</small>
      </section>

      {:else if activeSection === "appearance"}
      <section class="group">
        <h3>{t("set.appLangGroup")}</h3>
        <span style="display:block;font-size:12px;color:var(--text-muted);margin-bottom:8px">
          {t("set.appLangLabel")}
        </span>
        <div class="lang-grid" role="radiogroup" aria-label={t("set.appLangLabel")}>
          {#each UI_LANGUAGES as lang (lang.code)}
            <button
              type="button"
              role="radio"
              aria-checked={getUiLang() === lang.code}
              class="lang-pill"
              class:selected={getUiLang() === lang.code}
              onclick={() => void changeUiLanguage(lang.code)}
            >
              <strong>{lang.nativeName}</strong>
              <small>{lang.shortLabel}</small>
            </button>
          {/each}
        </div>
        <small style="margin-top:8px;display:block">{t("set.appLangHelp")}</small>
      </section>

      <section class="group">
        <h3>{t("set.themeGroup")}</h3>
        <label class="field"><span>{t("set.themeLabel")}</span>
          <select
            bind:value={draft.theme}
            onchange={() => {
              void persist(false);
            }}
          >
            <option value="system">{t("set.themeSystem")}</option>
            <option value="light">{t("set.themeLight")}</option>
            <option value="dark">{t("set.themeDark")}</option>
            <option value="sepia">{t("set.themeSepia")}</option>
          </select>
          <small>{t("set.themeHelp")}</small>
        </label>
        <label class="check-field">
          <input
            type="checkbox"
            bind:checked={themeAutoSync}
            onchange={broadcastUiPrefs}
          />
          <span>{t("set.themeAutoSync")}</span>
        </label>
        <small>{t("set.themeAutoSyncHelp")}</small>
      </section>

      <section class="group">
        <h3>{t("set.accentGroup")}</h3>
        <div class="swatch-row">
          {#each accentPresets as preset (preset.value)}
            <button
              type="button"
              class="color-swatch"
              class:selected={accentColor.toLowerCase() === preset.value.toLowerCase()}
              style:background={preset.value}
              title={`${preset.name} (${preset.value})`}
              aria-label={preset.name}
              onclick={() => setAccentColor(preset.value)}
            ></button>
          {/each}
          <label class="color-picker-wrap" title={t("set.accentCustom", { color: accentColor })}>
            <input
              type="color"
              value={accentColor}
              oninput={(e) => setAccentColor(e.currentTarget.value)}
            />
            <span>{t("set.accentCustom", { color: accentColor })}</span>
          </label>
        </div>
        <small>{t("set.accentHelp")}</small>
      </section>

      <section class="group">
        <h3>{t("set.glassGroup")}</h3>
        <label class="check-field">
          <input
            type="checkbox"
            bind:checked={glassTinted}
            onchange={onToggleGlassTinted}
          />
          <span>{t("set.glassCheck")}</span>
        </label>
        <small>{t("set.glassHelp")}</small>
      </section>

      {:else}
      <section class="group">
        <h3>{t("set.dataBackupGroup")}</h3>
        <p class="note">{t("set.dataBackupNote")}</p>
        <div class="row">
          <button class="ghost" type="button" onclick={exportData} disabled={exportingData}>
            {exportingData ? t("set.exportingBtn") : t("set.exportAllBtn")}
          </button>
          <button
            class="ghost"
            type="button"
            onclick={() => dataFileInput?.click()}
            disabled={importingData}
          >
            {importingData ? t("set.importingBtn") : t("set.importDataBtn")}
          </button>
          <input
            class="hidden-file"
            bind:this={dataFileInput}
            type="file"
            accept="application/json,.json"
            onchange={importData}
          />
        </div>
        <small>{t("set.dataBackupHelp")}</small>
      </section>

      <section class="group">
        <h3>{t("set.feedbackGroup")}</h3>
        <p class="note">{t("set.feedbackNote")}</p>
        <div class="row" style="flex-wrap:wrap;margin-bottom:10px">
          <button
            class="primary"
            type="button"
            onclick={() => void api.openOverlay("feedback").catch((e) => onnotify("error", errorMessage(e)))}
          >
            {t("set.feedbackOpenBtn")}
          </button>
          <button class="ghost" type="button" onclick={copyTesterDiagnostics} disabled={exportingDiagnostics}>
            {exportingDiagnostics ? t("set.diagCollecting") : t("set.diagCopyBtn")}
          </button>
          <button class="ghost" type="button" onclick={saveTesterDiagnostics} disabled={exportingDiagnostics}>
            {exportingDiagnostics ? t("set.diagCollecting") : t("set.diagSaveBtn")}
          </button>
        </div>
        <label class="field">
          <span>{t("set.feedbackRelayLabel")}</span>
          <input
            type="url"
            spellcheck="false"
            placeholder="https://your-worker.workers.dev/feedback"
            bind:value={draft.feedbackEndpointUrl}
          />
          <small>{t("set.feedbackRelayHelp")}</small>
        </label>
      </section>
      {/if}
      </div>
    </div>

    <footer class="panel-foot">
      {#if !embedded}
        <button class="ghost" type="button" onclick={onclose}>{t("common.cancel")}</button>
      {/if}
      <button class="primary" type="button" onclick={saveAndClose} disabled={saving}>
        {saving ? t("common.saving") : t("common.save")}
      </button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 40;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 24px;
    background: rgba(10, 12, 16, 0.45);
    backdrop-filter: blur(2px);
  }

  .backdrop.embedded {
    padding: 0;
    background: var(--surface);
    backdrop-filter: none;
  }

  .panel {
    display: flex;
    flex-direction: column;
    width: min(760px, 100%);
    height: min(640px, 86vh);
    border: 1px solid var(--border);
    border-radius: 16px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 24px 80px rgba(0, 0, 0, 0.35);
    overflow: hidden;
  }

  .panel.embedded {
    width: 100%;
    height: 100%;
    border: 0;
    border-radius: 0;
    box-shadow: none;
  }

  .panel-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 16px;
    padding: 18px 22px 14px;
    border-bottom: 1px solid var(--border);
  }

  .panel-head h2 {
    margin: 0;
    font-size: 17px;
  }

  .panel-head p {
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--text-muted);
  }

  .icon-button {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border: none;
    border-radius: 8px;
    background: transparent;
    color: var(--text-muted);
    font-size: 20px;
    line-height: 1;
    cursor: pointer;
  }

  .icon-button:hover {
    background: var(--surface-hover);
    color: var(--text);
  }

  .panel-body {
    display: grid;
    grid-template-columns: 160px minmax(0, 1fr);
    gap: 20px;
    flex: 1;
    min-height: 0;
    overflow: hidden;
    padding: 18px 22px 8px;
  }

  .settings-nav { display: flex; flex-direction: column; gap: 4px; border-right: 1px solid var(--border); padding-right: 12px; }
  .settings-nav button { height: 34px; padding: 0 10px; border: 0; border-radius: 7px; background: transparent; color: var(--text-muted); text-align: left; font-size: 12px; cursor: pointer; }
  .settings-nav button:hover { background: var(--surface-hover); color: var(--text); }
  .settings-nav button.active { background: var(--accent-soft); color: var(--accent); font-weight: 600; }
  .settings-content { min-width: 0; min-height: 0; overflow-y: auto; padding-right: 4px; }

  .group {
    margin-bottom: 22px;
    padding-bottom: 18px;
    border-bottom: 1px solid var(--border);
  }

  .group:last-child {
    border-bottom: none;
  }

  .group h3 {
    margin: 0 0 12px;
    font-size: 12px;
    letter-spacing: 0.08em;
    text-transform: uppercase;
    color: var(--text-muted);
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin-bottom: 12px;
    font-size: 12.5px;
  }

  .field > span {
    color: var(--text-muted);
  }

  .field small {
    color: var(--text-soft);
    font-size: 11px;
    line-height: 1.5;
  }

  .note {
    margin: 0 0 12px;
    color: var(--text-soft);
    font-size: 11.5px;
    line-height: 1.55;
  }

  .hidden-file {
    display: none;
  }

  input,
  select {
    width: 100%;
    height: 34px;
    padding: 0 10px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
    font-size: 13px;
    outline: none;
  }

  input:focus,
  select:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  input[type="checkbox"] {
    width: 16px;
    height: 16px;
    margin: 0;
    accent-color: var(--accent);
  }

  .check-field {
    display: flex;
    align-items: center;
    gap: 9px;
    margin-bottom: 6px;
    font-size: 12.5px;
  }

  .swatch-row {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin-bottom: 8px;
  }

  .color-swatch {
    width: 28px;
    height: 28px;
    padding: 0;
    border-radius: 50% !important;
    border: 2px solid rgba(255, 255, 255, 0.78);
    box-shadow: 0 2px 8px rgba(0, 0, 0, 0.18);
    cursor: pointer;
    transition: transform 0.16s ease, box-shadow 0.16s ease;
  }

  .color-swatch:hover {
    transform: scale(1.12);
  }

  .color-swatch.selected {
    transform: scale(1.15);
    box-shadow: 0 0 0 3px var(--accent-soft), 0 4px 12px rgba(0, 0, 0, 0.25);
    border-color: #ffffff;
  }

  .color-picker-wrap {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    height: 30px;
    padding: 0 11px 0 6px;
    border: 1px solid var(--glass-border, var(--border-strong));
    border-radius: 999px;
    background: var(--glass-pill-active, var(--surface-2));
    font-size: 11.5px;
    color: var(--text);
    cursor: pointer;
  }

  .color-picker-wrap input[type="color"] {
    width: 20px;
    height: 20px;
    padding: 0 !important;
    border: 0;
    border-radius: 50% !important;
    background: transparent !important;
    cursor: pointer;
  }

  input[type="number"] {
    max-width: 140px;
  }

  .row {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .row.end {
    justify-content: flex-end;
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(200px, 1fr));
    gap: 4px 16px;
  }

  button {
    font: inherit;
  }

  .ghost,
  .primary {
    height: 34px;
    padding: 0 14px;
    border-radius: 8px;
    font-size: 12.5px;
    white-space: nowrap;
    cursor: pointer;
  }

  .ghost {
    border: 1px solid var(--border-strong);
    background: var(--surface-2);
    color: var(--text);
  }

  .ghost:hover:not(:disabled) {
    border-color: var(--accent);
    color: var(--accent);
  }

  .primary {
    border: 1px solid transparent;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }

  .primary:hover:not(:disabled) {
    filter: brightness(1.06);
  }

  button:disabled {
    opacity: 0.55;
    cursor: default;
  }

  .panel-foot button {
    min-width: 96px;
    display: inline-flex;
    align-items: center;
    justify-content: center;
  }

  .panel-foot {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
    padding: 14px 22px;
    border-top: 1px solid var(--border);
    background: var(--surface-2);
  }

  @media (max-width: 640px) {
    .panel-body { grid-template-columns: 1fr; gap: 8px; }
    .settings-nav { flex-direction: row; flex-wrap: wrap; border-right: 0; border-bottom: 1px solid var(--border); padding: 0 0 8px; }
  }

  .test-result {
    display: flex;
    gap: 8px;
    align-items: baseline;
    margin-top: 10px;
    padding: 9px 12px;
    border-radius: 8px;
    background: var(--err-soft, #fdeceb);
    color: var(--err, #b42318);
    font-size: 12px;
    line-height: 1.5;
  }

  .test-result.ok {
    background: var(--ok-soft, #e8f5ec);
    color: var(--ok, #1f7a3f);
  }

  .test-result strong {
    flex: 0 0 auto;
  }

  .profile-hero {
    display: flex;
    align-items: center;
    gap: 14px;
    padding: 12px 14px;
    margin-bottom: 14px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface-2);
  }
  .avatar {
    display: grid;
    place-items: center;
    width: 44px;
    height: 44px;
    border-radius: 50%;
    background: var(--accent);
    color: #fff;
    font-size: 18px;
    font-weight: 700;
    flex: 0 0 auto;
  }
  .profile-meta {
    display: grid;
    gap: 2px;
    flex: 1;
    min-width: 0;
  }
  .profile-meta strong { font-size: 14px; }
  .profile-meta span, .profile-meta small { font-size: 11.5px; color: var(--text-muted); }
  .profile-meta .ok-text { color: #22c55e; font-weight: 600; }

  .sync-list {
    display: flex;
    flex-direction: column;
  }
  .sync-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 14px;
    padding: 10px 4px;
    border-top: 1px solid var(--border);
  }
  .sync-row:first-child { border-top: 0; }
  .sync-row strong { display: block; font-size: 13px; margin-bottom: 2px; }
  .sync-row small { display: block; font-size: 11.5px; color: var(--text-muted); }

  .toggle-switch {
    position: relative;
    width: 42px;
    height: 23px;
    padding: 0;
    border: 1px solid var(--border-strong);
    border-radius: 999px;
    background: var(--surface-2);
    cursor: pointer;
    flex: 0 0 auto;
    transition: background 0.18s ease, border-color 0.18s ease;
  }
  .toggle-switch.on {
    background: var(--accent);
    border-color: var(--accent);
  }
  .toggle-switch .knob {
    position: absolute;
    top: 2px;
    left: 2px;
    width: 17px;
    height: 17px;
    border-radius: 50%;
    background: #fff;
    box-shadow: 0 1px 3px rgba(0, 0, 0, 0.25);
    transition: transform 0.18s cubic-bezier(0.22, 1, 0.36, 1);
  }
  .toggle-switch.on .knob {
    transform: translateX(19px);
  }

  .chip-list {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 8px;
  }
  .chip {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    height: 26px;
    padding: 0 10px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface-2);
    font-size: 11.5px;
  }
  .chip button {
    border: 0;
    background: transparent;
    color: var(--text-muted);
    cursor: pointer;
    font-size: 14px;
    padding: 0;
    line-height: 1;
  }

  .compact-list {
    display: grid;
    gap: 6px;
    margin-top: 8px;
    max-height: 210px;
    overflow: auto;
  }
  .list-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    padding: 8px 10px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-2);
  }
  .list-meta {
    min-width: 0;
    display: grid;
    gap: 2px;
  }
  .list-meta strong, .list-meta small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .list-meta strong { font-size: 12.5px; }
  .list-meta small { font-size: 11px; color: var(--text-muted); }
  .small-btn {
    height: 28px !important;
    padding: 0 10px !important;
    font-size: 11.5px !important;
  }

  .avatar.google { background: linear-gradient(135deg, #4285f4, #34a853); }
  .avatar.microsoft { background: linear-gradient(135deg, #0078d4, #00bcf2); }

  .account-provider-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
  }
  .provider-card {
    display: flex;
    align-items: center;
    gap: 11px;
    padding: 11px 12px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface-2);
    color: var(--text);
    text-align: left;
    cursor: pointer;
    transition: transform 0.2s cubic-bezier(0.2, 1.25, 0.35, 1), border-color 0.2s ease, box-shadow 0.2s ease;
  }
  .provider-card:hover { transform: translateY(-2px); border-color: var(--accent); }
  .provider-card.selected {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .provider-card strong { display: block; font-size: 13px; margin-bottom: 2px; }
  .provider-card small { display: block; font-size: 11px; color: var(--text-muted); }
  .provider-badge {
    display: grid;
    place-items: center;
    width: 30px;
    height: 30px;
    border-radius: 50%;
    color: #fff;
    font-weight: 800;
    font-size: 14px;
    flex: 0 0 auto;
  }
  .provider-badge.google { background: linear-gradient(135deg, #4285f4, #ea4335); }
  .provider-badge.microsoft { background: linear-gradient(135deg, #0078d4, #00bcf2); }

  .tracking-grid {
    display: grid;
    grid-template-columns: repeat(3, minmax(0, 1fr));
    gap: 10px;
  }
  .tracking-card {
    display: flex;
    flex-direction: column;
    gap: 6px;
    padding: 12px;
    border: 1.5px solid var(--border);
    border-radius: 12px;
    background: var(--surface-2);
    color: var(--text);
    text-align: left;
    cursor: pointer;
    transition: transform 0.2s cubic-bezier(0.2, 1.25, 0.35, 1), border-color 0.2s ease, box-shadow 0.2s ease;
  }
  .tracking-card:hover { transform: translateY(-2px); border-color: var(--accent); }
  .tracking-card.selected {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 8%, var(--surface-2));
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .tracking-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 6px;
  }
  .tracking-head strong { font-size: 13px; }
  .tracking-badge {
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--accent);
    color: #fff;
    font-size: 10px;
    font-weight: 700;
  }
  .tracking-card small { font-size: 11px; color: var(--text-muted); line-height: 1.45; }

  .folder-nav-row {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 12px;
    width: 100%;
    margin-top: 8px;
    padding: 11px 14px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface-2);
    color: var(--text);
    text-align: left;
    cursor: pointer;
    transition: transform 0.2s cubic-bezier(0.2, 1.25, 0.35, 1), border-color 0.2s ease, background 0.2s ease;
  }
  .folder-nav-row:hover {
    border-color: var(--accent);
    background: var(--surface-hover);
    transform: translateX(3px);
  }
  .folder-nav-row strong { display: block; font-size: 13px; margin-bottom: 2px; }
  .folder-nav-row small { display: block; font-size: 11.5px; color: var(--text-muted); }
  .folder-nav-row .chevron { font-size: 20px; color: var(--text-muted); line-height: 1; }

  @keyframes subpageSlideIn {
    0% { opacity: 0; transform: translateX(18px); }
    100% { opacity: 1; transform: translateX(0); }
  }
  .subpage {
    animation: subpageSlideIn 0.24s cubic-bezier(0.16, 1, 0.3, 1);
  }
  .subpage-back {
    display: inline-flex;
    align-items: center;
    gap: 6px;
    margin-bottom: 12px;
    padding: 6px 12px;
    border: 1px solid var(--border);
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--text);
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
    transition: transform 0.18s ease, border-color 0.18s ease;
  }
  .subpage-back:hover { border-color: var(--accent); transform: translateX(-2px); }

  .hy-panel {
    display: flex;
    flex-direction: column;
    gap: 12px;
    margin-bottom: 12px;
  }
  .hy-hw-card {
    padding: 12px 14px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: color-mix(in srgb, var(--surface-2) 82%, transparent);
    backdrop-filter: blur(18px) saturate(1.4);
  }
  .hy-hw-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    margin-bottom: 6px;
  }
  .hy-hw-head strong { font-size: 13px; }
  .hy-hw-sub { font-size: 12px; color: var(--text-muted); margin-top: 2px; }
  .hy-rec { font-size: 12px; color: var(--accent); font-weight: 600; }
  .hy-badge {
    padding: 3px 9px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 700;
    background: var(--surface-hover);
    border: 1px solid var(--border);
    white-space: nowrap;
  }
  .hy-badge.gpu {
    background: color-mix(in srgb, var(--accent) 16%, transparent);
    border-color: color-mix(in srgb, var(--accent) 45%, transparent);
    color: var(--accent);
  }
  .hy-models {
    display: flex;
    flex-direction: column;
    gap: 8px;
  }
  .hy-models-head {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
  }
  .hy-models-head strong { font-size: 13px; }
  .hy-model-row {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 12px;
    background: var(--surface-2);
    transition: border-color 0.2s ease, background 0.2s ease;
  }
  .hy-model-row.selected {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 7%, var(--surface-2));
  }
  .hy-model-main {
    flex: 1 1 auto;
    min-width: 0;
    display: flex;
    flex-direction: column;
    gap: 4px;
  }
  .hy-model-main small { font-size: 11.5px; color: var(--text-muted); }
  .hy-model-main small.hy-note { color: #f59e0b; }
  .hy-model-title {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 6px;
  }
  .hy-pick-btn {
    display: inline-flex;
    align-items: center;
    gap: 7px;
    padding: 0;
    border: 0;
    background: transparent;
    color: var(--text);
    cursor: pointer;
    font: inherit;
    text-align: left;
  }
  .hy-pick-btn strong { font-size: 13px; }
  .hy-radio {
    width: 14px;
    height: 14px;
    border-radius: 50%;
    border: 2px solid var(--border);
    display: inline-block;
    flex: 0 0 auto;
    position: relative;
  }
  .hy-radio.checked {
    border-color: var(--accent);
    box-shadow: inset 0 0 0 3px var(--accent);
  }
  .hy-pill {
    padding: 1px 7px;
    border-radius: 999px;
    font-size: 10.5px;
    font-weight: 700;
    background: var(--surface-hover);
    color: var(--text-muted);
  }
  .hy-pill.rec {
    background: color-mix(in srgb, var(--accent) 18%, transparent);
    color: var(--accent);
  }
  .hy-pill.ok {
    background: color-mix(in srgb, #22c55e 18%, transparent);
    color: #22c55e;
  }
  .hy-pill.live {
    background: color-mix(in srgb, #06b6d4 20%, transparent);
    color: #06b6d4;
  }
  .hy-model-actions { flex: 0 0 auto; display: flex; gap: 6px; }
  .hy-prog-wrap { margin-top: 4px; display: flex; flex-direction: column; gap: 3px; }
  .hy-prog-bar {
    height: 6px;
    border-radius: 999px;
    background: var(--surface-hover);
    overflow: hidden;
  }
  .hy-prog-fill {
    height: 100%;
    border-radius: 999px;
    background: linear-gradient(90deg, var(--accent), #38bdf8);
    transition: width 0.16s ease;
  }
  .hy-prog-meta {
    display: flex;
    justify-content: space-between;
    font-size: 11px;
    color: var(--text-muted);
  }
  .lang-grid {
    display: grid;
    grid-template-columns: repeat(4, minmax(0, 1fr));
    gap: 8px;
  }
  @media (max-width: 640px) {
    .lang-grid {
      grid-template-columns: repeat(2, minmax(0, 1fr));
    }
  }
  .lang-pill {
    display: flex;
    flex-direction: column;
    align-items: flex-start;
    gap: 2px;
    padding: 10px 12px;
    border: 1.5px solid var(--border);
    border-radius: 12px;
    background: var(--surface-2);
    color: var(--text);
    text-align: left;
    cursor: pointer;
    font: inherit;
    transition: border-color 0.16s ease, box-shadow 0.16s ease, transform 0.16s ease;
  }
  .lang-pill:hover {
    border-color: var(--accent);
    transform: translateY(-1px);
  }
  .lang-pill.selected {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 10%, var(--surface-2));
    box-shadow: 0 0 0 3px var(--accent-soft);
  }
  .lang-pill strong { font-size: 13px; }
  .lang-pill small { font-size: 11px; color: var(--text-muted); }
  .manual-sync-box {
    padding: 10px 12px;
    border: 1px dashed var(--border);
    border-radius: 12px;
    background: var(--surface-2);
  }
  .inline-confirm-bar {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
    padding: 9px 12px;
    margin-bottom: 10px;
    border-radius: 10px;
    border: 1px solid color-mix(in srgb, #ef4444 45%, var(--border));
    background: color-mix(in srgb, #ef4444 10%, var(--surface-2));
    font-size: 12px;
  }
  .danger {
    border: 1px solid color-mix(in srgb, #ef4444 55%, var(--border));
    border-radius: 999px;
    background: color-mix(in srgb, #ef4444 16%, transparent);
    color: #ef4444;
    padding: 6px 12px;
    font: inherit;
    font-weight: 600;
    cursor: pointer;
  }
</style>
