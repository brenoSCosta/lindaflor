# Go migration — guide for implementers

The Rust/Topcoat app (`src/`, `crates/`, `migrations/`, `assets/`,
`styles.css`) is the **source of truth for behavior** until Wave 3 deletes
it. The Go app lives alongside it: `go.mod` at the repo root, module
`github.com/wellington-vell/lindaflor`. Plan:
`~/.claude/plans/federated-honking-moonbeam.md`.

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

**Gate before you finish:**
`nix develop -c bash -c 'make generate && go build ./... && go vet ./... && golangci-lint run ./... && go test ./...'`

Generated code is **committed**: `*_templ.go`, `internal/db/sqlc/`,
`web/static/css/app.css`. Run `make generate` after editing `.templ`,
`db/queries/*.sql`, migrations or classes, and commit the output.

## Layout

```
cmd/server         main: config → pool (migrate/verify) → valkey → router → http.Server (30s graceful) → jobs.Start
cmd/seed           TODO(wave1-C)
cmd/create-admin   TODO(wave1-A)
db/migrations      goose SQL (00001_init.sql = consolidated Rust 0001..0030); embedded by package db (db/embed.go)
db/queries         sqlc query files, ONE FILE PER AREA (see below)
internal/
  config           every env var (KEY or KEY_FILE), Load() validates production
  db               pgxpool Open/Migrate/RequireSchemaCurrent, WithTx, EscapeLike; sqlc/ generated
  valkey           go-redis client + in-process fallback; lockouts, TOTP reuse, pending-2FA binding, enroll flash
  httpx            middleware (request id, client IP, recover, security headers, body limit, origin), redirects, htmx helpers
  httpx/toast      flash toasts (cookie for full pages, X-Toast header for htmx)
  auth mail audit payments storage ratelimit seo metrics logging jobs   (contracts; see owners)
  app              Deps container + ShellLoader type
  web              router.go (middleware stack + Mount calls — do not edit in waves 1/2)
  web/render       Page / Status / NotFound / Fragment
  web/layout       Root, StorefrontShell (+Header/Footer/AnnouncementBar/CartCount), AppShell, NotFound
  web/components   templ UI kit (port of src/components/*), lucide icons
  web/format       Price (R$ 1.234,56), DateTimeBR (UTC-3), DateBR, DateISO, TodayBR
  web/{authpages,authapi,storefront,checkout,account,admin,platform}   one Mount(r, d) each
  testutil         Postgres per test binary + template DB per test, miniredis, test server + cookie-jar client
web/static         embedded assets (css/app.css generated, js/, vendor/) + URL()/Handler() cache busting
web/styles         input.css (Tailwind v4 source)
```

## Ownership (no two agents edit the same file)

| Agent | Owns |
|---|---|
| **wave1-A Auth** | `internal/auth/**`, `internal/web/authapi/**`, `internal/web/authpages/**`, `cmd/create-admin/**`, `db/queries/auth*.sql` (+ `users.sql`) |
| **wave1-B Platform** | `internal/mail/**`, `internal/storage/**`, `internal/payments/**`, `internal/ratelimit/**`, `internal/logging/**`, `internal/metrics/**`, `internal/seo/**`, `internal/web/platform/**` (keep `/api/health` + `/api/readyz`), `db/queries/{mail,payments,seo,platform}.sql` |
| **wave1-C Store domain** | `internal/store/**` (new), `internal/jobs/**`, `cmd/seed/**`, `db/queries/{store_*,maintenance,seed}.sql`; exposes domain functions (e.g. store settings, nav sets, cart count) that D calls from `ShellLoader` |
| **wave2-D Storefront** | `internal/web/storefront/**` (incl. `ShellLoader`), page templ files there |
| **wave2-E Checkout & account** | `internal/web/checkout/**`, `internal/web/account/**` |
| **wave2-F Admin** | `internal/web/admin/**` |
| **wave0 / orchestrator** | `internal/web/router.go`, `internal/app`, `internal/httpx/**`, `internal/web/{components,layout,render,format}`, `internal/config`, `internal/db/db.go`, `db/migrations/00001_init.sql`, `testutil`, nix/Makefile/CI |

Need a change in a wave-0 file (new component variant, config field,
middleware)? Prefer adding a **new file** in your own package. If a shared
file must change, keep the edit minimal and mention it in your report — the
orchestrator merges. New migrations: add `db/migrations/000NN_<area>_<what>.sql`
(pick the next free number; renumber on merge if two collide), forward-only.

## Contracts (Wave 0 signatures)

