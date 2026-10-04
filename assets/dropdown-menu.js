(function () {
  if (window.__tcDropdownMenuBound) return;
  window.__tcDropdownMenuBound = true;

  // Key matching lives in `hotkey.js` (`window.__tcHotkey`), emitted before
  // this script by `dropdown_menu`. The fallbacks below keep dismissal
  // working if that script ever fails to load.
  function targetOf(e) {
    var hk = window.__tcHotkey;
    return hk ? hk.eventTarget(e) : e.target;
  }

  function isDismiss(e) {
    var hk = window.__tcHotkey;
    if (hk) return hk.isDismissEvent(e);
    if (e.defaultPrevented || e.isComposing || e.repeat) return false;
    var key = e.key || "";
    return key === "Escape" || key === "Esc";
  }

  function isMacCancel(e) {
    var hk = window.__tcHotkey;
    return hk ? hk.isMacCancelEvent(e) : false;
  }

  function panelOf(d) {
    return d.querySelector(":scope > [data-dropdown-menu-content]");
  }

  function triggerOf(d) {
    return d.querySelector(":scope > summary");
  }

  function clearPlacement(panel) {
    panel.style.position = "";
    panel.style.top = "";
    panel.style.left = "";
    panel.style.right = "";
    panel.style.bottom = "";
    panel.style.margin = "";
    panel.style.zIndex = "";
  }

  function numAttr(el, name, fallback) {
    var v = parseInt(el.getAttribute(name) || "", 10);
    return isNaN(v) ? fallback : v;
  }

  // Float the panel above surrounding content. Absolute positioning is
  // clipped by `overflow` ancestors (e.g. a table's horizontal scroller), so
  // measure the trigger and pin the panel with `fixed` instead. Re-run on
  // scroll/resize to keep it glued to the trigger; flip to the opposite side
  // when there is no room, and clamp to the viewport.
  function placeMenu(d) {
    var trigger = triggerOf(d);
    var panel = panelOf(d);
    if (!trigger || !panel) return;
    var side = panel.getAttribute("data-side") || "bottom";
    var align = panel.getAttribute("data-align") || "start";
    var sideOffset = numAttr(panel, "data-side-offset", 4);
    var alignOffset = numAttr(panel, "data-align-offset", 0);
    var margin = 8;
    panel.style.position = "fixed";
    panel.style.margin = "0";
    panel.style.right = "auto";
    panel.style.bottom = "auto";
    panel.style.left = "0";
    panel.style.top = "0";
    panel.style.zIndex = "60";
    var t = trigger.getBoundingClientRect();
    var w = panel.offsetWidth;
    var h = panel.offsetHeight;
    var maxLeft = Math.max(margin, window.innerWidth - w - margin);
    var maxTop = Math.max(margin, window.innerHeight - h - margin);
    function crossStart() {
      return align === "center"
        ? t.left + t.width / 2 - w / 2 + alignOffset
        : align === "end"
          ? t.right - w + alignOffset
          : t.left + alignOffset;
    }
    function crossTop() {
      return align === "center"
        ? t.top + t.height / 2 - h / 2 + alignOffset
        : align === "end"
          ? t.bottom - h + alignOffset
          : t.top + alignOffset;
    }
    function clamp(v, max) {
      return Math.max(margin, Math.min(v, max));
    }
    var left = 0;
    var top = 0;
    if (side === "top" || side === "bottom") {
      left = clamp(crossStart(), maxLeft);
      top = t.bottom + sideOffset;
      var fitsBelow = top + h <= window.innerHeight - margin;
      var fitsAbove = t.top - sideOffset - h >= margin;
      if (side === "bottom") {
        if (!fitsBelow && fitsAbove) top = t.top - sideOffset - h;
        else top = Math.min(top, maxTop);
      } else {
        top = t.top - sideOffset - h;
        if (!fitsAbove && fitsBelow) top = t.bottom + sideOffset;
        else top = clamp(top, maxTop);
      }
    } else {
      top = clamp(crossTop(), maxTop);
      left = t.right + sideOffset;
      var fitsRight = left + w <= window.innerWidth - margin;
      var fitsLeft = t.left - sideOffset - w >= margin;
      if (side === "right") {
        if (!fitsRight && fitsLeft) left = t.left - sideOffset - w;
        else left = Math.min(left, maxLeft);
      } else {
        left = t.left - sideOffset - w;
        if (!fitsLeft && fitsRight) left = t.right + sideOffset;
        else left = clamp(left, maxLeft);
      }
    }
    panel.style.left = left + "px";
    panel.style.top = top + "px";
  }

  function placeAllOpen() {
    document
      .querySelectorAll("details[data-dropdown-menu][open]")
      .forEach(placeMenu);
  }

  // `toggle` does not bubble, so listen in capture phase. This also covers
  // menus added later (e.g. re-rendered shards).
  document.addEventListener(
    "toggle",
    function (e) {
      var d = e.target;
      if (!d || !d.matches || !d.matches("details[data-dropdown-menu]"))
        return;
      if (d.hasAttribute("open")) {
        placeMenu(d);
      } else {
        var panel = panelOf(d);
        if (panel) clearPlacement(panel);
      }
    },
    true
  );

  var placeScheduled = false;
  function schedulePlaceAll() {
    if (placeScheduled) return;
    placeScheduled = true;
    requestAnimationFrame(function () {
      placeScheduled = false;
      placeAllOpen();
    });
  }
  window.addEventListener("scroll", schedulePlaceAll, {
    capture: true,
    passive: true,
  });
  window.addEventListener("resize", schedulePlaceAll);

  function closeMenu(d, refocus) {
    d.removeAttribute("open");
    if (!refocus) return;
    var trigger = d.querySelector("summary");
    if (trigger && document.activeElement !== trigger) {
      try {
        trigger.focus({ preventScroll: true });
      } catch (err) {
        trigger.focus();
      }
    }
  }

  // Close open menus when clicking outside of them. A native `<details>`
  // element does not dismiss itself, so the primitive ships this scripting.
  document.addEventListener(
    "click",
    function (e) {
      var t = targetOf(e);
      document
        .querySelectorAll("details[data-dropdown-menu][open]")
        .forEach(function (d) {
          if (!d.contains(t)) closeMenu(d, false);
        });
    },
    true
  );

  // Dismiss on `Escape` (`Cmd+.` too on macOS — see `hotkey.js`). When the
  // press lands inside a menu only that menu closes and focus returns to
  // its trigger; otherwise every open menu closes.
  document.addEventListener("keydown", function (e) {
    if (!isDismiss(e)) return;
    var open = document.querySelectorAll(
      "details[data-dropdown-menu][open]"
    );
    if (!open.length) return;
    if (isMacCancel(e)) e.preventDefault();
    var t = targetOf(e);
    var origin =
      t && t.closest ? t.closest("details[data-dropdown-menu]") : null;
    open.forEach(function (d) {
      if (origin && d !== origin) return;
      closeMenu(
        d,
        Boolean((t && d.contains(t)) || d.contains(document.activeElement))
      );
    });
  });

  // Close the menu when activating a link or button inside it, so it never
  // stays open after navigation.
  document.addEventListener("click", function (e) {
    var target = targetOf(e);
    var t =
      target && target.closest
        ? target.closest(
            "details[data-dropdown-menu] a,details[data-dropdown-menu] button"
          )
        : null;
    if (t) {
      var d = t.closest("details[data-dropdown-menu]");
      if (d) d.removeAttribute("open");
    }
  });
})();
