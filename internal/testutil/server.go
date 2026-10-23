package testutil

import (
	"bytes"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/http/cookiejar"
	"net/http/httptest"
	"net/url"
	"os"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/alicebob/miniredis/v2"
	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/redis/go-redis/v9"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/mail"
	"github.com/wellington-vell/lindaflor/internal/payments"
	"github.com/wellington-vell/lindaflor/internal/storage"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/valkey"
	"github.com/wellington-vell/lindaflor/internal/web"
)

// Cookie names (mirror crates/test-support).
const (
	SessionCookie        = "session"
	ProdSessionCookie    = "__Host-session"
	ProdPending2FACookie = "__Host-lindaflor_pending_2fa"
	ProdCartCookie       = "__Host-lindaflor_cart"
	ProdToastCookie      = "__Host-toast"
	DevPending2FACookie  = "lindaflor_pending_2fa"
	DevCartCookie        = "lindaflor_cart"
	DevToastCookie       = "toast"
	// ProdAppOrigin is the origin trusted by production-mode test servers.
	ProdAppOrigin = "https://lindaflor.example.com"
	// testTotpKey is 32 zero bytes (base64), test-only.
	testTotpKey = "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA="
)

// NewValkey returns a Valkey client backed by miniredis (and the miniredis
// handle for TTL/time-travel assertions).
func NewValkey(t testing.TB) (*valkey.Client, *miniredis.Miniredis) {
	t.Helper()
	mr := miniredis.RunT(t)
	rdb := redis.NewClient(&redis.Options{Addr: mr.Addr()})
	t.Cleanup(func() { _ = rdb.Close() })
	return valkey.NewWithRedis(rdb), mr
}

var relaxOnce sync.Once

// RelaxRateLimits sets generous per-IP budgets (process-wide env, once) so
// tests that hammer endpoints do not 429. Rate-limit tests override per
// scope with their own server.
func RelaxRateLimits() {
	relaxOnce.Do(func() {
		for _, scope := range []string{
			"GLOBAL", "AUTH_SIGN_IN", "AUTH_SIGN_UP", "AUTH_PASSWORD_RESET", "AUTH_VERIFY_EMAIL",
			"AUTH_VERIFY_TOTP", "AUTH_VERIFY_BACKUP_CODE", "AUTH_STEP_UP", "AUTH_TWO_FACTOR",
			"PEDIDO_POLL", "PEDIDO_PAGE", "COUPON_APPLY", "COUPON_CART",
		} {
			_ = os.Setenv("RATE_LIMIT_"+scope+"_PER_MIN", "10000")
		}
		_ = os.Setenv("RATE_LIMIT_CHECKOUT_ORDER_PER_WINDOW", "10000")
	})
}

// Options configure NewServer.
type Options struct {
	// Prod: production config (APP_ENV=production, __Host- cookies,
	// HSTS, prod CSP, APP_ORIGIN=ProdAppOrigin) served over TLS so Secure
	// cookies round-trip.
	Prod bool
	// Mutate adjusts the config before the router is built.
	Mutate func(*config.Config)
	// Deps adjusts dependencies before the router is built.
	MutateDeps func(*app.Deps)
	// KeepRateLimits leaves rate-limit env untouched (default relaxes them).
	KeepRateLimits bool
}

// Server is a running app with its dependencies.
type Server struct {
	*httptest.Server
	Deps   *app.Deps
	Pool   *pgxpool.Pool
	Q      *sqlc.Queries
	Valkey *valkey.Client
	Redis  *miniredis.Miniredis
	Store  *storage.Memory
	Prod   bool
}

