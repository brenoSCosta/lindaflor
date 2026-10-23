package ratelimit

import (
	"net/http"
	"net/http/httptest"
	"slices"
	"testing"
	"time"

	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/httpx"
)

func TestFormLimitsAndMutationOverridesStayInSync(t *testing.T) {
	paths := make([]string, 0, len(FormRateLimits))
	for _, f := range FormRateLimits {
		paths = append(paths, f.Path)
	}
	if !slices.Equal(paths, GlobalOverrideMutationPrefixes) {
		t.Fatal(paths)
	}
	for path, scope := range map[string]string{
		"/forgot-password": ScopePasswordReset, "/reset-password": ScopePasswordReset,
		"/check-email/resend": ScopeVerifyEmail, "/verify-email/resend": ScopeVerifyEmail,
		"/two-factor": ScopeTOTP, "/carrinho/cupom": ScopeCoupon,
	} {
		if !slices.ContainsFunc(FormRateLimits, func(f FormLimit) bool { return f.Path == path && f.Scope == scope }) {
			t.Errorf("%s must be limited under %s", path, scope)
		}
	}
}

func TestWindowedBudgetAllowsBurstThenBlocks(t *testing.T) {
	for range 5 {
		if !CheckWindow("test:window", "10.9.9.1", 5, 10*time.Minute) {
			t.Fatal("burst")
		}
	}
	if CheckWindow("test:window", "10.9.9.1", 5, 10*time.Minute) {
		t.Fatal("6th must block")
	}
	if !CheckWindow("test:window", "10.9.9.2", 5, 10*time.Minute) {
		t.Fatal("per IP")
	}
}

func TestCheckoutOrderBudgetIsCheckedInTheHandler(t *testing.T) {
	if slices.Contains(GlobalOverridePrefixes, "/checkout") || slices.Contains(GlobalOverrideMutationPrefixes, "/checkout") {
		t.Fatal("/checkout keeps consuming the global budget")
	}
	for _, f := range FormRateLimits {
		if f.Scope == ScopeCheckoutOrder {
			t.Fatal("checkout order is not a form limit")
		}
	}
}

func TestOnlyUnsafeMethodsAreMutations(t *testing.T) {
	for m, want := range map[string]bool{
		http.MethodGet: false, http.MethodHead: false, http.MethodOptions: false,
		http.MethodPost: true, http.MethodPut: true, http.MethodDelete: true,
	} {
		if isMutation(m) != want {
			t.Error(m)
		}
	}
}

func TestPathPrefixIsSegmentAware(t *testing.T) {
	for _, c := range []struct {
		path, prefix string
		want         bool
	}{
		{"/pedido/123", "/pedido", true},
		{"/pedido", "/pedido", true},
		{"/pedido2", "/pedido", false},
		{"/pedido-status", "/pedido", false},
		{"/api/auth/sign-in/email", "/api/auth/sign-in", true},
		{"/api/auth/sign-upstream", "/api/auth/sign-up", false},
	} {
		if pathHasPrefix(c.path, c.prefix) != c.want {
			t.Errorf("%+v", c)
		}
	}
}

func TestGCRARefillsOneCellPerInterval(t *testing.T) {
	l := &limiter{q: perMinute(2), tat: map[string]time.Time{}}
	t0 := time.Unix(1_700_000_000, 0)
	for i, want := range []bool{true, true, false} {
		if l.allow("k", t0) != want {
			t.Fatalf("request %d", i+1)
		}
	}
	if l.allow("k", t0.Add(29*time.Second)) {
		t.Fatal("no cell before 30s")
	}
	if !l.allow("k", t0.Add(30*time.Second)) || l.allow("k", t0.Add(30*time.Second)) {
		t.Fatal("one cell after 30s")
	}
}

func TestPruneDropsReplenishedBuckets(t *testing.T) {
	Reset()
	t.Cleanup(func() { clock = time.Now })
	t0 := time.Unix(1_700_000_000, 0)
	clock = func() time.Time { return t0 }
	Check("test:prune", "a", 60)
	CheckWindow("test:prune-window", "b", 5, 10*time.Minute)
	l := getLimiter("test:prune", perMinute(60))
	if len(l.tat) != 1 {
		t.Fatal(len(l.tat))
	}
	clock = func() time.Time { return t0.Add(time.Second) }
	Prune()
	if len(l.tat) != 0 {
		t.Fatal("replenished bucket must be dropped")
	}
	if w := getLimiter("test:prune-window", perWindow(5, 10*time.Minute)); len(w.tat) != 1 {
		t.Fatal("window bucket still draining")
	}
}

func get(t *testing.T, h http.Handler, method, path, xff string) *httptest.ResponseRecorder {
	t.Helper()
	req := httptest.NewRequest(method, path, nil)
	req.RemoteAddr = "192.0.2.10:5555"
	if xff != "" {
		req.Header.Set("X-Forwarded-For", xff)
	}
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)
	return rec
}

