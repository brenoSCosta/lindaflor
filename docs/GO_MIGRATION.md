# Go migration — record and conventions

**Status: complete.** Lindaflor was rewritten from Rust/Topcoat (sqlx,
~65k lines) to Go 1.27 + chi + templ + htmx 4 + Alpine.js + Tailwind v4 +
pgx/sqlc + goose on branch `migration-golang`, reaching route and test
parity (verified by a parity audit: every Rust route mapped, the Rust test
suite ported next to the Go code, a browser walk-through of the storefront,
checkout, account and admin). The Rust tree (`src/`, `crates/`, `tests/`,
`migrations/`, `assets/`, `styles.css`, `Cargo.*`, `.sqlx/`) was then
deleted; it remains in git history (the `migration` branch and the commits
before the deletion) as the behavioral reference.

This document keeps what is useful for future work: the package layout,
the shared contracts, the conventions every feature package follows, the
deliberate behavior differences from the Rust app, and a short history of
how the migration was run. Setup and deployment live in
[`README.md`](../README.md); agent/contributor rules in
[`AGENTS.md`](../AGENTS.md); htmx 4 specifics in
[`htmx4-notes.md`](htmx4-notes.md).

## Toolchain & commands

`nix develop` (flake) or `devenv shell` — Go 1.27.3, templ 0.3.1020, sqlc
1.31, goose 3.29, golangci-lint 2.15, gofumpt, goimports, gopls, air,
tailwindcss 4.3, postgres 17, valkey 9.

| Task | Command |
|---|---|
| Generate templ + sqlc + CSS | `make generate` |
| Build | `make build` (or `go build ./...`) |
| Test (all) | `go test ./...` · race: `go test -race ./...` |
| One package | `go test ./internal/web/storefront/...` |
| Lint | `make lint` (golangci-lint v2 + `sqlc vet` + `templ fmt -fail`) |
| Format | `make fmt` (gofumpt + goimports via `golangci-lint fmt`, `templ fmt`) |
| Drift check (CI) | `make drift` |
| Dev server | `devenv up` (postgres :4201, valkey :4202, rustfs :4203/4204, pgweb :4205, app :4200 via air) or `make dev` |
| Migrate | `make migrate` (goose, `$DATABASE_URL`) — dev server also migrates on boot |

**Gate before you finish** (CI runs the same):
`nix develop -c bash -c 'make generate && go build ./... && go vet ./... && make lint && go test -race ./...'`

Generated code is **committed**: `*_templ.go`, `internal/db/sqlc/`,
`web/static/css/app.css`. Run `make generate` after editing `.templ`,
`db/queries/*.sql`, migrations or classes, and commit the output.

## Layout

```
cmd/server         main: config → pool (migrate/verify) → valkey → router → http.Server (30s graceful) → jobs.Start
cmd/seed           demo data: admin, settings, warehouse, catalog (data/catalog.json), looks, sets (idempotent)
cmd/create-admin   create/promote an admin (password on stdin); ships in the runtime image
db/migrations      goose SQL (00001_init.sql = fresh baseline: Rust 0001..0030 folded + sqlc domains + views); embedded by package db (db/embed.go)
db/queries         sqlc query files, ONE FILE PER AREA (see below)
internal/
  config           every env var (KEY or KEY_FILE), Load() validates production
  db               pgxpool Open/Migrate/RequireSchemaCurrent, WithTx, EscapeLike; sqlc/ generated
  valkey           go-redis client + in-process fallback; lockouts, TOTP reuse, pending-2FA binding, enroll flash
  captcha          Cloudflare Turnstile siteverify (policy below), wired via Deps.Captcha
  httpx            middleware (request id, client IP, recover, security headers, body limit, origin), redirects, htmx helpers
  httpx/toast      flash toasts (cookie for full pages, X-Toast header for htmx)
  auth             argon2id, sessions, one-time tokens, TOTP/backup codes, step-up, Google OAuth (PKCE), middleware
  mail audit payments storage ratelimit seo metrics logging   platform services (outbox, MP + static PIX, S3, GCRA limits, …)
  store            domain logic: cart, inventory, coupons, pricing, checkout, order FSM, returns, reviews, reports, …
  jobs maintenance background loops (outbox worker, reservation sweep, hygiene, stock alerts, abandoned carts)
  app              Deps container + ShellLoader type
  web              router.go (middleware stack + one Mount call per feature package)
  web/render       Page / Status / NotFound / Fragment
  web/layout       Root, StorefrontShell (+Header/Footer/AnnouncementBar/CartCount), AppShell, NotFound
  web/components   templ UI kit (port of src/components/*), lucide icons
  web/format       Price (R$ 1.234,56), DateTimeBR (UTC-3), DateBR, DateISO, TodayBR
  web/{authpages,authapi,storefront,checkout,account,admin,platform}   one Mount(r, d) each
  testutil         Postgres per test binary + template DB per test, miniredis, test server + cookie-jar client
web/static         embedded assets (css/app.css generated, img/, js/, vendor/) + URL()/Handler() cache busting
web/styles         input.css (Tailwind v4 source)
```

