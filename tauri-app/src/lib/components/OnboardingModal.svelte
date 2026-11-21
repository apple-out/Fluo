<script lang="ts">
  import { getUiLang, t, UI_LANGUAGES } from "$lib/i18n.svelte";
  import type { UiLanguage } from "$lib/types";

  let {
    firstRun = false,
    currentTargetLanguage = "한국어",
    onlanguagechange,
    oncomplete,
    onclose,
  }: {
    firstRun?: boolean;
    currentTargetLanguage?: string;
    onlanguagechange: (lang: UiLanguage) => Promise<void> | void;
    oncomplete: (targetLang: string) => Promise<void> | void;
    onclose: () => void;
  } = $props();

  // Capture initial prop value intentionally as editable draft state for initial setup.
  // svelte-ignore state_referenced_locally
  let draftTargetLang = $state(currentTargetLanguage || "한국어");
  let busy = $state(false);

  const TARGET_PRESETS = [
    "한국어",
    "English",
    "日本語",
    "中文(简体)",
    "中文(繁體)",
    "Español",
    "Français",
    "Deutsch",
  ];

  function defaultTargetForUi(lang: UiLanguage): string {
    switch (lang) {
      case "en":
        return "English";
      case "ja":
        return "日本語";
      case "zh-CN":
        return "中文(简体)";
      default:
        return "한국어";
    }
  }

  async function selectLanguage(lang: UiLanguage) {
    if (busy) return;
    draftTargetLang = defaultTargetForUi(lang);
    busy = true;
    try {
      await onlanguagechange(lang);
    } finally {
      busy = false;
    }
  }

  async function finish() {
    if (busy) return;
    busy = true;
    try {
      const target = draftTargetLang.trim() || defaultTargetForUi(getUiLang());
      await oncomplete(target);
    } finally {
      busy = false;
    }
  }
</script>

<div
  class="onb-backdrop"
  role="presentation"
  onclick={(event) => {
    if (!firstRun && event.target === event.currentTarget) onclose();
  }}
