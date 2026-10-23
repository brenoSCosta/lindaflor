// Development-only live reload. The server renders data-version=<BuildID>
// on this script; air restarts the process on every rebuild, so a changed
// /api/dev/version means the page (and its CSS URL) is stale.
(function () {
  var script = document.currentScript;
  var version = script && script.getAttribute("data-version");
  if (!version) return;

  var delay = 1000;

  function poll() {
    fetch("/api/dev/version", { cache: "no-store", credentials: "same-origin" })
      .then(function (res) {
        if (!res.ok) throw new Error(res.status);
        return res.json();
      })
      .then(function (data) {
        if (data && data.version && data.version !== version) {
          window.location.reload();
          return;
        }
        setTimeout(poll, delay);
      })
      .catch(function () {
        setTimeout(poll, delay);
      });
  }

  setTimeout(poll, delay);
})();
