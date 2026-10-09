// Luminous user guide — section switching, app theme sync, screenshot lightbox.
// Loaded synchronously in <head> so sections are hidden before first paint.
(function () {
  "use strict";

  const root = document.documentElement;
  root.classList.add("js");

  const DEFAULT_SECTION = "getting-started";
  const STORAGE_KEY = "luminous-guide-section";
  const ALL_LABELS = {
    en: { fit: "Fit to screen", actual: "Actual size (1:1)", close: "Close (Esc)" },
    fr: { fit: "Ajuster à l’écran", actual: "Taille réelle (1:1)", close: "Fermer (Échap)" },
    de: { fit: "An Bildschirm anpassen", actual: "Originalgröße (1:1)", close: "Schließen (Esc)" },
    es: { fit: "Ajustar a la pantalla", actual: "Tamaño real (1:1)", close: "Cerrar (Esc)" },
    it: { fit: "Adatta allo schermo", actual: "Dimensioni reali (1:1)", close: "Chiudi (Esc)" },
    ru: { fit: "По размеру экрана", actual: "Реальный размер (1:1)", close: "Закрыть (Esc)" },
    uk: { fit: "За розміром екрана", actual: "Справжній розмір (1:1)", close: "Закрити (Esc)" },
  };
  const LABELS = ALL_LABELS[root.lang] || ALL_LABELS.en;

  // ── Theme: mirror the app's colours when embedded in the Help view ──

  const THEME_VARS = [
    "--bg-main", "--bg-sidebar", "--bg-playerbar",
    "--color-accent", "--color-accent-hover", "--color-accent-contrast",
    "--color-accent-text", "--color-accent-text-hover",
    "--color-text-primary", "--color-text-secondary", "--color-border",
  ];

  function luminance(color) {
    const probe = document.createElement("span");
    probe.style.color = color;
    document.body.appendChild(probe);
    const computed = getComputedStyle(probe).color;
    probe.remove();
    const rgb = computed.match(/[\d.]+/g);
    if (!rgb || rgb.length < 3) return null;
    const [r, g, b] = rgb.slice(0, 3).map(Number);
    // color(srgb …) (e.g. from color-mix) reports 0–1 channels; rgb() reports 0–255.
    const scale = computed.startsWith("color(") ? 1 : 255;
    return (0.2126 * r + 0.7152 * g + 0.0722 * b) / scale;
  }

  function syncTheme(parentDoc) {
    const cs = parentDoc.defaultView.getComputedStyle(parentDoc.documentElement);
    for (const name of THEME_VARS) {
      const value = cs.getPropertyValue(name).trim();
      if (value) root.style.setProperty(name, value);
      else root.style.removeProperty(name);
    }
    const bg = cs.getPropertyValue("--bg-main").trim();
    const lum = bg ? luminance(bg) : null;
    if (lum !== null) root.style.colorScheme = lum < 0.5 ? "dark" : "light";
    syncScreenshots();
  }

  // ── Screenshots: show the light or dark capture matching the guide's scheme ──
  // The markup points at the dark capture (assets/{locale}/screenshots/dark/x.png); swap the
  // folder to follow the app theme when embedded, else the OS preference.
  const darkQuery = window.matchMedia("(prefers-color-scheme: dark)");
  function syncScreenshots() {
    const scheme = root.style.colorScheme || (darkQuery.matches ? "dark" : "light");
    for (const img of document.querySelectorAll("img[src*='/dark/'], img[src*='/light/']")) {
      const next = img.getAttribute("src").replace(/\/(?:light|dark)\//, `/${scheme}/`);
      if (next !== img.getAttribute("src")) {
        // Until a light capture exists, fall back to the dark one.
        img.onerror = () => { img.onerror = null; img.setAttribute("src", next.replace("/light/", "/dark/")); };
        img.setAttribute("src", next);
      }
    }
  }
  darkQuery.addEventListener("change", syncScreenshots);

  function watchParentTheme() {
    let parentDoc;
    try {
      if (window.parent === window) return;
      parentDoc = window.parent.document; // throws when cross-origin
    } catch {
      return;
    }
    if (!parentDoc.getElementById("luminous-theme-style")) return;
    let queued = false;
    const update = () => {
      if (queued) return;
      queued = true;
      requestAnimationFrame(() => {
        queued = false;
        try { syncTheme(parentDoc); } catch { /* keep the last good theme */ }
      });
    };
    update();
    const observer = new MutationObserver(update);
    observer.observe(parentDoc.head, { childList: true, subtree: true, characterData: true });
    observer.observe(parentDoc.documentElement, { attributes: true, attributeFilter: ["style", "class"] });
  }

  // ── Sections ──

  function sectionFor(id) {
    const el = id && document.getElementById(id);
    return el && el.hasAttribute("data-section") ? el : null;
  }

  function show(id, scroll) {
    const target = sectionFor(id) || sectionFor(DEFAULT_SECTION);
    for (const s of document.querySelectorAll("[data-section]")) s.classList.toggle("active", s === target);
    for (const a of document.querySelectorAll(".sidebar a")) {
      if (a.getAttribute("href") === "#" + target.id) a.setAttribute("aria-current", "page");
      else a.removeAttribute("aria-current");
    }
    try { localStorage.setItem(STORAGE_KEY, target.id); } catch { /* storage unavailable */ }
    if (scroll) window.scrollTo(0, 0);
  }

  function initSections() {
    let saved = null;
    try { saved = localStorage.getItem(STORAGE_KEY); } catch { /* storage unavailable */ }
    show(location.hash.slice(1) || saved, false);

    // Replace rather than push the hash: in the app the guide shares history with
    // the Help view, and mouse back/forward buttons should navigate the app.
    document.addEventListener("click", (e) => {
      const a = e.target.closest('a[href^="#"]');
      if (!a || !sectionFor(a.getAttribute("href").slice(1))) return;
      e.preventDefault();
      history.replaceState(null, "", a.getAttribute("href"));
      show(a.getAttribute("href").slice(1), true);
    });
    window.addEventListener("hashchange", () => show(location.hash.slice(1), true));
  }

  // ── Lightbox ──

  const ICON = {
    zoom: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><circle cx="11" cy="11" r="8"/><line x1="21" y1="21" x2="16.65" y2="16.65"/><line x1="11" y1="8" x2="11" y2="14"/><line x1="8" y1="11" x2="14" y2="11"/></svg>',
    fit: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="4 14 10 14 10 20"/><polyline points="20 10 14 10 14 4"/><line x1="14" y1="10" x2="21" y2="3"/><line x1="3" y1="21" x2="10" y2="14"/></svg>',
    actual: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><polyline points="15 3 21 3 21 9"/><polyline points="9 21 3 21 3 15"/><line x1="21" y1="3" x2="14" y2="10"/><line x1="3" y1="21" x2="10" y2="14"/></svg>',
    close: '<svg width="15" height="15" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round"><line x1="18" y1="6" x2="6" y2="18"/><line x1="6" y1="6" x2="18" y2="18"/></svg>',
  };

  let lightbox = null;

  function buildLightbox() {
    const overlay = document.createElement("div");
    overlay.className = "lb-overlay";
    overlay.setAttribute("role", "dialog");
    overlay.setAttribute("aria-modal", "true");
    overlay.innerHTML =
      '<div class="lb-backdrop"></div>' +
      '<div class="lb-modal">' +
      '<div class="lb-bar"><div class="lb-title"></div><div class="lb-controls">' +
      '<button type="button" class="lb-btn lb-zoom"></button>' +
      `<button type="button" class="lb-btn lb-close" title="${LABELS.close}" aria-label="${LABELS.close}">${ICON.close}</button>` +
      "</div></div>" +
      '<div class="lb-stage"><img class="lb-img" alt=""></div>' +
      "</div>";

    const img = overlay.querySelector(".lb-img");
    const zoomBtn = overlay.querySelector(".lb-zoom");
    const setZoom = (zoomed) => {
      img.classList.toggle("lb-zoomed", zoomed);
      const label = zoomed ? LABELS.fit : LABELS.actual;
      zoomBtn.title = label;
      zoomBtn.setAttribute("aria-label", label);
      zoomBtn.innerHTML = zoomed ? ICON.fit : ICON.actual;
    };

    overlay.querySelector(".lb-backdrop").addEventListener("click", closeLightbox);
    overlay.querySelector(".lb-close").addEventListener("click", closeLightbox);
    overlay.querySelector(".lb-stage").addEventListener("click", (e) => {
      if (e.target === img) setZoom(!img.classList.contains("lb-zoomed"));
      else closeLightbox();
    });
    zoomBtn.addEventListener("click", () => setZoom(!img.classList.contains("lb-zoomed")));

    return { overlay, img, title: overlay.querySelector(".lb-title"), setZoom, opener: null };
  }

  function openLightbox(source) {
    lightbox = lightbox || buildLightbox();
    const lb = lightbox;
    lb.opener = source;
    lb.img.src = source.currentSrc || source.src;
    lb.img.alt = source.alt;
    lb.title.textContent = source.alt;
    lb.overlay.setAttribute("aria-label", source.alt);
    lb.setZoom(false);
    document.body.appendChild(lb.overlay);
    document.body.style.overflow = "hidden";
    void lb.overlay.offsetWidth; // commit the closed state so the fade-in transitions
    lb.overlay.classList.add("open");
    lb.overlay.querySelector(".lb-close").focus();
    document.addEventListener("keydown", onKey);
  }

  function closeLightbox() {
    const lb = lightbox;
    if (!lb || !lb.overlay.isConnected) return;
    document.removeEventListener("keydown", onKey);
    lb.overlay.classList.remove("open");
    document.body.style.overflow = "";
    const done = () => lb.overlay.remove();
    const ms = parseFloat(getComputedStyle(lb.overlay).transitionDuration) * 1000;
    if (ms > 0) setTimeout(done, ms); else done();
    if (lb.opener) lb.opener.focus({ preventScroll: true });
  }

  function onKey(e) {
    if (e.key === "Escape") {
      e.preventDefault();
      e.stopPropagation();
      closeLightbox();
    }
  }

  function initLightbox() {
    for (const img of document.querySelectorAll(".shot img")) {
      img.tabIndex = 0;
      img.setAttribute("role", "button");
      const hint = document.createElement("span");
      hint.className = "zoom-hint";
      hint.setAttribute("aria-hidden", "true");
      hint.innerHTML = ICON.zoom;
      img.parentElement.appendChild(hint);
      img.addEventListener("click", () => openLightbox(img));
      img.addEventListener("keydown", (e) => {
        if (e.key === "Enter" || e.key === " ") {
          e.preventDefault();
          openLightbox(img);
        }
      });
    }
  }

  document.addEventListener("DOMContentLoaded", () => {
    initSections();
    initLightbox();
    watchParentTheme();
    syncScreenshots();
  });
})();
