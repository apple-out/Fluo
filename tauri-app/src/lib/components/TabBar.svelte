<script lang="ts">
  import { t } from "$lib/i18n.svelte";
  import type { TabState } from "$lib/types";

  let {
    tabs,
    activeTab,
    onselect,
    onmovetab,
    onclose,
    onnew,
    onreorder,
    workspaces,
    activeWorkspace,
    onopenworkspacemenu,
    onopentabmenu,
    onmutetab,
    onopentabsearch,
    compact = false,
  }: {
    tabs: TabState[];
    activeTab: string | null;
    onselect: (id: string) => void;
    onmovetab: (tabId: string, workspaceId: string) => void;
    onclose: (id: string) => void;
    onnew: () => void;
    onreorder: (id: string, beforeId: string | null) => void;
    workspaces: Array<{ id: string; name: string }>;
    activeWorkspace: string;
    onopenworkspacemenu: (anchor: { left: number; top: number; right: number; bottom: number }) => void;
    onopentabmenu: (tabId: string, anchor: { left: number; top: number; right: number; bottom: number }) => void;
    onmutetab?: (tabId: string) => void;
    onopentabsearch?: (anchor: { left: number; top: number; right: number; bottom: number }) => void;
    compact?: boolean;
  } = $props();

  let dragId = $state<string | null>(null);
  let dragOverWorkspace = $state<string | null>(null);
  let failedFavicons = $state<Record<string, boolean>>({});

  function faviconUrl(url: string): string | null {
    try {
      const parsed = new URL(url);
      if (parsed.protocol !== "http:" && parsed.protocol !== "https:") return null;
      return `${parsed.origin}/favicon.ico`;
    } catch {
      return null;
    }
  }

  function host(url: string): string {
    try {
      return new URL(url).hostname.replace(/^www\./, "");
    } catch {
      return url;
    }
  }

  function workspaceLabel(w?: { id: string; name: string } | null): string {
    if (!w) return t("tabbar.workspace");
    if (w.id === "default" && w.name === "기본 작업공간") return t("common.defaultWorkspace");
    return w.name;
  }

  function label(tab: TabState): string {
    const title = tab.title.trim();
    return title.length > 0 ? title : host(tab.url);
  }

  function selectByKeyboard(event: KeyboardEvent, id: string) {
    if (event.key === "Enter" || event.key === " ") {
      event.preventDefault();
      onselect(id);
    }
  }

  function openTabMenu(event: MouseEvent, tabId: string) {
    event.preventDefault();
    event.stopPropagation();
    onopentabmenu(tabId, {
      left: event.clientX,
      top: event.clientY,
      right: event.clientX,
      bottom: event.clientY,
    });
  }

  function onDragStart(event: DragEvent, id: string) {
    dragId = id;
    if (event.dataTransfer) {
      event.dataTransfer.effectAllowed = "move";
      event.dataTransfer.setData("text/plain", id);
    }
  }

  function draggedId(event: DragEvent): string {
    return dragId ?? event.dataTransfer?.getData("text/plain") ?? "";
  }

  function onTabDrop(event: DragEvent, beforeId: string) {
    event.preventDefault();
    event.stopPropagation();
    const source = draggedId(event);
    dragId = null;
    if (source && source !== beforeId) onreorder(source, beforeId);
  }

  function onTabsDrop(event: DragEvent) {
    event.preventDefault();
    const source = draggedId(event);
    dragId = null;
    dragOverWorkspace = null;
    if (source) onreorder(source, null);
  }

  function onWorkspaceDrop(event: DragEvent, workspaceId: string) {
    event.preventDefault();
    event.stopPropagation();
    const source = draggedId(event);
    dragId = null;
    dragOverWorkspace = null;
    if (source && workspaceId !== activeWorkspace) {
      onmovetab(source, workspaceId);
    }
  }
</script>

