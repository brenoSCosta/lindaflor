// Cloudflare Turnstile glue for components.Turnstile (loaded on every page;
// a no-op unless a `.cf-turnstile[data-sitekey]` element is present).
//
// - Loads https://challenges.cloudflare.com/turnstile/v0/api.js once
//   (render=explicit, async) the first time a widget is on the page.
// - Renders every widget explicitly with turnstile.render (size from
//   data-size, else fitted to the container), on load and
//   after htmx swaps (`htmx:after:swap`, `htmx:after:settle`), and drops
//   widgets whose element left the DOM.
// - Resets the widgets of a form after its htmx request finishes: a token
//   is single-use, so a failed submit needs a fresh one.
(function () {
  if (window.lindaflorTurnstile) return;
  window.lindaflorTurnstile = true;

  var API = "https://challenges.cloudflare.com/turnstile/v0/api.js?render=explicit&onload=lindaflorTurnstileReady";
  var SELECTOR = ".cf-turnstile[data-sitekey]";
  var loading = false;
  var rendered = []; // [{el, id}]

  function prune() {
    rendered = rendered.filter(function (w) {
      if (w.el.isConnected) return true;
      try { window.turnstile.remove(w.id); } catch (_) { /* already gone */ }
      return false;
    });
  }

  function renderAll() {
    if (!window.turnstile) return;
    prune();
    document.querySelectorAll(SELECTOR).forEach(function (el) {
      if (el.hasAttribute("data-turnstile-id")) return;
      var id = window.turnstile.render(el, {
        sitekey: el.getAttribute("data-sitekey"),
        theme: el.getAttribute("data-theme") || "auto",
        language: el.getAttribute("data-language") || "auto",
        // "flexible" fills the form (min 300px); narrower forms (the
        // max-w-xs login card) get the 150px "compact" widget instead
        // of overflowing.
        size: el.getAttribute("data-size") || (el.clientWidth >= 300 ? "flexible" : "compact"),
      });
      if (id !== undefined && id !== null) {
        el.setAttribute("data-turnstile-id", id);
        rendered.push({ el: el, id: id });
      }
    });
  }

  function ensure() {
    if (!document.querySelector(SELECTOR)) {
      if (window.turnstile) prune();
      return;
    }
    if (window.turnstile) { renderAll(); return; }
    if (loading) return;
    loading = true;
    var s = document.createElement("script");
    s.src = API;
    s.async = true;
    s.defer = true;
    document.head.appendChild(s);
  }

  window.lindaflorTurnstileReady = renderAll;

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", ensure);
  } else {
    ensure();
  }
  document.addEventListener("htmx:after:swap", ensure);
  document.addEventListener("htmx:after:settle", ensure);

  document.addEventListener("htmx:after:request", function (e) {
    if (!window.turnstile) return;
    var src = e.target;
    var form = src && src.closest ? src.closest("form") : null;
    if (!form) return;
    form.querySelectorAll(".cf-turnstile[data-turnstile-id]").forEach(function (el) {
      try { window.turnstile.reset(el.getAttribute("data-turnstile-id")); } catch (_) { /* removed */ }
    });
  });
})();
