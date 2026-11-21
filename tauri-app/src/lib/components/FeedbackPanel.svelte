<script lang="ts">
  import { onMount, untrack } from "svelte";
  import { api, errorMessage } from "$lib/api";
  import { t } from "$lib/i18n.svelte";
  import type {
    FeedbackCategory,
    FeedbackConfigStatus,
    FeedbackDraft,
    FeedbackSubmitResult,
  } from "$lib/types";

  let {
    initialDraft = null,
    currentUrl = "",
    onclose,
    onnotify,
  }: {
    initialDraft?: FeedbackDraft | null;
    currentUrl?: string;
    onclose: () => void;
    onnotify: (kind: "info" | "success" | "error", text: string) => void;
  } = $props();

  let category = $state<FeedbackCategory>("bug");
  let summary = $state("");
  let description = $state("");
  let pageUrl = $state("");
  let sourceText = $state("");
  let translatedText = $state("");
  let suggestedTranslation = $state("");
  let contact = $state("");
  let includeDiagnostics = $state(true);

  let config = $state<FeedbackConfigStatus>({
    repoUrl: "https://github.com/apple-out/transratebrowser",
    issuesUrl: "https://github.com/apple-out/transratebrowser/issues",
    directSendConfigured: false,
    endpointSource: "none",
  });
  let submitting = $state(false);
  let savingFile = $state(false);
  let fallbackResult = $state<FeedbackSubmitResult | null>(null);
  let showTranslationContext = $state(false);

  $effect(() => {
    const d = initialDraft;
    const urlSnapshot = untrack(() => currentUrl);
    const nextCat = d?.category ?? "bug";
    const nextSource = d?.sourceText ?? "";
    const nextTranslated = d?.translatedText ?? "";
    category = nextCat;
    summary = d?.summary ?? "";
    description = d?.description ?? "";
    pageUrl = d?.pageUrl || (urlSnapshot.startsWith("http") ? urlSnapshot : "");
    sourceText = nextSource;
    translatedText = nextTranslated;
    suggestedTranslation = "";
    showTranslationContext = Boolean(
      nextCat === "translation" || nextSource.trim() || nextTranslated.trim(),
    );
    fallbackResult = null;
  });

  onMount(() => {
    void api
      .getFeedbackConfig()
      .then((cfg) => {
        config = cfg;
      })
      .catch(() => {});
  });

  function selectCategory(next: FeedbackCategory) {
    category = next;
    if (next === "translation") {
      showTranslationContext = true;
    }
    fallbackResult = null;
  }

  async function submitReport() {
    if (submitting) return;
    if (
      !summary.trim() &&
      !description.trim() &&
      !sourceText.trim() &&
      !translatedText.trim()
    ) {
      onnotify("error", t("fb.needContent"));
      return;
    }
    submitting = true;
    try {
      const result = await api.submitFeedbackReport({
        category,
        summary: summary.trim(),
        description: description.trim(),
        pageUrl: pageUrl.trim() || null,
        sourceText: sourceText.trim() || null,
        translatedText: translatedText.trim() || null,
        suggestedTranslation: suggestedTranslation.trim() || null,
        contact: contact.trim() || null,
        includeDiagnostics,
      });
      if (result.delivered) {
        onnotify("success", result.issueUrl ? `${result.message} (${result.issueUrl})` : result.message);
        onclose();
      } else {
        fallbackResult = result;
        onnotify("info", result.message);
      }
    } catch (error) {
      onnotify("error", errorMessage(error));
    } finally {
      submitting = false;
    }
  }

  async function openIssuePage(url: string) {
    try {
      await api.openFeedbackUrl(url);
      onnotify("info", t("fb.openedGithubToast"));
    } catch (error) {
      onnotify("error", errorMessage(error));
    }
  }

  async function copyFullReport(result: FeedbackSubmitResult) {
    try {
      const fullText = `# ${result.title}\n\n${result.body}`;
      await navigator.clipboard.writeText(fullText);
      onnotify("success", t("fb.copiedReportToast"));
    } catch (error) {
      onnotify("error", errorMessage(error));
    }
  }

  async function saveReportToFile(result: FeedbackSubmitResult) {
    if (savingFile) return;
    savingFile = true;
    try {
      const path = await api.saveFeedbackReportFile(result.title, result.body);
      onnotify("success", t("fb.savedFileToast", { path }));
    } catch (error) {
      onnotify("error", errorMessage(error));
    } finally {
      savingFile = false;
    }
  }
</script>

<div
  class="backdrop"
  role="presentation"
  onclick={(event) => {
    if (event.target === event.currentTarget) onclose();
  }}
