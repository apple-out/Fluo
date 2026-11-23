(() => {
  if (window.__tbRegionPickerCleanup) {
    window.__tbRegionPickerCleanup();
  }
  document.querySelectorAll("[data-rb-picked-root]").forEach((el) => {
    el.removeAttribute("data-rb-picked-root");
  });

  const overlay = document.createElement("div");
  overlay.id = "rb-region-picker-box";
  overlay.style.cssText =
    "position:fixed;pointer-events:none;z-index:2147483646;border:2px solid var(--rb-accent, #684cff);background:color-mix(in srgb, var(--rb-accent, #684cff) 16%, transparent);box-shadow:0 0 0 2px rgba(255,255,255,0.45), 0 8px 28px color-mix(in srgb, var(--rb-accent, #684cff) 28%, transparent);border-radius:14px;transition:all 60ms ease;display:none;";

  const REGION_TIPS = {
    ko: "클릭하여 이 영역 번역 · Esc 취소",
    en: "Click to translate this region · Esc to cancel",
    ja: "クリックしてこの範囲を翻訳 · Escでキャンセル",
    "zh-CN": "点击翻译此区域 · 按 Esc 取消",
  };
  const isIOS = Boolean(window.webkit?.messageHandlers?.fluoAction) || /iPhone|iPad|iPod/i.test(navigator.userAgent);
  const tip = document.createElement("div");
  tip.id = "rb-region-picker-tip";
  tip.className = "rb-liquid-surface";
  tip.textContent = REGION_TIPS[window.__tbUiLang] || REGION_TIPS.ko;
  tip.style.cssText =
    `position:fixed;top:96px;left:50%;transform:translateX(-50%);z-index:2147483647;padding:8px 16px;border-radius:999px;font:600 12px/1.4 'Pretendard',system-ui,sans-serif;pointer-events:none;background:var(--rb-ui-bg,rgba(24,28,42,0.88));color:var(--rb-ui-text,#f8faff);border:1px solid var(--rb-ui-border,rgba(255,255,255,0.28));box-shadow:0 8px 24px rgba(0,0,0,0.25);-webkit-backdrop-filter:blur(24px) saturate(1.9);backdrop-filter:blur(24px) saturate(1.9);display:${isIOS ? "none" : "block"};`;

  document.documentElement.appendChild(overlay);
  document.documentElement.appendChild(tip);

  window.__tbRegionPick = { status: "picking" };
  let hovered = null;

  function resolveRegionTarget(rawTarget) {
    if (!rawTarget || rawTarget === overlay || rawTarget === tip || rawTarget === document.documentElement) {
      return null;
    }
    const inlineTags = new Set(["SPAN", "A", "B", "I", "EM", "STRONG", "CODE", "FONT", "SMALL", "SUB", "SUP", "MARK", "U", "S", "IMG", "SVG", "PATH"]);
    let el = rawTarget;
    if (el.tagName && inlineTags.has(el.tagName)) {
      const block = el.closest("p, article, section, li, blockquote, td, th, dd, dt, h1, h2, h3, h4, h5, h6, main, div");
      if (block && block !== document.body && block !== document.documentElement) {
        el = block;
      }
    }
    return el;
  }

  function highlightElement(target) {
    const el = resolveRegionTarget(target);
    if (!el) return null;
    hovered = el;
    const rect = el.getBoundingClientRect();
    overlay.style.display = "block";
    overlay.style.left = `${rect.left}px`;
    overlay.style.top = `${rect.top}px`;
    overlay.style.width = `${rect.width}px`;
    overlay.style.height = `${rect.height}px`;
    return el;
  }

  function onMove(event) {
    highlightElement(event.target);
  }

  function cleanup() {
    document.removeEventListener("mousemove", onMove, true);
    document.removeEventListener("click", onClick, true);
    document.removeEventListener("keydown", onKey, true);
    overlay.remove();
    tip.remove();
    window.__tbRegionPickerCleanup = null;
  }

  function onClick(event) {
    event.preventDefault();
    event.stopPropagation();
    const target = highlightElement(event.target) || hovered;
    if (target && target.setAttribute) {
      target.setAttribute("data-rb-picked-root", "1");
      window.__tbRegionPick = { status: "picked" };
      try {
        window.webkit?.messageHandlers?.fluoAction?.postMessage({ type: "region-picked", status: "picked" });
      } catch {}
    } else {
      window.__tbRegionPick = { status: "cancelled" };
    }
    cleanup();
  }

  function onKey(event) {
    if (event.key === "Escape") {
      event.preventDefault();
      event.stopPropagation();
      window.__tbRegionPick = { status: "cancelled" };
      cleanup();
    }
  }

  document.addEventListener("mousemove", onMove, true);
  document.addEventListener("click", onClick, true);
  document.addEventListener("keydown", onKey, true);
  window.__tbRegionPickerCleanup = cleanup;
  return "picking";
})()
