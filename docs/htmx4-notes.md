# htmx 4 cheat-sheet (Lindaflor)

Vendored: `web/static/vendor/htmx.min.js` (htmx.org **4.0.0**) +
`hx-alpine-compat.min.js` (keeps Alpine state through swaps/morphs) and
Alpine **3.15.12** + `@alpinejs/focus`. Docs: <https://four.htmx.org/docs>,
<https://four.htmx.org/docs/whats-new-in-htmx-4>. Unminified source for
grepping: `https://cdn.jsdelivr.net/npm/htmx.org@4.0.0/dist/htmx.js`.

Everything below was checked against the 4.0.0 source.

## What changed from htmx 2 (the traps)

| Topic | htmx 4 |
|---|---|
| Inheritance | **Off by default.** Write `hx-target:inherited="#x"`, `hx-confirm:inherited="…"`, `hx-boost:inherited="true"`; `:append` extends an inherited value (`hx-include:inherited:append=".extra"`). |
| Error responses | **4xx and 5xx are swapped** (only 204/304 are not). Our `<meta name="htmx-config">` sets `"noSwap":[204,304,"5xx"]`, so 5xx are not swapped (toast.js shows an error toast) but **422 form errors are swapped** — return the re-rendered form with status 422. |
| Per-status handling | `hx-status:422="target:#errors"`, `hx-status:5xx="swap:none"`, `hx-status:404="target:#x swap:outerHTML"`. |
| Multi-target updates | `<hx-partial hx-target="#cart-count" hx-swap="outerHTML">…</hx-partial>` in the response (converted to `<template hx type="partial">`). Use `components.HXPartial(target, swap)`. Main content swaps first, partials after. A response made **only** of partials skips the main swap. `hx-swap-oob="true"` still works too. |
| Events | `htmx:phase:action`: `htmx:before:request`, `htmx:after:request`, `htmx:before:swap`, `htmx:after:swap`, `htmx:after:settle`, `htmx:config:request`, `htmx:confirm`, `htmx:response:error` (status ≥ 400), `htmx:error` (network/exception), `htmx:finally:request`, `htmx:after:init`, `htmx:after:process`, `htmx:abort`, history: `htmx:before:history:restore`, `htmx:after:history:push/replace/update`. Detail is `{ctx}`; `ctx.response.headers` is the fetch `Headers`, `ctx.response.status`, `ctx.text` the body. |
| hx-on | `hx-on:click="…"`; htmx events: `hx-on:htmx:after:request="…"` or the shorthand `hx-on::after:request="…"`. Uses eval (our CSP keeps `'unsafe-eval'`). Prefer Alpine (`x-on:`) for client logic. |
| Renamed attrs | `hx-disable` (was `hx-disabled-elt`) disables elements during a request (`hx-disable="this"`, `"closest fieldset"`); `hx-ignore` (was `hx-disable`) turns htmx off for a subtree. |
| Request syntax | `hx-get/post/put/patch/delete` as before, or `hx-action="/x" hx-method="post"`. |
| Swap styles | `innerHTML` (default), `outerHTML`, `outerSync`, `before`/`after`/`prepend`/`append` (aliases of beforebegin…), `delete`, `none`, `innerMorph`, `outerMorph`, `textContent`. Scroll modifiers are separate keys: `hx-swap="innerHTML show:top showTarget:#x"`. |
| Trigger `from:` | Selectors with spaces must be quoted: `hx-trigger="keyup from:'closest form'"`. No trigger queue — use `hx-sync`. |
| Timeout | Default 60s (`defaultTimeout`). |
| History | No localStorage snapshots: back/forward **re-fetches** the page from the server (header `HX-History-Restore-Request`) and swaps it into `body` (or `[hx-history-elt]`). Pages must render correctly from a plain GET. |
| Preserve | `hx-preserve` + `id` keeps an element through swaps (our toaster `#toaster` uses it). |

## Headers

Sent by htmx (read with `internal/httpx`):

- `HX-Request: true` — `httpx.IsHTMX(r)`
- `HX-Request-Type: partial|full` — `full` when the target is `body` or the
  request uses `hx-select`; `httpx.IsPartial(r)` = htmx && not full.
  `render.Page` returns **only the page component** for partial requests and
  the whole document for full ones.
- `HX-Current-URL` — page URL (`httpx.CurrentPath(r)`)
- `HX-Source`, `HX-Target` (`tag#id`), `HX-Boosted`, `HX-History-Restore-Request`
- `HX-Trigger`/`HX-Trigger-Name` request headers are **gone**.

Honored in responses: `HX-Location` (ajax GET + push; `httpx.Redirect` uses it
for htmx requests with status 200), `HX-Redirect` (full navigation),
`HX-Refresh: true`, `HX-Retarget`, `HX-Reswap`, `HX-Reselect`, `HX-Trigger`
(events, JSON allowed), `HX-Push-Url`, `HX-Replace-Url`.
`HX-Trigger-After-Swap/Settle` are **gone** (use `HX-Trigger`).

Redirects: `fetch` follows 3xx transparently and htmx would swap the final
page into the target, so **never answer an htmx request with 303** — call
`httpx.Redirect` / `toast.Redirect` (303 for plain requests, 200 +
`HX-Location` for htmx).

## Config (`<meta name="htmx-config">`, set in `internal/web/layout`)

```json
{"noSwap":[204,304,"5xx"]}
```

Other keys: `defaultSwap`, `defaultTimeout`, `history` (`true`/`false`/`"reload"`),
`transitions`, `mode` (`same-origin` default), `extensions` (allow-list,
empty = all), `implicitInheritance` (keep `false`), `includeIndicatorCSS`,
`indicatorClass`, `requestClass`.

## Project conventions

- Every form must work without JS (POST → 303). Add `hx-post`/`hx-target`
  as progressive enhancement; handlers branch on `httpx.IsHTMX(r)` only when
  they return a fragment instead of redirecting.
- Former Topcoat **shards** = GET fragment endpoints at the same paths;
  **procedures** = POST endpoints returning fragments + `<hx-partial>` for
  extra regions (e.g. cart lines + `#cart-count` via `layout.CartCount(n)`).
- Toasts on htmx responses travel in the `X-Toast` header (`toast.Set` does
  it); `data-toast-loading="Salvando…"` on the triggering element shows a
  loading toast that the response's toast replaces.
- Alpine owns client-only state (sheets, dropdowns, tabs, sidebar). The
  `hx-alpine-compat` extension keeps Alpine components alive across swaps;
  swapped-in `x-data` initializes automatically.
- Scripts inside swapped content run (htmx processes `<script>` in fragments);
  page scripts like `sobre.js`/`avatar-cropper.js` must be idempotent.
- `hx-boost` is **not** enabled globally yet (decide in wave 2; if enabled,
  put `hx-boost:inherited="true"` on `<body>` — remember inheritance is explicit).