>
  <div class="panel" role="dialog" aria-modal="true" aria-label={t("fb.aria")}>
    <header class="panel-head">
      <div>
        <h2>{t("fb.title")}</h2>
        <p>
          {t("fb.subtitle")}
          <span class="status-pill" class:live={config.directSendConfigured}>
            {config.directSendConfigured ? t("fb.badgeRelay") : t("fb.badgeIssues")}
          </span>
        </p>
      </div>
      <button class="icon-button" type="button" aria-label={t("common.close")} onclick={onclose}>×</button>
    </header>

    <div class="panel-body">
      <div class="category-row" role="tablist" aria-label={t("fb.catAria")}>
        <button
          type="button"
          class="cat-btn"
          class:active={category === "bug"}
          onclick={() => selectCategory("bug")}
        >
          {t("fb.catBug")}
        </button>
        <button
          type="button"
          class="cat-btn"
          class:active={category === "ux"}
          onclick={() => selectCategory("ux")}
        >
          {t("fb.catUx")}
        </button>
        <button
          type="button"
          class="cat-btn"
          class:active={category === "translation"}
          onclick={() => selectCategory("translation")}
        >
          {t("fb.catTranslation")}
        </button>
      </div>

      <label class="field">
        <span>{t("fb.summary")}</span>
        <input
          type="text"
          maxlength="120"
          placeholder={category === "translation"
            ? t("fb.summaryPhTranslation")
            : category === "ux"
              ? t("fb.summaryPhUx")
              : t("fb.summaryPhBug")}
          bind:value={summary}
        />
      </label>

      <label class="field">
        <span>{t("fb.description")}</span>
        <textarea
          rows="4"
          placeholder={t("fb.descriptionPh")}
          bind:value={description}
        ></textarea>
      </label>

      <div class="toggle-line">
        <button
          type="button"
          class="link-btn"
          onclick={() => (showTranslationContext = !showTranslationContext)}
        >
          {showTranslationContext ? t("fb.toggleContextOpen") : t("fb.toggleContextClosed")}
        </button>
      </div>

      {#if showTranslationContext}
        <div class="translation-box">
          <label class="field">
            <span>{t("fb.pageUrl")}</span>
            <input type="url" placeholder="https://..." bind:value={pageUrl} />
          </label>
          <div class="grid-two">
            <label class="field">
              <span>{t("fb.sourceText")}</span>
              <textarea rows="3" placeholder={t("fb.sourceTextPh")} bind:value={sourceText}></textarea>
            </label>
            <label class="field">
              <span>{t("fb.translatedText")}</span>
              <textarea rows="3" placeholder={t("fb.translatedTextPh")} bind:value={translatedText}></textarea>
            </label>
          </div>
          <label class="field">
            <span>{t("fb.suggested")}</span>
            <input
              type="text"
              placeholder={t("fb.suggestedPh")}
              bind:value={suggestedTranslation}
            />
          </label>
        </div>
      {/if}

      <div class="grid-two">
        <label class="field">
          <span>{t("fb.contact")}</span>
          <input
            type="text"
            maxlength="120"
            placeholder={t("fb.contactPh")}
            bind:value={contact}
          />
        </label>
        <div class="field diag-wrap">
          <span>{t("fb.diagTitle")}</span>
          <label class="check-field">
            <input type="checkbox" bind:checked={includeDiagnostics} />
            <span>{t("fb.diagCheck")}</span>
          </label>
        </div>
      </div>

      {#if fallbackResult}
        <div class="fallback-card">
          <div class="fallback-title">{t("fb.cardTitle")}</div>
          <p>{fallbackResult.message}</p>
          <div class="fallback-actions">
            <button
              type="button"
              class="primary"
              onclick={() => void openIssuePage(fallbackResult!.githubIssueUrl)}
            >
              {t("fb.openGithubBtn")}
            </button>
            <button
              type="button"
              class="ghost"
              onclick={() => void copyFullReport(fallbackResult!)}
            >
              {t("fb.copyMarkdownBtn")}
            </button>
            <button
              type="button"
              class="ghost"
              disabled={savingFile}
              onclick={() => void saveReportToFile(fallbackResult!)}
            >
              {t("fb.saveFileBtn")}
            </button>
          </div>
        </div>
      {/if}
    </div>

    <footer class="panel-foot">
      <button class="ghost" type="button" onclick={onclose} disabled={submitting}>{t("common.close")}</button>
      <button class="primary" type="button" onclick={submitReport} disabled={submitting}>
        {submitting
          ? t("fb.submitting")
          : config.directSendConfigured
            ? t("fb.submitRelay")
            : t("fb.submitPrepare")}
      </button>
    </footer>
  </div>
</div>

<style>
  .backdrop {
    position: fixed;
    inset: 0;
    z-index: 45;
    display: flex;
    align-items: center;
    justify-content: center;
    padding: 20px;
    background: rgba(10, 12, 18, 0.48);
    backdrop-filter: blur(3px);
  }

  .panel {
    display: flex;
    flex-direction: column;
    width: min(720px, 100%);
    max-height: min(680px, 90vh);
    border: 1px solid var(--border);
    border-radius: 16px;
    background: var(--surface);
    color: var(--text);
    box-shadow: 0 24px 80px rgba(0, 0, 0, 0.36);
    overflow: hidden;
  }

  .panel-head {
    display: flex;
    align-items: flex-start;
    justify-content: space-between;
    gap: 14px;
    padding: 16px 20px 13px;
    border-bottom: 1px solid var(--border);
  }

  .panel-head h2 {
    margin: 0;
    font-size: 16.5px;
  }

  .panel-head p {
    display: flex;
    align-items: center;
    flex-wrap: wrap;
    gap: 8px;
    margin: 4px 0 0;
    font-size: 12px;
    color: var(--text-muted);
  }

  .status-pill {
    padding: 2px 8px;
    border-radius: 999px;
    font-size: 11px;
    font-weight: 600;
    background: var(--surface-2);
    border: 1px solid var(--border);
    color: var(--text-soft);
  }

  .status-pill.live {
    background: var(--ok-soft, rgba(22, 163, 74, 0.14));
    border-color: rgba(22, 163, 74, 0.35);
    color: var(--ok, #15803d);
  }

  .icon-button {
    width: 30px;
    height: 30px;
    border: 1px solid var(--border);
    border-radius: 8px;
    background: var(--surface-2);
    color: var(--text);
    font-size: 17px;
    cursor: pointer;
  }

  .panel-body {
    flex: 1;
    overflow: auto;
    padding: 16px 20px;
    display: grid;
    gap: 12px;
  }

  .category-row {
    display: grid;
    grid-template-columns: repeat(3, 1fr);
    gap: 8px;
  }

  .cat-btn {
    height: 36px;
    border-radius: 10px;
    border: 1px solid var(--border-strong, var(--border));
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
    font-weight: 500;
    cursor: pointer;
  }

  .cat-btn.active {
    background: var(--accent);
    border-color: transparent;
    color: #fff;
    font-weight: 600;
  }

  .field {
    display: grid;
    gap: 5px;
    font-size: 12px;
  }

  .field > span {
    font-weight: 600;
    color: var(--text-muted);
  }

  input,
  textarea {
    width: 100%;
    box-sizing: border-box;
    padding: 8px 10px;
    border: 1px solid var(--border-strong, var(--border));
    border-radius: 9px;
    background: var(--surface-2);
    color: var(--text);
    font: inherit;
    font-size: 12.5px;
    outline: none;
  }

  textarea {
    resize: vertical;
  }

  input:focus,
  textarea:focus {
    border-color: var(--accent);
    box-shadow: 0 0 0 3px var(--accent-soft);
  }

  .toggle-line {
    display: flex;
    align-items: center;
  }

  .link-btn {
    border: 0;
    padding: 0;
    background: transparent;
    color: var(--accent);
    font: inherit;
    font-size: 12px;
    font-weight: 600;
    cursor: pointer;
  }

  .translation-box {
    display: grid;
    gap: 10px;
    padding: 12px;
    border-radius: 12px;
    border: 1px solid var(--border);
    background: color-mix(in srgb, var(--surface-2) 70%, transparent);
  }

  .grid-two {
    display: grid;
    grid-template-columns: repeat(auto-fit, minmax(230px, 1fr));
    gap: 10px;
  }

  .diag-wrap {
    align-content: start;
  }

  .check-field {
    display: flex;
    align-items: flex-start;
    gap: 8px;
    font-size: 11.5px;
    line-height: 1.45;
    color: var(--text-soft);
    cursor: pointer;
  }

  .check-field input {
    width: 16px;
    height: 16px;
    margin: 2px 0 0;
    accent-color: var(--accent);
    flex-shrink: 0;
  }

  .fallback-card {
    padding: 12px 14px;
    border-radius: 12px;
    border: 1px solid color-mix(in srgb, var(--accent) 45%, var(--border));
    background: color-mix(in srgb, var(--accent-soft) 55%, var(--surface-2));
    display: grid;
    gap: 8px;
  }

  .fallback-title {
    font-size: 12.5px;
    font-weight: 700;
  }

  .fallback-card p {
    margin: 0;
    font-size: 12px;
    line-height: 1.45;
    color: var(--text-muted);
  }

  .fallback-actions {
    display: flex;
    flex-wrap: wrap;
    gap: 8px;
  }

  .panel-foot {
    display: flex;
    align-items: center;
    justify-content: flex-end;
    gap: 8px;
    padding: 12px 20px;
    border-top: 1px solid var(--border);
  }

  .ghost,
  .primary {
    height: 34px;
    padding: 0 14px;
    border-radius: 9px;
    font: inherit;
    font-size: 12.5px;
    cursor: pointer;
  }

  .ghost {
    border: 1px solid var(--border-strong, var(--border));
    background: var(--surface-2);
    color: var(--text);
  }

  .primary {
    border: 1px solid transparent;
    background: var(--accent);
    color: #fff;
    font-weight: 600;
  }

  button:disabled {
    opacity: 0.6;
    cursor: default;
  }
</style>
