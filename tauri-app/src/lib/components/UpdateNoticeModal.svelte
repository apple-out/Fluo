<script lang="ts">
  import { t } from "$lib/i18n.svelte";
  import type { AppUpdateInfo } from "$lib/types";

  let {
    info,
    ondownload,
    onignore,
    ondontshowagain,
  }: {
    info: AppUpdateInfo | null;
    ondownload: () => Promise<void> | void;
    onignore: () => Promise<void> | void;
    ondontshowagain: () => Promise<void> | void;
  } = $props();

  let busy = $state(false);

  async function runAction(fn: () => Promise<void> | void) {
    if (busy) return;
    busy = true;
    try {
      await fn();
    } finally {
      busy = false;
    }
  }
</script>

<div
  class="upd-backdrop"
  role="presentation"
  onclick={(event) => {
    if (event.target === event.currentTarget) void runAction(onignore);
  }}
>
  <div class="upd-modal" role="dialog" aria-modal="true" aria-label={t("update.title")}>
    <header class="upd-head">
      <div class="upd-brand">
        <img src="/favicon.png" alt="Fluo" class="upd-logo" />
        <div>
          <div class="upd-title-row">
            <strong>{t("update.title")}</strong>
            <span class="upd-pill">{t("update.badge")}</span>
          </div>
          {#if info}
            <p class="upd-subline">
              {t("update.versionRange", {
                current: info.currentVersion,
                latest: info.latestVersion,
              })}
              {#if info.releaseName}
                · {info.releaseName}
              {/if}
            </p>
          {/if}
        </div>
      </div>
      <button
        type="button"
        class="upd-close"
        aria-label={t("common.close")}
        onclick={() => void runAction(onignore)}
        disabled={busy}
      >
        ×
      </button>
    </header>

    <div class="upd-body">
      <div class="upd-notes-label">{t("update.notesLabel")}</div>
      <pre class="upd-notes-box">{info?.releaseNotes?.trim() || t("update.noNotes")}</pre>
    </div>

    <footer class="upd-foot">
      <button
        type="button"
        class="upd-ghost-danger"
        disabled={busy}
        onclick={() => void runAction(ondontshowagain)}
      >
        {t("update.dontShowAgain")}
      </button>
      <div class="upd-actions-right">
        <button
          type="button"
          class="upd-secondary"
          disabled={busy}
          onclick={() => void runAction(onignore)}
        >
          {t("update.ignore")}
        </button>
        <button
          type="button"
          class="upd-primary"
          disabled={busy}
          onclick={() => void runAction(ondownload)}
        >
          {t("update.download")}
        </button>
      </div>
    </footer>
  </div>
</div>

<style>
  .upd-backdrop {
    position: fixed;
    inset: 0;
    z-index: 200;
    display: grid;
    place-items: center;
    padding: 20px;
    background: rgba(8, 11, 20, 0.56);
    -webkit-backdrop-filter: blur(8px);
    backdrop-filter: blur(8px);
  }

  .upd-modal {
    width: min(580px, 100%);
    max-height: min(560px, calc(100vh - 40px));
    display: flex;
    flex-direction: column;
    overflow: hidden;
    border-radius: 24px;
    border: 1px solid var(--glass-border-strong, var(--border));
    background: var(--glass-island, var(--surface));
    box-shadow: var(--glass-shadow, 0 24px 70px rgba(0, 0, 0, 0.45));
    -webkit-backdrop-filter: var(--glass-backdrop, blur(26px) saturate(165%));
    backdrop-filter: var(--glass-backdrop, blur(26px) saturate(165%));
    color: var(--text);
  }

  .upd-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
    padding: 18px 22px 14px;
    border-bottom: 1px solid var(--glass-border, var(--border));
  }

  .upd-brand {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .upd-logo {
    width: 42px;
    height: 42px;
    border-radius: 12px;
    box-shadow: 0 8px 20px rgba(52, 86, 255, 0.28);
    flex: 0 0 auto;
  }

  .upd-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .upd-title-row strong {
    font-size: 17px;
    letter-spacing: -0.02em;
  }

  .upd-pill {
    font-size: 11px;
    font-weight: 700;
    padding: 2px 9px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
  }

  .upd-subline {
    margin: 4px 0 0;
    font-size: 12.5px;
    color: var(--text-muted);
  }

  .upd-close {
    width: 30px;
    height: 30px;
    border-radius: 50%;
    border: 1px solid var(--glass-border, var(--border));
    background: var(--glass-pill, var(--surface-2));
    color: var(--text);
    font-size: 17px;
    cursor: pointer;
    display: grid;
    place-items: center;
    flex: 0 0 auto;
  }

  .upd-body {
    flex: 1;
    overflow-y: auto;
    padding: 18px 22px;
    display: flex;
    flex-direction: column;
    gap: 8px;
  }

  .upd-notes-label {
    font-size: 12px;
    font-weight: 700;
    color: var(--text-muted);
  }

  .upd-notes-box {
    margin: 0;
    padding: 12px 14px;
    border-radius: 14px;
    border: 1px solid var(--glass-border, var(--border));
    background: var(--glass-pill, var(--surface-2));
    font-family: inherit;
    font-size: 12.5px;
    line-height: 1.55;
    white-space: pre-wrap;
    word-break: break-word;
    max-height: 280px;
    overflow-y: auto;
  }

  .upd-foot {
    display: flex;
    align-items: center;
    justify-content: space-between;
    gap: 10px;
    flex-wrap: wrap;
    padding: 14px 22px;
    border-top: 1px solid var(--glass-border, var(--border));
    background: var(--glass-pill, var(--surface-2));
  }

  .upd-actions-right {
    display: flex;
    align-items: center;
    gap: 8px;
  }

  .upd-ghost-danger,
  .upd-secondary,
  .upd-primary {
    height: 36px;
    padding: 0 15px;
    border-radius: 999px;
    font: inherit;
    font-size: 12.5px;
    font-weight: 600;
    cursor: pointer;
  }

  .upd-ghost-danger {
    border: 1px solid transparent;
    background: transparent;
    color: var(--text-muted);
    padding: 0 8px;
  }

  .upd-ghost-danger:hover {
    color: var(--text);
    text-decoration: underline;
  }

  .upd-secondary {
    border: 1px solid var(--glass-border, var(--border));
    background: var(--glass-pill-active, var(--surface-2));
    color: var(--text);
  }

  .upd-primary {
    border: 1px solid transparent;
    background: var(--primary-grad, linear-gradient(135deg, #684cff, #0ea5e9));
    color: #fff;
    box-shadow: 0 8px 22px rgba(104, 76, 255, 0.35);
  }

  button:disabled {
    opacity: 0.55;
    cursor: default;
  }
</style>
