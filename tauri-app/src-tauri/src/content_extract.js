(() => {
  try {
    // Restore all marked originals, including those whose translation sibling
    // was removed by a site's own DOM update.
    document.querySelectorAll("[data-rb-inline-key]").forEach((element) => {
      element.hidden = false;
      element.removeAttribute("data-rb-inline-key");
      element.removeAttribute("data-rb-bilingual");
    });
    document.querySelectorAll("[data-rb-link-idx]").forEach((element) => {
      element.removeAttribute("data-rb-link-idx");
      element.removeAttribute("data-rb-verbatim");
    });
    document.querySelectorAll("[data-rb-media-idx]").forEach((element) => {
      element.removeAttribute("data-rb-media-idx");
      element.removeAttribute("data-rb-media-w");
      element.removeAttribute("data-rb-media-h");
      element.removeAttribute("data-rb-media-disp");
    });
    document
      .querySelectorAll("[data-rb-inline-translation-for]")
      .forEach((element) => element.remove());
    document.getElementById("rb-inline-style")?.remove();

    const norm = (value) =>
      (value || "")
        .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/g, "")
        .replace(/[ \t]+/g, " ")
        .replace(/[ \t]*\n[ \t]*/g, "\n")
        .replace(/\n{3,}/g, "\n\n")
        .trim();

    const normPre = (value) =>
      (value || "")
        .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f]/g, "")
        .replace(/\r\n?/g, "\n")
        .replace(/^\n+|\n+$/g, "");

    const isHidden = (element) => {
      const style = window.getComputedStyle(element);
      return style.display === "none" || style.visibility === "hidden";
    };

    const textLength = (element) => (element.innerText || "").length;

    const score = (element) => {
      const length = textLength(element);
      if (length < 200) return 0;
      const linkLength = Array.from(element.querySelectorAll("a")).reduce(
        (total, link) => total + textLength(link),
        0,
      );
      const paragraphs = element.querySelectorAll("p").length;
      const linkRatio = Math.max(0.1, 1 - linkLength / Math.max(length, 1));
      return length * linkRatio * (1 + Math.min(paragraphs, 50) / 50);
    };

    const pickedRoot = document.querySelector("[data-rb-picked-root='1']");
    if (pickedRoot) {
      pickedRoot.removeAttribute("data-rb-picked-root");
    }

    let root = pickedRoot;
    if (!root) {
      root = document.body || document.documentElement;
    }

    const selector =
      "h1,h2,h3,h4,h5,h6,p,li,blockquote,pre,figcaption,td,th,dd,dt,summary,div";
    const blockChildSelector =
      "h1,h2,h3,h4,h5,h6,p,li,blockquote,pre,figcaption,td,th,dd,dt,summary,div,section,article,main,ul,ol,table,aside,nav,header,footer,form";
    const isCommentArea = (element) =>
      Boolean(
        element.closest(
          "[id*='comment' i],[class*='comment' i],[id*='reply' i],[class*='reply' i],[id*='review' i],[class*='review' i],[class*='kansou' i]",
        ),
      );
    let elements = Array.from(root.querySelectorAll(selector)).filter(
      (element) => {
        if (isHidden(element) || element.closest("[id^='rb-'], script, style, noscript, svg, template")) {
          return false;
        }
        if (element.tagName === "DIV") {
          if (element.querySelector(blockChildSelector)) return false;
          if (element.closest("a, button, label")) return false;
          const t = norm(element.innerText || element.textContent);
          if (t.length < 3) return false;
        }
        if (pickedRoot) return true;
        if (
          element.closest(
            "nav, [role='navigation'], .fandom-ad-placeholder, .fandom-ad-wrapper, [class*='fandom-ad' i], cnx-lit-ui-template, .cnx-main-container, .page__right-rail, .right-rail-wrapper, .auth-right-rail, .wds-collapsible-panel.wds-is-collapsed, .page-footer__trfc-147",
          )
        ) {
          return false;
        }
        if (
          element.closest("header, footer") &&
          !element.matches("h1, h2") &&
          !element.closest("article, main, [role='main']") &&
          !isCommentArea(element)
        ) {
          return false;
        }
        return true;
      },
    );
    if (pickedRoot && elements.length === 0) {
      elements = [pickedRoot];
    }

    const chosen = [];
    for (const element of elements) {
      if (chosen.some((parent) => parent.contains(element))) continue;
      if (
        (element.tagName === "LI" ||
          element.tagName === "BLOCKQUOTE" ||
          element.tagName === "TD" ||
          element.tagName === "TH" ||
          element.tagName === "DD" ||
          element.tagName === "DT") &&
        element.querySelector("p,h1,h2,h3,h4,h5,h6,li,blockquote,table,div")
      ) {
        continue;
      }
      chosen.push(element);
    }

    function serializeWithLinks(element) {
      const interactives = [];
      const media = [];
      const preservePre = window.getComputedStyle(element).whiteSpace.startsWith("pre");
      function walk(node) {
        if (node.nodeType === Node.TEXT_NODE) {
          const val = node.nodeValue || "";
          return preservePre ? val : val.replace(/[\r\n\t]+/g, " ");
        }
        if (node.nodeType !== Node.ELEMENT_NODE) return "";
        if (isHidden(node)) return "";
        const tag = node.tagName;
        if (tag === "RT" || tag === "RP") return "";
        if (
          node.classList &&
          (node.classList.contains("mw-editsection") ||
            node.classList.contains("mw-collapsible-toggle") ||
            node.classList.contains("mw-collapsible-text"))
        ) {
          return "";
        }
        if (tag === "BR") return "\n";
        if (tag === "IMG" || tag === "SVG") {
          if (tag === "IMG") {
            const src = (node.getAttribute("src") || "").trim();
            const dataSrc = (node.getAttribute("data-src") || "").trim();
            if ((!src || src.startsWith("data:image/gif")) && !dataSrc) {
              return "";
            }
            if (
              src.startsWith("data:image/svg+xml") &&
              node.parentElement &&
              node.parentElement.querySelectorAll(":scope > img").length > 1
            ) {
              return "";
            }
          }
          const cs = window.getComputedStyle(node);
          if (
            cs.display === "none" ||
            cs.visibility === "hidden" ||
            parseFloat(cs.opacity || "1") === 0
          ) {
            return "";
          }
          let rect = node.getBoundingClientRect();
          if ((rect.width < 1 || rect.height < 1) && node.parentElement) {
            const pRect = node.parentElement.getBoundingClientRect();
            if (
              pRect.width >= 1 &&
              pRect.height >= 1 &&
              pRect.width <= 1200 &&
              pRect.height <= 1200
            ) {
              rect = pRect;
            }
          }
          if (rect.width < 1 || rect.height < 1) return "";
          if (media.length >= 24) return "";
          const mIdx = media.length;
          media.push(node);
          node.setAttribute("data-rb-media-idx", String(mIdx));
          node.setAttribute("data-rb-media-w", String(Math.round(rect.width * 10) / 10));
          node.setAttribute("data-rb-media-h", String(Math.round(rect.height * 10) / 10));
          node.setAttribute(
            "data-rb-media-disp",
            cs.display === "block" && rect.width >= 120 ? "block" : "inline-block",
          );
          return `[[img${mIdx}]]`;
        }
        if (
          tag === "A" ||
          tag === "BUTTON" ||
          ((tag === "DEL" || tag === "S" || tag === "CODE") && !node.querySelector("a, button"))
        ) {
          const inner = norm(node.innerText || node.textContent)
            .replace(/\[\[/g, "(")
            .replace(/\]\]/g, ")");
          if (!inner) {
            let childOut = "";
            for (const child of node.childNodes) childOut += walk(child);
            return childOut;
          }
          if (interactives.length >= 250) return inner;
          const idx = interactives.length;
          interactives.push(node);
          node.setAttribute("data-rb-link-idx", String(idx));
          if (
            tag === "CODE" &&
            inner.length <= 64 &&
            (!/\s/.test(inner) ||
              inner.startsWith("-") ||
              inner.includes("_") ||
              /^[A-Za-z0-9_./:\\-]+$/.test(inner))
          ) {
            node.setAttribute("data-rb-verbatim", "1");
          }
          return `[[${idx}|${inner}]]`;
        }
        let out = "";
        for (const child of node.childNodes) {
          out += walk(child);
        }
        return out;
      }
      const raw = walk(element);
      return preservePre ? normPre(raw) : norm(raw);
    }

    const blocks = [];
    const seen = new Set();

    for (const element of chosen) {
      const tag = element.tagName;
      let kind = "paragraph";
      let level = null;
      let text = "";
      let rows = null;

      const inTableCell = Boolean(
        tag === "TD" || tag === "TH" || element.closest("td, th"),
      );
      if (tag === "PRE") {
        kind = "code";
        text = (element.textContent || "").replace(/\s+$/g, "");
        if (text.length > 12000) text = text.slice(0, 12000);
      } else {
        if (inTableCell) {
          kind = "tableCell";
        } else if (/^H[1-6]$/.test(tag)) {
          kind = "heading";
          level = Number(tag.slice(1));
        } else if (tag === "LI") {
          kind = "listItem";
        } else if (tag === "BLOCKQUOTE") {
          kind = "quote";
        } else if (tag === "FIGCAPTION") {
          kind = "caption";
        }
        text = serializeWithLinks(element);
      }

      const textWithoutMedia = text.replace(/\[\[img\d+\]\]/g, "").trim();
      if (textWithoutMedia.length < 2) continue;

      if (kind !== "code" && !inTableCell) {
        const dedupeKey = kind + "\u0001" + text;
        if (seen.has(dedupeKey)) continue;
        seen.add(dedupeKey);
      }

      const inlineKey = `rb-${Date.now()}-${blocks.length}-${Math.random().toString(36).slice(2)}`;
      element.setAttribute("data-rb-inline-key", inlineKey);
      if (inTableCell) {
        element.setAttribute("data-rb-bilingual", "1");
      }
      const rect = element.getBoundingClientRect();
      const vh = window.innerHeight || 900;
      const group = rect.bottom >= 0 && rect.top <= vh ? 0 : rect.top > vh ? 1 : 2;
      const dist = group === 2 ? -rect.bottom : rect.top;
      const docIndex = blocks.length;
      blocks.push({ kind, level, text, rows, inlineKey, docIndex, _g: group, _d: dist });
      if (blocks.length >= 3000) break;
    }

    blocks.sort((a, b) => a._g - b._g || a._d - b._d);
    const cleanBlocks = blocks.map(({ _g, _d, ...rest }) => rest);

    try {
      window.__tbDisconnectViewport?.();
      const inViewport = new Set();
      const nearViewport = new Set();
      window.__tbViewportSeq = (window.__tbViewportSeq || 0) + 1;
      const bump = () => {
        window.__tbViewportSeq = (window.__tbViewportSeq || 0) + 1;
      };
      const ioVisible = typeof IntersectionObserver === "function"
        ? new IntersectionObserver((entries) => {
            let changed = false;
            for (const entry of entries) {
              const key = entry.target.getAttribute("data-rb-inline-key");
              if (!key) continue;
              if (entry.isIntersecting) {
                if (!inViewport.has(key)) { inViewport.add(key); changed = true; }
              } else if (inViewport.delete(key)) {
                changed = true;
              }
            }
            if (changed) bump();
          }, { rootMargin: "0px" })
        : null;
      const ioLookahead = typeof IntersectionObserver === "function"
        ? new IntersectionObserver((entries) => {
            let changed = false;
            for (const entry of entries) {
              const key = entry.target.getAttribute("data-rb-inline-key");
              if (!key) continue;
              if (entry.isIntersecting) {
                if (!nearViewport.has(key)) { nearViewport.add(key); changed = true; }
              } else if (nearViewport.delete(key)) {
                changed = true;
              }
            }
            if (changed) bump();
          }, { rootMargin: "100% 0px" })
        : null;

      const collectMarked = () => {
        const list = [];
        const rootNode = document.body || document.documentElement;
        if (!rootNode) return list;
        const walker = document.createTreeWalker(rootNode, NodeFilter.SHOW_ELEMENT, {
          acceptNode(node) {
            return node.hasAttribute && node.hasAttribute("data-rb-inline-key")
              ? NodeFilter.FILTER_ACCEPT
              : NodeFilter.FILTER_SKIP;
          },
        });
        let node;
        while ((node = walker.nextNode())) {
          list.push(node);
        }
        return list;
      };

      const targets = collectMarked();
      for (const el of targets) {
        ioVisible?.observe(el);
        ioLookahead?.observe(el);
      }

      const onScrollOrResize = () => bump();
      window.addEventListener("scroll", onScrollOrResize, { passive: true });
      window.addEventListener("resize", onScrollOrResize, { passive: true });

      window.__tbGetViewportOrder = () => {
        const vh = window.innerHeight || 900;
        const ranked = [];
        for (const el of collectMarked()) {
          const key = el.getAttribute("data-rb-inline-key");
          if (!key) continue;
          const rect = el.getBoundingClientRect();
          let bucket = 3;
          let dist = -rect.bottom;
          if (inViewport.has(key) || (rect.bottom >= 0 && rect.top <= vh)) {
            bucket = 0;
            dist = rect.top;
          } else if (nearViewport.has(key) || (rect.top > vh && rect.top <= vh * 2.1)) {
            bucket = 1;
            dist = rect.top;
          } else if (rect.top > vh * 2.1) {
            bucket = 2;
            dist = rect.top;
          }
          ranked.push({ key, bucket, dist });
        }
        ranked.sort((a, b) => a.bucket - b.bucket || a.dist - b.dist);
        return {
          seq: window.__tbViewportSeq || 0,
          keys: ranked.map((r) => r.key),
        };
      };

      window.__tbDisconnectViewport = () => {
        ioVisible?.disconnect();
        ioLookahead?.disconnect();
        window.removeEventListener("scroll", onScrollOrResize);
        window.removeEventListener("resize", onScrollOrResize);
      };
    } catch (_) {
      // Fallback safely when CSP or headless contexts restrict IntersectionObserver.
    }

    return {
      title: norm(document.title),
      url: location.href,
      site: location.hostname,
      lang: document.documentElement.lang || null,
      blocks: cleanBlocks,
    };
  } catch (error) {
    return {
      error: error && error.message ? error.message : String(error),
    };
  }
})()