New migrations: add `db/migrations/000NN_<area>_<what>.sql` (next free
number), forward-only, compatible with the previous release still running
(expand → deploy → contract). Never edit an applied migration.

## Contracts

```go
// internal/app
type ShellLoader func(r *http.Request) (layout.ShellData, error)
type Deps struct {
	Cfg *config.Config; Pool *pgxpool.Pool; Q *sqlc.Queries; Valkey *valkey.Client
	Mailer mail.Mailer; Storage storage.Store; Payments payments.Provider
	Orders payments.OrderTransitioner /* store.OrderFSM */; Logger *slog.Logger; Shell ShellLoader
	Captcha *captcha.Verifier /* Turnstile; NewRouter defaults it from Cfg.Captcha */
}
// every feature package
func Mount(r chi.Router, d *app.Deps)

// internal/auth
type User struct{ ID uuid.UUID; Name, Email string; EmailVerified bool; Image *string; TwoFactorEnabled bool; Role *string; Banned bool; BanReason *string; BanExpires *time.Time }
func (u *User) IsAdmin() bool
type Session struct{ ID, UserID uuid.UUID; ExpiresAt time.Time; ImpersonatedBy *uuid.UUID; TokenHash string }
func CurrentUser(ctx) (*User, bool); func CurrentSession(ctx) (*Session, bool); func IsImpersonating(ctx) bool
func WithUser(ctx, *User, *Session) context.Context        // middleware + tests
type MiddlewareDeps struct{ Pool *pgxpool.Pool; Cfg *config.Config; Valkey *valkey.Client }
func Middleware(MiddlewareDeps) func(http.Handler) http.Handler // resolves the session cookie
func RequireUser(http.Handler) http.Handler /* → /login?next=… */; func RequireAdmin(http.Handler) http.Handler
func SessionCookieName(secure bool) string                // "session" / "__Host-session"

// internal/mail
type Msg struct{ Kind, To, Subject, HTML, Text, DedupeKey string }
func Enqueue(ctx, db sqlc.DBTX, msg Msg) error             // real insert into email_outbox
type Mailer interface{ Send(ctx, Msg) error; Kind() string }
// Kind* constants for every outbox kind

// internal/audit (complete)
type Event struct{ AggregateType, AggregateID, EntityType string; EntityID uuid.UUID; ActorID *uuid.UUID; ActorName *string; Metadata any }
func Record(ctx, db sqlc.DBTX, e Event) error

// internal/payments
type Provider interface {
	Kind() config.ProviderKind; AcceptingCharges() bool
	CreateCharge(ctx, NewCharge) (Charge, error); FetchStatus(ctx, int64) (PaymentInfo, error); Refund(ctx, int64) error
}
type StaticPix struct{}; func FromConfig(config.PaymentConfig) Provider
// NewCharge, Charge, PaymentInfo, ChargeStatus, ErrUnavailable, ErrNotSupported, *TransientError, *RejectedError, IsTransient

// internal/storage
type Store interface{ Put(ctx, key, body, contentType) error; Delete(ctx, key) error; PresignGet(ctx, key) (string, error); EnsureBucket(ctx) error; HealthCheck(ctx) error }
type Memory; NewMemory(); type Unavailable; IsUnavailable(Store) bool

// internal/ratelimit
func Limit(scope string, defaultPerMin uint32) func(http.Handler) http.Handler
func MutationsOnly(scope string, defaultPerMin uint32) func(http.Handler) http.Handler
func Global() func(http.Handler) http.Handler
// Scope* and Default*PerMin constants; env override config.RateLimitFor(scope, def)

// internal/seo
type Head struct{ Title, Description, Canonical string; NoIndex bool; OpenGraph *OpenGraph; JSONLD []any }
func New(title) Head; SiteDefault() Head; (Head).WithDescription/WithCanonical/WithNoIndex/WithOpenGraph/WithJSONLD
func SetHead(ctx, Head)        // call in the handler BEFORE render.Page
func HeadFrom(ctx) (Head, bool); func Resolve(...) Resolved; IsPrivatePath; SiteOrigin; AbsoluteURL; JSONLDScriptBody; DecimalPrice; Summarize

// internal/jobs
func Start(ctx, d *app.Deps) *sync.WaitGroup               // server shutdown waits on it

// internal/valkey (complete port of src/valkey.rs)
func Connect(ctx, url) (*Client, error); NewWithRedis(*redis.Client); NewFallbackOnly()
(c).RecordAuthFail/AuthFailCount/IsAuthLockedOut/ClearAuthFails, RecordAuthEmailFail/…EmailLockedOut/ClearAuthEmailFails,
TotpReserve, RecordTotpFail, IsTotpLockedOut, PendingAttempt(s), PendingClear, PendingBindEager, PendingCheckBinding,
TotpEnrollmentFlashPut/Take; IPUAHash, PendingTokenHash, TotpReuseKey

// internal/captcha (Cloudflare Turnstile siteverify; nil or unconfigured = no-op)
func New(config.CaptchaConfig, *http.Client) *Verifier // built by NewRouter into Deps.Captcha when nil
(v).Enabled() / Enforced() / SiteKey() /* "" = no widget */; (v).Verify(ctx, token, remoteIP) (ok bool, err error)
(v).CheckForm(r) /* parsed form: cf-turnstile-response, else x-captcha-response */; (v).Check(r, captcha.Token(r, body.CaptchaToken))
captcha.FormField, JSONField ("captchaToken"), Header, ErrorCode ("CAPTCHA_FAILED"), Message (pt-BR copy)
components.Turnstile(siteKey) // widget div inside the <form>; web/static/js/turnstile.js loads api.js and renders it

// internal/httpx
RequestID, ClientIPMiddleware(TrustedProxies), Recover, SecurityHeaders(env), OriginProtection(appOrigin),
GlobalBodyLimit(n), BodyLimit(n) /* per-route override */, IsBodyTooLarge(err), TooLarge(w)
ClientIP(r) string /* "unknown" */, UserAgent(r), RequestIDFrom(ctx)
SafeLocalRedirect(raw) (string, bool), SafeLocalRedirectOr(raw, fallback)
IsHTMX(r), IsPartial(r), CurrentPath(r), Redirect(w, r, loc) /* 303 or 200+HX-Location */

// internal/httpx/toast
toast.Success/Error/Info/Warning/Plain/Loading(title) Toast; .WithDescription/.WithDuration/.WithAction/.WithCancel
toast.Set(w, r, t); toast.Redirect(w, r, loc); toast.Take(w, r) /* layout only */; toast.CookieName(secure)

// internal/web/render
render.Page(w, r, d, content); render.Status(w, r, d, code, content); render.NotFound(w, r, d); render.Fragment(w, r, code, c)

// internal/web/layout
type ShellData struct{ User *auth.User; Impersonating bool; AvatarURL string; CartCount int; Settings StoreSettings; Sets []NavSet; SidebarExpanded bool; CartSheet templ.Component }
layout.CartCount(n) /* #cart-count badge for <hx-partial> */; IsAppShellPath; WhatsappLink; Initials

// web/static
static.URL("css/app.css") → "/static/css/app.css?v=<hash>"; static.Favicon (/favicon.ico)
```

