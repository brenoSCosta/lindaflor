# AGENTS.md — lindaflor (Topcoat)

This repo is a Rust web app built on **Topcoat 0.9** (`topcoat`, `tokio`, `sqlx`/Postgres, `redis`/Valkey). Devenv provides Postgres + Valkey + `topcoat dev`.

## Canonical reference: Topcoat examples

Treat https://github.com/tokio-rs/topcoat/tree/main/examples as the **source of truth** for Topcoat APIs. Do not guess macro signatures, builder methods, or view syntax — look them up first.

Fetch raw files directly, e.g.:

```bash
curl -s https://raw.githubusercontent.com/tokio-rs/topcoat/main/examples/<name>/src/main.rs
curl -s https://api.github.com/repos/tokio-rs/topcoat/contents/examples/<name>  # list files
```

### Example index (what to consult for what)

| Example | Use when you need |
|---|---|
| `hello-world` | Minimal app: `module_router!().build()`, `#[page]`, `#[component]`, `view!`, `topcoat::dev::script()` |
| `module-router` | File-module routing, `#[layout]` + `Slot`, `href!()` type-safe links |
| `manual-router`, `routerless`, `module-router` | Router construction trade-offs |
| `ui` | Canonical UI usage: `components/<name>.rs` + `button_variants(...)`, `discover()`, `runtime()`, `AssetBundle::load()`, icons/fonts/assets in views |
| `tailwind` | Layout with `<link rel="stylesheet" href=(tailwind::stylesheet!())>`, `build.rs` Tailwind config |
| `asset` | `asset!()`, `AssetBundle`, `RouterBuilderAssetExt::assets()` |
| `session`, `cookie`, `app-context`, `context` | `.cookies()`, `.sessions(SessionConfig)`, `.app_context(pool/valkey)`, `app_context::<PgPool>(cx)`, `session::token_hash` |
| `request-response`, `path-query-params`, `error` | `#[route(GET "/...")]`, `Form<T>`, `StatusCode`, `NotFoundError`, `not_found!("/")`, `see_other` |
| `toasty-todo`, `htmx`, `alpine-ajax`, `datastar`, `live`, `sse`, `websocket`, `suspense`, `runtime` | Interactivity / realtime patterns — prefer these over ad-hoc JS |
| `icon`, `font`, `mail` | `iconify_icon`, fontsource fonts, mail setup |

## Workflow

```bash
cargo fmt
cargo clippy --all-targets
cargo test            # embedded Postgres
topcoat dev --bin lindaflor   # live server (port 4200)
sqlx migrate run      # after editing migrations/
```