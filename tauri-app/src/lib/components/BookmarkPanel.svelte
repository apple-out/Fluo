<script lang="ts">
  import { api, errorMessage } from "$lib/api";
  import { normalizeBookmarkUrl } from "$lib/browser-utils";
  import { t } from "$lib/i18n.svelte";
  import type { Bookmark, TabState } from "$lib/types";

  let {
    embedded = false,
    current,
    onclose,
    onnavigate,
    onnotify,
  }: {
    embedded?: boolean;
    current: TabState | null;
    onclose: () => void;
    onnavigate: (url: string) => void;
    onnotify: (kind: "info" | "success" | "error", text: string) => void;
  } = $props();

  let entries = $state<Bookmark[]>([]);
  let explicitFolders = $state<string[]>([]);
  let query = $state("");
  let folderFilter = $state<"__all__" | "__favorites__" | "__unfiled__" | string>("__all__");
  let sortBy = $state<"favorite" | "title" | "recent" | "modified">("favorite");
  let editing = $state<Bookmark | null>(null);
  let folderMode = $state<null | "create" | "rename" | "delete">(null);
  let folderDraft = $state("");
  let confirmDeleteBookmarkId = $state<string | null>(null);

  $effect(() => {
    void load();
  });

  let folders = $derived(
    [...new Set([...explicitFolders, ...entries.map((item) => item.folder).filter(Boolean)])].sort((a, b) =>
      a.localeCompare(b, "ko", { sensitivity: "base" }),
    ),
  );

  let filtered = $derived(
    entries
      .filter((item) => {
        if (folderFilter === "__favorites__" && !item.favorite) return false;
        if (folderFilter === "__unfiled__" && item.folder !== "") return false;
        if (
          folderFilter !== "__all__" &&
          folderFilter !== "__favorites__" &&
          folderFilter !== "__unfiled__" &&
          item.folder !== folderFilter
        ) {
          return false;
        }
        return (
          !query.trim() ||
          `${item.title} ${item.url} ${item.folder}`.toLowerCase().includes(query.trim().toLowerCase())
        );
      })
      .sort((left, right) => {
        if (sortBy === "recent") return right.createdAt - left.createdAt || left.title.localeCompare(right.title);
        if (sortBy === "modified") return right.updatedAt - left.updatedAt || left.title.localeCompare(right.title);
        if (sortBy === "title") return left.title.localeCompare(right.title, "ko", { sensitivity: "base" });
        return (
          Number(right.favorite) - Number(left.favorite) ||
          left.title.localeCompare(right.title, "ko", { sensitivity: "base" })
        );
      }),
  );

  let isNamedFolder = $derived(
    folderFilter !== "__all__" && folderFilter !== "__favorites__" && folderFilter !== "__unfiled__",
  );

  async function load() {
    try {
      const [loadedEntries, loadedFolders] = await Promise.all([
        api.listBookmarks(),
        api.listBookmarkFolders(),
      ]);
      entries = loadedEntries;
      explicitFolders = loadedFolders;
    } catch (error) {
      onnotify("error", errorMessage(error));
    }
  }

  async function addCurrent() {
    if (!current) return;
    try {
      const url = normalizeBookmarkUrl(current.url);
      await load();
      const existing = entries.find((item) => {
        try {
          return normalizeBookmarkUrl(item.url) === url;
        } catch {
          return false;
        }
      });
      if (existing) {
        editing = { ...existing };
        onnotify("info", t("bmPanel.alreadySaved"));
        return;
      }
      const now = Math.floor(Date.now() / 1000);
      editing = {
        id: crypto.randomUUID(),
        url,
        title: current.title || url,
        folder: isNamedFolder ? folderFilter : "",
        favorite: false,
        createdAt: now,
        updatedAt: now,
      };
    } catch (error) {
      onnotify("error", errorMessage(error));
    }
  }

  async function save() {
    if (!editing) return;
    try {
      const folder = editing.folder.trim();
      if (folder) await api.createBookmarkFolder(folder);
      await api.saveBookmark({ ...editing, folder });
      editing = null;
      await load();
      onnotify("success", t("bmPanel.savedToast"));
    } catch (error) {
      onnotify("error", errorMessage(error));
    }
  }

  async function remove(item: Bookmark) {
    confirmDeleteBookmarkId = null;
    try {
      await api.deleteBookmark(item.id);
      await load();
    } catch (error) {
      onnotify("error", errorMessage(error));
    }
  }

  async function favorite(item: Bookmark) {
    try {
      await api.saveBookmark({ ...item, favorite: !item.favorite });
      await load();
    } catch (error) {
      onnotify("error", errorMessage(error));
    }
  }

  function startCreateFolder() {
    folderMode = "create";
    folderDraft = "";
  }

  function startRenameFolder() {
    if (!isNamedFolder) return;
    folderMode = "rename";
    folderDraft = folderFilter;
  }

  function startDeleteFolder() {
    if (!isNamedFolder) return;
    folderMode = "delete";
  }

  async function submitCreateFolder() {
    const name = folderDraft.trim();
    if (!name) return;
    try {
      await api.createBookmarkFolder(name);
      folderFilter = name;
      folderMode = null;
      folderDraft = "";
      await load();
      onnotify("success", t("bmPanel.folderCreated", { name }));
    } catch (error) {
      onnotify("error", errorMessage(error));
    }
  }

  async function submitRenameFolder() {
    if (!isNamedFolder) return;
    const next = folderDraft.trim();
    if (!next || next === folderFilter) {
      folderMode = null;
      return;
    }
    try {
      const count = await api.renameBookmarkFolder(folderFilter, next);
      folderFilter = next;
      folderMode = null;
      folderDraft = "";
      await load();
      onnotify("success", t("bmPanel.folderRenamed", { count }));
    } catch (error) {
      onnotify("error", errorMessage(error));
    }
  }

  async function confirmDeleteFolder() {
    if (!isNamedFolder) return;
    try {
      await api.deleteBookmarkFolder(folderFilter);
      folderFilter = "__all__";
      folderMode = null;
      await load();
      onnotify("success", t("bmPanel.folderDeleted"));
    } catch (error) {
      onnotify("error", errorMessage(error));
    }
  }