## Conventions

**Errors & logging.** Return `error`s wrapped with `%w`; handle at the
handler. Log with `slog` (`slog.ErrorContext(r.Context(), "msg", "error", err, …)`);
never log secrets, tokens, cookie values or full emails. Unexpected errors
→ 500 (plain text in handlers is fine; the logging middleware records it).
User-facing messages are pt-BR (copied verbatim from the Rust app where
it had them).

**Handlers.** `func(w http.ResponseWriter, r *http.Request)` closures that
capture `d *app.Deps`, registered in the package's `Mount`. Parse forms
with `r.ParseForm()`; check `httpx.IsBodyTooLarge(err)` → `httpx.TooLarge(w)`.
Per-route body limits: `r.With(httpx.BodyLimit(2<<20)).Post("/settings/avatar", h)`
(product create/update: 20 MiB). Rate limits: `r.With(ratelimit.Limit(scope, def))`.
Use `d.Q` (sqlc) for queries; transactions via `db.WithTx(ctx, d.Pool, func(tx pgx.Tx) error { q := sqlc.New(tx); … })`.
Cookies: `__Host-` prefixed + Secure when `d.Cfg.SecureCookies`, plain
names in development; always `HttpOnly`, `SameSite=Lax`, `Path=/`.

**Pages & forms (no-JS first).** GET renders with `render.Page`. A POST
handles the form and answers with `toast.Set` + `toast.Redirect` (303 —
POST/redirect/GET). Validation errors re-render the page with
`render.Status(w, r, d, http.StatusUnprocessableEntity, page)`. Then add
htmx on top: `hx-post` + `hx-target`, with the same handler branching on
`httpx.IsHTMX(r)` when it should return a fragment.