<div class="tabbar" class:compact>
  <button
    class="workspace-select-btn"
    type="button"
    aria-label={t("tabbar.workspace")}
    aria-haspopup="menu"
    title={t("tabbar.workspaceMenuTitle")}
    onclick={(event) => {
      const r = event.currentTarget.getBoundingClientRect();
      onopenworkspacemenu({ left: r.left, top: r.top, right: r.right, bottom: r.bottom });
    }}
  >
    <span class="ws-dot" aria-hidden="true"></span>
    <span>{workspaceLabel(workspaces.find((w) => w.id === activeWorkspace))}</span>
    <span class="caret">▾</span>
  </button>
  {#if dragId && workspaces.length > 1}
    <div class="workspace-targets" aria-label={t("tabbar.moveTabToWorkspace")}>
      {#each workspaces.filter((workspace) => workspace.id !== activeWorkspace) as workspace (workspace.id)}
        <div
          class="workspace-target"
          class:hover={dragOverWorkspace === workspace.id}
          role="button"
          tabindex="-1"
          ondragover={(event) => {
            event.preventDefault();
            dragOverWorkspace = workspace.id;
          }}
          ondragleave={() => {
            if (dragOverWorkspace === workspace.id) dragOverWorkspace = null;
          }}
          ondrop={(event) => onWorkspaceDrop(event, workspace.id)}
        >
          → {workspaceLabel(workspace)}
        </div>
      {/each}
    </div>
  {/if}
  <div
    class="tabs"
    role="tablist"
    aria-label={t("tabbar.openTabs")}
    tabindex="-1"
    ondragover={(event) => event.preventDefault()}
    ondrop={onTabsDrop}
  >
    {#each tabs as tab (tab.id)}
      <div
        class="tab"
        class:active={tab.id === activeTab}
        class:pinned={tab.pinned}
        class:private={tab.isPrivate}
        class:sleeping={tab.sleeping}
        role="tab"
        tabindex="0"
        draggable="true"
        aria-selected={tab.id === activeTab}
        onclick={() => onselect(tab.id)}
        onauxclick={(event) => {
          if (event.button === 1) {
            event.preventDefault();
            onclose(tab.id);
          }
        }}
        onkeydown={(event) => selectByKeyboard(event, tab.id)}
        oncontextmenu={(event) => void openTabMenu(event, tab.id)}
        ondragstart={(event) => onDragStart(event, tab.id)}
        ondragover={(event) => event.preventDefault()}
        ondrop={(event) => onTabDrop(event, tab.id)}
        ondragend={() => {
          dragId = null;
          dragOverWorkspace = null;
        }}
        title={`${tab.isPrivate ? t("tabbar.privateTag") : ""}${tab.url}\n${t("tabbar.tabHint")}`}
      >
        {#if tab.loading}
          <span class="spinner" aria-label={t("chrome.loading")}></span>
        {:else if tab.isPrivate}
          <span class="private-badge" aria-label={t("tabbar.privateTab")}>🕶</span>
        {:else if faviconUrl(tab.url) && !failedFavicons[faviconUrl(tab.url)!]}
          <img
            class="favicon-img"
            src={faviconUrl(tab.url)!}
            alt=""
            onerror={() => {
              const icon = faviconUrl(tab.url);
              if (icon) failedFavicons = { ...failedFavicons, [icon]: true };
            }}
          />
        {:else}
          <span class="favicon">{host(tab.url).slice(0, 1).toUpperCase()}</span>
        {/if}
        {#if !tab.pinned}
          <span class="tab-title">{label(tab)}</span>
        {/if}
        {#if tab.muted}
          <button
            class="tab-state-badge muted"
            type="button"
            aria-label={t("tabbar.unmuteTab")}
            title={t("tabbar.mutedTitle")}
            onclick={(event) => {
              event.stopPropagation();
              onmutetab?.(tab.id);
            }}
          >
            🔇
          </button>
        {/if}
        {#if tab.sleeping}
          <span class="tab-state-badge sleep" title={t("tabbar.sleepingTitle")}>💤</span>
        {/if}
        {#if !tab.pinned}
          <button
            class="tab-menu-trigger"
            type="button"
            aria-label={t("tabbar.tabMenu")}
            title={t("tabbar.tabMenu")}
            onclick={(event) => void openTabMenu(event, tab.id)}
          >
            ⋯
          </button>
          <button
            class="tab-close"
            type="button"
            aria-label={t("tabbar.closeTab")}
            onclick={(event) => {
              event.stopPropagation();
              onclose(tab.id);
            }}
          >
            ×
          </button>
        {/if}
      </div>
    {/each}
  </div>
  <div class="tabbar-actions">
    <button class="new-tab" type="button" aria-label={t("tabbar.newTab")} title={t("tabbar.newTabTitle")} onclick={onnew}>
      +
    </button>
    {#if onopentabsearch}
      <button
        class="tab-search-btn"
        type="button"
        aria-label={t("tabbar.searchTabs")}
        title={t("tabbar.searchTabsTitle")}
        onclick={(event) => {
          const r = event.currentTarget.getBoundingClientRect();
          onopentabsearch({ left: r.left, top: r.top, right: r.right, bottom: r.bottom });
        }}
      >
        <svg viewBox="0 0 16 16" aria-hidden="true">
          <circle cx="7" cy="7" r="4.3" />
          <path d="m10.3 10.3 3 3" />
        </svg>
      </button>
    {/if}
  </div>
</div>

<style>
  .tabbar {
    position: relative;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 6px;
    height: 38px;
    padding: 4px 114px 2px 8px;
    background: transparent;
    -webkit-user-select: none;
    user-select: none;
  }

  @keyframes tabPillIn {
    0% {
      opacity: 0;
      transform: translateY(12px) scale(0.82);
    }
    70% {
      opacity: 1;
      transform: translateY(-2px) scale(1.03);
    }
    100% {
      opacity: 1;
      transform: translateY(0) scale(1);
    }
  }

  .tabbar.compact {
    flex: 1 1 auto;
    min-width: 160px;
    height: 34px;
    padding: 0;
    animation: tabPillIn 0.3s cubic-bezier(0.2, 1.24, 0.32, 1);
  }

  .tabs {
    display: flex;
    align-items: center;
    gap: 5px;
    min-width: 0;
    flex: 1 1 auto;
    overflow-x: auto;
    scrollbar-width: none;
    padding: 2px 1px;
  }

  .workspace-select-btn {
    position: relative;
    display: inline-flex;
    align-items: center;
    gap: 6px;
    max-width: 154px;
    height: 28px;
    padding: 0 11px;
    border: 1px solid var(--glass-border, rgba(15, 23, 42, 0.16));
    border-radius: 999px;
    background: var(--glass-pill, rgba(255, 255, 255, 0.14));
    -webkit-backdrop-filter: var(--glass-backdrop, blur(22px) saturate(2.1));
    backdrop-filter: var(--glass-backdrop, blur(22px) saturate(2.1));
    box-shadow: var(--glass-bevel, inset 0 1px 0 rgba(255, 255, 255, 0.85));
    color: var(--text);
    font: inherit;
    font-size: 11.5px;
    font-weight: 600;
    cursor: pointer;
    flex: 0 0 auto;
    transition:
      transform 0.24s cubic-bezier(0.2, 1.28, 0.36, 1),
      background 0.2s ease,
      box-shadow 0.22s ease;
  }
  .workspace-select-btn::before,
  .tab::before,
  .new-tab::before,
  .tab-search-btn::before {
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
    opacity: 0.56;
  }
  .workspace-select-btn:hover {
    background: var(--glass-pill-active, rgba(255, 255, 255, 0.85));
    transform: translateY(-2px) scale(1.04);
  }
  .workspace-select-btn:active {
    transform: translateY(0) scale(0.92);
  }
  .ws-dot {
    width: 7px;
    height: 7px;
    border-radius: 50%;
    background: linear-gradient(135deg, var(--accent), color-mix(in srgb, var(--accent) 60%, #3ecfcf));
    box-shadow: 0 0 6px color-mix(in srgb, var(--accent) 55%, transparent);
    flex: 0 0 auto;
  }
  .workspace-select-btn span:nth-child(2) {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .workspace-select-btn .caret {
    font-size: 10px;
    color: var(--text-muted);
  }

  .tabs::-webkit-scrollbar {
    display: none;
  }

  .tab {
    position: relative;
    display: flex;
    align-items: center;
    gap: 7px;
    min-width: 120px;
    max-width: 215px;
    height: 30px;
    padding: 0 7px 0 10px;
    border: 1px solid var(--glass-border, rgba(15, 23, 42, 0.16));
    border-radius: 999px;
    background: var(--glass-tab, rgba(255, 255, 255, 0.1));
    -webkit-backdrop-filter: var(--glass-backdrop, blur(22px) saturate(2.1));
    backdrop-filter: var(--glass-backdrop, blur(22px) saturate(2.1));
    box-shadow: var(--glass-bevel, inset 0 1px 0 rgba(255, 255, 255, 0.8));
    color: var(--text-muted);
    cursor: pointer;
    outline: none;
    animation: tabPillIn 0.3s cubic-bezier(0.2, 1.24, 0.32, 1);
    transition:
      background 0.22s ease,
      border-color 0.22s ease,
      box-shadow 0.24s ease,
      color 0.2s ease,
      transform 0.24s cubic-bezier(0.2, 1.28, 0.36, 1);
  }

  .tab:hover {
    background: var(--glass-pill, rgba(255, 255, 255, 0.2));
    border-color: var(--glass-border, rgba(15, 23, 42, 0.22));
    color: var(--text);
    transform: translateY(-2px) scale(1.025);
  }

  .tab:active {
    transform: translateY(0) scale(0.95);
  }

  .tab.active {
    background: var(--glass-pill-active, rgba(255, 255, 255, 0.32));
    border-color: var(--glass-border-strong, rgba(15, 23, 42, 0.24));
    color: var(--text);
    font-weight: 600;
    box-shadow: var(
      --glass-bevel,
      inset 0 1px 0 var(--glass-highlight, rgba(255, 255, 255, 0.95))
    );
  }

  .favicon {
    display: grid;
    place-items: center;
    width: 18px;
    height: 18px;
    flex: 0 0 auto;
    border-radius: 50%;
    background: linear-gradient(135deg, rgba(124, 92, 255, 0.2), rgba(62, 207, 207, 0.22));
    color: var(--accent);
    font-size: 9.5px;
    font-weight: 700;
  }

  .favicon-img {
    width: 15px;
    height: 15px;
    flex: 0 0 auto;
    border-radius: 4px;
    object-fit: contain;
  }

  .spinner {
    width: 12px;
    height: 12px;
    flex: 0 0 auto;
    border: 2px solid var(--border-strong);
    border-top-color: var(--accent);
    border-radius: 50%;
    animation: spin 0.8s linear infinite;
  }

  @keyframes spin {
    to {
      transform: rotate(360deg);
    }
  }

  .tab-title {
    flex: 1;
    min-width: 0;
    overflow: hidden;
    white-space: nowrap;
    text-overflow: ellipsis;
    font-size: 12px;
  }

  .tab-state-badge {
    display: inline-grid;
    place-items: center;
    width: 18px;
    height: 18px;
    padding: 0;
    border: 0;
    border-radius: 50%;
    background: transparent;
    font-size: 11px;
    line-height: 1;
    flex: 0 0 auto;
  }

  .tab-state-badge.muted {
    cursor: pointer;
    background: rgba(239, 68, 68, 0.12);
  }
  .tab-state-badge.muted:hover {
    background: rgba(239, 68, 68, 0.24);
  }

  .tab-close {
    display: grid;
    place-items: center;
    width: 20px;
    height: 20px;
    flex: 0 0 auto;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--text-soft);
    font-size: 14px;
    line-height: 1;
    padding: 0;
    cursor: pointer;
  }

  .tab-close:hover {
    background: var(--surface-strong);
    color: var(--text);
  }

  .tab-menu-trigger {
    display: grid;
    place-items: center;
    width: 19px;
    height: 19px;
    flex: 0 0 auto;
    border: none;
    border-radius: 50%;
    background: transparent;
    color: var(--text-soft);
    font-size: 14px;
    line-height: 1;
    padding: 0;
    opacity: 0;
    cursor: pointer;
  }

  .tab:hover .tab-menu-trigger,
  .tab-menu-trigger:focus-visible {
    opacity: 1;
  }

  .tab-menu-trigger:disabled {
    display: none;
  }

  .tab-menu-trigger:hover {
    background: var(--surface-strong);
    color: var(--text);
  }

  .tabbar-actions {
    display: flex;
    align-items: center;
    gap: 5px;
    flex: 0 0 auto;
  }

  .new-tab,
  .tab-search-btn {
    position: relative;
    display: grid;
    place-items: center;
    width: 28px;
    height: 28px;
    flex: 0 0 auto;
    border: 1px solid var(--glass-border, rgba(15, 23, 42, 0.16));
    border-radius: 50%;
    background: var(--glass-pill, rgba(255, 255, 255, 0.14));
    -webkit-backdrop-filter: var(--glass-backdrop, blur(22px) saturate(2.1));
    backdrop-filter: var(--glass-backdrop, blur(22px) saturate(2.1));
    box-shadow: var(--glass-bevel, inset 0 1px 0 rgba(255, 255, 255, 0.85));
    color: var(--text-muted);
    font-size: 17px;
    line-height: 1;
    padding: 0;
    cursor: pointer;
    transition:
      transform 0.24s cubic-bezier(0.2, 1.28, 0.36, 1),
      background 0.2s ease,
      color 0.2s ease;
  }

  .new-tab:hover,
  .tab-search-btn:hover {
    background: var(--glass-pill-active, rgba(255, 255, 255, 0.9));
    color: var(--text);
    transform: translateY(-2px) scale(1.08);
  }

  .tab-search-btn svg {
    width: 13.5px;
    height: 13.5px;
    fill: none;
    stroke: currentColor;
    stroke-width: 1.7;
    stroke-linecap: round;
  }

  .workspace-targets {
    display: flex;
    align-items: center;
    gap: 4px;
    flex: 0 0 auto;
  }

  .workspace-target {
    display: inline-flex;
    align-items: center;
    height: 26px;
    padding: 0 10px;
    border: 1px dashed var(--accent);
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
    font-size: 11px;
    font-weight: 600;
    white-space: nowrap;
  }

  .workspace-target.hover {
    background: var(--accent);
    color: #ffffff;
  }

  .tab.pinned {
    flex: 0 0 auto;
    min-width: 34px;
    max-width: 38px;
    justify-content: center;
    padding: 0 7px;
  }

  .tab.private {
    border-color: rgba(139, 92, 246, 0.42);
    background: linear-gradient(135deg, rgba(139, 92, 246, 0.16), rgba(99, 102, 241, 0.1));
  }

  .tab.sleeping {
    opacity: 0.72;
  }

  .private-badge {
    font-size: 12px;
    line-height: 1;
  }
</style>