```go
// internal/app
type ShellLoader func(r *http.Request) (layout.ShellData, error)
type Deps struct {
	Cfg *config.Config; Pool *pgxpool.Pool; Q *sqlc.Queries; Valkey *valkey.Client
	Mailer mail.Mailer; Storage storage.Store; Payments payments.Provider
	Logger *slog.Logger; Shell ShellLoader
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
func Middleware(MiddlewareDeps) func(http.Handler) http.Handler // placeholder: anonymous
func RequireUser(http.Handler) http.Handler; func RequireAdmin(http.Handler) http.Handler
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
func Limit(scope string, defaultPerMin uint32) func(http.Handler) http.Handler   // no-op until wave1-B
func MutationsOnly(scope string, defaultPerMin uint32) func(http.Handler) http.Handler
func Global() func(http.Handler) http.Handler
// Scope* and Default*PerMin constants; env override config.RateLimitFor(scope, def)

// internal/seo
type Head struct{ Title, Description, Canonical string; NoIndex bool; OpenGraph *OpenGraph; JSONLD []any }
func New(title) Head; SiteDefault() Head; (Head).WithDescription/WithCanonical/WithNoIndex/WithOpenGraph/WithJSONLD
func SetHead(ctx, Head)        // call in the handler BEFORE render.Page
func HeadFrom(ctx) (Head, bool); func Resolve(...) Resolved; IsPrivatePath; SiteOrigin; AbsoluteURL; JSONLDScriptBody; DecimalPrice; Summarize

// internal/jobs
func Start(ctx, d *app.Deps) *sync.WaitGroup               // TODO loops

// internal/valkey (complete port of src/valkey.rs)
func Connect(ctx, url) (*Client, error); NewWithRedis(*redis.Client); NewFallbackOnly()
(c).RecordAuthFail/AuthFailCount/IsAuthLockedOut/ClearAuthFails, RecordAuthEmailFail/…EmailLockedOut/ClearAuthEmailFails,
TotpReserve, RecordTotpFail, IsTotpLockedOut, PendingAttempt(s), PendingClear, PendingBindEager, PendingCheckBinding,
TotpEnrollmentFlashPut/Take; IPUAHash, PendingTokenHash, TotpReuseKey; VerifyCaptchaToken(required, token)

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
static.URL("css/app.css") → "/static/css/app.css?v=<hash>"
```

## Conventions

**Errors & logging.** Return `error`s wrapped with `%w`; handle at the
handler. Log with `slog` (`slog.ErrorContext(r.Context(), "msg", "error", err, …)`);
never log secrets, tokens, cookie values or full emails. Unexpected errors
→ 500 (plain text in handlers is fine; the logging middleware records it).
User-facing messages are pt-BR and copied verbatim from the Rust code.

**Handlers.** `func(w http.ResponseWriter, r *http.Request)` closures that
capture `d *app.Deps`, registered in your package's `Mount`. Parse forms
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

**htmx 4.** Read `docs/htmx4-notes.md` (inheritance is explicit, 4xx swap,
`<hx-partial>`, new event names, never 303 an htmx request).

## sqlc

- Config: `sqlc.yaml` (pgx/v5, uuid → `google/uuid`, timestamptz →
  `time.Time` / `*time.Time`, jsonb → `json.RawMessage`, nullable columns as
  pointers, JSON tags, enum `Valid()`).
- Add queries to **your own file** `db/queries/<area>.sql` (create it), with
  a header comment `-- Owner: <agent>`. Never edit another area's file;
  name queries with an area-specific prefix to avoid collisions
  (`CartGetByToken`, `AdminListOrders`, …).
- Live availability: use the view `inventory_live` (`live_available`
  column) or `inventory_live_available(i.id, i.quantity, i.reserved)` —
  the Rust `LIVE_AVAILABLE_SQL`. Lock rows `ORDER BY i.id FOR UPDATE` as in Rust.
- No raw SQL strings in application code: every statement lives in
  `db/queries/*.sql` (tests may keep raw SQL fixtures). Arrays are
  `sqlc.arg(ids)::uuid[]`; optional filters `(sqlc.narg(x)::type IS NULL OR …)`.
- Dynamic catalog filters: `sqlc.narg()` + `CASE` sort keys (see
  `StoreListProducts` in `store_catalog.sql`); product cards select from
  the `product_summaries` view. Fall back to hand-written pgx in your
  package only when sqlc cannot express it, with a comment saying why.
- **Nullability:** sqlc types every computed expression (scalar subquery,
  `to_char`, `CASE`, aggregate, `x::text` of a nullable column) as NOT NULL,
  so a NULL fails the scan at runtime. Cast such an expression to a
  `null_<type>` domain (`null_text`, `null_int4`, `null_int8`, `null_bool`,
  `null_uuid`, `null_timestamptz`; migration 00002) to get a pointer, or
  make it non-null with `COALESCE`. `date` maps to `time.Time`.
- `make generate` (or `sqlc generate`) and commit `internal/db/sqlc/`;
  `make lint` runs `sqlc compile` + `sqlc vet`, `make drift` fails on stale
  generated code.

## Tests

- Port the Rust tests next to the Go code (`*_test.go`, same names in Go style).
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
  `c.SignUp/SignIn` hit `/api/auth/sign-*/email` (work once wave1-A lands).
- Add `func TestMain(m *testing.M) { testutil.Main(m) }` in packages that
  use the database (stops Postgres and removes its data dir).
- Postgres comes from `initdb`/`postgres` on PATH (nix shell); without
  them, embedded-postgres downloads a server. Rate limits are relaxed for
  test servers unless `Options.KeepRateLimits`.

## Not done in Wave 0 (by design)

Auth, mail worker/templates, S3, Mercado Pago, rate limiting, metrics,
sitemap/robots/OpenAPI, store domain, jobs, seed/create-admin, every page
except the placeholder home and the 404. Search for `TODO(wave` to find
the hooks.
