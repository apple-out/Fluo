((query, action) => {
  const MARK_ATTR = "data-tb-find";
  const ACTIVE_ATTR = "data-tb-find-active";
  const MAX_MARKS = 2000;
  const SKIP_TAGS = new Set(["SCRIPT", "STYLE", "NOSCRIPT", "TEXTAREA", "INPUT", "SELECT"]);

  function clearMarks() {
    const marks = document.querySelectorAll(`mark[${MARK_ATTR}]`);
    const parents = new Set();
    for (const mark of marks) {
      const parent = mark.parentNode;
      if (!parent) continue;
      parents.add(parent);
      parent.replaceChild(document.createTextNode(mark.textContent || ""), mark);
    }
    for (const parent of parents) {
      parent.normalize();
    }
  }

  function isVisible(el) {
    if (!el || el.nodeType !== 1) return false;
    const style = window.getComputedStyle(el);
    return style.display !== "none" && style.visibility !== "hidden";
  }

  function buildMarks(needle) {
    if (!document.body) return [];
    const lowerNeedle = needle.toLowerCase();
    const needleLen = needle.length;
    const walker = document.createTreeWalker(document.body, NodeFilter.SHOW_TEXT, {
      acceptNode(node) {
        if (!node.nodeValue || !node.nodeValue.trim()) return NodeFilter.FILTER_REJECT;
        const parent = node.parentElement;
        if (!parent || SKIP_TAGS.has(parent.tagName) || parent.hasAttribute(MARK_ATTR)) {
          return NodeFilter.FILTER_REJECT;
        }
        return isVisible(parent) ? NodeFilter.FILTER_ACCEPT : NodeFilter.FILTER_REJECT;
      },
    });

    const textNodes = [];
    let current;
    while ((current = walker.nextNode())) {
      textNodes.push(current);
    }

    const created = [];
    for (const node of textNodes) {
      if (created.length >= MAX_MARKS) break;
      const text = node.nodeValue || "";
      const lowerText = text.toLowerCase();
      let start = 0;
      let matchIdx = lowerText.indexOf(lowerNeedle, start);
      if (matchIdx === -1) continue;

      const frag = document.createDocumentFragment();
      while (matchIdx !== -1 && created.length < MAX_MARKS) {
        if (matchIdx > start) {
          frag.appendChild(document.createTextNode(text.slice(start, matchIdx)));
        }
        const mark = document.createElement("mark");
        mark.setAttribute(MARK_ATTR, "1");
        mark.style.cssText = "background:#fde047;color:#111;padding:0;border-radius:2px;";
        mark.textContent = text.slice(matchIdx, matchIdx + needleLen);
        frag.appendChild(mark);
        created.push(mark);
        start = matchIdx + needleLen;
        matchIdx = lowerText.indexOf(lowerNeedle, start);
      }
      if (start < text.length) {
        frag.appendChild(document.createTextNode(text.slice(start)));
      }
      node.parentNode?.replaceChild(frag, node);
    }
    return created;
  }

  function paintActive(marks, index) {
    for (let i = 0; i < marks.length; i += 1) {
      const mark = marks[i];
      if (i === index) {
        mark.setAttribute(ACTIVE_ATTR, "1");
        mark.style.background = "var(--rb-accent, #684cff)";
        mark.style.color = "#ffffff";
      } else {
        mark.removeAttribute(ACTIVE_ATTR);
        mark.style.background = "color-mix(in srgb, var(--rb-accent, #684cff) 28%, #fde047)";
        mark.style.color = "#111111";
      }
    }
    if (marks[index]) {
      marks[index].scrollIntoView({ block: "center", behavior: "smooth" });
    }
  }

  const trimmed = typeof query === "string" ? query : "";
  if (action === "clear" || trimmed.length === 0) {
    clearMarks();
    window.__tbFindState = null;
    return JSON.stringify({ total: 0, current: 0 });
  }

  const prev = window.__tbFindState;
  let marks = Array.from(document.querySelectorAll(`mark[${MARK_ATTR}]`));
  let index = 0;

  if (!prev || prev.query !== trimmed || action === "find" || marks.length === 0) {
    clearMarks();
    marks = buildMarks(trimmed);
    index = 0;
  } else if (action === "next") {
    index = (prev.index + 1) % marks.length;
  } else if (action === "prev") {
    index = (prev.index - 1 + marks.length) % marks.length;
  }

  if (marks.length === 0) {
    window.__tbFindState = { query: trimmed, index: 0 };
    return JSON.stringify({ total: 0, current: 0 });
  }

  paintActive(marks, index);
  window.__tbFindState = { query: trimmed, index };
  return JSON.stringify({ total: marks.length, current: index + 1 });
})
