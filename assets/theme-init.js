// Blocking init script for `<head>`: sync `dark` on `<html>` from the
// `theme` cookie before paint (next-themes-style FOUC prevention).
// Default is dark. Loaded once, synchronously, in the root layout.
(function () {
  var m = document.cookie.match(/(?:^|;\s*)theme=([^;]*)/);
  var t = m ? decodeURIComponent(m[1]) : "dark";
  var d = document.documentElement;
  if (t === "light") d.classList.remove("dark");
  else d.classList.add("dark");
})();
