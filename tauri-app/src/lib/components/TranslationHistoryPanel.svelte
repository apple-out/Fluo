<script lang="ts">
  import { api, errorMessage } from "$lib/api";
  import { t } from "$lib/i18n.svelte";
  import type { TranslationHistory } from "$lib/types";
  let { embedded = false, onclose, onnavigate, onnotify }: {
    embedded?: boolean;
    onclose: () => void;
    onnavigate: (url: string) => void;
    onnotify: (kind: "info" | "success" | "error", text: string) => void;
  } = $props();
  let query = $state("");
  let entries = $state<TranslationHistory[]>([]);
  let timer: ReturnType<typeof setTimeout>;
  $effect(() => {
    clearTimeout(timer);
    timer = setTimeout(() => api.listTranslationHistory(query).then((value) => entries = value)
      .catch((error) => onnotify("error", errorMessage(error))), 180);
    return () => clearTimeout(timer);
  });
</script>
<div class="backdrop" class:embedded role="presentation" onclick={(e) => { if (!embedded && e.target === e.currentTarget) onclose(); }}>
  <div class="panel" class:embedded role="dialog" aria-modal={!embedded} aria-label={t("trHist.aria")} tabindex="-1">
    <header><div><h2>{t("trHist.title")}</h2><p>{t("trHist.subtitle")}</p></div>{#if !embedded}<button onclick={onclose} aria-label={t("common.close")}>×</button>{/if}</header>
    <input type="search" placeholder={t("trHist.searchPlaceholder")} bind:value={query}/>
    <ul>{#each entries as item (item.id)}<li><button class="page" onclick={() => { onnavigate(item.url); onclose(); }}><small>{item.title || item.url} · {new Date(item.translatedAt * 1000).toLocaleString()}</small><strong>{item.source}</strong><span>{item.translation}</span><small>{item.model}</small></button></li>{/each}</ul>
    {#if !entries.length}<p class="empty">{t("trHist.empty")}</p>{/if}
  </div>
</div>
<style>
  .backdrop{position:fixed;inset:0;z-index:50;display:grid;place-items:center;padding:24px;background:#1118;backdrop-filter:blur(3px)}.backdrop.embedded{padding:0;background:var(--surface);backdrop-filter:none}.panel{display:flex;flex-direction:column;width:min(900px,100%);height:min(640px,88vh);padding:20px;border:1px solid var(--border);border-radius:16px;background:var(--surface);color:var(--text);box-shadow:0 24px 70px #0004}.panel.embedded{width:100%;height:100%;border:0;border-radius:0;box-shadow:none}header{display:flex;justify-content:space-between;align-items:center}h2{margin:0;font-size:18px}p,small{color:var(--text-muted);font-size:12px}input{height:36px;margin:10px 0;padding:0 10px;border:1px solid var(--border);border-radius:8px;background:var(--surface);color:var(--text)}button{font:inherit;cursor:pointer}header button{border:0;background:transparent;font-size:20px}ul{overflow:auto;list-style:none;margin:0;padding:0}li{border-top:1px solid var(--border)}.page{display:grid;gap:4px;width:100%;padding:10px;text-align:left;border:0;background:transparent;color:var(--text)}.page:hover{background:var(--surface-2)}.page span{color:var(--accent)}.empty{text-align:center;padding:24px}
</style>
