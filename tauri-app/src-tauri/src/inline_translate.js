((items, fontScale, lineHeight, bilingualAll, uiLang) => {
  const INLINE_I18N = {
    ko: {
      reviewNeeded: "검토 필요",
      sourcePrefix: "원문:",
      cancel: "취소",
      save: "저장",
      retryTitle: "이 문단 다시 번역",
      editTitle: "이 문단 번역 수정",
      reportTitle: "이 문단 오역·오류 제보",
    },
    en: {
      reviewNeeded: "Needs review",
      sourcePrefix: "Original:",
      cancel: "Cancel",
      save: "Save",
      retryTitle: "Retranslate this paragraph",
      editTitle: "Edit translation",
      reportTitle: "Report translation issue",
    },
    ja: {
      reviewNeeded: "要確認",
      sourcePrefix: "原文:",
      cancel: "キャンセル",
      save: "保存",
      retryTitle: "この段落を再翻訳",
      editTitle: "翻訳を編集",
      reportTitle: "誤訳・不具合を報告",
    },
    "zh-CN": {
      reviewNeeded: "需要检查",
      sourcePrefix: "原文:",
      cancel: "取消",
      save: "保存",
      retryTitle: "重新翻译此段落",
      editTitle: "编辑此段译文",
      reportTitle: "报告误译或错误",
    },
  };
  if (uiLang && INLINE_I18N[uiLang]) {
    window.__tbUiLang = uiLang;
  }
  const ilStr = () => INLINE_I18N[window.__tbUiLang] || INLINE_I18N.ko;

  let style = document.getElementById("rb-inline-style");
  if (!style) {
    style = document.createElement("style");
    style.id = "rb-inline-style";
    document.documentElement.appendChild(style);
  }
  const scale = Number.isFinite(fontScale) && fontScale > 0.5 ? fontScale : 1.0;
  const lh = Number.isFinite(lineHeight) && lineHeight > 1.0 ? lineHeight : 1.6;
  function buildInlineCss(s, l) {
    const sc = Math.abs(s - 1.0) > 0.01 ? `font-size:${(s * 100).toFixed(0)}%!important;` : "";
    return (
      `[data-rb-inline-key][hidden]{display:none!important}` +
      `[data-rb-inline-translation-for]{${sc}line-height:${l.toFixed(2)}!important;white-space:pre-wrap!important;transition:background-color 0.2s ease,box-shadow 0.2s ease}` +
      `[data-rb-inline-bilingual="1"]{margin-top:4px;padding:3px 0 3px 9px;border-left:2.5px solid var(--rb-accent, #684cff);opacity:0.94;font-weight:normal}` +
      `[data-rb-uncertain="1"]{border-left:2.5px solid #f59e0b;padding-left:7px}` +
      `[data-rb-tts-active="1"]{background:color-mix(in srgb, var(--rb-accent, #684cff) 16%, transparent)!important;box-shadow:0 0 0 2px color-mix(in srgb, var(--rb-accent, #684cff) 45%, transparent)!important;border-radius:8px}` +
      `[data-rb-inline-translation-for] [data-rb-cloned-media="1"]{position:static!important;inset:auto!important;top:auto!important;right:auto!important;bottom:auto!important;left:auto!important;float:none!important;transform:none!important;max-width:100%!important;vertical-align:middle!important}` +
      `#rb-para-actions.rb-liquid-surface,#rb-inline-editor.rb-liquid-surface{background:var(--rb-ui-bg,rgba(24,28,42,0.86))!important;color:var(--rb-ui-text,#f8faff)!important;border:1px solid var(--rb-ui-border,rgba(255,255,255,0.26))!important;box-shadow:0 8px 24px rgba(0,0,0,0.28),inset 0 1px 0 rgba(255,255,255,0.32)!important;-webkit-backdrop-filter:blur(24px) saturate(1.9)!important;backdrop-filter:blur(24px) saturate(1.9)!important}` +
      `#rb-para-actions .rb-liquid-btn,#rb-inline-editor .rb-liquid-btn{height:26px!important;padding:0 10px!important;border:1px solid rgba(255,255,255,0.24)!important;border-radius:999px!important;background:rgba(255,255,255,0.14)!important;color:inherit!important;font:600 12px/1 system-ui,-apple-system,sans-serif!important;cursor:pointer!important}` +
      `#rb-inline-editor .rb-liquid-btn.primary{background:var(--rb-accent,#684cff)!important;color:#fff!important}`
    );
  }
  style.textContent = buildInlineCss(scale, lh);
  window.__tbUpdateInlineStyle = (nextScale, nextLh) => {
    const s = Number.isFinite(nextScale) && nextScale > 0.5 ? nextScale : scale;
    const l = Number.isFinite(nextLh) && nextLh > 1.0 ? nextLh : lh;
    const el = document.getElementById("rb-inline-style");
    if (el) el.textContent = buildInlineCss(s, l);
  };

  const originals = new Map(
    [...document.querySelectorAll("[data-rb-inline-key]")].map((element) => [
      element.getAttribute("data-rb-inline-key"),
      element,
    ]),
  );
  let applied = 0;

  function wireClonedInteractive(clone, sourceEl) {
    clone.removeAttribute("data-rb-link-idx");
    clone.removeAttribute("data-rb-verbatim");
    clone.removeAttribute("id");
    if (clone.tagName === "BUTTON") {
      clone.addEventListener("click", (ev) => {
        ev.preventDefault();
        sourceEl.click();
      });
      return;
    }
    if (clone.tagName !== "A") return;

    const rawHref = (clone.getAttribute("href") || "").trim();
    const resolvedHref = sourceEl.href || "";
    if (rawHref && !rawHref.startsWith("#") && !rawHref.startsWith("javascript:") && resolvedHref) {
      clone.setAttribute("href", resolvedHref);
    }

    if (!rawHref || rawHref.startsWith("javascript:")) {
      clone.addEventListener("click", (ev) => {
        ev.preventDefault();
        sourceEl.click();
      });
    } else if (rawHref.startsWith("#")) {
      clone.addEventListener("click", (ev) => {
        ev.preventDefault();
        const targetId = rawHref.slice(1);
        const anchorTarget = targetId
          ? document.getElementById(decodeURIComponent(targetId)) || document.getElementsByName(targetId)[0]
          : null;
        if (anchorTarget) {
          anchorTarget.scrollIntoView({ behavior: "smooth", block: "center" });
        } else {
          sourceEl.click();
        }
      });
    } else {
      clone.addEventListener("click", (ev) => {
        if (ev.metaKey || ev.ctrlKey || ev.shiftKey || ev.altKey) return;
        const targetAttr = (clone.getAttribute("target") || "").toLowerCase();
        if (targetAttr === "_blank") return;
        // Let native <a> or framework delegates run; if an SPA delegate calls preventDefault() yet fails on the cloned node, navigate directly.
        const beforeUrl = location.href;
        setTimeout(() => {
          if (ev.defaultPrevented && location.href === beforeUrl && resolvedHref) {
            window.location.assign(resolvedHref);
          }
        }, 160);
      });
    }
  }

  function renderWithLinks(translated, original, text, bilingual) {
    const linkEls = [...original.querySelectorAll("[data-rb-link-idx]")];
    const mediaEls = [...original.querySelectorAll("[data-rb-media-idx]")];
    if (linkEls.length === 0 && mediaEls.length === 0) {
      translated.textContent = text;
      return;
    }
    const linkMap = new Map();
    for (const el of linkEls) {
      linkMap.set(el.getAttribute("data-rb-link-idx"), el);
    }
    const mediaMap = new Map();
    for (const el of mediaEls) {
      mediaMap.set(el.getAttribute("data-rb-media-idx"), el);
    }

    translated.textContent = "";
    const used = new Set();
    const tokenRe =
      /\[\[img(\d+)\]\]|\[\[(\d+)\s*[|:]\s*([\s\S]*?)\]\](?!\])|\[(\d+)\s*[|:]\s*([^\[\]\n]+)\]/g;
    let lastIndex = 0;
    let match;
    while ((match = tokenRe.exec(text)) !== null) {
      if (match.index > lastIndex) {
        translated.appendChild(document.createTextNode(text.slice(lastIndex, match.index)));
      }
      if (match[1] !== undefined) {
        const mediaEl = mediaMap.get(match[1]);
        if (mediaEl) {
          const w = parseFloat(mediaEl.getAttribute("data-rb-media-w") || "0");
          const h = parseFloat(mediaEl.getAttribute("data-rb-media-h") || "0");
          const disp = mediaEl.getAttribute("data-rb-media-disp") || "inline-block";
          if (bilingual && (w > 32 || h > 32)) {
            lastIndex = tokenRe.lastIndex;
            continue;
          }
          const clone = mediaEl.cloneNode(true);
          clone.removeAttribute("data-rb-media-idx");
          clone.removeAttribute("data-rb-media-w");
          clone.removeAttribute("data-rb-media-h");
          clone.removeAttribute("data-rb-media-disp");
          clone.removeAttribute("id");
          clone.removeAttribute("width");
          clone.removeAttribute("height");
          if (clone.tagName === "IMG") {
            const cSrc = (clone.getAttribute("src") || "").trim();
            const dSrc = (clone.getAttribute("data-src") || "").trim();
            if ((!cSrc || cSrc.startsWith("data:image/gif")) && dSrc) {
              clone.setAttribute("src", dSrc);
            }
            clone.removeAttribute("class");
            clone.removeAttribute("style");
          }
          clone.setAttribute("data-rb-cloned-media", "1");
          clone.style.setProperty("position", "static", "important");
          clone.style.setProperty("inset", "auto", "important");
          clone.style.setProperty("top", "auto", "important");
          clone.style.setProperty("right", "auto", "important");
          clone.style.setProperty("bottom", "auto", "important");
          clone.style.setProperty("left", "auto", "important");
          clone.style.setProperty("float", "none", "important");
          clone.style.setProperty("transform", "none", "important");
          clone.style.setProperty("z-index", "auto", "important");
          clone.style.setProperty("display", disp, "important");
          clone.style.setProperty("vertical-align", "middle", "important");
          if (clone.tagName === "IMG") {
            if (w > 0 && h > 0) {
              clone.style.setProperty("width", `min(${w}px, 100%)`, "important");
              clone.style.setProperty("height", "auto", "important");
              clone.style.setProperty("max-width", `${w}px`, "important");
              clone.style.setProperty("max-height", `${h}px`, "important");
              clone.style.setProperty("aspect-ratio", `${w} / ${h}`, "important");
              clone.style.setProperty("object-fit", "contain", "important");
              clone.style.setProperty("margin", "0 3px", "important");
            } else {
              clone.style.setProperty("max-width", "min(100%, 360px)", "important");
              clone.style.setProperty("max-height", "280px", "important");
              clone.style.setProperty("width", "auto", "important");
              clone.style.setProperty("height", "auto", "important");
            }
          } else {
            if (w > 0 && h > 0) {
              clone.style.setProperty("width", `${w}px`, "important");
              clone.style.setProperty("height", `${h}px`, "important");
              clone.style.setProperty("max-width", `${w}px`, "important");
              clone.style.setProperty("max-height", `${h}px`, "important");
              clone.style.setProperty("flex-shrink", "0", "important");
            } else {
              clone.style.setProperty("width", "1.1em", "important");
              clone.style.setProperty("height", "1.1em", "important");
              clone.style.setProperty("max-width", "24px", "important");
              clone.style.setProperty("max-height", "24px", "important");
            }
          }
          translated.appendChild(clone);
        }
      } else {
        const idx = match[2] ?? match[4];
        const innerText = match[3] ?? match[5] ?? "";
        const sourceEl = linkMap.get(idx);
        if (sourceEl) {
          used.add(idx);
          const isVerbatim = sourceEl.hasAttribute("data-rb-verbatim");
          const clone = sourceEl.cloneNode(true);
          if (!isVerbatim) {
            clone.textContent = innerText;
          }
          wireClonedInteractive(clone, sourceEl);
          translated.appendChild(clone);
        } else {
          translated.appendChild(document.createTextNode(innerText));
        }
      }
      lastIndex = tokenRe.lastIndex;
    }
    if (lastIndex < text.length) {
      translated.appendChild(document.createTextNode(text.slice(lastIndex)));
    }
    // Keep any link/button the model omitted so interactive elements stay accessible.
    for (const [idx, sourceEl] of linkMap.entries()) {
      if (used.has(idx)) continue;
      translated.appendChild(document.createTextNode(" "));
      const clone = sourceEl.cloneNode(true);
      wireClonedInteractive(clone, sourceEl);
      translated.appendChild(clone);
    }
  }

  for (const item of items) {
    const original = originals.get(item.key);
    if (!original) continue;

    let translated = [...document.querySelectorAll(
      "[data-rb-inline-translation-for]",
    )].find(
      (element) =>
        element.getAttribute("data-rb-inline-translation-for") === item.key,
    );
    const bilingual = Boolean(bilingualAll) || original.hasAttribute("data-rb-bilingual");
    if (!translated) {
      const isTableCell = original.tagName === "TD" || original.tagName === "TH";
      translated = document.createElement(isTableCell ? "div" : original.tagName.toLowerCase());
      if (!isTableCell && original.className) translated.className = original.className;
      const inlineStyle = !isTableCell ? original.getAttribute("style") : null;
      if (inlineStyle) translated.setAttribute("style", inlineStyle);
      for (const attribute of ["dir", "lang", "role"]) {
        const value = original.getAttribute(attribute);
        if (value) translated.setAttribute(attribute, value);
      }
      translated.setAttribute("data-rb-inline-translation-for", item.key);
      if (bilingual) {
        translated.setAttribute("data-rb-inline-bilingual", "1");
      } else {
        original.hidden = true;
      }
      if (isTableCell) {
        original.appendChild(translated);
      } else {
        original.insertAdjacentElement("afterend", translated);
      }
    }

    renderWithLinks(translated, original, String(item.translation ?? ""), bilingual);
    const sourceHint = (original.innerText || original.textContent || "").trim().slice(0, 300);
    const s = ilStr();
    if (item.uncertain) {
      translated.setAttribute("data-rb-uncertain", "1");
      const noteText = Array.isArray(item.notes) && item.notes.length > 0 ? item.notes.join(" · ") : s.reviewNeeded;
      translated.title = sourceHint ? `[${noteText}]\n${s.sourcePrefix} ${sourceHint}` : noteText;
    } else if (!bilingual && sourceHint) {
      translated.title = `${s.sourcePrefix} ${sourceHint}`;
    }
    applied++;
  }

  if (!window.__tbParaActionsInstalled) {
    window.__tbParaActionsInstalled = true;
    let activeTarget = null;
    let hideTimer = null;
    let actionBar = null;
    let editorBox = null;

    function dispatchParaAction(payload) {
      try {
        if (window.webkit?.messageHandlers?.fluoAction?.postMessage) {
          window.webkit.messageHandlers.fluoAction.postMessage(payload);
        }
      } catch {}
      if (window.__tbEmitAction) window.__tbEmitAction(payload);
      else window.__tbPendingAction = payload;
    }

    function removeActionBar() {
      actionBar?.remove();
      actionBar = null;
    }

    function openInlineEditor(el, key) {
      editorBox?.remove();
      removeActionBar();
      const rect = el.getBoundingClientRect();
      editorBox = document.createElement("div");
      editorBox.id = "rb-inline-editor";
      editorBox.className = "rb-liquid-surface";
      editorBox.style.cssText =
        `position:fixed;left:${Math.max(8, Math.min(rect.left, window.innerWidth - 420))}px;top:${Math.max(8, Math.min(rect.top, window.innerHeight - 180))}px;z-index:2147483647;width:min(420px,calc(100vw - 16px));padding:13px;border-radius:18px;font:12px/1.45 'Pretendard',system-ui,sans-serif;display:grid;gap:9px;`;
      const area = document.createElement("textarea");
      area.value = el.textContent || "";
      area.rows = 4;
      area.style.cssText =
        "width:100%;box-sizing:border-box;padding:9px 11px;border:1px solid var(--rb-ui-border, rgba(255,255,255,0.65));border-radius:12px;background:var(--rb-ui-pill, rgba(255,255,255,0.82));color:var(--rb-ui-text, #181c2c);font:inherit;resize:vertical;outline:none;";
      const s = ilStr();
      const row = document.createElement("div");
      row.style.cssText = "display:flex;justify-content:flex-end;gap:6px;";
      const cancel = document.createElement("button");
      cancel.type = "button";
      cancel.className = "rb-liquid-btn";
      cancel.textContent = s.cancel;
      cancel.onclick = () => {
        editorBox?.remove();
        editorBox = null;
      };
      const save = document.createElement("button");
      save.type = "button";
      save.className = "rb-liquid-btn primary";
      save.textContent = s.save;
      save.onclick = () => {
        const nextText = area.value;
        el.textContent = nextText;
        const origEl =
          originals.get(key) ||
          (key ? document.querySelector(`[data-rb-inline-key="${CSS.escape(key)}"]`) : null);
        const source = (origEl?.innerText || origEl?.textContent || "").trim();
        const payload = { type: "edit", key, source, translation: nextText };
        dispatchParaAction(payload);
        editorBox?.remove();
        editorBox = null;
      };
      row.appendChild(cancel);
      row.appendChild(save);
      editorBox.appendChild(area);
      editorBox.appendChild(row);
      document.documentElement.appendChild(editorBox);
      area.focus();
    }

    function showActionBarFor(el) {
      if (!el) return;
      clearTimeout(hideTimer);
      if (activeTarget === el && actionBar) return;
      activeTarget = el;
      removeActionBar();
      const key = el.getAttribute("data-rb-inline-translation-for");
      const rect = el.getBoundingClientRect();
      actionBar = document.createElement("div");
      actionBar.id = "rb-para-actions";
      actionBar.className = "rb-liquid-surface";
      actionBar.style.cssText =
        `position:fixed;left:${Math.max(8, Math.min(rect.right - 126, window.innerWidth - 138))}px;top:${Math.max(8, rect.top - 32)}px;z-index:2147483644;display:flex;gap:4px;padding:3px 5px;border-radius:999px;font:11px/1 'Pretendard',system-ui,sans-serif;`;
      const s = ilStr();
      const retry = document.createElement("button");
      retry.type = "button";
      retry.className = "rb-liquid-btn";
      retry.textContent = "↻";
      retry.title = s.retryTitle;
      retry.onclick = (e) => {
        e.stopPropagation();
        const origEl =
          originals.get(key) ||
          (key ? document.querySelector(`[data-rb-inline-key="${CSS.escape(key)}"]`) : null);
        const source = (origEl?.innerText || origEl?.textContent || "").trim();
        dispatchParaAction({ type: "retranslate", key, source });
        removeActionBar();
      };
      const edit = document.createElement("button");
      edit.type = "button";
      edit.className = "rb-liquid-btn";
      edit.textContent = "✎";
      edit.title = s.editTitle;
      edit.onclick = (e) => {
        e.stopPropagation();
        openInlineEditor(el, key);
      };
      const report = document.createElement("button");
      report.type = "button";
      report.className = "rb-liquid-btn";
      report.textContent = "⚑";
      report.title = s.reportTitle;
      report.onclick = (e) => {
        e.stopPropagation();
        const origEl =
          originals.get(key) ||
          (key ? document.querySelector(`[data-rb-inline-key="${CSS.escape(key)}"]`) : null);
        const source = (origEl?.innerText || origEl?.textContent || "").trim();
        const translation = (el.innerText || el.textContent || "").trim();
        dispatchParaAction({ type: "report-translation", key, source, translation });
        removeActionBar();
      };
      actionBar.onmouseenter = () => clearTimeout(hideTimer);
      actionBar.onmouseleave = () => {
        hideTimer = setTimeout(removeActionBar, 250);
      };
      actionBar.appendChild(retry);
      actionBar.appendChild(edit);
      actionBar.appendChild(report);
      document.documentElement.appendChild(actionBar);
    }

    document.addEventListener("mouseover", (event) => {
      const el = event.target?.closest?.("[data-rb-inline-translation-for]");
      if (el) showActionBarFor(el);
    });

    document.addEventListener("mouseout", (event) => {
      const el = event.target?.closest?.("[data-rb-inline-translation-for]");
      if (el) {
        hideTimer = setTimeout(removeActionBar, 250);
      }
    });

    document.addEventListener(
      "touchend",
      (event) => {
        if (event.target?.closest?.("#rb-para-actions, #rb-inline-editor, a, button, input, textarea")) {
          return;
        }
        const el = event.target?.closest?.("[data-rb-inline-translation-for]");
        if (el) {
          showActionBarFor(el);
          clearTimeout(hideTimer);
          hideTimer = setTimeout(removeActionBar, 4000);
        } else {
          removeActionBar();
        }
      },
      { passive: true },
    );
  }

  return applied;
})(__PAYLOAD__, __FONT_SCALE__, __LINE_HEIGHT__, __BILINGUAL__, __UI_LANG__)
