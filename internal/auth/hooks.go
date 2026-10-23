package auth

import (
	"net/http"
	"sync"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"
)

// LoginHook runs right after a regular (non-impersonated) session starts on
// the paths where Rust called start_session_for_user: HTML sign-up/sign-in,
// HTML two-factor completion and stop-impersonating (back to the admin).
// The JSON API sign-in/up and the Google callback do not run hooks (same
// as Rust). A hook error fails the request (500).
//
// The store package registers the guest-cart merge here (read the guest
// cart cookie, merge into the user's cart, write the user cart cookie):
//
//	auth.RegisterLoginHook(func(w http.ResponseWriter, r *http.Request, pool *pgxpool.Pool, userID uuid.UUID) error {
//		return cart.MergeGuestCartOnLogin(w, r, pool, userID)
//	})
type LoginHook func(w http.ResponseWriter, r *http.Request, pool *pgxpool.Pool, userID uuid.UUID) error

var (
	hooksMu    sync.RWMutex
	loginHooks []LoginHook
)

// RegisterLoginHook adds h to the hooks run after every regular login.
// Call it from an init or Mount (idempotency is the caller's job).
func RegisterLoginHook(h LoginHook) {
	hooksMu.Lock()
	defer hooksMu.Unlock()
	loginHooks = append(loginHooks, h)
}

// LoginHookCount reports how many hooks are registered (tests).
func LoginHookCount() int {
	hooksMu.RLock()
	defer hooksMu.RUnlock()
	return len(loginHooks)
}

func runLoginHooks(w http.ResponseWriter, r *http.Request, pool *pgxpool.Pool, userID uuid.UUID) error {
	hooksMu.RLock()
	hooks := append([]LoginHook(nil), loginHooks...)
	hooksMu.RUnlock()
	for _, h := range hooks {
		if err := h(w, r, pool, userID); err != nil {
			return err
		}
	}
	return nil
}

// RateAllow consumes one unit of the per-IP budget for scope (env
// override RATE_LIMIT_<SCOPE>_PER_MIN, default defaultPerMin) and reports
// whether the request is within it. It backs the in-handler checks Rust
// did with check_ip_rate: step-up re-authentication (auth:step-up) and
// the HTML /login and /signup forms (which answer with a toast instead of
// a bare 429). Wave 0 has no limiter, so the default allows everything;
// the platform package (internal/ratelimit) replaces it when it lands:
//
//	auth.RateAllow = func(r *http.Request, scope string, def uint32) bool {
//		return ratelimit.Allow(scope, httpx.ClientIP(r), config.RateLimitFor(scope, def))
//	}
var RateAllow = func(_ *http.Request, _ string, _ uint32) bool { return true }
