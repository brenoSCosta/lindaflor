// Subtle reveal + fragment handling for the storefront (notably `#sobre`).
//
// Goals:
// - Opening `/...#sobre` lands directly on the section instead of painting at
//   the top and then jumping.
// - In-page jumps to `#sobre` smooth-scroll and play a short highlight.
// - `[data-reveal]` blocks fade/slide in once when they enter the viewport.
// Loaded with `defer` alongside the page markup.
(function () {
  var reduced = window.matchMedia("(prefers-reduced-motion: reduce)").matches;

  function highlight(el) {
    if (!el || reduced) return;
    el.classList.remove("is-target");
    // Force reflow so re-clicking the link replays the animation.
    void el.offsetWidth;
    el.classList.add("is-target");
    window.setTimeout(function () {
      el.classList.remove("is-target");
    }, 1800);
  }

  function revealInit() {
    var items = document.querySelectorAll("[data-reveal]");
    if (!items.length) return;
    if (reduced || !("IntersectionObserver" in window)) {
      for (var i = 0; i < items.length; i++) items[i].classList.add("is-visible");
      return;
    }
    var seen = new IntersectionObserver(
      function (entries) {
        for (var k = 0; k < entries.length; k++) {
          var entry = entries[k];
          if (entry.isIntersecting) {
            entry.target.classList.add("is-visible");
            seen.unobserve(entry.target);
          }
        }
      },
      { threshold: 0.15, rootMargin: "0px 0px -8% 0px" },
    );
    for (var j = 0; j < items.length; j++) seen.observe(items[j]);
  }

  function scrollToHash(hash, smooth) {
    var id = hash.charAt(0) === "#" ? hash.slice(1) : hash;
    if (!id) return false;
    var el = document.getElementById(id);
    if (!el) return false;
    el.scrollIntoView({ behavior: smooth && !reduced ? "smooth" : "instant", block: "start" });
    highlight(el);
    return true;
  }

  // Landing directly on the section: the browser may paint at the top first
  // (images/fonts shifting layout), so settle on the target right away and
  // reveal it instead of showing the travel from the hero.
  function landOnHash() {
    if (!window.location.hash) return;
    var id = window.location.hash.slice(1);
    var el = document.getElementById(id);
    if (!el) return;
    // Instant (not smooth): we are already meant to be there.
    el.scrollIntoView({ behavior: "instant", block: "start" });
    // Reveal immediately even if the observer hasn't fired yet.
    var pending = el.hasAttribute("data-reveal")
      ? [el]
      : Array.prototype.slice.call(el.querySelectorAll("[data-reveal]"));
    for (var i = 0; i < pending.length; i++) pending[i].classList.add("is-visible");
    highlight(el);
  }

  document.addEventListener("click", function (event) {
    var anchor = event.target.closest('a[href*="#"]');
    if (!anchor) return;
    var href = anchor.getAttribute("href") || "";
    var hashIndex = href.indexOf("#");
    if (hashIndex === -1) return;
    var hash = href.slice(hashIndex);
    // Same-document fragment only: cross-page `#sobre` links are handled by
    // landOnHash() after navigation.
    var url;
    try {
      url = new URL(href, window.location.href);
    } catch {
      return;
    }
    if (url.pathname !== window.location.pathname) return;
    if (scrollToHash(hash, true)) {
      event.preventDefault();
      history.replaceState(null, "", hash);
    }
  });

  if (document.readyState === "loading") {
    document.addEventListener("DOMContentLoaded", function () {
      revealInit();
      landOnHash();
    });
  } else {
    revealInit();
    landOnHash();
  }
})();
