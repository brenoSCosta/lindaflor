# AGENTS.md

Lindaflor is a server-rendered Go web app (pt-BR swimwear store): Go 1.27,
chi, templ, htmx 4, Alpine.js, Tailwind CSS v4, PostgreSQL via pgx + sqlc,
goose migrations, Valkey. Start with [`README.md`](README.md) for setup and
[`docs/GO_MIGRATION.md`](docs/GO_MIGRATION.md) for the package layout,
contracts (`app.Deps`, `auth.CurrentUser`, `render.Page`, toasts, …),
conventions and test helpers. htmx 4 specifics: [`docs/htmx4-notes.md`](docs/htmx4-notes.md).

The app was migrated from Rust/Topcoat; that code has been deleted and only
lives in git history (comments like "port of src/…rs" point there). Behavior
differences that were chosen on purpose are listed in
`docs/GO_MIGRATION.md` ("Intentional differences from the Rust app").

## Canonical references

Treat these as the source of truth; do not guess APIs or attribute syntax.

- **templ**: https://templ.guide (syntax, components, attributes, `templ fmt`) — https://github.com/a-h/templ
- **htmx 4**: https://four.htmx.org (not the htmx 1/2 docs: inheritance is explicit, 4xx/5xx swap by default, `<hx-partial>`, renamed events)
- **Alpine.js**: https://alpinejs.dev
- **Tailwind CSS v4**: https://tailwindcss.com/docs
- **sqlc**: https://docs.sqlc.dev (pgx/v5 engine, `sqlc.narg`, `sqlc.slice`, `sqlc vet`)
- **goose**: https://pressly.github.io/goose/
- **chi**: https://go-chi.io and https://pkg.go.dev/github.com/go-chi/chi/v5
- **pgx**: https://pkg.go.dev/github.com/jackc/pgx/v5
- **Go**: https://go.dev/doc/effective_go, https://go.dev/wiki/CodeReviewComments

## Toolchain

Work inside the dev shell (`devenv shell` or `nix develop`); it pins
go 1.27.3 (`GOTOOLCHAIN=local`), templ, sqlc, goose, golangci-lint v2,
gofumpt, goimports, air, tailwindcss 4, postgres 17, govulncheck, hadolint,
actionlint, shellcheck. Don't install tools with `go install` or change the
Go version in `go.mod` independently of the flake.

**Gate before finishing a change** (CI runs the same):

```bash
make generate && go build ./... && go vet ./... && make lint && go test -race ./...
```

Infra changes (Dockerfile, `scripts/`, workflows) also need `make lint-infra`,
and ideally `make docker-smoke`.

## Conventions

- **Generated code is committed**: `*_templ.go`, `internal/db/sqlc/`,
  `web/static/css/app.css`. Run `make generate` after editing `.templ`,
  `db/queries/*.sql`, migrations or Tailwind classes, and commit the
  output; CI fails on drift. Never hand-edit generated files.
- **SQL**: queries live in `db/queries/<area>.sql` with area-prefixed names;
  use the generated `sqlc.Queries`; hand-written pgx only where sqlc cannot
  express the query. Lock inventory rows `ORDER BY i.id FOR UPDATE`; the
  order FSM in `internal/store` is the only writer of order status.
- **Migrations**: new goose file `db/migrations/000NN_<area>_<what>.sql`,
  forward-only, compatible with the previous release still running
  (expand → deploy → contract). Never edit an applied migration.
- **Handlers**: `Mount(r chi.Router, d *app.Deps)` per feature package;
  closures over `d`. No-JS first: forms POST → `toast.Set` + 303 redirect;
  validation errors re-render with 422. htmx enhances the same handlers
  (`httpx.IsHTMX(r)` → fragment via `render.Fragment`, `<hx-partial>` for
  other regions; never 303 an htmx request — use `httpx.Redirect`).
  Client-only state belongs in Alpine.
- **templ**: components take `Props` structs; no hand-built HTML strings,
  no `templ.Raw` with user data. Write complete Tailwind class names (the
  scanner can't see concatenated fragments). Icons via
  `components.Icon(name, class)`.
- **Errors & logging**: wrap with `%w`, handle at the handler, log with
  `slog` (`…Context` variants). Never log secrets, tokens, cookie values or
  full emails. User-facing copy is pt-BR.
- **Config**: every env var goes through `internal/config` (supports
  `KEY_FILE`); production validation lives in `config.Load`. Document new
  variables in the README table and `docker-compose.prod.yml`.
- **Security defaults**: same-origin only (cross-origin protection, no
  CORS), `__Host-` cookies in production, CSP `'self'` (vendor JS lives in
  `web/static/vendor/`, no CDNs; the only third-party origin is Cloudflare
  Turnstile, added only when its keys are configured), per-route body
  limits and rate limits. Abuse-prone public forms use `internal/captcha`
  (see `docs/GO_MIGRATION.md` → CAPTCHA).
- **Formatting/lint**: gofumpt + goimports (`make fmt`), golangci-lint v2
  config in `.golangci.yml`; `//nolint` needs a specific linter and reason.
- **Tests**: next to the code (`*_test.go`). DB/HTTP tests use
  `internal/testutil` (`testutil.NewPool`, `testutil.NewServer`, cookie-jar
  client) and `func TestMain(m *testing.M) { testutil.Main(m) }`. Use
  `t.Setenv` for env (not with `t.Parallel`).

## Deploy surface

`Dockerfile` (targets `runtime` — distroless, uid 10001 — and `migrate` —
goose + pg_dump), `docker-compose.prod.yml`, `scripts/migrate.sh`,
`scripts/backup.sh`, `scripts/docker-smoke.sh`, `.github/workflows/ci.yml`.
The runtime image has no shell: health probes use `/app/server -healthcheck`.
