// Package ratelimit: in-process per-IP request budgets (coarse DoS guard,
// not a security boundary — lockouts live in internal/valkey). Port of
// src/rate_limit.rs.
//
// Buckets are GCRA (the algorithm behind the Rust governor crate): a quota
// of n per period allows a burst of n and refills one cell every period/n.
// Each request consumes exactly one budget: the global limiter skips the
// paths owned by scoped limiters (GlobalOverridePrefixes), so a scoped
// budget replaces the global one instead of stacking on it.
package ratelimit

import (
	"net/http"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/httpx"
)

// Scopes (env override: RATE_LIMIT_<SCOPE>_PER_MIN, see config.RateLimitFor).
const (
	ScopeSignIn        = "auth:sign-in"
	ScopeSignUp        = "auth:sign-up"
	ScopePasswordReset = "auth:password-reset"
	ScopeVerifyEmail   = "auth:verify-email"
	ScopeTOTP          = "auth:verify-totp"
	ScopeBackupCode    = "auth:verify-backup-code"
	ScopeTwoFactor     = "auth:two-factor"
	ScopeStepUp        = "auth:step-up" // re-authentication on credential changes
	ScopePedidoPoll    = "pedido:poll"
	ScopePedidoPage    = "pedido:page"
	ScopeCoupon        = "coupon:apply"
	ScopeCouponCart    = "coupon:cart"
	ScopeMPWebhook     = "webhook:mercadopago"
	// ScopeCheckoutOrder: order creation (POST /checkout intent=pay),
	// checked in the handler with CheckWindow so coupon/CEP previews on the
	// same path are not counted.
	ScopeCheckoutOrder = "checkout:order"
	ScopeGlobal        = "global"
)

// Default per-minute budgets.
const (
	DefaultSignInPerMin        = 5
	DefaultSignUpPerMin        = 5
	DefaultPasswordResetPerMin = 5
	DefaultVerifyEmailPerMin   = 5
	DefaultTOTPPerMin          = 3
	DefaultBackupCodePerMin    = 3
	DefaultTwoFactorPerMin     = 10
	DefaultStepUpPerMin        = 10
	DefaultPedidoPollPerMin    = 10
	DefaultPedidoPagePerMin    = 30
	DefaultCouponPerMin        = 10 // per IP (route) and per cart (handler)
	DefaultCouponCartPerMin    = 10
	DefaultMPWebhookPerMin     = 120 // MP notifies from few IPs, in bursts
	DefaultGlobalPerMin        = 600
	// DefaultCheckoutOrdersPerWindow orders per IP per CheckoutOrderWindow
	// (env RATE_LIMIT_CHECKOUT_ORDER_PER_WINDOW).
	DefaultCheckoutOrdersPerWindow = 5
)

// CheckoutOrderWindow is the window of ScopeCheckoutOrder.
const CheckoutOrderWindow = 10 * time.Minute

// RetryAfterSecs is the Retry-After of every 429.
const RetryAfterSecs = 60

// GlobalOverridePrefixes are skipped by the global limiter: paths owned by
// scoped limiters (true override), /metrics scrapes, embedded static assets
// (immutable, content-hashed: CGNAT/mobile networks sharing few IPs must
// not burn the global budget on cacheable files) and /robots.txt.
var GlobalOverridePrefixes = []string{
	"/api/auth/sign-in",
	"/api/auth/sign-up",
	"/api/auth/request-password-reset",
	"/api/auth/reset-password",
	"/api/auth/send-verification-email",
	"/api/auth/verify-email",
	"/api/auth/two-factor/verify-totp",
	"/api/auth/two-factor/verify-backup-code",
	"/pedido/status-proc",
	"/pedido",
	"/api/webhooks/mercadopago",
	"/metrics",
	"/static",
	"/robots.txt",
}

// FormLimit is a MutationsOnly budget on an HTML form path.
type FormLimit struct {
	Path          string
	Scope         string
	DefaultPerMin uint32
}

// FormRateLimits are the HTML-form twins of the /api/auth/* endpoints plus
// coupon apply, sharing the API scopes (one bucket per abuse surface).
// Mount each as r.With(ratelimit.MutationsOnly(scope, def)) on its path;
// every path is in GlobalOverrideMutationPrefixes.
var FormRateLimits = []FormLimit{
	{"/forgot-password", ScopePasswordReset, DefaultPasswordResetPerMin},
	{"/reset-password", ScopePasswordReset, DefaultPasswordResetPerMin},
	{"/check-email/resend", ScopeVerifyEmail, DefaultVerifyEmailPerMin},
	{"/verify-email/resend", ScopeVerifyEmail, DefaultVerifyEmailPerMin},
	{"/two-factor", ScopeTOTP, DefaultTOTPPerMin},
	{"/login/google", ScopeSignIn, DefaultSignInPerMin},
	{"/carrinho/cupom", ScopeCoupon, DefaultCouponPerMin},
}

// GlobalOverrideMutationPrefixes: like GlobalOverridePrefixes but only for
// mutating requests — their GET pages keep consuming the global budget
// while the form POSTs consume exactly one (strict) scoped budget.
var GlobalOverrideMutationPrefixes = func() []string {
	out := make([]string, len(FormRateLimits))
	for i, f := range FormRateLimits {
		out[i] = f.Path
	}
	return out
}()

// ---------------------------------------------------------------------------
// GCRA buckets
// ---------------------------------------------------------------------------

type quota struct {
	interval time.Duration // one cell back every interval
	burst    int64
}

func perMinute(n uint32) quota {
	n = max(n, 1)
	return quota{interval: time.Minute / time.Duration(n), burst: int64(n)}
}