**Topcoat mapping.** Pages and `#[page([GET,POST])]` intents → GET/POST
handlers on the same path. **Shards → `hx-get` fragment endpoints at the
same paths**, returning just the fragment (`render.Fragment`) for htmx and
the full page otherwise. **Procedures → POST endpoints returning fragments
+ `components.HXPartial` for other regions** (cart lines + `#cart-count`),
and a 303 fallback for no-JS. Client-only signals → Alpine (`x-data`,
`$store.cart.open` for the cart sheet, `sidebar` for the app sidebar).

**templ.** One `.templ` per page/feature in your package. Components take
`Props` structs (`components.ButtonProps{Variant: components.ButtonOutline}`),
`Class` appends classes, `Attrs` (`components.Attr("type","submit")` or
`templ.Attributes{…}`) spreads onto the element, children in the body.
Style links as buttons with `components.ButtonVariants(v, s, extra…)`.
Icons: `@components.Icon("heart", "size-4")` (lucide; add SVGs to
`internal/web/components/icons/` — a test fails on unknown names). Never
build HTML strings by hand; never use `templ.Raw` with user data. Text
starting with `@` must be written as `{ "@…" }`. Tailwind scans `.templ`
and `.go` files under `internal/` — write full class names (no string
concatenation of partial classes). After changing classes run `make css`.

**SEO.** Call `seo.SetHead(r.Context(), seo.New("Título").WithDescription(…).WithCanonical("/x"))`
before `render.Page`. Private paths (`seo.IsPrivatePath`) are always noindex.

