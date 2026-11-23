export type ProviderKind = "googleGemini" | "openAiCompatible" | "codexLegacy" | "localBridge" | "hyMt2";
export type UiLanguage = "ko" | "en" | "ja" | "zh-CN";

export type DisplayMode = "original" | "translation" | "both";
export type ReaderTheme = "light" | "sepia" | "dark";
export type TranslationStyle = "auto" | "natural" | "literal" | "polished" | "novel";

export interface AppSettings {
  searchEngine: "google" | "bing" | "duckduckgo";
  startupMode: "restore" | "custom" | "search";
  homeUrl: string;
  provider: ProviderKind;
  baseUrl: string;
  model: string;
  adBlockEnabled: boolean;
  adBlockAllowlist: string[];
  trackingPrevention: "basic" | "balanced" | "strict";
  popupPolicy: "block" | "block-all" | "allow";
  popupAllowedOrigins: string[];
  hardwareAcceleration: boolean;
  sleepingTabsEnabled: boolean;
  sleepTimeoutMinutes: number;
  maxAwakeTabs: number;
  sleepAllowlist: string[];
  browserExtensionsEnabled: boolean;
  targetLanguage: string;
  uiLanguage: UiLanguage;
  style: string;
  theme: "system" | "light" | "dark" | "sepia" | string;
  requestsPerHour: number;
  requestsPerDay: number;
  minimumRequestIntervalMs: number;
  translationFontScale: number;
  translationLineHeight: number;
  hyMt2Tier: string;
  hyMt2GpuLayers: number;
  hyMt2CtxSize: number;
  hyMt2IdleUnloadSecs: number;
  hyMt2Threads: number;
  feedbackEndpointUrl?: string;
  onboardingCompleted: boolean;
}

export function defaultSettings(): AppSettings {
  return {
    searchEngine: "google",
    startupMode: "restore",
    homeUrl: "",
    provider: "googleGemini",
    baseUrl: "",
    model: "",
    adBlockEnabled: true,
    adBlockAllowlist: [],
    trackingPrevention: "balanced",
    popupPolicy: "block",
    popupAllowedOrigins: [],
    hardwareAcceleration: true,
    sleepingTabsEnabled: true,
    sleepTimeoutMinutes: 15,
    maxAwakeTabs: 6,
    sleepAllowlist: [],
    browserExtensionsEnabled: true,
    targetLanguage: "한국어",
    uiLanguage: "ko",
    style: "auto",
    theme: "system",
    requestsPerHour: 60,
    requestsPerDay: 500,
    minimumRequestIntervalMs: 250,
    translationFontScale: 1.0,
    translationLineHeight: 1.6,
    hyMt2Tier: "auto",
    hyMt2GpuLayers: -1,
    hyMt2CtxSize: 8192,
    hyMt2IdleUnloadSecs: 60,
    hyMt2Threads: 0,
    feedbackEndpointUrl: "",
    onboardingCompleted: false,
  };
}

export type FeedbackCategory = "bug" | "ux" | "translation";

export interface FeedbackDraft {
  category: FeedbackCategory;
  summary?: string;
  description?: string;
  pageUrl?: string;
  sourceText?: string;
  translatedText?: string;
}

export interface FeedbackReportInput {
  category: FeedbackCategory;
  summary: string;
  description: string;
  pageUrl?: string | null;
  sourceText?: string | null;
  translatedText?: string | null;
  suggestedTranslation?: string | null;
  contact?: string | null;
  includeDiagnostics: boolean;
}

export interface FeedbackConfigStatus {
  repoUrl: string;
  issuesUrl: string;
  directSendConfigured: boolean;
  endpointSource: string;
}

export interface FeedbackSubmitResult {
  delivered: boolean;
  method: "relay" | "github_issue" | string;
  title: string;
  body: string;
  githubIssueUrl: string;
  issueUrl?: string | null;
  message: string;
}

export interface HardwareProfile {
  totalRamMb: number;
  availableRamMb: number;
  cpuCores: number;
  cpuName?: string | null;
  gpuName: string | null;
  dedicatedVramMb: number;
  sharedVramMb: number;
  isAmdApu?: boolean;
  npuAvailable?: boolean;
  npuName?: string | null;
  vulkanCompiled: boolean;
  vulkanAvailable: boolean;
  activeBackend: string;
  computeDecision?: string;
  recommendedTier: string;
  recommendedModelId: string;
  recommendationReason: string;
  gpuVendor?: string;
  isDiscreteGpu?: boolean;
  selectedGpuDeviceIndex?: number | null;
  cudaCompiled?: boolean;
  cudaAvailable?: boolean;
  detectedGpus?: string[];
}

export interface HyMt2ModelState {
  id: string;
  displayName: string;
  tier: string;
  repo: string;
  filename: string;
  downloadUrl: string;
  expectedBytes: number;
  sha256: string;
  description: string;
  minRamMb: number;
  minVramMb: number;
  recommended: boolean;
  installed: boolean;
  loaded: boolean;
  downloading: boolean;
  filePath: string;
  installedBytes: number;
  partialBytes: number;
  experimentalKernelNote: string | null;
}

export interface ModelDownloadProgress {
  modelId: string;
  status: string;
  downloadedBytes: number;
  totalBytes: number;
  percent: number;
  speedMbps: number;
  message: string | null;
}

