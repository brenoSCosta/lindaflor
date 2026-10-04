// Shared keyboard helpers used by Topcoat primitives (`dropdown-menu.js`,
// `sidebar.js`). Exposes `window.__tcHotkey`; guarded so re-rendered components
// evaluate it only once. Framework-free and dependency-free.
//
// Core pipeline (`normalizeKeyboardEvent,
// `detectPlatform`, per-target matching): exact modifier matching, `Escape`
// firing even from inputs, and `Mod`-is-`Meta`-on-Mac platform handling.
// Note `KeyboardEvent.keyCode` is intentionally unused — it is deprecated;
// IME composition state comes from `isComposing` with a `key` fallback.
(function () {
  if (window.__tcHotkey) return;

  // Platform detection.
  // `navigator.userAgentData.platform` is preferred where available (it
  // survives user-agent reduction); otherwise fall back to the legacy
  // `navigator.platform` / `navigator.userAgent` checks. Detect once;
  // `navigator` never changes at runtime.
  function detectPlatform() {
    if (typeof navigator === "undefined") return "linux";
    var platform = "";
    if (navigator.userAgentData && navigator.userAgentData.platform) {
      platform = navigator.userAgentData.platform;
    } else {
      platform = navigator.platform || "";
    }
    var ua = navigator.userAgent || "";
    if (/mac/i.test(platform) || /mac/i.test(ua)) return "mac";
    if (/win/i.test(platform) || /win/i.test(ua)) return "windows";
    return "linux";
  }

  var platform = detectPlatform();

  function anyModifier(e) {
    return Boolean(e.metaKey || e.ctrlKey || e.altKey || e.shiftKey);
  }

  // Whether the event is part of an IME composition. Uses `isComposing`
  // with a `key === "Process"` fallback for older browsers instead of the
  // deprecated numeric key-code property.
  function isComposingEvent(e) {
    return Boolean(e.isComposing) || (e.key || "") === "Process";
  }

  // Exact, modifier-free `Escape` match, like a parsed `"Escape"` binding:
  // the legacy `"Esc"` alias plus a `code` fallback for layouts reporting
  // `"Unidentified"`. Fires even from inputs, mirroring Hotkeys' smart
  // `ignoreInputs` default for `Escape`.
  function isEscapeEvent(e) {
    var key = e.key || "";
    if (key === "Unidentified") {
      return e.code === "Escape" && !anyModifier(e);
    }
    return (
      (key === "Escape" || key === "Esc" || e.code === "Escape") &&
      !anyModifier(e)
    );
  }

  // On macOS, `Cmd+.` is the native cancel gesture (dialogs treat it as
  // `Escape`), so it dismisses too. Accepts the logical `"."` key or the
  // physical `Period` code for layout independence.
  function isMacCancelEvent(e) {
    if (platform !== "mac") return false;
    var key = e.key || "";
    return (
      Boolean(e.metaKey) &&
      !e.ctrlKey &&
      !e.altKey &&
      !e.shiftKey &&
      (key === "." || key === "Period" || e.code === "Period")
    );
  }

  // Whether the event is the platform's primary modifier plus `key`
  // (`Mod` resolves to Command on macOS and Control elsewhere).
  // Exact: no other modifiers held. Fires even from
  // inputs, matching Hotkeys' smart `ignoreInputs` default for `Mod`
  // shortcuts. `key` is one logical key (`"B"`); the physical `KeyB` /
  // `DigitB` code is also accepted so alternative layouts still match.
  function isModKeyEvent(e, key) {
    if (e.defaultPrevented) return false;
    if (isComposingEvent(e)) return false;
    if (e.repeat) return false;
    var want = (key || "").toUpperCase();
    if (!/^[A-Z0-9]$/.test(want)) return false;
    var pressed = (e.key || "").toUpperCase();
    var code = e.code || "";
    if (
      pressed !== want &&
      code !== "Key" + want &&
      code !== "Digit" + want
    ) {
      return false;
    }
    if (platform === "mac") {
      return Boolean(e.metaKey) && !e.ctrlKey && !e.altKey && !e.shiftKey;
    }
    return Boolean(e.ctrlKey) && !e.metaKey && !e.altKey && !e.shiftKey;
  }

  // Whether the event should dismiss an open overlay. Bails before any DOM
  // work on handled/composing/repeat presses, so listeners stay cheap.
  function isDismissEvent(e) {
    if (e.defaultPrevented) return false;
    if (isComposingEvent(e)) return false;
    if (e.repeat) return false;
    return isEscapeEvent(e) || isMacCancelEvent(e);
  }

  // Shadow-DOM aware event target:
  // `composedPath()` check: `e.target` alone is the shadow host from
  // outside, so `contains()` tests would misfire without this.
  function eventTarget(e) {
    if (e.composedPath) {
      var path = e.composedPath();
      if (path && path.length) return path[0];
    }
    return e.target;
  }

  window.__tcHotkey = {
    platform: platform,
    detectPlatform: detectPlatform,
    isComposingEvent: isComposingEvent,
    isEscapeEvent: isEscapeEvent,
    isMacCancelEvent: isMacCancelEvent,
    isModKeyEvent: isModKeyEvent,
    isDismissEvent: isDismissEvent,
    eventTarget: eventTarget,
  };
})();