func router(global func(http.Handler) http.Handler, scoped func(http.Handler) http.Handler) http.Handler {
	r := chi.NewRouter()
	r.Use(httpx.ClientIPMiddleware(httpx.TrustedProxies{}), global)
	ok := func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(http.StatusOK) }
	r.With(scoped).Get("/api/health", ok)
	r.With(scoped).Post("/two-factor", ok)
	r.Get("/two-factor", ok)
	return r
}

// A scoped limiter replaces the global one on its path instead of stacking.
func TestGlobalDefaultWithScopedOverride(t *testing.T) {
	Reset()
	t.Setenv("RATE_LIMIT_GLOBAL_PER_MIN", "2")
	t.Setenv("RATE_LIMIT_TEST_RATE_LIMIT_PER_MIN", "100")
	global := middleware(ScopeGlobal, DefaultGlobalPerMin, false, []string{"/api/health"}, nil)
	h := router(global, Limit("test:rate-limit", 100))
	for range 3 {
		if rec := get(t, h, http.MethodGet, "/api/health", ""); rec.Code != http.StatusOK {
			t.Fatal("scoped path owns its budget", rec.Code)
		}
	}
	// Unlisted path: global budget (2); 404s count too.
	for i, want := range []int{http.StatusNotFound, http.StatusNotFound, http.StatusTooManyRequests} {
		rec := get(t, h, http.MethodGet, "/no-such-route-for-rate-limit", "")
		if rec.Code != want {
			t.Fatalf("request %d: %d", i+1, rec.Code)
		}
		if want == http.StatusTooManyRequests && rec.Header().Get("Retry-After") != "60" {
			t.Fatal("Retry-After")
		}
	}
}

// With no trusted proxies a spoofed X-Forwarded-For shares the peer bucket.
func TestSpoofedXForwardedForSharesBucket(t *testing.T) {
	Reset()
	t.Setenv("RATE_LIMIT_TEST_XFORWARDED_SPOOF_PER_MIN", "1")
	pass := func(next http.Handler) http.Handler { return next }
	h := router(pass, Limit("test:xforwarded-spoof", 1))
	if rec := get(t, h, http.MethodGet, "/api/health", "203.0.113.9"); rec.Code != http.StatusOK {
		t.Fatal(rec.Code)
	}
	if rec := get(t, h, http.MethodGet, "/api/health", "198.51.100.23"); rec.Code != http.StatusTooManyRequests {
		t.Fatal(rec.Code)
	}
}

// Form limits only count mutations; GETs of the page use the global budget.
func TestMutationsOnlyAndGlobalMutationSkips(t *testing.T) {
	Reset()
	t.Setenv("RATE_LIMIT_GLOBAL_PER_MIN", "3")
	t.Setenv("RATE_LIMIT_AUTH_VERIFY_TOTP_PER_MIN", "1")
	h := router(Global(), MutationsOnly(ScopeTOTP, DefaultTOTPPerMin))
	if rec := get(t, h, http.MethodPost, "/two-factor", ""); rec.Code != http.StatusOK {
		t.Fatal(rec.Code)
	}
	if rec := get(t, h, http.MethodPost, "/two-factor", ""); rec.Code != http.StatusTooManyRequests {
		t.Fatal("scoped budget of 1", rec.Code)
	}
	// The POSTs did not consume the global budget (3): three GETs pass.
	for range 3 {
		if rec := get(t, h, http.MethodGet, "/two-factor", ""); rec.Code != http.StatusOK {
			t.Fatal("GET uses the global budget", rec.Code)
		}
	}
	if rec := get(t, h, http.MethodGet, "/two-factor", ""); rec.Code != http.StatusTooManyRequests {
		t.Fatal(rec.Code)
	}
	// Static assets and robots.txt skip the global budget.
	if rec := get(t, h, http.MethodGet, "/static/css/app.css", ""); rec.Code == http.StatusTooManyRequests {
		t.Fatal("static assets are exempt")
	}
}

func TestLimitExceptSkipsTighterSubPaths(t *testing.T) {
	Reset()
	t.Setenv("RATE_LIMIT_PEDIDO_PAGE_PER_MIN", "1")
	r := chi.NewRouter()
	r.Use(httpx.ClientIPMiddleware(httpx.TrustedProxies{}))
	ok := func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(http.StatusOK) }
	r.Route("/pedido", func(r chi.Router) {
		r.Use(LimitExcept(ScopePedidoPage, DefaultPedidoPagePerMin, "/pedido/status-proc"))
		r.Get("/{id}", ok)
		r.Get("/status-proc", ok)
	})
	if get(t, r, http.MethodGet, "/pedido/1", "").Code != http.StatusOK || get(t, r, http.MethodGet, "/pedido/1", "").Code != http.StatusTooManyRequests {
		t.Fatal("page budget")
	}
	if get(t, r, http.MethodGet, "/pedido/status-proc", "").Code != http.StatusOK {
		t.Fatal("status-proc is owned by its own scope")
	}
}
