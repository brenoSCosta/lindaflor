// Sidebar toggle shortcut (`Mod+B`: Command on macOS, Control elsewhere,
// matched by `__tcHotkey.isModKeyEvent`). Clicks the first opted-in
// `sidebar_trigger`, which runs its `@click` runtime handler to flip caller
// state. Clicking the trigger — rather than toggling state here — keeps one
// code path for mouse, touch and keyboard toggles (including cookie
// persistence in application handlers).
//
// The document listener binds only once; the `[data-mod-key]` labels refresh
// on every evaluation so late-rendered triggers update too.
(function () {
  function hotkeys() {
    return window.__tcHotkey || null;
  }

  // Fallback when `hotkey.js` failed to load: accept either modifier, since
  // the platform is unknown without it.
  function isToggle(e) {
    var hk = hotkeys();
    if (hk) return hk.isModKeyEvent(e, "B");
    if (e.defaultPrevented || e.isComposing || e.repeat) return false;
    var key = e.key || "";
    if (key !== "b" && key !== "B" && e.code !== "KeyB") return false;
    if (!e.metaKey && !e.ctrlKey) return false;
    return !e.altKey && !e.shiftKey;
  }

  function refreshModLabels() {
    var hk = hotkeys();
    var label = hk && hk.platform === "mac" ? "⌘" : "Ctrl";
    document.querySelectorAll("[data-mod-key]").forEach(function (el) {
      if (el.textContent !== label) el.textContent = label;
    });
  }

  function shortcutTrigger(e) {
    var hk = hotkeys();
    var t = hk ? hk.eventTarget(e) : e.target;
    if (!t || !t.closest) return null;
    return t.closest('[data-sidebar="trigger"][data-sidebar-shortcut]');
  }

  if (!window.__tcSidebarBound) {
    window.__tcSidebarBound = true;
    document.addEventListener("keydown", function (e) {
      if (!isToggle(e)) return;
      var trigger = document.querySelector(
        '[data-sidebar="trigger"][data-sidebar-shortcut]'
      );
      if (!trigger) return;
      e.preventDefault();
      trigger.click();
    });

    // A mouse click focuses the button, and the tooltip bubble shows on
    // `group-focus-within` — so the hint would stick around after the
    // pointer leaves. Release focus on pointer-initiated clicks (`detail`
    // is 0 for keyboard activation, whose focus must stay put).
    document.addEventListener("click", function (e) {
      if (!e.detail) return;
      var trigger = shortcutTrigger(e);
      if (!trigger) return;
      var active = document.activeElement;
      if (active && active.blur && trigger.contains(active)) active.blur();
    });
  }

  refreshModLabels();
})();
