// Package ratelimit: in-process per-IP request budgets (coarse DoS guard,
// not a security boundary — lockouts live in internal/valkey).
//
// Wave 0: scopes, defaults and the middleware contract (no-op).
// TODO(wave1-B): token buckets (x/time/rate keyed by httpx.ClientIP),
// global limiter with GLOBAL_OVERRIDE_PREFIXES skips, mutations-only form
// limits, windowed budgets, pruning, 429 response with Retry-After.
package ratelimit

import (
	"net/http"

	"github.com/wellington-vell/lindaflor/internal/config"
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
	ScopeStepUp        = "auth:step-up"
	ScopePedidoPoll    = "pedido:poll"
	ScopePedidoPage    = "pedido:page"
	ScopeCoupon        = "coupon:apply"
	ScopeCouponCart    = "coupon:cart"
	ScopeMPWebhook     = "webhook:mercadopago"
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
	DefaultCouponPerMin        = 10
	DefaultCouponCartPerMin    = 10
	DefaultMPWebhookPerMin     = 120
	DefaultGlobalPerMin        = 600
	// DefaultCheckoutOrdersPerWindow orders per IP per 10 minutes.
	DefaultCheckoutOrdersPerWindow = 5
)

// Limit applies the scope's per-IP budget to a route or group:
//
//	r.With(ratelimit.Limit(ratelimit.ScopeSignIn, ratelimit.DefaultSignInPerMin)).Post(...)
//
// Wave 0 no-op; the signature is the contract.
func Limit(scope string, defaultPerMin uint32) func(http.Handler) http.Handler {
	_ = config.RateLimitFor(scope, defaultPerMin)
	return func(next http.Handler) http.Handler { return next } // TODO(wave1-B)
}

// MutationsOnly is Limit for non-GET/HEAD/OPTIONS requests only (HTML
// form twins of the auth API, coupon apply).
func MutationsOnly(scope string, defaultPerMin uint32) func(http.Handler) http.Handler {
	return Limit(scope, defaultPerMin) // TODO(wave1-B)
}

// Global is the router-wide default budget (installed by router.go).
func Global() func(http.Handler) http.Handler {
	return Limit(ScopeGlobal, DefaultGlobalPerMin) // TODO(wave1-B)
}
