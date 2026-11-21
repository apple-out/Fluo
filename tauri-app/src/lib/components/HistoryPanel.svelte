<script lang="ts">
  import { api, errorMessage } from "$lib/api";
  import { t } from "$lib/i18n.svelte";
  import type { HistoryItem, SearchHistoryItem } from "$lib/types";

  let {
    embedded = false,
    onclose,
    onnavigate,
    onnotify,
  }: {
    embedded?: boolean;
    onclose: () => void;
    onnavigate: (url: string) => void;
    onnotify: (kind: "info" | "success" | "error", text: string) => void;
  } = $props();

  let activeTab = $state<"visits" | "searches">("visits");
  let query = $state("");
  let entries = $state<HistoryItem[]>([]);
  let searchEntries = $state<SearchHistoryItem[]>([]);
  let busy = $state(false);
  let confirmClear = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  function dayOfEpoch(ts: number): string {
    return new Date(ts * 1000).toLocaleDateString(undefined, {
      year: "numeric",
      month: "long",
      day: "numeric",
      weekday: "short",
    });
  }

  async function load() {
    try {
      if (activeTab === "visits") {
        entries = await api.listHistory(query, 300);
      } else {
        searchEntries = await api.listSearchHistory(query, 300);
      }
    } catch (cause) {
      onnotify("error", errorMessage(cause));
    }
  }

  async function removeVisit(id: number) {
    if (busy) return;
    busy = true;
    try {
      await api.deleteHistory(id);
      entries = entries.filter((item) => item.id !== id);
    } catch (cause) {
      onnotify("error", errorMessage(cause));
    } finally {
      busy = false;
    }
  }

  async function removeSearch(id: number) {
    if (busy) return;
    busy = true;
    try {
      await api.deleteSearchHistory(id);
      searchEntries = searchEntries.filter((item) => item.id !== id);
    } catch (cause) {
      onnotify("error", errorMessage(cause));
    } finally {
      busy = false;
    }
  }

  async function clearAll() {
    if (busy) return;
    const isVisits = activeTab === "visits";
    if ((isVisits ? entries.length : searchEntries.length) === 0) return;
    const label = isVisits ? t("hist.visitsTab") : t("hist.searchesTab");
    confirmClear = false;
    busy = true;
    try {
      if (isVisits) {
        await api.clearHistory();
        entries = [];
      } else {
        await api.clearSearchHistory();
        searchEntries = [];
      }
      query = "";
      onnotify("success", t("hist.clearedToast", { label }));
    } catch (cause) {
      onnotify("error", errorMessage(cause));
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    void activeTab;
    void query;
    clearTimeout(timer);
    timer = setTimeout(() => void load(), 120);
    return () => clearTimeout(timer);
  });
</script>

<div
  class="backdrop"
  class:embedded
  role="presentation"
  onclick={(event) => {
    if (!embedded && event.target === event.currentTarget) onclose();
  }}
