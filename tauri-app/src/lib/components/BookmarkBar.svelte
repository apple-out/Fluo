<script lang="ts">
  import { t } from "$lib/i18n.svelte";
  import type { Bookmark } from "$lib/types";

  let {
    bookmarks,
    onnavigate,
    onopen,
  }: {
    bookmarks: Bookmark[];
    onnavigate: (url: string) => void;
    onopen: () => void;
  } = $props();

  function host(url: string): string {
    try {
      return new URL(url).hostname.replace(/^www\./, "");
    } catch {
      return url;
    }
  }

  let items = $derived(
    [...bookmarks]
      .sort((left, right) => Number(right.favorite) - Number(left.favorite))
      .slice(0, 14),
  );
</script>

<div class="bookmark-bar" role="toolbar" aria-label={t("bmBar.aria")}>
  {#each items as bookmark (bookmark.id)}
    <button
      class="entry"
      type="button"
      title={`${bookmark.title}\n${bookmark.url}`}
      onclick={() => onnavigate(bookmark.url)}
    >
      <span class="star" aria-hidden="true">{bookmark.favorite ? "★" : "▪"}</span>
      <span class="label">{bookmark.title || host(bookmark.url)}</span>
    </button>
  {/each}
  {#if items.length === 0}
    <span class="hint">{t("bmBar.empty")}</span>
  {/if}
  <button class="more" type="button" title={t("bmBar.allTitle")} onclick={onopen}>
    {t("bmBar.all")}
  </button>
</div>

<style>
  .bookmark-bar {
    position: relative;
    z-index: 2;
    display: flex;
    align-items: center;
    gap: 4px;
    height: 29px;
    padding: 0 8px 2px;
    overflow-x: auto;
    scrollbar-width: none;
    -webkit-user-select: none;
    user-select: none;
  }

  .bookmark-bar::-webkit-scrollbar {
    display: none;
  }

  .entry {
    display: flex;
    align-items: center;
    gap: 5px;
    max-width: 190px;
    min-width: 0;
    height: 23px;
    padding: 0 9px;
    border: 1px solid transparent;
    border-radius: 999px;
    background: var(--glass-tab, rgba(255, 255, 255, 0.1));
    box-shadow: none;
    color: var(--text-muted);
    font: inherit;
    font-size: 11px;
    white-space: nowrap;
    cursor: pointer;
    transition: background 0.15s ease, border-color 0.15s ease, color 0.15s ease;
  }

  .entry:hover {
    background: var(--glass-pill-active, rgba(255, 255, 255, 0.34));
    border-color: var(--glass-border, rgba(15, 23, 42, 0.16));
    color: var(--text);
  }

  .star {
    color: var(--accent);
    font-size: 9px;
  }

  .label {
    overflow: hidden;
    text-overflow: ellipsis;
  }

  .hint {
    padding: 0 6px;
    color: var(--text-soft);
    font-size: 11px;
  }

  .more {
    margin-left: auto;
    padding: 3px 10px;
    border: 1px solid var(--glass-border, rgba(15, 23, 42, 0.16));
    border-radius: 999px;
    background: var(--glass-pill, rgba(255, 255, 255, 0.14));
    box-shadow: var(--glass-bevel, inset 0 1px 0 rgba(255, 255, 255, 0.8));
    color: var(--text-muted);
    font: inherit;
    font-size: 11px;
    font-weight: 500;
    white-space: nowrap;
    cursor: pointer;
  }

  .more:hover {
    background: var(--glass-pill-active, rgba(255, 255, 255, 0.34));
    color: var(--text);
  }
</style>