>
  <div class="onb-modal" role="dialog" aria-modal="true" aria-label={t("onb.aria")}>
    <header class="onb-head">
      <div class="onb-brand">
        <img src="/favicon.png" alt="Fluo" class="onb-logo" />
        <div>
          <div class="onb-title-row">
            <strong>{t("app.name")}</strong>
            <span class="onb-pill-tagline">{t("app.tagline")}</span>
          </div>
          <p class="onb-subline">{t("app.taglineLocal")}</p>
        </div>
      </div>
      <button
        type="button"
        class="onb-close"
        aria-label={t("common.close")}
        onclick={onclose}
        disabled={busy}
      >
        ×
      </button>
    </header>

    <div class="onb-body">
      <h2>{t("onb.title")}</h2>
      <p class="onb-lead">{t("onb.desc")}</p>

      <div class="onb-lang-grid" role="radiogroup" aria-label={t("set.appLangLabel")}>
        {#each UI_LANGUAGES as lang (lang.code)}
          <button
            type="button"
            role="radio"
            aria-checked={getUiLang() === lang.code}
            class="onb-lang-card"
            class:selected={getUiLang() === lang.code}
            disabled={busy}
            onclick={() => void selectLanguage(lang.code)}
          >
            <strong>{lang.nativeName}</strong>
            <small>{lang.shortLabel}</small>
          </button>
        {/each}
      </div>

      <label class="onb-field">
        <span>{t("onb.targetLabel")}</span>
        <input
          type="text"
          list="onb-target-presets"
          bind:value={draftTargetLang}
          placeholder={t("translate.targetLangPlaceholder")}
        />
        <datalist id="onb-target-presets">
          {#each TARGET_PRESETS as item (item)}
            <option value={item}></option>
          {/each}
        </datalist>
        <small>{t("onb.targetHint")}</small>
      </label>
    </div>

    <footer class="onb-foot">
      <button type="button" class="onb-primary" onclick={() => void finish()} disabled={busy}>
        {t("onb.startBtn")}
      </button>
    </footer>
  </div>
</div>

<style>
  .onb-backdrop {
    position: fixed;
    inset: 0;
    z-index: 90;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
    background: rgba(8, 11, 20, 0.56);
    -webkit-backdrop-filter: blur(8px);
    backdrop-filter: blur(8px);
  }

  .onb-modal {
    width: min(620px, 100%);
    max-height: min(600px, calc(100vh - 40px));
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

  .onb-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
    padding: 18px 22px 14px;
    border-bottom: 1px solid var(--glass-border, var(--border));
  }

  .onb-brand {
    display: flex;
    align-items: center;
    gap: 12px;
  }

  .onb-logo {
    width: 44px;
    height: 44px;
    border-radius: 12px;
    box-shadow: 0 8px 20px rgba(52, 86, 255, 0.3);
    flex: 0 0 auto;
  }

  .onb-title-row {
    display: flex;
    align-items: center;
    gap: 8px;
    flex-wrap: wrap;
  }

  .onb-title-row strong {
    font-size: 18px;
    letter-spacing: -0.02em;
  }

  .onb-pill-tagline {
    font-size: 11px;
    font-weight: 700;
    padding: 2px 9px;
    border-radius: 999px;
    background: var(--accent-soft);
    color: var(--accent);
  }

  .onb-subline {
    margin: 3px 0 0;
    font-size: 12px;
    color: var(--text-muted);
  }

  .onb-close {
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

  .onb-body {
    flex: 1;
    overflow-y: auto;
    padding: 20px 22px;
  }

  .onb-body h2 {
    margin: 0 0 6px;
    font-size: 17.5px;
    letter-spacing: -0.02em;
  }

  .onb-lead {
    margin: 0 0 16px;
    font-size: 13px;
    line-height: 1.55;
    color: var(--text-muted);
  }

  .onb-lang-grid {
    display: grid;
    grid-template-columns: repeat(2, minmax(0, 1fr));
    gap: 10px;
    margin-bottom: 16px;
  }

  @media (max-width: 560px) {
    .onb-lang-grid {
      grid-template-columns: 1fr;
    }
  }

  .onb-lang-card {
    display: flex;
    align-items: center;
    justify-content: space-between;
    padding: 14px 16px;
    border-radius: 16px;
    border: 1.5px solid var(--glass-border, var(--border));
    background: var(--glass-pill, var(--surface-2));
    color: var(--text);
    font: inherit;
    cursor: pointer;
    text-align: left;
    transition: transform 0.16s ease, border-color 0.16s ease, box-shadow 0.16s ease;
  }

  .onb-lang-card:hover {
    border-color: var(--accent);
    transform: translateY(-1px);
  }

  .onb-lang-card.selected {
    border-color: var(--accent);
    background: color-mix(in srgb, var(--accent) 12%, var(--glass-pill-active, transparent));
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  .onb-lang-card strong {
    font-size: 15px;
  }

  .onb-lang-card small {
    font-size: 12px;
    color: var(--text-muted);
    font-weight: 700;
  }

  .onb-field {
    display: flex;
    flex-direction: column;
    gap: 6px;
    font-size: 12.5px;
  }

  .onb-field span {
    font-weight: 600;
  }

  .onb-field input {
    height: 38px;
    padding: 0 14px;
    border-radius: 999px;
    border: 1px solid var(--glass-border, var(--border));
    background: var(--glass-pill-active, var(--surface-2));
    color: var(--text);
    font: inherit;
    font-size: 13px;
    outline: none;
  }

  .onb-field input:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  .onb-field small {
    font-size: 11.5px;
    color: var(--text-muted);
  }

  .onb-foot {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 10px;
    padding: 14px 22px;
    border-top: 1px solid var(--glass-border, var(--border));
    background: var(--glass-pill, var(--surface-2));
  }

  .onb-primary {
    height: 38px;
    padding: 0 20px;
    border-radius: 999px;
    font: inherit;
    font-size: 13px;
    font-weight: 600;
    cursor: pointer;
    border: 1px solid transparent;
    background: var(--primary-grad, linear-gradient(135deg, #684cff, #0ea5e9));
    color: #fff;
    box-shadow: 0 8px 22px rgba(104, 76, 255, 0.35);
  }

  .onb-primary:disabled {
    opacity: 0.55;
    cursor: default;
  }
</style>
