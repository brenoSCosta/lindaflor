// Client-side theme toggle used by `theme_toggle` (`src/theme.rs`): flip
// `dark` on `<html>`, persist the `theme` cookie, and update the button
// labels. Icons swap via CSS (`dark:`) — no navigation.
function themeToggle(btn) {
  var r = document.documentElement;
  var dark = r.classList.toggle("dark");
  var t = dark ? "dark" : "light";
  // Keep in sync with `THEME_COOKIE_MAX_AGE_SECS` in `src/theme.rs` (365 days).
  document.cookie = "theme=" + t + "; path=/; max-age=31536000";
  var l = dark ? "Ativar tema claro" : "Ativar tema escuro";
  btn.setAttribute("aria-label", l);
  btn.setAttribute("title", l);
}
window.themeToggle = themeToggle;