>
  <div class="panel" class:embedded role="dialog" aria-modal={!embedded} aria-label={t("hist.aria")} tabindex="-1">
    <header>
      <div>
        <h2>{t("hist.title")}</h2>
        <p>{t("hist.subtitle")}</p>
      </div>
      {#if !embedded}
        <button class="close" onclick={onclose} aria-label={t("common.close")}>×</button>
      {/if}
    </header>
    <div class="tabs-row" role="tablist">
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "visits"}
        class:active={activeTab === "visits"}
        onclick={() => {
          confirmClear = false;
          activeTab = "visits";
        }}
      >
        {t("hist.visitsTab")}
      </button>
      <button
        type="button"
        role="tab"
        aria-selected={activeTab === "searches"}
        class:active={activeTab === "searches"}
        onclick={() => {
          confirmClear = false;
          activeTab = "searches";
        }}
      >
        {t("hist.searchesTab")}
      </button>
    </div>
    <div class="search">
      <input
        type="search"
        placeholder={activeTab === "visits" ? t("hist.visitsSearch") : t("hist.searchesSearch")}
        bind:value={query}
      />
      {#if confirmClear}
        <button class="ghost" type="button" onclick={clearAll} disabled={busy}>
          {t("common.confirmDelete")}
        </button>
        <button class="ghost" type="button" onclick={() => (confirmClear = false)}>
          {t("common.cancel")}
        </button>
      {:else}
        <button
          class="ghost"
          type="button"
          onclick={() => (confirmClear = true)}
          disabled={busy || (activeTab === "visits" ? entries.length === 0 : searchEntries.length === 0)}
        >
          {t("hist.clearBtn")}
        </button>
      {/if}
    </div>
    {#if activeTab === "visits"}
      <ul>
        {#each entries as item, index (item.id)}
          {#if index === 0 || dayOfEpoch(item.visitedAt) !== dayOfEpoch(entries[index - 1].visitedAt)}
            <li class="day">{dayOfEpoch(item.visitedAt)}</li>
          {/if}
          <li class="row">
            <button class="page" type="button" onclick={() => onnavigate(item.url)}>
              <strong>{item.title || item.url}</strong>
              <small>
                {item.url} · {new Date(item.visitedAt * 1000).toLocaleTimeString()}
              </small>
            </button>
            <button
              class="remove"
              type="button"
              aria-label={t("hist.removeVisit")}
              title={t("hist.removeVisit")}
              onclick={() => void removeVisit(item.id)}
              disabled={busy}
            >
              ×
            </button>
          </li>
        {/each}
      </ul>
      {#if entries.length === 0}
        <p class="empty">{t("hist.emptyVisits")}</p>
      {/if}
    {:else}
      <ul>
        {#each searchEntries as item, index (item.id)}
          {#if index === 0 || dayOfEpoch(item.searchedAt) !== dayOfEpoch(searchEntries[index - 1].searchedAt)}
            <li class="day">{dayOfEpoch(item.searchedAt)}</li>
          {/if}
          <li class="row">
            <button class="page" type="button" onclick={() => onnavigate(item.query)}>
              <strong>🔍 {item.query}</strong>
              <small>
                {item.engine} · {new Date(item.searchedAt * 1000).toLocaleTimeString()}
              </small>
            </button>
            <button
              class="remove"
              type="button"
              aria-label={t("hist.removeSearch")}
              title={t("hist.removeSearch")}
              onclick={() => void removeSearch(item.id)}
              disabled={busy}
            >
              ×
            </button>
          </li>
        {/each}
      </ul>
      {#if searchEntries.length === 0}
        <p class="empty">{t("hist.emptySearches")}</p>
      {/if}
    {/if}
  </div>
</div>

<style>
  .backdrop{position:fixed;inset:0;z-index:50;display:grid;place-items:center;padding:24px;background:#1115;backdrop-filter:blur(5px)}
  .backdrop.embedded{padding:0;background:var(--surface);backdrop-filter:none}
  .panel{display:flex;flex-direction:column;width:min(900px,100%);height:min(640px,88vh);padding:20px;border:1px solid var(--border);border-radius:16px;background:var(--surface);color:var(--text);box-shadow:0 24px 70px #0004}
  .panel.embedded{width:100%;height:100%;border:0;border-radius:0;box-shadow:none}
  header{display:flex;justify-content:space-between;align-items:center}
  h2{margin:0;font-size:18px}
  p,small{color:var(--text-muted);font-size:12px}
  .tabs-row{display:flex;gap:6px;margin:10px 0 4px}
  .tabs-row button{height:32px;padding:0 14px;border:1px solid var(--border);border-radius:999px;background:transparent;color:var(--text-muted);font-weight:600;font-size:12.5px}
  .tabs-row button.active{background:var(--accent);border-color:transparent;color:#fff}
  .search{display:flex;gap:8px;margin:8px 0}
  .search input{flex:1;height:36px;padding:0 10px;border:1px solid var(--border);border-radius:10px;background:var(--surface);color:var(--text);font:inherit}
  button{font:inherit;cursor:pointer}
  .close{border:0;background:transparent;font-size:20px;color:var(--text)}
  .ghost{height:36px;padding:0 12px;border:1px solid var(--border);border-radius:10px;background:var(--surface);color:var(--text)}
  .ghost:hover:not(:disabled){background:var(--surface-hover)}
  .ghost:disabled{opacity:.5;cursor:default}
  ul{overflow:auto;list-style:none;margin:0;padding:0}
  .day{position:sticky;top:0;padding:10px 8px 5px;background:var(--surface);color:var(--text-soft);font-size:11px;font-weight:600}
  .row{display:flex;align-items:center;gap:6px;border-top:1px solid var(--border)}
  .page{display:grid;gap:2px;flex:1;min-width:0;padding:9px;text-align:left;border:0;background:transparent;color:var(--text)}
  .page:hover{background:var(--surface-2)}
  .page strong,.page small{overflow:hidden;text-overflow:ellipsis;white-space:nowrap}
  .remove{width:28px;height:28px;margin-right:6px;border:0;border-radius:6px;background:transparent;color:var(--text-soft);font-size:16px}
  .remove:hover:not(:disabled){background:var(--surface-hover);color:var(--text)}
  .empty{text-align:center;padding:24px}
</style>