// TestConfig returns the config a test server uses.
func TestConfig(prod bool) *config.Config {
	cfg := &config.Config{
		AppEnv: "test", Host: "127.0.0.1", Port: 0,
		PGMaxConns: 5, PGAcquireTimeout: 10 * time.Second, PGIdleTimeout: time.Minute,
		Mail:                config.MailConfig{Transport: config.MailTransportLog, From: config.DefaultMailFrom},
		MailPollInterval:    time.Second,
		MailMaxAttempts:     8,
		Payments:            config.PaymentConfig{Kind: config.ProviderStaticPix},
		TokenPepper:         "test-pepper",
		ReservationSweep:    time.Minute,
		AbandonedCartOn:     true,
		AbandonedCartAfter:  180 * time.Minute,
		LogSampleRate:       1,
		LogSlowThreshold:    time.Second,
		PasswordMinLen:      8,
		PasswordMaxLen:      128,
		PasswordMinZxcvbn:   3,
		OpenAPIDocsEnabled:  false,
		SecureCookies:       false,
		DevCSP:              true,
		HSTS:                false,
		GoogleOAuthTestBase: "",
	}
	if prod {
		cfg.AppEnv = "production"
		cfg.AppOrigin = ProdAppOrigin
		cfg.MetricsToken = "test-metrics-token"
		cfg.SecureCookies, cfg.DevCSP, cfg.HSTS = true, false, true
	}
	return cfg
}

// NewServer builds the full router on a fresh database + miniredis.
// Production mode also exports APP_ENV/APP_ORIGIN/TOTP_ENCRYPTION_KEY for
// code that reads the environment directly (t.Setenv: such tests cannot
// use t.Parallel).
func NewServer(t testing.TB, opts Options) *Server {
	t.Helper()
	if !opts.KeepRateLimits {
		RelaxRateLimits()
	}
	if opts.Prod {
		if tt, ok := t.(*testing.T); ok {
			tt.Setenv("APP_ENV", "production")
			tt.Setenv("APP_ORIGIN", ProdAppOrigin)
			if os.Getenv("TOTP_ENCRYPTION_KEY") == "" {
				tt.Setenv("TOTP_ENCRYPTION_KEY", testTotpKey)
			}
		}
	}
	cfg := TestConfig(opts.Prod)
	if opts.Mutate != nil {
		opts.Mutate(cfg)
	}
	pool := NewPool(t)
	vk, mr := NewValkey(t)
	mem := storage.NewMemory()
	d := &app.Deps{
		Cfg: cfg, Pool: pool, Q: sqlc.New(pool), Valkey: vk,
		Mailer: mail.LogMailer{}, Storage: mem, Payments: payments.StaticPix{},
		Orders: store.OrderFSM{Pool: pool},
		Logger: slog.New(slog.NewTextHandler(io.Discard, nil)),
	}
	if opts.MutateDeps != nil {
		opts.MutateDeps(d)
	}
	h, err := web.NewRouter(d)
	if err != nil {
		t.Fatalf("testutil: router: %v", err)
	}
	var hs *httptest.Server
	if opts.Prod {
		hs = httptest.NewTLSServer(h)
	} else {
		hs = httptest.NewServer(h)
	}
	t.Cleanup(hs.Close)
	return &Server{Server: hs, Deps: d, Pool: pool, Q: d.Q, Valkey: vk, Redis: mr, Store: mem, Prod: opts.Prod}
}

// Client is a cookie-jar HTTP client bound to a Server. Redirects are not
// followed (tests assert 303 + Location) unless FollowRedirects is set.
type Client struct {
	t       testing.TB
	srv     *Server
	HTTP    *http.Client
	Headers http.Header
}

// Client returns a fresh client (own cookie jar) for the server.
func (s *Server) Client(t testing.TB) *Client {
	t.Helper()
	jar, err := cookiejar.New(nil)
	if err != nil {
		t.Fatal(err)
	}
	hc := s.Server.Client()
	hc.Jar = jar
	hc.CheckRedirect = func(*http.Request, []*http.Request) error { return http.ErrUseLastResponse }
	return &Client{t: t, srv: s, HTTP: hc, Headers: http.Header{}}
}

