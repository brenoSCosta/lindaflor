// Alpine components for the templ UI kit (internal/web/components).
// Loaded (deferred) before Alpine itself so `alpine:init` fires after
// these listeners are registered.
//
// - dropdownMenu: <details x-data="dropdownMenu"> — outside click / Escape
//   (Cmd+. on macOS) dismissal, closes after activating an item, pins the
//   panel with `fixed` coordinates so overflow containers do not clip it.
// - sidebar(expanded): app sidebar state (desktop collapse persisted in the
//   `sidebar_state` cookie for 7 days, mobile sheet), Mod+B shortcut.
// - $store.cart: { open } for the storefront cart sheet.
(function () {
  var isMac = /Mac|iPhone|iPad|iPod/.test(navigator.platform || navigator.userAgent || "");

  function isDismiss(e) {
    if (e.defaultPrevented || e.isComposing) return false;
    if (e.key === "Escape" || e.key === "Esc") return true;
    return isMac && e.metaKey && e.key === ".";
  }

  function isModB(e) {
    if (e.defaultPrevented || e.isComposing || e.repeat) return false;
    var key = e.key || "";
    if (key !== "b" && key !== "B" && e.code !== "KeyB") return false;
    var mod = isMac ? e.metaKey : e.ctrlKey;
    return mod && !e.altKey && !e.shiftKey;
  }

  function numAttr(el, name, fallback) {
    var v = parseInt(el.getAttribute(name) || "", 10);
    return isNaN(v) ? fallback : v;
  }

  function placeMenu(details) {
    var trigger = details.querySelector(":scope > summary");
    var panel = details.querySelector(":scope > [data-dropdown-menu-content]");
    if (!trigger || !panel) return;
    var side = panel.getAttribute("data-side") || "bottom";
    var align = panel.getAttribute("data-align") || "start";
    var sideOffset = numAttr(panel, "data-side-offset", 4);
    var alignOffset = numAttr(panel, "data-align-offset", 0);
    var margin = 8;
    var s = panel.style;
    s.position = "fixed"; s.margin = "0"; s.right = "auto"; s.bottom = "auto";
    s.left = "0"; s.top = "0"; s.zIndex = "60";
    var t = trigger.getBoundingClientRect();
    var w = panel.offsetWidth, h = panel.offsetHeight;
    var maxLeft = Math.max(margin, window.innerWidth - w - margin);
    var maxTop = Math.max(margin, window.innerHeight - h - margin);
    function clamp(v, max) { return Math.max(margin, Math.min(v, max)); }
    var crossLeft = align === "center" ? t.left + t.width / 2 - w / 2 + alignOffset
      : align === "end" ? t.right - w + alignOffset : t.left + alignOffset;
    var crossTop = align === "center" ? t.top + t.height / 2 - h / 2 + alignOffset
      : align === "end" ? t.bottom - h + alignOffset : t.top + alignOffset;
    var left, top;
    if (side === "top" || side === "bottom") {
      left = clamp(crossLeft, maxLeft);
      var fitsBelow = t.bottom + sideOffset + h <= window.innerHeight - margin;
      var fitsAbove = t.top - sideOffset - h >= margin;
      if (side === "bottom") {
        top = !fitsBelow && fitsAbove ? t.top - sideOffset - h : Math.min(t.bottom + sideOffset, maxTop);
      } else {
        top = !fitsAbove && fitsBelow ? t.bottom + sideOffset : clamp(t.top - sideOffset - h, maxTop);
      }
    } else {
      top = clamp(crossTop, maxTop);
      var fitsRight = t.right + sideOffset + w <= window.innerWidth - margin;
      var fitsLeft = t.left - sideOffset - w >= margin;
      if (side === "right") {
        left = !fitsRight && fitsLeft ? t.left - sideOffset - w : Math.min(t.right + sideOffset, maxLeft);
      } else {
        left = !fitsLeft && fitsRight ? t.right + sideOffset : clamp(t.left - sideOffset - w, maxLeft);
      }
    }
    s.left = left + "px";
    s.top = top + "px";
  }

  function clearPlacement(details) {
    var panel = details.querySelector(":scope > [data-dropdown-menu-content]");
    if (!panel) return;
    ["position", "top", "left", "right", "bottom", "margin", "zIndex"].forEach(function (k) {
      panel.style[k] = "";
    });
  }

  function writeSidebarCookie(expanded) {
    document.cookie = "sidebar_state=" + (expanded ? "expanded" : "collapsed") + "; Path=/; Max-Age=604800; SameSite=Lax";
  }

  function isDesktop() {
    return window.matchMedia("(min-width: 768px)").matches;
  }

  document.addEventListener("alpine:init", function () {
    var Alpine = window.Alpine;

    Alpine.data("dropdownMenu", function () {
      return {
        _reflow: null,
        init: function () {
          var el = this.$el;
          var self = this;
          this._reflow = function () { if (el.open) placeMenu(el); };
          el.addEventListener("toggle", function () {
            if (el.open) {
              placeMenu(el);
              window.addEventListener("scroll", self._reflow, { capture: true, passive: true });
              window.addEventListener("resize", self._reflow);
            } else {
              clearPlacement(el);
              window.removeEventListener("scroll", self._reflow, { capture: true });
              window.removeEventListener("resize", self._reflow);
            }
          });
          document.addEventListener("click", function (e) {
            if (el.open && !el.contains(e.target)) el.open = false;
          }, true);
          el.addEventListener("click", function (e) {
            var item = e.target.closest && e.target.closest("a, button");
            if (item && el.contains(item) && !item.closest("summary")) el.open = false;
          });
          el.addEventListener("keydown", function (e) {
            if (!el.open || !isDismiss(e)) return;
            if (isMac && e.metaKey) e.preventDefault();
            el.open = false;
            var trigger = el.querySelector(":scope > summary");
            if (trigger) trigger.focus({ preventScroll: true });
          });
        },
        destroy: function () {
          window.removeEventListener("scroll", this._reflow, { capture: true });
          window.removeEventListener("resize", this._reflow);
        },
      };
    });

    Alpine.data("sidebar", function (expanded) {
      return {
        expanded: !!expanded,
        mobileOpen: false,
        init: function () {
          var self = this;
          this._onKey = function (e) {
            if (!isModB(e)) return;
            e.preventDefault();
            self.toggle();
          };
          document.addEventListener("keydown", this._onKey);
          document.querySelectorAll("[data-mod-key]").forEach(function (el) {
            el.textContent = isMac ? "⌘" : "Ctrl";
          });
        },
        destroy: function () {
          document.removeEventListener("keydown", this._onKey);
        },
        toggle: function () {
          if (isDesktop()) {
            this.expanded = !this.expanded;
            writeSidebarCookie(this.expanded);
          } else {
            this.mobileOpen = !this.mobileOpen;
          }
          // A pointer click focuses the trigger and would pin its tooltip.
          if (document.activeElement && document.activeElement.blur) document.activeElement.blur();
        },
        close: function () {
          this.mobileOpen = false;
        },
      };
    });

    Alpine.store("cart", { open: false });
  });
})();