**CAPTCHA (Cloudflare Turnstile).** `internal/captcha` POSTs `secret`,
`response` and `remoteip` to siteverify (5s timeout). Policy: with
`TURNSTILE_SITE_KEY` + `TURNSTILE_SECRET_KEY` (or `_FILE`) set, the widget
renders, the CSP adds `https://challenges.cloudflare.com` (script-src,
connect-src, frame-src; `httpx.CSPFor(env, true)`) and tokens are verified.
`TURNSTILE_ENFORCE=1` rejects a missing, invalid or unverifiable token
(Cloudflare unreachable included); without it failures are logged and the
request passes (an unreachable Cloudflare is a warning). Not configured →
no-op, CSP byte-identical. Wired on POST `/signup`,
`/api/auth/sign-up/email`, `/forgot-password`,
`/api/auth/request-password-reset`, `/checkout` (`intent=pay` only),
`/newsletter` and `/avise-me`; never on `/login` (lockouts cover it).
Failures: pt-BR toast + redirect back (or the 422 checkout re-render),
`400 {"code":"CAPTCHA_FAILED"}` on the JSON API. A new protected form:
`@components.Turnstile(d.Captcha.SiteKey())` inside the `<form>` and
`if !d.Captcha.CheckForm(r) { … captcha.Message … }` in the handler. Tests:
`f := testutil.NewFakeTurnstile(t)` + `Options{Mutate: f.WithTurnstile(true)}`,
token `testutil.TurnstilePass`. `TURNSTILE_VERIFY_URL` overrides siteverify
in dev/test only.

**htmx 4.** Read `docs/htmx4-notes.md` (inheritance is explicit, 4xx swap,
`<hx-partial>`, new event names, never 303 an htmx request).

## sqlc

- Config: `sqlc.yaml` (pgx/v5, uuid → `google/uuid`, timestamptz →
  `time.Time` / `*time.Time`, jsonb → `json.RawMessage`, nullable columns as
  pointers, JSON tags, enum `Valid()`).
- Queries live in `db/queries/<area>.sql` (one file per area, header
  comment `-- Owner: <package>`), named with an area prefix to avoid
  collisions (`CartGetByToken`, `AdminListOrders`, …).
- Live availability: use the view `inventory_live` (`live_available`
  column) or `inventory_live_available(i.id, i.quantity, i.reserved)` —
  the Rust `LIVE_AVAILABLE_SQL`. Lock rows `ORDER BY i.id FOR UPDATE`.
- No raw SQL strings in application code: every statement lives in
  `db/queries/*.sql` (tests may keep raw SQL fixtures). Arrays are
  `sqlc.arg(ids)::uuid[]`; optional filters `(sqlc.narg(x)::type IS NULL OR …)`.
- Dynamic catalog filters: `sqlc.narg()` + `CASE` sort keys (see
  `StoreListProducts` in `store_catalog.sql`); product cards select from
  the `product_summaries` view. Fall back to hand-written pgx in the
  package only when sqlc cannot express it, with a comment saying why.
- **Nullability:** sqlc types every computed expression (scalar subquery,
  `to_char`, `CASE`, aggregate, `x::text` of a nullable column) as NOT NULL,
  so a NULL fails the scan at runtime. Cast such an expression to a
  `null_<type>` domain (`null_text`, `null_int4`, `null_int8`, `null_bool`,
  `null_uuid`, `null_timestamptz`; defined in `00001_init.sql`) to get a pointer, or
  make it non-null with `COALESCE`. `date` maps to `time.Time`.
- `make generate` (or `sqlc generate`) and commit `internal/db/sqlc/`;
  `make lint` runs `sqlc compile` + `sqlc vet`, `make drift` fails on stale
  generated code.

## Tests

- Tests live next to the code (`*_test.go`); ported Rust tests keep their
  names in Go style and usually cite the Rust test in a comment.
- Unit tests: plain `go test`. Use `t.Setenv` for env (not with `t.Parallel`).
- DB tests: `pool := testutil.NewPool(t)` (fresh migrated DB cloned from a
  template, dropped at cleanup).
- HTTP tests: `s := testutil.NewServer(t, testutil.Options{})` (dev cookies)
  or `Options{Prod: true}` (TLS, `__Host-` cookies, prod CSP/HSTS,
  APP_ORIGIN `https://lindaflor.example.com`). `c := s.Client(t)` keeps a
  cookie jar and does **not** follow redirects (assert 303 + Location);
  `c.Get`, `c.PostForm`, `c.PostJSON`, `c.HTMX()`, `c.Cookie(name)`,
  `res.JSON(t)`, `res.SetCookies()`; `s.Pool`, `s.Q`, `s.Valkey`,
  `s.Redis` (miniredis, `FastForward` for TTLs), `s.Store` (memory storage).
  Cookie assertions: `testutil.AssertHardenedCookie`,
  `AssertExpiredHardenedCookie(Dev)`, `FindSetCookie`, `RedactSetCookie`.
  `c.SignUp/SignIn` hit `/api/auth/sign-*/email`.