func perWindow(maxN uint32, window time.Duration) quota {
	maxN = max(maxN, 1)
	return quota{interval: max(window/time.Duration(maxN), time.Millisecond), burst: int64(maxN)}
}

type limiterKey struct {
	scope string
	q     quota
}

type limiter struct {
	q   quota
	mu  sync.Mutex
	tat map[string]time.Time // theoretical arrival time per key
}

var (
	registryMu sync.Mutex
	registry   = map[limiterKey]*limiter{}
	clock      = time.Now
)

func getLimiter(scope string, q quota) *limiter {
	registryMu.Lock()
	defer registryMu.Unlock()
	k := limiterKey{scope, q}
	l, ok := registry[k]
	if !ok {
		l = &limiter{q: q, tat: map[string]time.Time{}}
		registry[k] = l
	}
	return l
}

func (l *limiter) allow(key string, now time.Time) bool {
	l.mu.Lock()
	defer l.mu.Unlock()
	tat := l.tat[key]
	if tat.Before(now) {
		tat = now
	}
	next := tat.Add(l.q.interval)
	if next.Sub(now) > l.q.interval*time.Duration(l.q.burst) {
		return false
	}
	l.tat[key] = next
	return true
}

func check(scope, key string, q quota) bool {
	return getLimiter(scope, q).allow(scope+":"+key, clock())
}

// Check consumes one cell of the (scope, key) per-minute bucket (key is
// usually the client IP; a cart id for ScopeCouponCart) and reports
// whether the request may proceed. The budget honours the env override.
func Check(scope, key string, defaultPerMin uint32) bool {
	return check(scope, key, perMinute(config.RateLimitFor(scope, defaultPerMin)))
}

// CheckWindow is Check for budgets coarser than a minute: maxN requests per
// window (burst maxN, one cell back every window/maxN), e.g.
// ScopeCheckoutOrder. The env override is RATE_LIMIT_<SCOPE>_PER_WINDOW.
func CheckWindow(scope, key string, defaultMax uint32, window time.Duration) bool {
	return check(scope, key, perWindow(config.WindowRateLimitFor(scope, defaultMax), window))
}

// Prune drops fully replenished buckets so every IP ever seen does not
// keep an entry for the process lifetime. Call periodically (jobs).
func Prune() {
	now := clock()
	registryMu.Lock()
	limiters := make([]*limiter, 0, len(registry))
	for _, l := range registry {
		limiters = append(limiters, l)
	}
	registryMu.Unlock()
	for _, l := range limiters {
		l.mu.Lock()
		for k, tat := range l.tat {
			if !tat.After(now) {
				delete(l.tat, k)
			}
		}
		l.mu.Unlock()
	}
}

// Reset clears every bucket (tests).
func Reset() {
	registryMu.Lock()
	defer registryMu.Unlock()
	registry = map[limiterKey]*limiter{}
}

// ---------------------------------------------------------------------------
// Middleware
// ---------------------------------------------------------------------------

// TooManyRequests writes the generic 429 (no oracle detail).
func TooManyRequests(w http.ResponseWriter) {
	w.Header().Set("Retry-After", strconv.Itoa(RetryAfterSecs))
	http.Error(w, "too many requests", http.StatusTooManyRequests)
}

func isMutation(method string) bool {
	switch method {
	case http.MethodGet, http.MethodHead, http.MethodOptions:
		return false
	}
	return true
}

// pathHasPrefix is segment-aware: path equals prefix or continues with "/".
func pathHasPrefix(path, prefix string) bool {
	rest, ok := strings.CutPrefix(path, prefix)
	return ok && (rest == "" || rest[0] == '/')
}

func anyPrefix(path string, prefixes []string) bool {
	for _, p := range prefixes {
		if pathHasPrefix(path, p) {
			return true
		}
	}
	return false
}

func middleware(scope string, defaultPerMin uint32, mutationsOnly bool, skips, mutationSkips []string) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			mutation := isMutation(r.Method)
			if (mutationsOnly && !mutation) || anyPrefix(r.URL.Path, skips) || (mutation && anyPrefix(r.URL.Path, mutationSkips)) {
				next.ServeHTTP(w, r)
				return
			}
			if !Check(scope, httpx.ClientIP(r), defaultPerMin) {
				TooManyRequests(w)
				return
			}
			next.ServeHTTP(w, r)
		})
	}
}

// Limit applies the scope's per-IP budget to a route or group:
//
//	r.With(ratelimit.Limit(ratelimit.ScopeSignIn, ratelimit.DefaultSignInPerMin)).Post(...)
//
// The path must be listed in GlobalOverridePrefixes (or the request
// consumes the global budget too).
func Limit(scope string, defaultPerMin uint32) func(http.Handler) http.Handler {
	return middleware(scope, defaultPerMin, false, nil, nil)
}

// LimitExcept is Limit skipping sub-paths owned by a tighter scope (e.g.
// the /pedido page budget skips /pedido/status-proc).
func LimitExcept(scope string, defaultPerMin uint32, skips ...string) func(http.Handler) http.Handler {
	return middleware(scope, defaultPerMin, false, skips, nil)
}

// MutationsOnly is Limit for non-GET/HEAD/OPTIONS requests only (HTML
// form twins of the auth API, coupon apply), so a strict budget on
// POST /two-factor does not throttle rendering GET /two-factor.
func MutationsOnly(scope string, defaultPerMin uint32) func(http.Handler) http.Handler {
	return middleware(scope, defaultPerMin, true, nil, nil)
}

// Global is the router-wide default budget (installed by router.go). It
// wraps every request, including 404/405, except the override prefixes.
func Global() func(http.Handler) http.Handler {
	return middleware(ScopeGlobal, DefaultGlobalPerMin, false, GlobalOverridePrefixes, GlobalOverrideMutationPrefixes)
}
