(() => {
  window.__tbDisconnectViewport?.();
  window.__tbSetDynamicTranslate?.(false);
  window.__fluoSetDynamicObserver?.(false);
  delete window.__tbGetViewportOrder;
  delete window.__tbDisconnectViewport;
  const originals = [...document.querySelectorAll("[data-rb-inline-key]")];
  for (const original of originals) {
    original.hidden = false;
    original.removeAttribute("data-rb-inline-key");
    original.removeAttribute("data-rb-bilingual");
  }
  for (const link of document.querySelectorAll("[data-rb-link-idx]")) {
    link.removeAttribute("data-rb-link-idx");
    link.removeAttribute("data-rb-verbatim");
  }
  for (const media of document.querySelectorAll("[data-rb-media-idx]")) {
    media.removeAttribute("data-rb-media-idx");
    media.removeAttribute("data-rb-media-w");
    media.removeAttribute("data-rb-media-h");
    media.removeAttribute("data-rb-media-disp");
  }
  for (const translated of document.querySelectorAll(
    "[data-rb-inline-translation-for]",
  )) {
    translated.remove();
  }
  document.getElementById("rb-inline-style")?.remove();
  document.getElementById("rb-para-actions")?.remove();
  document.getElementById("rb-inline-editor")?.remove();
  return originals.length;
})()
