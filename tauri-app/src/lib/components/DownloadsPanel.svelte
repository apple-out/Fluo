<script lang="ts">
  import { onMount } from "svelte";
  import { listen, type UnlistenFn } from "@tauri-apps/api/event";
  import { api, errorMessage } from "$lib/api";
  import { t } from "$lib/i18n.svelte";
  import type { DownloadItem } from "$lib/types";

  let {
    embedded = false,
    onclose,
    onnotify,
  }: {
    embedded?: boolean;
    onclose: () => void;
    onnotify: (kind: "info" | "success" | "error", text: string) => void;
  } = $props();

  let query = $state("");
  let entries = $state<DownloadItem[]>([]);
  let busy = $state(false);
  let confirmClear = $state(false);
  let timer: ReturnType<typeof setTimeout> | undefined;

  function statusLabel(status: DownloadItem["status"]): string {
    if (status === "completed") return t("dl.statusCompleted");
    if (status === "failed") return t("dl.statusFailed");
    return t("dl.statusInProgress");
  }

  async function load() {
    try {
      entries = await api.listDownloads(query, 200);
    } catch (cause) {
      onnotify("error", errorMessage(cause));
    }
  }

  async function showInFolder(path: string) {
    try {
      await api.showDownloadInFolder(path);
    } catch (cause) {
      onnotify("error", errorMessage(cause));
    }
  }

  async function remove(id: number) {
    if (busy) return;
    busy = true;
    try {
      await api.deleteDownload(id);
      entries = entries.filter((item) => item.id !== id);
    } catch (cause) {
      onnotify("error", errorMessage(cause));
    } finally {
      busy = false;
    }
  }

  async function clearAll() {
    if (busy || entries.length === 0) return;
    confirmClear = false;
    busy = true;
    try {
      await api.clearDownloads();
      await load();
      onnotify("success", t("dl.clearedToast"));
    } catch (cause) {
      onnotify("error", errorMessage(cause));
    } finally {
      busy = false;
    }
  }

  $effect(() => {
    clearTimeout(timer);
    timer = setTimeout(() => void load(), 160);
    return () => clearTimeout(timer);
  });

  onMount(() => {
    let unlisten: UnlistenFn | undefined;
    listen("download-status", () => {
      void load();
    }).then((dispose) => {
      unlisten = dispose;
    });
    return () => {
      unlisten?.();
    };
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
  <div class="panel" class:embedded role="dialog" aria-modal={!embedded} aria-label={t("dl.aria")} tabindex="-1">
    <header>
      <div>
        <h2>{t("dl.title")}</h2>
        <p>{t("dl.subtitle")}</p>
      </div>
      {#if !embedded}
        <button class="close" type="button" onclick={onclose} aria-label={t("common.close")}>×</button>
      {/if}
    </header>
    <div class="search">
      <input type="search" placeholder={t("dl.searchPlaceholder")} bind:value={query} />
      {#if confirmClear}
        <button class="ghost" type="button" onclick={clearAll} disabled={busy}>
          {t("common.confirmDelete")}
        </button>
        <button class="ghost" type="button" onclick={() => (confirmClear = false)}>
          {t("common.cancel")}
        </button>
      {:else}
        <button class="ghost" type="button" onclick={() => (confirmClear = true)} disabled={busy || entries.length === 0}>
          {t("dl.clearBtn")}
        </button>
      {/if}
    </div>
    <ul>
      {#each entries as item (item.id)}
        <li class="row">
          <div class="info">
            <div class="title-row">
              <strong>{item.fileName}</strong>
              <span
                class="badge"
                class:completed={item.status === "completed"}
                class:failed={item.status === "failed"}
              >
                {statusLabel(item.status)}
              </span>
            </div>
            <small>
              {item.url} · {new Date(item.startedAt * 1000).toLocaleString()}
            </small>
          </div>
          {#if item.filePath && item.status === "completed"}
            <button type="button" onclick={() => void showInFolder(item.filePath)}>
              {t("dl.showInFolder")}
            </button>
          {/if}
          <button
            class="remove"
            type="button"
            aria-label={t("dl.removeTitle")}
            title={t("dl.removeTitle")}
            onclick={() => void remove(item.id)}
            disabled={busy}
          >
            ×
          </button>
        </li>
      {/each}
    </ul>
    {#if entries.length === 0}
      <p class="empty">{t("dl.empty")}</p>
    {/if}
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 50;
    display: grid;
    place-items: center;
    padding: 24px;
    background: #1118;
    backdrop-filter: blur(3px);
  }

  .backdrop.embedded {
    padding: 0;
    background: var(--surface);
    backdrop-filter: none;
  }

  .panel {
    display: flex;
    flex-direction: column;
    width: min(860px, 100%);
    height: min(640px, 86vh);
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: 16px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 24px 70px #0004;
  }

  .panel.embedded {
    width: 100%;
    height: 100%;
    border: 0;
    border-radius: 0;
    box-shadow: none;
  }

  header,
  .search,
  .row,
  .title-row {
    display: flex;
    align-items: center;
    gap: 10px;
  }

  header {
    justify-content: space-between;
  }

  h2 {
    margin: 0;
    font-size: 18px;
  }

  p {
    margin: 4px 0 0;
    color: var(--text-muted);
    font-size: 12px;
  }

  .search {
    margin: 12px 0;
  }

  input,
  button {
    font: inherit;
  }

  input {
    flex: 1;
    min-width: 0;
    height: 36px;
    padding: 0 11px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    color: var(--text);
  }

  button {
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface);
    color: var(--text);
    padding: 7px 11px;
    cursor: pointer;
    font-size: 12px;
    white-space: nowrap;
  }

  button:hover:not(:disabled) {
    background: var(--surface-hover);
  }

  button:disabled {
    opacity: 0.45;
    cursor: default;
  }

  ul {
    overflow: auto;
    list-style: none;
    padding: 0;
    margin: 0;
  }

  .row {
    padding: 9px 4px;
    border-bottom: 1px solid var(--border);
  }

  .info {
    display: grid;
    gap: 3px;
    flex: 1;
    min-width: 0;
  }

  .title-row {
    gap: 8px;
  }

  .info strong {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    font-size: 13px;
  }

  .info small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
    font-size: 11px;
  }

  .badge {
    padding: 2px 7px;
    border-radius: 999px;
    background: var(--surface-2);
    color: var(--text-muted);
    font-size: 11px;
    font-weight: 600;
    flex-shrink: 0;
  }

  .badge.completed {
    background: var(--ok-soft);
    color: var(--ok);
  }

  .badge.failed {
    background: var(--err-soft);
    color: var(--err);
  }

  .remove,
  .close {
    width: 30px;
    height: 30px;
    padding: 0;
    display: grid;
    place-items: center;
  }

  .empty {
    text-align: center;
    padding: 28px 0;
    color: var(--text-muted);
    font-size: 13px;
  }
</style>