- Add `func TestMain(m *testing.M) { testutil.Main(m) }` in packages that
  use the database (stops Postgres and removes its data dir).
- Postgres comes from `initdb`/`postgres` on PATH (nix shell); without
  them, embedded-postgres downloads a server. Rate limits are relaxed for
  test servers unless `Options.KeepRateLimits`.

## Intentional differences from the Rust app

Behavior is otherwise the same (paths, pt-BR copy, business rules, security
defaults). These differences were chosen on purpose; don't "fix" them back.

- **Upload limits.** Rust only raised the 1 MiB global body limit for
  avatar (2 MiB) and product create/update (20 MiB), so set and banner
  image uploads failed with 413. Go also raises set create/update to 20 MiB
  (several images per form) and banner create to 3 MiB (one image).
- **Page titles.** Every admin and app-shell page sets its own `<title>`
  (`seo.SetHead`), where Rust reused a generic one.
- **Settings tabs are links** (`/settings?tab=security`, …): each tab is a
  real URL that works without JS, instead of client-only tab state.
- **htmx toasts ride the `X-Toast` response header** (read by
  `web/static/js/toast.js`), never the cookie. The toast cookie is only for
  full-page (no-JS) POST → 303 → GET. Rust's `X-Toast-Promise` /
  same-path cookie logic is gone because htmx never navigates the document.
- **No-JS add to cart**: `POST /carrinho/adicionar` without htmx answers
  303 → `/carrinho/sheet`, which renders the cart as a full page.
- **`/pedido/{id}`** answers 404 (branded page) for an unknown order or a
  denied access token, where Rust rendered a 200 "not found" page.
- **Static PIX** shows the QR code too (rendered from the BR Code), not
  just the copy-and-paste code.
- **`auth.RequireUser`** redirects to `/login?next=<path>` so login returns
  to the page the user wanted.
- **Admin estoque/avaliações** actions answer 303 + toast (no-JS
  POST/redirect/GET), with htmx enhancement on top.
- **Schema**: one consolidated goose baseline (`00001_init.sql`), not the
  29 sqlx migrations; it does not upgrade a Rust-era database.
- **Dashboard order statuses** are shown with pt-BR labels
  (`Aguardando pagamento`, `Pago`, …), not raw enum values.
- **CAPTCHA is real.** Rust only had a fail-open stub
  (`VerifyCaptchaToken` accepted any non-blank token and nothing called
  it). Go verifies Turnstile tokens with siteverify on sign-up, password
  reset, checkout, newsletter and avise-me (see Conventions → CAPTCHA).
- **Small additions**: a favicon (`web/static/img/favicon.svg`, also at
  `/favicon.ico`); the 2FA setup page shows the base32 secret under the QR
  for manual entry.

## History

The migration followed the plan in
`~/.claude/plans/federated-honking-moonbeam.md`, run as waves of parallel
agents in isolated worktrees, each owning disjoint packages and merged by
an orchestrator into `migration-golang`:

- **Wave 0 — foundation**: flake/devenv, config, consolidated schema +
  goose, sqlc, pgx pool, Valkey client, middleware stack, router with one
  `Mount` per feature package, layouts, templ component kit, Tailwind,
  vendored htmx 4/Alpine, toasts, `testutil`.
- **Wave 1 — backend**: auth (`/api/auth/*`, pages, create-admin);
  platform services (mail, S3, payments, rate limits, logging/metrics, SEO,
  OpenAPI); store domain + jobs + seed.
- **Wave 2 — UI**: storefront; checkout & account; admin.
- **Wave 3 — integration**: server wiring and graceful shutdown, Docker
  (distroless runtime + migrate image), compose, CI, docs, a parity audit
  (routes, tests, Playwright walk-through) with fix-ups, then the deletion
  of the Rust tree.