export type TranslationStreamEvent =
  | { type: "started"; total: number; backend: string; model: string; detectedStyle?: string | null }
  | { type: "segment"; result: SegmentResult }
  | { type: "progress"; done: number; total: number }
  | { type: "done"; total: number; uncertain: number }
  | { type: "cancelled"; message: string }
  | { type: "error"; message: string };

export interface ViewportOrder {
  seq: number;
  keys: string[];
}

export interface SettingsView {
  settings: AppSettings;
  hasApiKey: boolean;
  hasBridgeToken: boolean;
  hasCodexAuth: boolean;
  codexAccountId?: string | null;
  codexEmail?: string | null;
}

export interface TabState {
  id: string;
  url: string;
  title: string;
  loading: boolean;
  workspaceId: string;
  zoom: number;
  pinned: boolean;
  isPrivate: boolean;
  sleeping: boolean;
  muted: boolean;
}

export interface Workspace { id: string; name: string; }

export interface Bookmark {
  id: string;
  url: string;
  title: string;
  folder: string;
  favorite: boolean;
  createdAt: number;
  updatedAt: number;
}

export interface TranslationHistory {
  id: number;
  url: string;
  title: string;
  source: string;
  translation: string;
  model: string;
  translatedAt: number;
}

export interface HistoryItem {
  id: number;
  url: string;
  title: string;
  visitedAt: number;
}

export interface DownloadItem {
  id: number;
  url: string;
  fileName: string;
  filePath: string;
  status: "in_progress" | "completed" | "failed" | string;
  startedAt: number;
  finishedAt: number | null;
}

export interface FindResult {
  total: number;
  current: number;
}

export interface AddressSuggestion {
  kind: "bookmark" | "history" | "search" | "calc" | "tab";
  title: string;
  url: string;
}

export interface SearchHistoryItem {
  id: number;
  query: string;
  engine: string;
  searchedAt: number;
}

export interface SavedLogin {
  id: string;
  origin: string;
  username: string;
  password: string;
  updatedAt: number;
}

export interface AutofillProfile {
  name: string;
  email: string;
  phone: string;
  address: string;
  postalCode: string;
}

export interface SyncConfig {
  provider: string;
  signedIn: boolean;
  displayName: string;
  email: string;
  syncDir: string;
  lastSyncedAt: number | null;
  syncBookmarks: boolean;
  syncSettings: boolean;
  syncPersonalInfo: boolean;
  syncPasswords: boolean;
  syncHistory: boolean;
  syncOpenTabs: boolean;
  syncExtensions: boolean;
  syncGlossary: boolean;
  syncWorkspaces: boolean;
  syncUsage: boolean;
}

export interface CloudAccountRoot {
  provider: string;
  label: string;
  root: string;
  available: boolean;
}

export interface CookieEntry {
  name: string;
  domain: string;
  path: string;
  valuePreview: string;
  secure: boolean;
  httpOnly: boolean;
  session: boolean;
}

export interface UserScriptItem {
  id: string;
  name: string;
  matchPattern: string;
  code: string;
  css: string;
  enabled: boolean;
  updatedAt: number;
}

export interface WebExtensionInfo {
  id: string;
  name: string;
  enabled: boolean;
  path: string;
}

export interface ExternalImportSummary {
  source: string;
  bookmarks: number;
  history: number;
  logins: number;
}

export interface DetectedBrowserProfile {
  id: string;
  label: string;
  hasBookmarks: boolean;
  hasHistory: boolean;
}

export interface ImportSummary {
  glossary: number;
  bookmarks: number;
  history: number;
}

export interface BrowserState {
  tabs: TabState[];
  activeTab: string | null;
  canGoBack: boolean;
  canGoForward: boolean;
  workspaces: Workspace[];
  activeWorkspace: string;
}

export interface Block {
  id: string;
  kind: string;
  docIndex?: number | null;
  inlineKey?: string | null;
  level?: number | null;
  text: string;
  rows?: string[][] | null;
}

export interface TranslationContextPair {
  source: string;
  translation?: string | null;
}

export interface Article {
  title: string;
  url: string;
  site: string;
  lang?: string | null;
  blocks: Block[];
}

export interface SegmentInput {
  id: string;
  text: string;
}

export interface SegmentResult {
  id: string;
  translation: string;
  uncertain: boolean;
  notes: string[];
  cached: boolean;
}

export interface ModelInfo {
  name: string;
  displayName: string;
}

export interface ConnectionTest {
  ok: boolean;
  message: string;
  model: string;
}

export interface GlossaryEntry {
  id: string;
  source: string;
  reading: string;
  translation: string;
  alternatives: string;
  aliases: string;
  doNotTranslate: boolean;
  caseSensitive: boolean;
  exactMatch: boolean;
  useRegex: boolean;
  enabled: boolean;
  projectId?: string | null;
  note: string;
}

export function emptyGlossaryEntry(): GlossaryEntry {
  return {
    id: crypto.randomUUID(),
    source: "",
    reading: "",
    translation: "",
    alternatives: "",
    aliases: "",
    doNotTranslate: false,
    caseSensitive: false,
    exactMatch: false,
    useRegex: false,
    enabled: true,
    projectId: null,
    note: "",
  };
}
