"use strict";
// Loaded in <head>, before anything is drawn, so the page never flashes the wrong
// appearance. Choices stay in this browser (or app window).
(() => {
  const root = document.documentElement;
  const media = window.matchMedia("(prefers-color-scheme: dark)");
  const DEFAULTS = { theme: "system", text: "m", width: "normal", send: "mod" };
  const get = (key) => {
    try { return localStorage.getItem("airth-" + key) || DEFAULTS[key]; } catch { return DEFAULTS[key]; }
  };
  const resolve = (pref) => (pref === "system" ? (media.matches ? "dark" : "light") : pref);
  const applyTheme = (pref) => {
    root.dataset.theme = resolve(pref);
    root.dataset.themePref = pref;
  };
  applyTheme(get("theme"));
  // Set by the Mac app's window, which leaves the title bar to the page.
  if (window.airthShell) root.dataset.shell = window.airthShell;
  root.dataset.text = get("text");
  root.dataset.width = get("width");
  media.addEventListener("change", () => { if (get("theme") === "system") applyTheme("system"); });

  /** Reading preferences: text size ("s", "m", "l", "xl"), width, how to send. */
  window.airthPrefs = {
    get,
    set(key, value) {
      try { localStorage.setItem("airth-" + key, value); } catch {}
      if (key === "text" || key === "width") root.dataset[key] = value;
    },
  };

  window.airthTheme = {
    pref: () => get("theme"),
    current: () => resolve(get("theme")),
    /** Switch theme; with a click event, reveal the new theme in a circle from the pointer. */
    set(pref, event) {
      try { localStorage.setItem("airth-theme", pref); } catch {}
      const changes = resolve(pref) !== root.dataset.theme;
      const still = window.matchMedia("(prefers-reduced-motion: reduce)").matches;
      if (!changes || still || !document.startViewTransition) return applyTheme(pref);
      const x = event && event.clientX != null ? event.clientX : window.innerWidth / 2;
      const y = event && event.clientY != null ? event.clientY : window.innerHeight / 2;
      const r = Math.hypot(Math.max(x, window.innerWidth - x), Math.max(y, window.innerHeight - y));
      const t = document.startViewTransition(() => applyTheme(pref));
      t.ready.then(() => {
        root.animate(
          { clipPath: [`circle(0px at ${x}px ${y}px)`, `circle(${r}px at ${x}px ${y}px)`] },
          { duration: 520, easing: "cubic-bezier(.2,.8,.2,1)", pseudoElement: "::view-transition-new(root)" });
      }).catch(() => {});
    },
  };
})();
