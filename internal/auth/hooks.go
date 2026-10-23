package auth

import (
	"net/http"
	"sync"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
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

type registeredHook struct {
	id int
	fn LoginHook
}

var (
	hooksMu    sync.RWMutex
	loginHooks []registeredHook
	nextHookID int
)

// RegisterLoginHook adds h to the hooks run after every regular login and
// returns a function that removes it again (tests). Call it once, from
// an init or Mount.
func RegisterLoginHook(h LoginHook) (unregister func()) {
	hooksMu.Lock()
	defer hooksMu.Unlock()
	id := nextHookID
	nextHookID++
	loginHooks = append(loginHooks, registeredHook{id: id, fn: h})
	return func() {
		hooksMu.Lock()
		defer hooksMu.Unlock()
		for i, rh := range loginHooks {
			if rh.id == id {
				loginHooks = append(loginHooks[:i:i], loginHooks[i+1:]...)
				return
			}
		}
	}
}

// LoginHookCount reports how many hooks are registered (tests).
func LoginHookCount() int {
	hooksMu.RLock()
	defer hooksMu.RUnlock()
	return len(loginHooks)
}

func runLoginHooks(w http.ResponseWriter, r *http.Request, pool *pgxpool.Pool, userID uuid.UUID) error {
	hooksMu.RLock()
	hooks := append([]registeredHook(nil), loginHooks...)
	hooksMu.RUnlock()
	for _, h := range hooks {
		if err := h.fn(w, r, pool, userID); err != nil {
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
// a bare 429). Tests may swap it.
var RateAllow = func(r *http.Request, scope string, defaultPerMin uint32) bool {
	return ratelimit.Check(scope, httpx.ClientIP(r), defaultPerMin)
}