// FollowRedirects makes the client follow redirects.
func (c *Client) FollowRedirects() *Client {
	c.HTTP.CheckRedirect = nil
	return c
}

// HTMX marks subsequent requests as htmx partial requests.
func (c *Client) HTMX() *Client {
	c.Headers.Set("HX-Request", "true")
	c.Headers.Set("HX-Request-Type", "partial")
	return c
}

// Response is a fully read response.
type Response struct {
	*http.Response
	Body string
}

// JSON decodes the body (Fatal on failure).
func (r *Response) JSON(t testing.TB) map[string]any {
	t.Helper()
	var v map[string]any
	if err := json.Unmarshal([]byte(r.Body), &v); err != nil {
		t.Fatalf("not JSON (%d): %s", r.StatusCode, r.Body)
	}
	return v
}

// SetCookies are the raw Set-Cookie lines.
func (r *Response) SetCookies() []string { return r.Header.Values("Set-Cookie") }

// Do sends a request built from method/path/body with the client headers.
func (c *Client) Do(method, path string, body io.Reader, header http.Header) *Response {
	c.t.Helper()
	req, err := http.NewRequest(method, c.srv.URL+path, body) //nolint:noctx // test helper
	if err != nil {
		c.t.Fatal(err)
	}
	for k, vs := range c.Headers {
		for _, v := range vs {
			req.Header.Add(k, v)
		}
	}
	for k, vs := range header {
		for _, v := range vs {
			req.Header.Add(k, v)
		}
	}
	res, err := c.HTTP.Do(req)
	if err != nil {
		c.t.Fatalf("%s %s: %v", method, path, err)
	}
	defer func() { _ = res.Body.Close() }()
	b, err := io.ReadAll(res.Body)
	if err != nil {
		c.t.Fatal(err)
	}
	return &Response{Response: res, Body: string(b)}
}

// Get issues a GET.
func (c *Client) Get(path string) *Response { return c.Do(http.MethodGet, path, nil, nil) }

// PostForm issues an urlencoded POST (the no-JS form path).
func (c *Client) PostForm(path string, form url.Values) *Response {
	return c.Do(http.MethodPost, path, strings.NewReader(form.Encode()),
		http.Header{"Content-Type": {"application/x-www-form-urlencoded"}})
}

// PostJSON issues a JSON POST.
func (c *Client) PostJSON(path string, body any) *Response {
	c.t.Helper()
	b, err := json.Marshal(body)
	if err != nil {
		c.t.Fatal(err)
	}
	return c.Do(http.MethodPost, path, bytes.NewReader(b), http.Header{"Content-Type": {"application/json"}})
}

// Cookie returns the jar's value of name for the server URL ("" if absent).
func (c *Client) Cookie(name string) string {
	u, _ := url.Parse(c.srv.URL)
	for _, ck := range c.HTTP.Jar.Cookies(u) {
		if ck.Name == name {
			return ck.Value
		}
	}
	return ""
}

// SessionCookieName is the env-appropriate session cookie for this server.
func (c *Client) SessionCookieName() string {
	if c.srv.Prod {
		return ProdSessionCookie
	}
	return SessionCookie
}

// SignUp calls POST /api/auth/sign-up/email.
// TODO(wave1-A): works once the auth API lands.
func (c *Client) SignUp(name, email, password string) *Response {
	return c.PostJSON("/api/auth/sign-up/email", map[string]string{"name": name, "email": email, "password": password})
}

// SignIn calls POST /api/auth/sign-in/email.
// TODO(wave1-A): works once the auth API lands.
func (c *Client) SignIn(email, password string) *Response {
	return c.PostJSON("/api/auth/sign-in/email", map[string]string{"email": email, "password": password})
}

// UniqueEmail is "<prefix>-<uuid>@example.com".
func UniqueEmail(prefix string) string {
	return fmt.Sprintf("%s-%s@example.com", prefix, strings.ReplaceAll(uuid.NewString(), "-", ""))
}