</script>

<div
  class="backdrop"
  class:embedded
  role="presentation"
  onclick={(e) => {
    if (!embedded && e.target === e.currentTarget) onclose();
  }}
>
  <div class="panel" class:embedded role="dialog" aria-modal={!embedded} aria-label={t("bmPanel.aria")} tabindex="-1">
    <header>
      <div>
        <h2>{t("bmPanel.title")}</h2>
        <p>{t("bmPanel.subtitle")}</p>
      </div>
      {#if !embedded}
        <button onclick={onclose} aria-label={t("common.close")}>×</button>
      {/if}
    </header>

    <div class="layout">
      <aside class="folders" aria-label={t("bmPanel.folders")}>
        <div class="folders-head">
          <span>{t("bmPanel.folders")}</span>
          <button type="button" class="mini" onclick={startCreateFolder}>{t("bmPanel.newFolder")}</button>
        </div>
        {#if folderMode === "create"}
          <form
            class="folder-inline-form"
            onsubmit={(e) => {
              e.preventDefault();
              void submitCreateFolder();
            }}
          >
            <input type="text" placeholder={t("bmPanel.newFolderPlaceholder")} bind:value={folderDraft} />
            <div class="folder-inline-actions">
              <button type="submit" class="mini primary">{t("common.add")}</button>
              <button type="button" class="mini" onclick={() => (folderMode = null)}>{t("common.cancel")}</button>
            </div>
          </form>
        {/if}
        <button
          type="button"
          class="folder-item"
          class:active={folderFilter === "__all__"}
          onclick={() => {
            folderMode = null;
            folderFilter = "__all__";
          }}
        >
          <span>{t("bmPanel.all")}</span>
          <small>{entries.length}</small>
        </button>
        <button
          type="button"
          class="folder-item"
          class:active={folderFilter === "__favorites__"}
          onclick={() => {
            folderMode = null;
            folderFilter = "__favorites__";
          }}
        >
          <span>{t("bmPanel.favorites")}</span>
          <small>{entries.filter((b) => b.favorite).length}</small>
        </button>
        <button
          type="button"
          class="folder-item"
          class:active={folderFilter === "__unfiled__"}
          onclick={() => {
            folderMode = null;
            folderFilter = "__unfiled__";
          }}
        >
          <span>{t("bmPanel.unfiled")}</span>
          <small>{entries.filter((b) => !b.folder).length}</small>
        </button>
        {#if folders.length > 0}
          <div class="folder-divider"></div>
          {#each folders as folder (folder)}
            <button
              type="button"
              class="folder-item"
              class:active={folderFilter === folder}
              onclick={() => {
                folderMode = null;
                folderFilter = folder;
              }}
            >
              <span class="folder-name">📁 {folder}</span>
              <small>{entries.filter((b) => b.folder === folder).length}</small>
            </button>
          {/each}
        {/if}
      </aside>

      <div class="main">
        <div class="tools">
          <input type="search" placeholder={t("bmPanel.searchPlaceholder")} bind:value={query} />
          {#if isNamedFolder}
            <button type="button" onclick={startRenameFolder}>{t("bmPanel.renameFolderBtn")}</button>
            <button type="button" onclick={startDeleteFolder}>{t("bmPanel.deleteFolderBtn")}</button>
          {/if}
          <select bind:value={sortBy} aria-label={t("bmPanel.sortAria")}>
            <option value="favorite">{t("bmPanel.sortFav")}</option>
            <option value="title">{t("bmPanel.sortTitle")}</option>
            <option value="recent">{t("bmPanel.sortRecent")}</option>
            <option value="modified">{t("bmPanel.sortModified")}</option>
          </select>
          {#if current && !current.url.startsWith("tb://")}
            <button class="primary" onclick={addCurrent}>{t("bmPanel.saveCurrent")}</button>
          {/if}
        </div>

        {#if folderMode === "rename" && isNamedFolder}
          <form
            class="editor"
            onsubmit={(e) => {
              e.preventDefault();
              void submitRenameFolder();
            }}
          >
            <input type="text" placeholder={t("bmPanel.renameFolderPlaceholder")} bind:value={folderDraft} style="flex:1" />
            <button class="primary" type="submit">{t("common.save")}</button>
            <button type="button" onclick={() => (folderMode = null)}>{t("common.cancel")}</button>
          </form>
        {:else if folderMode === "delete" && isNamedFolder}
          <div class="editor">
            <span style="flex:1;font-size:12px">{t("bmPanel.deleteFolderConfirm", { name: folderFilter })}</span>
            <button class="primary" type="button" onclick={() => void confirmDeleteFolder()}>{t("common.confirmDelete")}</button>
            <button type="button" onclick={() => (folderMode = null)}>{t("common.cancel")}</button>
          </div>
        {/if}

        {#if editing}
          <form
            class="editor"
            onsubmit={(e) => {
              e.preventDefault();
              void save();
            }}
          >
            <label>{t("bmPanel.nameLabel")}<input bind:value={editing.title} /></label>
            <label>{t("bmPanel.urlLabel")}<input bind:value={editing.url} /></label>
            <label>
              {t("bmPanel.folderLabel")}
              <input bind:value={editing.folder} list="bookmark-folders" placeholder={t("bmPanel.folderPlaceholder")} />
              <datalist id="bookmark-folders">
                {#each folders as folder}<option value={folder}></option>{/each}
              </datalist>
            </label>
            <label class="check"><input type="checkbox" bind:checked={editing.favorite} /> {t("bmPanel.favCheck")}</label>
            <button type="button" onclick={() => (editing = null)}>{t("common.cancel")}</button>
            <button class="primary">{t("common.save")}</button>
          </form>
        {/if}

        <ul>
          {#each filtered as item (item.id)}
            <li>
              <button
                class="open"
                onclick={() => {
                  onnavigate(item.url);
                  if (!embedded) onclose();
                }}
              >
                <strong>{item.title}</strong>
                <small>{item.folder || t("bmPanel.unfiled")} · {item.url}</small>
              </button>
              <button title={t("bmPanel.favTitle")} onclick={() => void favorite(item)}>{item.favorite ? "★" : "☆"}</button>
              <button onclick={() => (editing = { ...item })}>{t("common.edit")}</button>
              {#if confirmDeleteBookmarkId === item.id}
                <button class="primary" onclick={() => void remove(item)}>{t("common.confirmDelete")}</button>
                <button onclick={() => (confirmDeleteBookmarkId = null)}>{t("common.cancel")}</button>
              {:else}
                <button onclick={() => (confirmDeleteBookmarkId = item.id)}>{t("common.delete")}</button>
              {/if}
            </li>
          {/each}
        </ul>
        {#if filtered.length === 0}<p class="empty">{t("bmPanel.empty")}</p>{/if}
      </div>
    </div>
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
    width: min(940px, 100%);
    height: min(640px, 88vh);
    padding: 20px;
    border: 1px solid var(--border);
    border-radius: 16px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 24px 70px #0004;
    overflow: hidden;
  }
  .panel.embedded {
    width: 100%;
    height: 100%;
    border: 0;
    border-radius: 0;
    box-shadow: none;
  }
  header,
  .tools,
  li,
  .editor,
  .folders-head {
    display: flex;
    align-items: center;
    gap: 9px;
  }
  header,
  .folders-head {
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
  .layout {
    display: grid;
    grid-template-columns: 200px minmax(0, 1fr);
    gap: 16px;
    flex: 1;
    min-height: 0;
    margin-top: 12px;
  }
  .folders {
    display: flex;
    flex-direction: column;
    gap: 4px;
    padding-right: 12px;
    border-right: 1px solid var(--border);
    overflow-y: auto;
  }
  .folders-head {
    margin-bottom: 6px;
    font-size: 12px;
    font-weight: 600;
    color: var(--text-muted);
  }
  .folder-inline-form {
    display: grid;
    gap: 6px;
    margin-bottom: 6px;
    padding: 8px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
  }
  .folder-inline-form input {
    height: 28px;
    font-size: 12px;
  }
  .folder-inline-actions {
    display: flex;
    gap: 4px;
    justify-content: flex-end;
  }
  .folder-item {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 8px;
    width: 100%;
    border: 0;
    border-radius: 8px;
    padding: 7px 10px;
    background: transparent;
    color: var(--text);
    text-align: left;
    font-size: 12.5px;
  }
  .folder-item:hover {
    background: var(--surface-hover);
  }
  .folder-item.active {
    background: var(--accent-soft);
    color: var(--accent);
    font-weight: 600;
  }
  .folder-name {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
  }
  .folder-divider {
    height: 1px;
    margin: 4px 0;
    background: var(--border);
  }
  .main {
    display: flex;
    flex-direction: column;
    min-width: 0;
    min-height: 0;
  }
  button,
  input,
  select {
    font: inherit;
  }
  button {
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--surface);
    color: var(--text);
    padding: 7px 10px;
    cursor: pointer;
    font-size: 12px;
  }
  button.mini {
    padding: 4px 8px;
    font-size: 11px;
  }
  input,
  select {
    min-width: 0;
    height: 34px;
    padding: 0 9px;
    border: 1px solid var(--border);
    border-radius: 7px;
    background: var(--surface);
    color: var(--text);
  }
  .tools {
    margin-bottom: 10px;
    flex-wrap: wrap;
  }
  .tools input {
    flex: 1 1 180px;
  }
  .tools select {
    flex: 0 1 140px;
  }
  .primary {
    background: var(--accent);
    color: white;
    border-color: transparent;
  }
  .editor {
    flex-wrap: wrap;
    padding: 10px;
    margin-bottom: 10px;
    background: var(--surface-2);
    border-radius: 8px;
  }
  .editor label {
    display: grid;
    gap: 4px;
    flex: 1;
    min-width: 160px;
    font-size: 11px;
  }
  .editor .check {
    display: flex;
    align-items: center;
  }
  .editor .check input {
    height: auto;
  }
  ul {
    flex: 1;
    overflow: auto;
    list-style: none;
    padding: 0;
    margin: 0;
  }
  li {
    padding: 8px 0;
    border-bottom: 1px solid var(--border);
  }
  .open {
    display: grid;
    flex: 1;
    text-align: left;
    border: 0;
    background: transparent;
  }
  .open small {
    overflow: hidden;
    text-overflow: ellipsis;
    white-space: nowrap;
    color: var(--text-muted);
  }
  .empty {
    text-align: center;
    padding: 24px;
    color: var(--text-muted);
  }
</style>
