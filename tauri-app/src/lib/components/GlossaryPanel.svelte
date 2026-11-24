<script lang="ts">
  import { api, errorMessage } from "$lib/api";
  import { t } from "$lib/i18n.svelte";
  import {
    emptyGlossaryEntry,
    type GlossaryEntry,
  } from "$lib/types";

  let {
    embedded = false,
    onclose,
    onnotify,
  }: {
    embedded?: boolean;
    onclose: () => void;
    onnotify: (kind: "info" | "success" | "error", text: string) => void;
  } = $props();

  let entries = $state<GlossaryEntry[]>([]);
  let query = $state("");
  let editing = $state<GlossaryEntry | null>(null);
  let loading = $state(true);
  let saving = $state(false);
  let error = $state<string | null>(null);
  let confirmDeleteId = $state<string | null>(null);

  let filtered = $derived(
    entries.filter((entry) => {
      const needle = query.trim().toLowerCase();
      if (!needle) return true;
      return (
        entry.source.toLowerCase().includes(needle) ||
        entry.translation.toLowerCase().includes(needle) ||
        entry.reading.toLowerCase().includes(needle)
      );
    }),
  );

  load();

  async function load() {
    loading = true;
    error = null;
    try {
      entries = await api.listGlossary();
    } catch (cause) {
      error = errorMessage(cause);
    } finally {
      loading = false;
    }
  }

  function startNew() {
    editing = emptyGlossaryEntry();
  }

  function startEdit(entry: GlossaryEntry) {
    editing = { ...entry };
  }

  async function saveEntry() {
    if (!editing) return;
    if (!editing.source.trim()) {
      onnotify("error", t("gloss.needSource"));
      return;
    }
    saving = true;
    try {
      await api.saveGlossary({ ...editing });
      editing = null;
      await load();
      onnotify("success", t("gloss.savedToast"));
    } catch (cause) {
      onnotify("error", errorMessage(cause));
    } finally {
      saving = false;
    }
  }

  async function removeEntry(entry: GlossaryEntry) {
    confirmDeleteId = null;
    if (editing?.id === entry.id) {
      editing = null;
    }
    try {
      await api.deleteGlossary(entry.id);
      await load();
      onnotify("info", t("gloss.deletedToast"));
    } catch (cause) {
      onnotify("error", errorMessage(cause));
    }
  }

  async function toggleEnabled(entry: GlossaryEntry) {
    try {
      await api.saveGlossary({ ...entry, enabled: !entry.enabled });
      await load();
    } catch (cause) {
      onnotify("error", errorMessage(cause));
    }
  }

  function csvCell(value: unknown): string {
    const text = String(value ?? "");
    return /[",\r\n]/.test(text) ? `"${text.replaceAll('"', '""')}"` : text;
  }

  function exportCsv() {
    const header = [
      "source",
      "reading",
      "translation",
      "alternatives",
      "aliases",
      "do_not_translate",
      "case_sensitive",
      "exact_match",
      "use_regex",
      "enabled",
      "note",
    ];
    const lines = [header.join(",")];
    for (const entry of entries) {
      lines.push(
        [
          entry.source,
          entry.reading,
          entry.translation,
          entry.alternatives,
          entry.aliases,
          entry.doNotTranslate ? 1 : 0,
          entry.caseSensitive ? 1 : 0,
          entry.exactMatch ? 1 : 0,
          entry.useRegex ? 1 : 0,
          entry.enabled ? 1 : 0,
          entry.note,
        ]
          .map(csvCell)
          .join(","),
      );
    }
    const blob = new Blob(["\uFEFF" + lines.join("\r\n")], {
      type: "text/csv;charset=utf-8",
    });
    const url = URL.createObjectURL(blob);
    const anchor = document.createElement("a");
    anchor.href = url;
    anchor.download = "glossary.csv";
    anchor.click();
    setTimeout(() => URL.revokeObjectURL(url), 2_000);
    onnotify("info", t("gloss.exportedCsvToast"));
  }

  function parseCsv(text: string): string[][] {
    const rows: string[][] = [];
    let row: string[] = [];
    let cell = "";
    let quoted = false;
    for (let index = 0; index < text.length; index += 1) {
      const character = text[index];
      if (quoted) {
        if (character === '"') {
          if (text[index + 1] === '"') {
            cell += '"';
            index += 1;
          } else {
            quoted = false;
          }
        } else {
          cell += character;
        }
      } else if (character === '"') {
        quoted = true;
      } else if (character === ",") {
        row.push(cell);
        cell = "";
      } else if (character === "\n") {
        row.push(cell);
        cell = "";
        rows.push(row);
        row = [];
      } else if (character !== "\r") {
        cell += character;
      }
    }
    if (cell.length > 0 || row.length > 0) {
      row.push(cell);
      rows.push(row);
    }
    if (quoted) throw new Error("CSV의 따옴표가 닫히지 않았습니다.");
    return rows.filter((line) => line.some((value) => value.trim().length > 0));
  }

  function truthy(value: string | undefined): boolean {
    if (!value) return false;
    return ["1", "true", "yes", "y", "예", "ㅇ"].includes(value.trim().toLowerCase());
  }

  async function importCsv(event: Event) {
    const input = event.currentTarget as HTMLInputElement;
    const file = input.files?.[0];
    if (!file) return;
    input.value = "";
    if (file.size > 5 * 1024 * 1024) {
      onnotify("error", "CSV 파일은 5 MiB 이하만 가져올 수 있습니다.");
      return;
    }
    let rows: string[][];
    try {
      const text = await file.text();
      rows = parseCsv(text.replace(/^\uFEFF/, ""));
    } catch (cause) {
      onnotify("error", errorMessage(cause));
      return;
    }
    if (rows.length === 0) {
      onnotify("error", "CSV에 데이터가 없습니다.");
      return;
    }
    if (rows.length > 5_001) {
      onnotify("error", "한 번에 가져올 수 있는 CSV는 5,000개 항목까지입니다.");
      return;
    }
    const first = rows[0].map((value) => value.trim().toLowerCase());
    const hasHeader = first.includes("source");
    const body = hasHeader ? rows.slice(1) : rows;
    const indexOf = (name: string, fallback: number) => {
      const found = first.indexOf(name);
      return hasHeader && found >= 0 ? found : fallback;
    };
    let imported = 0;
    const failures: string[] = [];
    for (const line of body) {
      const source = line[indexOf("source", 0)]?.trim();
      if (!source) continue;
      const enabledValue = line[indexOf("enabled", 8)];
      const entry: GlossaryEntry = {
        id: crypto.randomUUID(),
        source,
        reading: line[indexOf("reading", 1)]?.trim() ?? "",
        translation: line[indexOf("translation", 2)]?.trim() ?? "",
        alternatives: line[indexOf("alternatives", 3)]?.trim() ?? "",
        aliases: hasHeader ? line[indexOf("aliases", -1)]?.trim() ?? "" : "",
        doNotTranslate: truthy(line[indexOf("do_not_translate", 4)]),
        caseSensitive: truthy(line[indexOf("case_sensitive", 5)]),
        exactMatch: truthy(line[indexOf("exact_match", 6)]),
        useRegex: truthy(line[indexOf("use_regex", 7)]),
        enabled: !enabledValue || enabledValue.trim() === "" ? true : truthy(enabledValue),
        projectId: null,
        note: line[indexOf("note", 9)]?.trim() ?? "",
      };
      try {
        await api.saveGlossary(entry);
        imported += 1;
      } catch (cause) {
        failures.push(`${source}: ${errorMessage(cause)}`);
      }
    }
    await load();
    if (failures.length > 0) {
      onnotify("error", `${imported}개 가져옴, ${failures.length}개 실패 (${failures[0]})`);
    } else {
      onnotify("success", `용어집 ${imported}개를 가져왔습니다.`);
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
  <div class="panel" class:embedded role="dialog" aria-modal={!embedded} aria-label={t("gloss.aria")}>
    <header class="panel-head">
      <div>
        <h2>{t("gloss.title")}</h2>
        <p>{t("gloss.subtitle")}</p>
      </div>
      <div class="head-actions">
        <label class="file-button">
          {t("gloss.importCsv")}
          <input type="file" accept=".csv,text/csv" onchange={importCsv} />
        </label>
        <button class="ghost" type="button" onclick={exportCsv} disabled={entries.length === 0}>
          {t("gloss.exportCsv")}
        </button>
        {#if !embedded}
          <button class="icon-button" type="button" aria-label={t("common.close")} onclick={onclose}>
            ×
          </button>
        {/if}
      </div>
    </header>

    <div class="toolbar">
      <input
        class="search"
        type="search"
        placeholder={t("gloss.searchPlaceholder")}
        bind:value={query}
      />
      <button class="primary" type="button" onclick={startNew}>{t("gloss.addNew")}</button>
    </div>

    <div class="panel-body">
      {#if editing}
        <div class="editor">
          <div class="grid">
            <label class="field">
              <span>{t("gloss.source")}</span>
              <input bind:value={editing.source} placeholder="魔法" />
            </label>
            <label class="field">
              <span>{t("gloss.reading")}</span>
              <input bind:value={editing.reading} placeholder="まほう" />
            </label>
            <label class="field">
              <span>{t("gloss.translation")}</span>
              <input
                bind:value={editing.translation}
                disabled={editing.doNotTranslate}
                placeholder={editing.doNotTranslate ? t("gloss.dntBadge") : ""}
              />
            </label>
            <label class="field">
              <span>{t("gloss.alternatives")}</span>
              <input bind:value={editing.alternatives} placeholder="마술, 매직" />
            </label>
            <label class="field">
              <span>{t("gloss.aliases")}</span>
              <input bind:value={editing.aliases} placeholder={t("gloss.aliasesPlaceholder")} />
            </label>
          </div>
          <label class="field">
            <span>{t("gloss.note")}</span>
            <input bind:value={editing.note} placeholder={t("gloss.notePlaceholder")} />
          </label>
          <div class="checks">
            <label><input type="checkbox" bind:checked={editing.doNotTranslate} /> {t("gloss.dnt")}</label>
            <label><input type="checkbox" bind:checked={editing.caseSensitive} /> {t("gloss.caseSensitive")}</label>
            <label><input type="checkbox" bind:checked={editing.exactMatch} /> {t("gloss.exactMatch")}</label>
            <label><input type="checkbox" bind:checked={editing.useRegex} /> {t("gloss.useRegex")}</label>
            <label><input type="checkbox" bind:checked={editing.enabled} /> {t("gloss.enabled")}</label>
          </div>
          <small class="hint">{t("gloss.hint")}</small>
          <div class="editor-actions">
            <button class="ghost" type="button" onclick={() => (editing = null)}>{t("common.cancel")}</button>
            <button class="primary" type="button" onclick={saveEntry} disabled={saving}>
              {saving ? t("common.saving") : t("common.save")}
            </button>
          </div>
        </div>
      {/if}

      {#if error}
        <p class="error">{error}</p>
      {:else if loading}
        <p class="empty">{t("common.loading")}</p>
      {:else if filtered.length === 0}
        <p class="empty">
          {entries.length === 0 ? t("gloss.emptyAll") : t("gloss.emptySearch")}
        </p>
      {:else}
        <ul class="list">
          {#each filtered as entry (entry.id)}
            <li class:disabled={!entry.enabled}>
              <div class="entry-main">
                <div class="entry-terms">
                  <span class="source">{entry.source}</span>
                  {#if entry.reading}<span class="reading">{entry.reading}</span>{/if}
                  <span class="arrow">→</span>
                  <span class="target">{entry.doNotTranslate || !entry.translation.trim() ? t("gloss.dntBadge") : entry.translation}</span>
                </div>
                <div class="entry-flags">
                  {#if entry.alternatives}<span class="flag">{t("gloss.altPrefix")}{entry.alternatives}</span>{/if}
                  {#if entry.aliases}<span class="flag">{t("gloss.aliasPrefix")}{entry.aliases}</span>{/if}
                  {#if entry.exactMatch}<span class="flag">{t("gloss.exactMatch")}</span>{/if}
                  {#if entry.caseSensitive}<span class="flag">{t("gloss.caseSensitive")}</span>{/if}
                  {#if entry.useRegex}<span class="flag">{t("gloss.useRegex")}</span>{/if}
                  {#if entry.note}<span class="flag note">{entry.note}</span>{/if}
                </div>
              </div>
              <div class="entry-actions">
                <button class="mini" type="button" onclick={() => toggleEnabled(entry)}>
                  {entry.enabled ? t("common.disable") : t("common.enable")}
                </button>
                <button class="mini" type="button" onclick={() => startEdit(entry)}>{t("common.edit")}</button>
                {#if confirmDeleteId === entry.id}
                  <button class="mini danger" type="button" onclick={() => removeEntry(entry)}>
                    {t("common.confirmDelete")}
                  </button>
                  <button class="mini" type="button" onclick={() => (confirmDeleteId = null)}>
                    {t("common.cancel")}
                  </button>
                {:else}
                  <button class="mini danger" type="button" onclick={() => (confirmDeleteId = entry.id)}>
                    {t("common.delete")}
                  </button>
                {/if}
              </div>
            </li>
          {/each}
        </ul>
      {/if}
    </div>

    <footer class="panel-foot">
      <span class="count">{t("gloss.count", { count: entries.length })}</span>
      {#if !embedded}
        <button class="primary" type="button" onclick={onclose}>{t("common.close")}</button>
      {/if}
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
    width: min(860px, 100%);
    height: min(640px, 88vh);
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

  .head-actions {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
    justify-content: flex-end;
  }

  .toolbar {
    display: flex;
    gap: 8px;
    padding: 12px 22px;
    border-bottom: 1px solid var(--border);
    background: var(--surface-2);
  }

  .search {
    flex: 1;
  }

  .panel-body {
    flex: 1;
    overflow-y: auto;
    padding: 16px 22px;
  }

  .editor {
    margin-bottom: 18px;
    padding: 14px;
    border: 1px solid var(--border-strong);
    border-radius: 12px;
    background: var(--surface-2);
  }

  .grid {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(190px, 1fr));
    gap: 4px 14px;
  }

  .field {
    display: flex;
    flex-direction: column;
    gap: 5px;
    margin-bottom: 10px;
    font-size: 12px;
  }

  .field > span {
    color: var(--text-muted);
  }

  input {
    height: 32px;
    padding: 0 9px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
    outline: none;
  }

  input:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  input:disabled {
    opacity: 0.5;
    cursor: not-allowed;
  }

  input[type="search"] {
    height: 34px;
  }

  .checks {
    display: flex;
    flex-wrap: wrap;
    gap: 8px 16px;
    margin-bottom: 12px;
    font-size: 12px;
    color: var(--text-muted);
  }

  .checks label {
    display: flex;
    align-items: center;
    gap: 6px;
  }

  .checks input {
    width: 14px;
    height: 14px;
    padding: 0;
    accent-color: var(--accent);
  }

  .editor-actions {
    display: flex;
    justify-content: flex-end;
    gap: 8px;
  }

  .list {
    display: flex;
    flex-direction: column;
    gap: 6px;
    margin: 0;
    padding: 0;
    list-style: none;
  }

  .list li {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 12px;
    padding: 10px 12px;
    border: 1px solid var(--border);
    border-radius: 10px;
    background: var(--surface-2);
  }

  .list li.disabled {
    opacity: 0.55;
  }

  .entry-main {
    min-width: 0;
    flex: 1;
  }

  .entry-terms {
    display: flex;
    align-items: baseline;
    flex-wrap: wrap;
    gap: 7px;
    font-size: 13px;
  }

  .source {
    font-weight: 600;
  }

  .reading {
    font-size: 11px;
    color: var(--text-soft);
  }

  .arrow {
    color: var(--text-soft);
  }

  .target {
    color: var(--accent);
  }

  .entry-flags {
    display: flex;
    flex-wrap: wrap;
    gap: 6px;
    margin-top: 4px;
  }

  .flag {
    padding: 1px 7px;
    border-radius: 999px;
    background: var(--surface-strong);
    color: var(--text-muted);
    font-size: 10.5px;
  }

  .flag.note {
    background: transparent;
    padding: 0;
  }

  .entry-actions {
    display: flex;
    gap: 5px;
    flex: 0 0 auto;
  }

  .mini {
    padding: 3px 9px;
    border: 1px solid var(--border-strong);
    border-radius: 7px;
    background: var(--surface);
    color: var(--text-muted);
    font-size: 11px;
    cursor: pointer;
  }

  .mini:hover {
    border-color: var(--accent);
    color: var(--accent);
  }

  .mini.danger:hover {
    border-color: #d4524a;
    color: #d4524a;
  }

  .empty {
    padding: 40px 0;
    text-align: center;
    color: var(--text-soft);
    font-size: 12.5px;
  }

  .error {
    padding: 20px;
    color: #b42318;
    font-size: 12.5px;
  }

  button {
    font: inherit;
  }

  .ghost,
  .primary {
    height: 32px;
    padding: 0 13px;
    border-radius: 8px;
    font-size: 12px;
    white-space: nowrap;
    cursor: pointer;
  }

  .ghost {
    display: inline-flex;
    align-items: center;
    border: 1px solid var(--border-strong);
    background: var(--surface);
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

  .file-button {
    position: relative;
    display: inline-flex;
    align-items: center;
    height: 32px;
    padding: 0 13px;
    border: 1px solid var(--border-strong);
    border-radius: 8px;
    background: var(--surface);
    color: var(--text);
    font-size: 12px;
    white-space: nowrap;
    cursor: pointer;
  }

  .file-button:hover {
    border-color: var(--accent);
    color: var(--accent);
  }

  .file-button input {
    position: absolute;
    inset: 0;
    opacity: 0;
    cursor: pointer;
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

  .panel-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 12px 22px;
    border-top: 1px solid var(--border);
    background: var(--surface-2);
  }

  .count {
    font-size: 11.5px;
    color: var(--text-muted);
  }

  .hint {
    display: block;
    margin-top: 8px;
    color: var(--text-muted);
    font-size: 11px;
  }
</style>
