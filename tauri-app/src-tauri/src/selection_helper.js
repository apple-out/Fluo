(() => {
  if (window.__tbSelectionHelperInstalled) return;
  window.__tbSelectionHelperInstalled = true;

  const UI_I18N = {
    ko: {
      translating: "번역 중…",
      addGlossary: "용어집에 추가",
      sourcePlaceholder: "원문",
      translationPlaceholder: "번역어 (비우면 원문 유지/번역 금지)",
      cancel: "취소",
      save: "저장",
      quickTranslate: "선택 번역",
      glossaryPlus: "용어집+",
      autofillProfile: "개인정보 자동 채우기",
    },
    en: {
      translating: "Translating…",
      addGlossary: "Add to Glossary",
      sourcePlaceholder: "Source text",
      translationPlaceholder: "Translation (leave blank to keep original / do not translate)",
      cancel: "Cancel",
      save: "Save",
      quickTranslate: "Translate selection",
      glossaryPlus: "Glossary+",
      autofillProfile: "Autofill personal info",
    },
    ja: {
      translating: "翻訳中…",
      addGlossary: "用語集に追加",
      sourcePlaceholder: "原文",
      translationPlaceholder: "訳語（空欄の場合は原文維持／翻訳禁止）",
      cancel: "キャンセル",
      save: "保存",
      quickTranslate: "選択を翻訳",
      glossaryPlus: "用語集+",
      autofillProfile: "個人情報を自動入力",
    },
    "zh-CN": {
      translating: "正在翻译…",
      addGlossary: "添加到术语表",
      sourcePlaceholder: "原文",
      translationPlaceholder: "译文（留空则保留原文/禁止翻译）",
      cancel: "取消",
      save: "保存",
      quickTranslate: "翻译选中内容",
      glossaryPlus: "术语表+",
      autofillProfile: "自动填充个人信息",
    },
  };
  if (!window.__tbUiLang) window.__tbUiLang = "ko";
  window.__tbSetUiLang = (lang) => {
    window.__tbUiLang = UI_I18N[lang] ? lang : "ko";
  };
  const uiStr = () => UI_I18N[window.__tbUiLang] || UI_I18N.ko;

  let bar = null;
  let card = null;

  function postWebMessage(payload) {
    try {
      if (window.chrome?.webview?.postMessage) {
        window.chrome.webview.postMessage(JSON.stringify(payload));
        return true;
      }
    } catch {
      // Ignore postMessage failure.
    }
    return false;
  }

  function emitPageAction(payload) {
    if (!postWebMessage(payload)) {
      window.__tbPendingAction = payload;
    }
  }
  window.__tbEmitAction = emitPageAction;

  function removeBar() {
    bar?.remove();
    bar = null;
  }

  function removeCard() {
    card?.remove();
    card = null;
  }

  function positionBox(el, rect) {
    const x = Math.max(8, Math.min(rect.left, window.innerWidth - 280));
    const y = rect.bottom + 8 + 140 < window.innerHeight ? rect.bottom + 8 : Math.max(8, rect.top - 48);
    el.style.left = `${x}px`;
    el.style.top = `${y}px`;
  }

  window.__tbShowQuickResult = (resultText, isError) => {
    if (!card) return;
    const body = card.querySelector("[data-rb-card-body]");
    if (!body) return;
    body.textContent = resultText;
    if (isError) {
      body.style.color = "#ef4444";
    } else {
      body.style.color = "var(--rb-ui-text, #181c2c)";
    }
  };

  function showQuickCard(rect, title) {
    ensureTopGlassVeil();
    removeCard();
    card = document.createElement("div");
    card.id = "rb-sel-card";
    card.className = "rb-liquid-surface";
    card.style.cssText =
      "position:fixed;z-index:2147483646;width:min(336px,calc(100vw - 16px));padding:12px 14px;border-radius:18px;font:12.5px/1.55 'Pretendard',system-ui,sans-serif;";
    const head = document.createElement("div");
    head.style.cssText = "display:flex;justify-content:space-between;align-items:center;margin-bottom:7px;font-weight:600;font-size:11.5px;color:var(--rb-ui-muted);";
    const label = document.createElement("span");
    label.textContent = title;
    const closeBtn = document.createElement("button");
    closeBtn.type = "button";
    closeBtn.className = "rb-liquid-icon-btn";
    closeBtn.textContent = "×";
    closeBtn.onclick = removeCard;
    head.appendChild(label);
    head.appendChild(closeBtn);
    const body = document.createElement("div");
    body.setAttribute("data-rb-card-body", "1");
    body.style.cssText = "white-space:pre-wrap;word-break:break-word;max-height:190px;overflow:auto;color:var(--rb-ui-text);";
    body.textContent = uiStr().translating;
    card.appendChild(head);
    card.appendChild(body);
    positionBox(card, rect);
    document.documentElement.appendChild(card);
  }

  function showGlossaryCard(rect, sourceText) {
    ensureTopGlassVeil();
    removeCard();
    card = document.createElement("div");
    card.id = "rb-sel-card";
    card.className = "rb-liquid-surface";
    card.style.cssText =
      "position:fixed;z-index:2147483646;width:min(320px,calc(100vw - 16px));padding:13px 14px;border-radius:18px;font:12px/1.45 'Pretendard',system-ui,sans-serif;display:grid;gap:8px;";
    const s = uiStr();
    const title = document.createElement("div");
    title.textContent = s.addGlossary;
    title.style.cssText = "font-weight:700;font-size:12.5px;color:var(--rb-ui-text);";
    const srcInput = document.createElement("input");
    srcInput.className = "rb-liquid-input";
    srcInput.value = sourceText;
    srcInput.placeholder = s.sourcePlaceholder;
    const trInput = document.createElement("input");
    trInput.className = "rb-liquid-input";
    trInput.placeholder = s.translationPlaceholder;
    const row = document.createElement("div");
    row.style.cssText = "display:flex;justify-content:flex-end;gap:6px;margin-top:2px;";
    const cancel = document.createElement("button");
    cancel.type = "button";
    cancel.className = "rb-liquid-btn";
    cancel.textContent = s.cancel;
    cancel.onclick = removeCard;
    const save = document.createElement("button");
    save.type = "button";
    save.className = "rb-liquid-btn primary";
    save.textContent = s.save;
    save.onclick = () => {
      const src = srcInput.value.trim();
      if (!src) return;
      const tr = trInput.value.trim();
      emitPageAction({
        type: "add-glossary",
        source: src,
        translation: tr || src,
        doNotTranslate: tr.length === 0,
      });
      removeCard();
    };
    row.appendChild(cancel);
    row.appendChild(save);
    card.appendChild(title);
    card.appendChild(srcInput);
    card.appendChild(trInput);
    card.appendChild(row);
    positionBox(card, rect);
    document.documentElement.appendChild(card);
    trInput.focus();
  }

  document.addEventListener("mouseup", (event) => {
    if (event.button !== 0) return;
    const target = event.target;
    if (target && target.closest && (target.closest("#rb-sel-bar") || target.closest("#rb-sel-card") || target.closest("#rb-para-actions") || target.closest("#rb-inline-editor"))) {
      return;
    }
    setTimeout(() => {
      const sel = window.getSelection();
      const text = sel ? sel.toString().trim() : "";
      if (!text || text.length > 600 || !sel || sel.rangeCount === 0) {
        removeBar();
        return;
      }
      const rect = sel.getRangeAt(0).getBoundingClientRect();
      if (!rect || (rect.width === 0 && rect.height === 0)) {
        removeBar();
        return;
      }
      ensureTopGlassVeil();
      removeBar();
      bar = document.createElement("div");
      bar.id = "rb-sel-bar";
      bar.className = "rb-liquid-surface";
      bar.style.cssText =
        "position:fixed;z-index:2147483645;display:flex;gap:5px;padding:4px 5px;border-radius:999px;font:11.5px/1 'Pretendard',system-ui,sans-serif;";
      const s = uiStr();
      const trBtn = document.createElement("button");
      trBtn.type = "button";
      trBtn.className = "rb-liquid-btn primary";
      trBtn.textContent = s.quickTranslate;
      trBtn.onclick = () => {
        removeBar();
        showQuickCard(rect, s.quickTranslate);
        emitPageAction({ type: "quick-translate", text });
      };
      const glBtn = document.createElement("button");
      glBtn.type = "button";
      glBtn.className = "rb-liquid-btn";
      glBtn.textContent = s.glossaryPlus;
      glBtn.onclick = () => {
        removeBar();
        showGlossaryCard(rect, text);
      };
      bar.appendChild(trBtn);
      bar.appendChild(glBtn);
      positionBox(bar, rect);
      document.documentElement.appendChild(bar);
    }, 10);
  });

  document.addEventListener("mousedown", (event) => {
    const target = event.target;
    if (target && target.closest) {
      if (!target.closest("#rb-sel-bar")) removeBar();
      if (!target.closest("#rb-sel-card")) removeCard();
    }
  });

  let lastScrollY = window.scrollY || 0;
  let currentScrollY = lastScrollY;
  let collapseProgress = 0;
  let scrollRaf = 0;
  window.__tbScrollState = "up";
  window.__tbAdblockEnabled = window.__tbAdblockEnabled !== undefined ? window.__tbAdblockEnabled : true;

  function parseRgb(str) {
    const m = str && str.match(/rgba?\((\d+),\s*(\d+),\s*(\d+)(?:,\s*([\d.]+))?\)/);
    if (!m) return null;
    const a = m[4] !== undefined ? parseFloat(m[4]) : 1;
    if (a < 0.08) return null;
    return [parseInt(m[1], 10), parseInt(m[2], 10), parseInt(m[3], 10)];
  }

  function readBgRgb() {
    const samplePoints = [
      [Math.round(window.innerWidth * 0.5), 24],
      [Math.round(window.innerWidth * 0.25), 24],
      [Math.round(window.innerWidth * 0.75), 24],
    ];
    if (typeof document.elementsFromPoint === "function") {
      const rList = [];
      const gList = [];
      const bList = [];
      for (const [x, y] of samplePoints) {
        const els = document.elementsFromPoint(x, y) || [];
        let found = null;
        for (const el of els) {
          if (!el || (el.id && el.id.startsWith("rb-"))) continue;
          const rgb = parseRgb(getComputedStyle(el).backgroundColor);
          if (rgb) {
            found = rgb;
            break;
          }
        }
        if (found) {
          rList.push(found[0]);
          gList.push(found[1]);
          bList.push(found[2]);
        }
      }
      if (rList.length > 0) {
        const avg = (arr) => Math.round(arr.reduce((a, b) => a + b, 0) / arr.length);
        return [avg(rList), avg(gList), avg(bList)];
      }
    }
    let rgb = null;
    if (document.body) {
      rgb = parseRgb(getComputedStyle(document.body).backgroundColor);
    }
    if (!rgb && document.documentElement) {
      rgb = parseRgb(getComputedStyle(document.documentElement).backgroundColor);
    }
    if (!rgb) {
      const darkScheme =
        window.matchMedia && window.matchMedia("(prefers-color-scheme: dark)").matches;
      rgb = darkScheme ? [32, 33, 36] : [255, 255, 255];
    }
    return rgb;
  }

  function syncAdblockStyle(enabled) {
    window.__tbAdblockEnabled = Boolean(enabled);
    if (!document.documentElement) return;
    const existing = document.getElementById("rb-adblock-cosmetic");
    if (!window.__tbAdblockEnabled) {
      existing?.remove();
      return;
    }
    if (existing) return;
    const style = document.createElement("style");
    style.id = "rb-adblock-cosmetic";
    style.textContent = `
      ins.adsbygoogle,
      .adsbygoogle,
      [id^="google_ads_iframe"],
      [id^="div-gpt-ad"],
      iframe[src*="googlesyndication.com"],
      iframe[src*="doubleclick.net"],
      iframe[src*="ad.namu.live"],
      iframe[src*="taboola.com"],
      iframe[src*="outbrain.com"],
      iframe[src*="criteo."],
      iframe[src*="dable.io"],
      iframe[src*="cauly.co.kr"],
      iframe[src*="mobon.net"],
      [data-ad-client],
      [data-ad-slot],
      [data-ad-unit] {
        display: none !important;
        height: 0 !important;
        min-height: 0 !important;
        margin: 0 !important;
        padding: 0 !important;
        overflow: hidden !important;
      }
    `;
    document.documentElement.appendChild(style);
  }

  window.__tbSetAdblock = (enabled) => {
    syncAdblockStyle(enabled);
  };

  function ensureTopGlassVeil() {
    if (!document.documentElement) return;
    syncAdblockStyle(window.__tbAdblockEnabled);
    if (document.getElementById("rb-top-glass-style")) return;
    const style = document.createElement("style");
    style.id = "rb-top-glass-style";
    style.textContent = `
      :root {
        --rb-bar-h: 88px;
        --rb-fade-opacity: 1;
        --rb-page-bg: rgb(255, 255, 255);
        --rb-accent: #684cff;
        --rb-ui-bg: rgba(255, 255, 255, 0.84);
        --rb-ui-pill: rgba(255, 255, 255, 0.88);
        --rb-ui-border: rgba(15, 23, 42, 0.24);
        --rb-ui-text: #0f1422;
        --rb-ui-muted: #2d364d;
        --rb-ui-shadow:
          inset 0 1px 0 rgba(255, 255, 255, 0.92),
          inset 0 -1px 0 rgba(15, 23, 42, 0.10),
          0 6px 20px rgba(15, 23, 42, 0.14);
        --rb-scroll-thumb: rgba(15, 23, 42, 0.18);
        --rb-scroll-thumb-hover: rgba(15, 23, 42, 0.32);
        --rb-scroll-border: rgba(255, 255, 255, 0.82);
      }
      :root[data-rb-ui-dark="1"] {
        --rb-ui-bg: rgba(20, 24, 36, 0.84);
        --rb-ui-pill: rgba(30, 36, 52, 0.86);
        --rb-ui-border: rgba(255, 255, 255, 0.26);
        --rb-ui-text: #f8faff;
        --rb-ui-muted: #dce3f2;
        --rb-ui-shadow:
          inset 0 1px 0 rgba(255, 255, 255, 0.32),
          inset 0 -1px 0 rgba(0, 0, 0, 0.24),
          0 6px 20px rgba(0, 0, 0, 0.45);
        --rb-scroll-thumb: rgba(255, 255, 255, 0.22);
        --rb-scroll-thumb-hover: rgba(255, 255, 255, 0.38);
        --rb-scroll-border: rgba(255, 255, 255, 0.24);
      }
      :root[data-rb-ui-tinted="1"] {
        --rb-ui-bg: #f4f6fb;
        --rb-ui-pill: #ffffff;
        --rb-ui-border: rgba(15, 23, 42, 0.24);
        --rb-scroll-thumb: rgba(15, 23, 42, 0.24);
        --rb-scroll-thumb-hover: rgba(15, 23, 42, 0.42);
        --rb-scroll-border: rgba(255, 255, 255, 0.6);
      }
      :root[data-rb-ui-dark="1"][data-rb-ui-tinted="1"] {
        --rb-ui-bg: #232630;
        --rb-ui-pill: #2d3140;
        --rb-ui-border: rgba(255, 255, 255, 0.16);
        --rb-scroll-thumb: rgba(255, 255, 255, 0.24);
        --rb-scroll-thumb-hover: rgba(255, 255, 255, 0.42);
        --rb-scroll-border: rgba(255, 255, 255, 0.14);
      }
      @keyframes rbPopIn {
        0% {
          opacity: 0;
          transform: translateY(-10px) scale(0.86);
        }
        70% {
          opacity: 1;
          transform: translateY(2px) scale(1.025);
        }
        100% {
          opacity: 1;
          transform: translateY(0) scale(1);
        }
      }
      .rb-liquid-surface {
        background: var(--rb-ui-bg) !important;
        color: var(--rb-ui-text) !important;
        border: 1px solid var(--rb-ui-border) !important;
        box-shadow: var(--rb-ui-shadow) !important;
        -webkit-backdrop-filter: blur(28px) saturate(2.15) !important;
        backdrop-filter: blur(28px) saturate(2.15) !important;
        animation: rbPopIn 0.28s cubic-bezier(0.2, 1.24, 0.32, 1) !important;
      }
      .rb-liquid-btn {
        height: 26px !important;
        padding: 0 11px !important;
        border: 1px solid var(--rb-ui-border) !important;
        border-radius: 999px !important;
        background: var(--rb-ui-pill) !important;
        color: var(--rb-ui-text) !important;
        font: 600 11.5px/1 'Pretendard', system-ui, sans-serif !important;
        cursor: pointer !important;
        transition: transform 0.16s ease, filter 0.16s ease !important;
      }
      .rb-liquid-btn:hover {
        transform: translateY(-1px) scale(1.02) !important;
        filter: brightness(1.05) !important;
      }
      .rb-liquid-btn.primary {
        background: color-mix(in srgb, var(--rb-accent) 78%, transparent) !important;
        border-color: rgba(255, 255, 255, 0.42) !important;
        color: #ffffff !important;
        box-shadow: inset 0 1px 0 rgba(255, 255, 255, 0.52) !important;
      }
      .rb-liquid-input {
        width: 100% !important;
        height: 30px !important;
        padding: 0 11px !important;
        box-sizing: border-box !important;
        border: 1px solid var(--rb-ui-border) !important;
        border-radius: 999px !important;
        background: var(--rb-ui-pill) !important;
        color: var(--rb-ui-text) !important;
        font: 12px/1.4 'Pretendard', system-ui, sans-serif !important;
        outline: none !important;
      }
      .rb-liquid-input:focus {
        border-color: var(--rb-accent) !important;
        box-shadow: 0 0 0 3px color-mix(in srgb, var(--rb-accent) 22%, transparent) !important;
      }
      .rb-liquid-icon-btn {
        display: inline-grid !important;
        place-items: center !important;
        width: 22px !important;
        height: 22px !important;
        padding: 0 !important;
        border: 1px solid var(--rb-ui-border) !important;
        border-radius: 50% !important;
        background: var(--rb-ui-pill) !important;
        color: var(--rb-ui-muted) !important;
        font-size: 14px !important;
        line-height: 1 !important;
        cursor: pointer !important;
      }
      html {
        scroll-padding-top: calc(var(--rb-bar-h, 88px) + 12px) !important;
      }
      body {
        position: relative !important;
        padding-top: var(--rb-top-pad, 92px) !important;
      }
      #rb-top-veil {
        position: fixed;
        top: 0;
        left: 0;
        right: 0;
        height: calc(var(--rb-bar-h, 88px) + 18px);
        z-index: 2147483640;
        pointer-events: none;
        will-change: opacity, transform;
      }
      #rb-top-lens {
        position: absolute;
        inset: 0;
        -webkit-backdrop-filter: blur(26px) saturate(1.85);
        backdrop-filter: blur(26px) saturate(1.85);
        -webkit-mask-image: linear-gradient(
          180deg,
          rgba(0, 0, 0, 1) 0%,
          rgba(0, 0, 0, 0.96) 72%,
          rgba(0, 0, 0, 0.42) 90%,
          transparent 100%
        );
        mask-image: linear-gradient(
          180deg,
          rgba(0, 0, 0, 1) 0%,
          rgba(0, 0, 0, 0.96) 72%,
          rgba(0, 0, 0, 0.42) 90%,
          transparent 100%
        );
      }
      #rb-top-fade {
        position: absolute;
        inset: 0;
        background: linear-gradient(
          180deg,
          color-mix(in srgb, var(--rb-page-bg) 86%, transparent) 0%,
          color-mix(in srgb, var(--rb-page-bg) 72%, transparent) 68%,
          color-mix(in srgb, var(--rb-page-bg) 24%, transparent) 88%,
          transparent 100%
        );
        opacity: 0.88;
        transition: opacity 0.22s linear;
        will-change: opacity;
      }
      #rb-popup-lens {
        position: fixed;
        inset: 0;
        z-index: 2147483639;
        pointer-events: none;
        -webkit-backdrop-filter: blur(8px) saturate(1.45);
        backdrop-filter: blur(8px) saturate(1.45);
        background: color-mix(in srgb, var(--rb-page-bg) 18%, transparent);
        opacity: 0;
        transition: opacity 0.28s cubic-bezier(0.16, 1, 0.3, 1);
      }
      #rb-popup-lens.open {
        opacity: 1;
      }
      ::-webkit-scrollbar {
        width: 9px;
        height: 9px;
        background: transparent;
      }
      ::-webkit-scrollbar-track {
        background: transparent;
        margin: 4px;
      }
      ::-webkit-scrollbar-thumb {
        border-radius: 999px;
        background: var(--rb-scroll-thumb, rgba(15, 23, 42, 0.16));
        border: 2px solid transparent;
        background-clip: padding-box;
        box-shadow: inset 0 0 0 1px var(--rb-scroll-border, rgba(255, 255, 255, 0.78));
      }
      ::-webkit-scrollbar-thumb:hover {
        background: color-mix(in srgb, var(--rb-accent, #684cff) 44%, var(--rb-scroll-thumb-hover, rgba(15, 23, 42, 0.3)));
        background-clip: padding-box;
      }
      ::-webkit-scrollbar-corner {
        background: transparent;
      }
    `;
    document.documentElement.appendChild(style);

    const veil = document.createElement("div");
    veil.id = "rb-top-veil";
    veil.setAttribute("aria-hidden", "true");
    const lens = document.createElement("div");
    lens.id = "rb-top-lens";
    const fade = document.createElement("div");
    fade.id = "rb-top-fade";
    veil.appendChild(lens);
    veil.appendChild(fade);
    document.documentElement.appendChild(veil);

    const popupLens = document.createElement("div");
    popupLens.id = "rb-popup-lens";
    popupLens.setAttribute("aria-hidden", "true");
    document.documentElement.appendChild(popupLens);

    syncTopVeil();
  }

  window.__tbSetUiTheme = (opts) => {
    ensureTopGlassVeil();
    if (!document.documentElement || !opts) return;
    if (opts.accent) {
      document.documentElement.style.setProperty("--rb-accent", opts.accent);
    }
    if (typeof opts.isDark === "boolean") {
      window.__tbForcedDark = opts.isDark;
      if (opts.isDark) document.documentElement.setAttribute("data-rb-ui-dark", "1");
      else document.documentElement.removeAttribute("data-rb-ui-dark");
    }
    if (typeof opts.tinted === "boolean") {
      if (opts.tinted) document.documentElement.setAttribute("data-rb-ui-tinted", "1");
      else document.documentElement.removeAttribute("data-rb-ui-tinted");
    }
  };

  window.__tbRunContextAction = (action, text, x, y) => {
    const cx = typeof x === "number" ? x : Math.round(window.innerWidth / 2);
    const cy = typeof y === "number" ? y : 140;
    const rect = { left: cx, top: cy, right: cx + 20, bottom: cy + 10, width: 20, height: 10 };
    if (action === "quick-translate" && text) {
      showQuickCard(rect, uiStr().quickTranslate);
      emitPageAction({ type: "quick-translate", text });
    } else if (action === "add-glossary" && text) {
      showGlossaryCard(rect, text);
    } else if (action === "copy") {
      document.execCommand("copy");
    } else if (action === "cut") {
      document.execCommand("cut");
    } else if (action === "select-all") {
      const target = window.__tbContextTarget;
      if (target && typeof target.select === "function") {
        target.focus();
        target.select();
      } else {
        document.execCommand("selectAll");
      }
    } else if (action === "paste" && typeof text === "string") {
      const target = window.__tbContextTarget;
      if (target && (target.tagName === "INPUT" || target.tagName === "TEXTAREA")) {
        target.focus();
        const start = target.selectionStart ?? target.value.length;
        const end = target.selectionEnd ?? target.value.length;
        target.value = target.value.slice(0, start) + text + target.value.slice(end);
        target.selectionStart = target.selectionEnd = start + text.length;
        target.dispatchEvent(new Event("input", { bubbles: true }));
      } else if (target && target.isContentEditable) {
        target.focus();
        document.execCommand("insertText", false, text);
      }
    } else if (action === "pip") {
      if (document.pictureInPictureElement && typeof document.exitPictureInPicture === "function") {
        document.exitPictureInPicture().catch(() => {});
      } else {
        const videos = [...document.querySelectorAll("video")];
        const targetVideo =
          videos.find((v) => !v.paused && !v.ended && v.readyState >= 2) ||
          videos.sort((a, b) => b.clientWidth * b.clientHeight - a.clientWidth * a.clientHeight)[0];
        if (targetVideo) {
          if (typeof targetVideo.requestPictureInPicture === "function") {
            targetVideo.requestPictureInPicture().catch(() => {});
          } else if (typeof targetVideo.webkitSetPresentationMode === "function") {
            targetVideo.webkitSetPresentationMode(
              targetVideo.webkitPresentationMode === "picture-in-picture" ? "inline" : "picture-in-picture",
            );
          }
        }
      }
    } else if (action === "scroll-top") {
      window.scrollTo({ top: 0, behavior: "smooth" });
    } else if (action === "scroll-bottom") {
      window.scrollTo({ top: document.documentElement.scrollHeight || 999999, behavior: "smooth" });
    }
  };

  window.addEventListener(
    "contextmenu",
    (event) => {
      const target = event.target;
      if (
        target &&
        target.closest &&
        (target.closest("#rb-sel-bar") ||
          target.closest("#rb-sel-card") ||
          target.closest("#rb-inline-editor") ||
          target.closest("#rb-para-actions"))
      ) {
        return;
      }
      event.preventDefault();
      event.stopPropagation();
      removeBar();
      const linkEl = target && target.closest ? target.closest("a[href]") : null;
      const imgEl = target && target.closest ? target.closest("img[src]") : null;
      const editEl =
        target && target.closest
          ? target.closest("input, textarea, [contenteditable='true'], [contenteditable='']")
          : null;
      window.__tbContextTarget = editEl || target || null;
      const sel = window.getSelection();
      let selectedText = sel ? sel.toString().trim().slice(0, 600) : "";
      if (!selectedText && editEl && (editEl.tagName === "INPUT" || editEl.tagName === "TEXTAREA")) {
        const s = editEl.selectionStart ?? 0;
        const e = editEl.selectionEnd ?? 0;
        if (e > s && typeof editEl.value === "string") {
          selectedText = editEl.value.slice(s, e).trim().slice(0, 600);
        }
      }
      const payload = {
        type: "page-contextmenu",
        x: Math.round(event.clientX),
        y: Math.round(event.clientY),
        linkUrl: linkEl ? linkEl.href || "" : "",
        linkText: linkEl ? (linkEl.innerText || linkEl.textContent || "").trim().slice(0, 80) : "",
        imageUrl: imgEl ? imgEl.src || "" : "",
        selectedText,
        isEditable: Boolean(editEl),
      };
      try {
        if (window.chrome && window.chrome.webview && typeof window.chrome.webview.postMessage === "function") {
          window.chrome.webview.postMessage(JSON.stringify(payload));
          return;
        }
      } catch {
        // Fallback to polling below.
      }
      window.__tbPendingAction = payload;
    },
    true,
  );

  window.__tbSetPopupBlur = (open) => {
    ensureTopGlassVeil();
    const el = document.getElementById("rb-popup-lens");
    if (el) {
      el.classList.toggle("open", Boolean(open));
    }
  };

  let cachedBgRgb = null;
  let lastBgSampleAt = 0;
  function getBgRgbCached(force = false) {
    const now = performance.now();
    if (!force && cachedBgRgb && now - lastBgSampleAt < 1200) {
      return cachedBgRgb;
    }
    lastBgSampleAt = now;
    cachedBgRgb = readBgRgb();
    return cachedBgRgb;
  }

  let lastPostedP = -1;
  let lastPostedDark = null;
  let lastPostedBg = "";
  function postScrollToShell(force = false) {
    scrollRaf = 0;
    const [r, g, b] = getBgRgbCached(force);
    const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
    const isDark = lum < 0.46;
    const bg = `rgb(${r}, ${g}, ${b})`;
    const p = Number(collapseProgress.toFixed(4));
    if (
      !force &&
      Math.abs(p - lastPostedP) < 0.015 &&
      !(p === 0 && lastPostedP !== 0) &&
      !(p === 1 && lastPostedP !== 1) &&
      isDark === lastPostedDark &&
      bg === lastPostedBg
    ) {
      return;
    }
    lastPostedP = p;
    lastPostedDark = isDark;
    lastPostedBg = bg;
    try {
      if (window.chrome && window.chrome.webview && typeof window.chrome.webview.postMessage === "function") {
        window.chrome.webview.postMessage(
          JSON.stringify({
            type: "scroll",
            p,
            y: Math.round(currentScrollY),
            bg,
            isDark,
          }),
        );
      }
    } catch {
      // Ignore postMessage errors.
    }
  }

  function scheduleScrollPost() {
    if (!scrollRaf) {
      scrollRaf = requestAnimationFrame(() => postScrollToShell(false));
    }
  }

  let headerScanTimer = 0;
  function adjustFixedTopHeaders() {
    if (!document.body) return;
    const vw = window.innerWidth || 1280;
    const vh = window.innerHeight || 720;
    const candidates = document.querySelectorAll(
      [
        "header",
        "nav",
        "aside",
        "[role='banner']",
        "[role='navigation']",
        "[role='search']",
        "#mw-head",
        "#mw-panel",
        "#mw-page-base",
        "#mw-head-base",
        "#p-personal",
        "#p-namespaces",
        "#p-views",
        "#p-cactions",
        "#p-search",
        "#p-logo",
        ".vector-header-container",
        ".mw-header",
        "ytd-masthead",
        "#masthead-container",
        "ytd-mini-guide-renderer",
        "tp-yt-app-drawer",
        "bard-sidenav",
        "bard-sidenav-container",
        "bard-sidenav-content",
        "desktop-controls",
        "side-navigation-content",
        "[class*='masthead' i]",
        "[id*='masthead' i]",
        "[class*='header' i]",
        "[id*='header' i]",
        "[class*='navbar' i]",
        "[id*='navbar' i]",
        "[class*='topbar' i]",
        "[id*='topbar' i]",
        "[class*='sidenav' i]",
        "[class*='sidebar' i]",
        "[class*='app-bar' i]",
        "[class*='appbar' i]",
        "body > *",
        "body > * > *",
        "body > * > * > *",
      ].join(", "),
    );
    const scrollTop = window.scrollY || document.documentElement.scrollTop || 0;
    for (const el of candidates) {
      if (!el || (el.id && el.id.startsWith("rb-"))) continue;
      if (!el.dataset || el.dataset.rbFixedAdjusted || el.dataset.rbFixedShell) continue;
      if (el.closest("[data-rb-fixed-adjusted]")) continue;
      if (el.matches("script, style, link, meta, noscript, svg")) continue;
      if (el.matches("[role='dialog'], [aria-modal='true']") || el.closest("[role='dialog'], [aria-modal='true']")) {
        continue;
      }
      const st = getComputedStyle(el);
      if (st.display === "none" || st.visibility === "hidden") continue;
      const pos = st.position;
      if (pos !== "fixed" && pos !== "sticky" && pos !== "absolute") continue;
      const rect = el.getBoundingClientRect();
      const docTop = pos === "absolute" ? rect.top + scrollTop : rect.top;
      if (docTop > 96 || rect.bottom < 8) continue;

      if (pos === "absolute") {
        // Only offset root-anchored absolute headers/Sidebars/Search bars (like MediaWiki / TCRF #mw-head, #mw-panel)
        // whose offsetParent is <html> or <body> (or null) and that sit inside the top chrome zone.
        const op = el.offsetParent;
        const isRootAnchored = !op || op === document.body || op === document.documentElement;
        if (!isRootAnchored) continue;
        const origTop = st.top && st.top !== "auto" ? st.top : `${Math.max(0, Math.round(docTop))}px`;
        el.dataset.rbFixedAdjusted = "1";
        el.style.setProperty("top", `calc(${origTop} + var(--rb-bar-h, 88px) + 6px)`, "important");
        continue;
      }

      if (rect.height >= 24 && rect.height <= 160 && rect.width >= 60 && rect.top <= 36) {
        const origTop = st.top && st.top !== "auto" ? st.top : "0px";
        el.dataset.rbFixedAdjusted = "1";
        el.style.setProperty("top", `calc(${origTop} + var(--rb-bar-h, 88px) + 4px)`, "important");
        if (!st.transition || st.transition === "all 0s ease 0s") {
          el.style.setProperty("transition", "top 0.18s ease");
        }
      } else if (rect.height > 160 && rect.width >= 40 && rect.width <= vw * 0.36) {
        el.dataset.rbFixedAdjusted = "1";
        if (rect.top > 20) {
          const origTop = st.top && st.top !== "auto" ? st.top : `${Math.round(rect.top)}px`;
          el.style.setProperty("top", `calc(${origTop} + var(--rb-bar-h, 88px) + 4px)`, "important");
        } else {
          const origPad = parseFloat(st.paddingTop) || 0;
          el.style.setProperty("box-sizing", "border-box", "important");
          el.style.setProperty("padding-top", `calc(${origPad}px + var(--rb-bar-h, 88px) + 4px)`, "important");
        }
      } else if (rect.top <= 24 && rect.height >= vh * 0.85 && rect.width >= vw * 0.85) {
        // Only offset full-screen SPA containers (like chat apps) if they have no separately adjusted top bar inside.
        const hasChildFixedHeader = el.querySelector("[data-rb-fixed-adjusted]");
        if (!hasChildFixedHeader) {
          const origPad = parseFloat(st.paddingTop) || 0;
          el.dataset.rbFixedShell = "1";
          el.style.setProperty("box-sizing", "border-box", "important");
          el.style.setProperty("padding-top", `calc(${origPad}px + var(--rb-bar-h, 88px))`, "important");
        }
      }
    }
  }

  let lastHeaderScanAt = 0;
  function scheduleHeaderScan(delayMs = 180) {
    if (document.hidden) return;
    const now = performance.now();
    if (headerScanTimer || now - lastHeaderScanAt < 180) return;
    headerScanTimer = window.setTimeout(() => {
      headerScanTimer = 0;
      lastHeaderScanAt = performance.now();
      if (!document.hidden) adjustFixedTopHeaders();
    }, delayMs);
  }

  let veilAppliedBg = "";
  function syncTopVeil(forceTheme = false) {
    if (!document.documentElement) return;
    if (forceTheme || !veilAppliedBg) {
      const [r, g, b] = getBgRgbCached(forceTheme);
      const nextBg = `rgb(${r}, ${g}, ${b})`;
      if (nextBg !== veilAppliedBg) {
        veilAppliedBg = nextBg;
        document.documentElement.style.setProperty("--rb-page-bg", nextBg);
      }
      if (window.__tbForcedDark === undefined) {
        const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
        if (lum < 0.46) document.documentElement.setAttribute("data-rb-ui-dark", "1");
        else document.documentElement.removeAttribute("data-rb-ui-dark");
      }
    }
    const veilEl = document.getElementById("rb-top-veil");
    const fadeEl = document.getElementById("rb-top-fade");
    const scrolledFactor = Math.max(0, Math.min(1, (currentScrollY - 4) / 48));
    if (veilEl) {
      veilEl.style.opacity = "1";
    }
    if (fadeEl) {
      fadeEl.style.opacity = (0.86 + scrolledFactor * 0.12).toFixed(3);
    }
  }

  window.__tbSetTopBarHeight = (h) => {
    ensureTopGlassVeil();
    if (document.documentElement && typeof h === "number" && Number.isFinite(h)) {
      const px = `${Math.round(h)}px`;
      document.documentElement.style.setProperty("--rb-bar-h", px);
      document.documentElement.style.setProperty("--rb-top-pad", px);
      scheduleHeaderScan(40);
    }
  };

  window.__tbPageFadeOut = () => {
    if (document.documentElement) {
      document.documentElement.style.transition = "opacity 0.2s cubic-bezier(0.4, 0, 1, 1)";
      document.documentElement.style.opacity = "0";
    }
  };

  window.__tbPageFadeIn = () => {
    if (document.documentElement) {
      document.documentElement.style.opacity = "1";
    }
    syncTopVeil(true);
    scheduleHeaderScan(60);
  };

  // Password capture + Autofill pill
  let autofillHints = { origin: location.origin, usernames: [], hasProfile: false };
  let autofillChip = null;
  function removeAutofillChip() {
    autofillChip?.remove();
    autofillChip = null;
  }

  window.__tbSetAutofillHints = (hints) => {
    if (hints && typeof hints === "object") {
      autofillHints = hints;
    }
  };

  function setNativeInputValue(input, val) {
    if (!input || val === undefined || val === null) return;
    const proto = input instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    const setter = Object.getOwnPropertyDescriptor(proto, "value")?.set;
    if (setter) setter.call(input, String(val));
    else input.value = String(val);
    input.dispatchEvent(new Event("input", { bubbles: true }));
    input.dispatchEvent(new Event("change", { bubbles: true }));
  }

  window.__tbApplyLoginAutofill = (cred) => {
    if (!cred) return;
    const pw = document.querySelector("input[type='password']:not([disabled])");
    const form = pw?.form || document;
    const userField = form.querySelector(
      "input[autocomplete*='username'], input[type='email'], input[name*='user' i], input[name*='id' i], input[name*='login' i], input[type='text']",
    );
    if (userField && cred.username) setNativeInputValue(userField, cred.username);
    if (pw && cred.password) setNativeInputValue(pw, cred.password);
    removeAutofillChip();
  };

  window.__tbApplyProfileAutofill = (profile) => {
    if (!profile) return;
    const inputs = document.querySelectorAll("input:not([type='password']):not([type='hidden']):not([disabled]), textarea:not([disabled])");
    for (const el of inputs) {
      const hint = `${el.name || ""} ${el.id || ""} ${el.autocomplete || ""} ${el.placeholder || ""}`.toLowerCase();
      if (profile.email && (el.type === "email" || hint.includes("email") || hint.includes("mail"))) {
        setNativeInputValue(el, profile.email);
      } else if (profile.phone && (el.type === "tel" || hint.includes("phone") || hint.includes("tel") || hint.includes("mobile"))) {
        setNativeInputValue(el, profile.phone);
      } else if (profile.postalCode && (hint.includes("zip") || hint.includes("postal") || hint.includes("우편"))) {
        setNativeInputValue(el, profile.postalCode);
      } else if (profile.address && (hint.includes("address") || hint.includes("street") || hint.includes("주소"))) {
        setNativeInputValue(el, profile.address);
      } else if (profile.name && (hint.includes("name") || hint.includes("fullname") || hint.includes("이름") || hint.includes("성명"))) {
        setNativeInputValue(el, profile.name);
      }
    }
    removeAutofillChip();
  };

  document.addEventListener(
    "submit",
    (e) => {
      const form = e.target;
      if (!(form instanceof HTMLFormElement)) return;
      const pwInput = form.querySelector("input[type='password']");
      if (!pwInput || !pwInput.value) return;
      const userInput = form.querySelector(
        "input[autocomplete*='username'], input[type='email'], input[name*='user' i], input[name*='id' i], input[name*='login' i], input[type='text']",
      );
      const username = userInput ? (userInput.value || "").trim() : "";
      if (!username) return;
      postWebMessage({
        type: "save-login",
        origin: location.origin,
        username,
        password: pwInput.value,
      });
    },
    true,
  );

  document.addEventListener("focusin", (e) => {
    const el = e.target;
    if (!(el instanceof HTMLInputElement)) {
      removeAutofillChip();
      return;
    }
    const isLoginField =
      el.type === "password" ||
      (el.form && el.form.querySelector("input[type='password']") && el.type !== "hidden" && el.type !== "submit");
    const hasLogins = Array.isArray(autofillHints.usernames) && autofillHints.usernames.length > 0;
    const hint = `${el.name || ""} ${el.id || ""} ${el.autocomplete || ""} ${el.placeholder || ""}`.toLowerCase();
    const isProfileField =
      autofillHints.hasProfile &&
      (el.type === "email" ||
        el.type === "tel" ||
        /(name|email|phone|tel|address|zip|postal|이름|주소)/.test(hint));
    if ((!isLoginField || !hasLogins) && !isProfileField) {
      removeAutofillChip();
      return;
    }
    ensureTopGlassVeil();
    removeAutofillChip();
    const rect = el.getBoundingClientRect();
    if (rect.width < 20 || rect.height < 10) return;
    autofillChip = document.createElement("div");
    autofillChip.id = "rb-autofill-chip";
    autofillChip.className = "rb-liquid-surface";
    autofillChip.style.cssText = `position:fixed;left:${Math.max(8, Math.min(rect.left, window.innerWidth - 240))}px;top:${Math.min(window.innerHeight - 44, rect.bottom + 6)}px;z-index:2147483645;display:flex;gap:5px;padding:4px 6px;border-radius:999px;`;
    if (isLoginField && hasLogins) {
      for (const uname of autofillHints.usernames.slice(0, 3)) {
        const btn = document.createElement("button");
        btn.type = "button";
        btn.className = "rb-liquid-btn primary";
        btn.textContent = `🔑 ${uname}`;
        btn.onmousedown = (ev) => {
          ev.preventDefault();
          postWebMessage({ type: "request-autofill-login", origin: location.origin, username: uname });
        };
        autofillChip.appendChild(btn);
      }
    }
    if (isProfileField) {
      const btn = document.createElement("button");
      btn.type = "button";
      btn.className = "rb-liquid-btn";
      btn.textContent = uiStr().autofillProfile;
      btn.onmousedown = (ev) => {
        ev.preventDefault();
        postWebMessage({ type: "request-autofill-profile" });
      };
      autofillChip.appendChild(btn);
    }
    document.documentElement.appendChild(autofillChip);
  });

  document.addEventListener("focusout", () => {
    window.setTimeout(() => {
      if (!document.activeElement || document.activeElement === document.body) {
        removeAutofillChip();
      }
    }, 150);
  });

  function notifyFirstPaint() {
    if (window.__tbFirstPaintSent) return;
    window.__tbFirstPaintSent = true;
    requestAnimationFrame(() => {
      requestAnimationFrame(() => {
        postWebMessage({ type: "first-paint" });
      });
    });
  }

  function installSpaHeaderObserver() {
    if (window.__tbHeaderObserver || !document.documentElement || typeof MutationObserver === "undefined") {
      return;
    }
    const obs = new MutationObserver((mutations) => {
      for (const m of mutations) {
        if (m.addedNodes && m.addedNodes.length > 0) {
          scheduleHeaderScan(220);
          break;
        }
      }
    });
    obs.observe(document.documentElement, { childList: true, subtree: true });
    window.__tbHeaderObserver = obs;
    window.setTimeout(() => {
      obs.disconnect();
      window.__tbHeaderObserver = null;
    }, 15000);
  }

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", () => {
      ensureTopGlassVeil();
      scheduleHeaderScan(40);
      scheduleHeaderScan(350);
      scheduleHeaderScan(1000);
      installSpaHeaderObserver();
      postScrollToShell(true);
      notifyFirstPaint();
    }, { once: true });
  } else {
    ensureTopGlassVeil();
    scheduleHeaderScan(40);
    scheduleHeaderScan(350);
    scheduleHeaderScan(1000);
    installSpaHeaderObserver();
    postScrollToShell(true);
    notifyFirstPaint();
  }

  window.__tbReadPageMeta = () => {
    ensureTopGlassVeil();
    syncTopVeil(true);
    const [r, g, b] = getBgRgbCached(true);
    const lum = (0.299 * r + 0.587 * g + 0.114 * b) / 255;
    return {
      bg: `rgb(${r}, ${g}, ${b})`,
      isDark: lum < 0.46,
      scrollState: collapseProgress > 0.5 ? "down" : "up",
      collapseProgress: Number(collapseProgress.toFixed(4)),
    };
  };

  window.addEventListener(
    "scroll",
    (e) => {
      const target =
        e.target === document || e.target === window
          ? document.scrollingElement || document.documentElement
          : e.target;
      const y =
        target && typeof target.scrollTop === "number"
          ? target.scrollTop
          : window.scrollY || 0;
      const dy = y - lastScrollY;
      currentScrollY = y;
      lastScrollY = y;
      if (Math.abs(dy) < 0.5 && y > 6) return;

      if (y <= 6) {
        collapseProgress = 0;
      } else {
        collapseProgress = Math.max(0, Math.min(1, collapseProgress + dy / 140));
      }
      window.__tbScrollState = collapseProgress > 0.5 ? "down" : "up";
      syncTopVeil(false);
      scheduleScrollPost();
    },
    { capture: true, passive: true },
  );

  // Real-time audible media playback detection for tab speaker badge
  let lastAudibleState = false;
  let audioCheckTimer = 0;
  function checkAudibleMedia() {
    audioCheckTimer = 0;
    const els = document.querySelectorAll("video, audio");
    let playing = false;
    for (const el of els) {
      if (!el.paused && !el.ended && !el.muted && (typeof el.volume !== "number" || el.volume > 0.01)) {
        playing = true;
        break;
      }
    }
    if (playing !== lastAudibleState) {
      lastAudibleState = playing;
      postWebMessage({ type: "media-audio-state", playing });
    }
  }
  function scheduleAudioCheck() {
    if (!audioCheckTimer) {
      audioCheckTimer = window.setTimeout(checkAudibleMedia, 120);
    }
  }
  for (const evName of ["play", "playing", "pause", "ended", "volumechange", "emptied"]) {
    document.addEventListener(evName, scheduleAudioCheck, true);
  }

  // Page primary language detection (for Chrome/Edge-style auto-translate banner & site rules)
  window.__tbDetectPageLanguage = () => {
    const sample = ((document.body && document.body.innerText) || "").slice(0, 2400);
    let hangul = 0;
    let kana = 0;
    let cjk = 0;
    let latin = 0;
    for (let i = 0; i < sample.length; i++) {
      const code = sample.charCodeAt(i);
      if ((code >= 0xac00 && code <= 0xd7a3) || (code >= 0x3131 && code <= 0x318e)) hangul++;
      else if (code >= 0x3040 && code <= 0x30ff) kana++;
      else if (code >= 0x4e00 && code <= 0x9fff) cjk++;
      else if ((code >= 0x0041 && code <= 0x005a) || (code >= 0x0061 && code <= 0x007a)) latin++;
    }
    const total = hangul + kana + cjk + latin;
    if (total < 24) {
      const htmlLang = (document.documentElement?.lang || "").toLowerCase();
      if (htmlLang.startsWith("ko")) return "ko";
      if (htmlLang.startsWith("ja")) return "ja";
      if (htmlLang.startsWith("zh")) return "zh";
      if (htmlLang.startsWith("en")) return "en";
      return "unknown";
    }
    if (hangul / total > 0.18) return "ko";
    if (kana > 8 || (kana + cjk) / total > 0.22 && kana > 0) return "ja";
    if (cjk / total > 0.22 && kana === 0) return "zh";
    if (latin / total > 0.45) return "en";
    return "other";
  };

  window.setTimeout(() => {
    if (location.protocol === "http:" || location.protocol === "https:") {
      const lang = window.__tbDetectPageLanguage();
      if (lang && lang !== "unknown") {
        postWebMessage({ type: "page-language-detected", lang, url: location.href });
      }
    }
  }, 650);

  // Dynamic infinite-scroll / SPA mutation observer while inline translation is active
  let dynamicObs = null;
  let dynamicTimer = 0;
  window.__tbSetDynamicTranslate = (enabled) => {
    window.__tbDynamicTranslateActive = Boolean(enabled);
    if (!enabled) {
      if (dynamicObs) {
        dynamicObs.disconnect();
        dynamicObs = null;
      }
      if (dynamicTimer) {
        clearTimeout(dynamicTimer);
        dynamicTimer = 0;
      }
      return;
    }
    if (dynamicObs || !document.body || typeof MutationObserver === "undefined") return;
    dynamicObs = new MutationObserver((mutations) => {
      if (!window.__tbDynamicTranslateActive) return;
      let hasMeaningfulAddition = false;
      for (const m of mutations) {
        for (const node of m.addedNodes || []) {
          if (node.nodeType !== 1) continue;
          const el = node;
          if (
            el.id?.startsWith("rb-") ||
            el.hasAttribute?.("data-rb-inline-translation-for") ||
            el.closest?.("[data-rb-inline-translation-for]")
          ) {
            continue;
          }
          const txt = (el.innerText || el.textContent || "").trim();
          if (txt.length >= 25) {
            hasMeaningfulAddition = true;
            break;
          }
        }
        if (hasMeaningfulAddition) break;
      }
      if (hasMeaningfulAddition && !dynamicTimer) {
        dynamicTimer = window.setTimeout(() => {
          dynamicTimer = 0;
          if (window.__tbDynamicTranslateActive) {
            postWebMessage({ type: "dynamic-blocks-added", url: location.href });
          }
        }, 900);
      }
    });
    dynamicObs.observe(document.body, { childList: true, subtree: true });
  };

  // Edge-style Read Aloud (TTS) engine with paragraph highlight & auto-scroll
  const ttsState = {
    playing: false,
    paused: false,
    rate: 1.0,
    index: 0,
    elements: [],
  };

  function clearTtsHighlight() {
    document.querySelectorAll("[data-rb-tts-active]").forEach((el) => {
      el.removeAttribute("data-rb-tts-active");
    });
  }

  function collectTtsElements() {
    const translated = [...document.querySelectorAll("[data-rb-inline-translation-for]")].filter(
      (el) => (el.innerText || el.textContent || "").trim().length > 1,
    );
    if (translated.length > 0) return translated;
    const inlineOrig = [...document.querySelectorAll("[data-rb-inline-key]:not([hidden])")].filter(
      (el) => (el.innerText || el.textContent || "").trim().length > 1,
    );
    if (inlineOrig.length > 0) return inlineOrig;
    return [...document.querySelectorAll("article p, main p, p, h1, h2, h3, li")].filter((el) => {
      const t = (el.innerText || el.textContent || "").trim();
      return t.length >= 12 && el.offsetParent !== null;
    });
  }

  function speakCurrentTtsIndex() {
    if (!("speechSynthesis" in window)) return;
    window.speechSynthesis.cancel();
    clearTtsHighlight();
    if (!ttsState.playing) return;
    if (ttsState.index < 0 || ttsState.index >= ttsState.elements.length) {
      ttsState.playing = false;
      ttsState.paused = false;
      postWebMessage({ type: "tts-state", playing: false, paused: false, index: 0, total: ttsState.elements.length });
      return;
    }
    const el = ttsState.elements[ttsState.index];
    const text = (el.innerText || el.textContent || "").trim();
    if (!text) {
      ttsState.index++;
      speakCurrentTtsIndex();
      return;
    }
    el.setAttribute("data-rb-tts-active", "1");
    el.scrollIntoView({ behavior: "smooth", block: "center" });
    const utter = new SpeechSynthesisUtterance(text);
    utter.rate = ttsState.rate;
    const hasHangul = /[가-힣]/.test(text);
    const hasKana = /[ぁ-んァ-ン]/.test(text);
    const hasCjk = /[\u4e00-\u9fff]/.test(text);
    utter.lang = hasHangul ? "ko-KR" : hasKana ? "ja-JP" : hasCjk ? "zh-CN" : "en-US";
    utter.onend = () => {
      if (!ttsState.playing || ttsState.paused) return;
      ttsState.index++;
      speakCurrentTtsIndex();
    };
    utter.onerror = () => {
      if (!ttsState.playing || ttsState.paused) return;
      ttsState.index++;
      speakCurrentTtsIndex();
    };
    postWebMessage({
      type: "tts-state",
      playing: true,
      paused: false,
      index: ttsState.index + 1,
      total: ttsState.elements.length,
    });
    window.speechSynthesis.speak(utter);
  }

  window.__tbTtsControl = (cmd, arg) => {
    if (!("speechSynthesis" in window)) {
      return { playing: false, paused: false, index: 0, total: 0, unsupported: true };
    }
    if (cmd === "start" || cmd === "toggle") {
      if (ttsState.playing && cmd === "toggle") {
        if (ttsState.paused) {
          ttsState.paused = false;
          window.speechSynthesis.resume();
        } else {
          ttsState.paused = true;
          window.speechSynthesis.pause();
        }
      } else {
        ttsState.elements = collectTtsElements();
        ttsState.index = 0;
        ttsState.playing = ttsState.elements.length > 0;
        ttsState.paused = false;
        speakCurrentTtsIndex();
      }
    } else if (cmd === "pause") {
      ttsState.paused = true;
      window.speechSynthesis.pause();
    } else if (cmd === "resume") {
      ttsState.paused = false;
      window.speechSynthesis.resume();
    } else if (cmd === "next") {
      if (ttsState.elements.length > 0) {
        ttsState.index = Math.min(ttsState.elements.length - 1, ttsState.index + 1);
        ttsState.playing = true;
        ttsState.paused = false;
        speakCurrentTtsIndex();
      }
    } else if (cmd === "prev") {
      if (ttsState.elements.length > 0) {
        ttsState.index = Math.max(0, ttsState.index - 1);
        ttsState.playing = true;
        ttsState.paused = false;
        speakCurrentTtsIndex();
      }
    } else if (cmd === "rate" && typeof arg === "number" && arg >= 0.5 && arg <= 2.5) {
      ttsState.rate = arg;
      if (ttsState.playing && !ttsState.paused) {
        speakCurrentTtsIndex();
      }
    } else if (cmd === "stop") {
      ttsState.playing = false;
      ttsState.paused = false;
      window.speechSynthesis.cancel();
      clearTtsHighlight();
    }
    return {
      playing: ttsState.playing,
      paused: ttsState.paused,
      rate: ttsState.rate,
      index: ttsState.playing ? ttsState.index + 1 : 0,
      total: ttsState.elements.length,
    };
  };
})();
