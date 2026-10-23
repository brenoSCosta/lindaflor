// Package auth: users, sessions, passwords, TOTP, one-time tokens, Google
// OAuth and the /api/auth JSON API.
//
// Wave 0 defines only the contract every other package codes against:
// the User/Session types, CurrentUser/CurrentSession readers and the
// Middleware constructor wired in internal/web/router.go.
// TODO(wave1-A): implement session loading (sha256 token lookup, 30d
// sliding expiry, impersonation 1h, banned users), cookies (`session` /
// `__Host-session`), argon2id, TOTP, tokens, Google PKCE, /api/auth/*.
package auth

import (
	"context"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/valkey"
)

// Session cookie names (dev drops Secure and the __Host- prefix).
const (
	SessionCookieDev    = "session"
	SessionCookieSecure = "__Host-session"
)

// SessionCookieName is the env-appropriate session cookie name.
func SessionCookieName(secure bool) string {
	if secure {
		return SessionCookieSecure
	}
	return SessionCookieDev
}

// User is the authenticated account (mirrors src/auth/user.rs).
type User struct {
	ID               uuid.UUID
	Name             string
	Email            string
	EmailVerified    bool
	Image            *string
	TwoFactorEnabled bool
	Role             *string
	Banned           bool
	BanReason        *string
	BanExpires       *time.Time
}

// IsAdmin reports role == "admin" (case-insensitive).
func (u *User) IsAdmin() bool { return u != nil && IsAdminRole(u.Role) }

// IsAdminRole is the role check used everywhere admin access is decided.
func IsAdminRole(role *string) bool { return role != nil && strings.EqualFold(*role, "admin") }

// Session is the active session behind the request.
type Session struct {
	ID        uuid.UUID
	UserID    uuid.UUID
	ExpiresAt time.Time
	// ImpersonatedBy is the admin acting as this user, if any.
	ImpersonatedBy *uuid.UUID
	// TokenHash is sha256(raw cookie token), the at-rest key.
	TokenHash string
}

type ctxKey int

const (
	userKey ctxKey = iota
	sessionKey
)

// WithUser stores the authenticated user/session in ctx. Used by the
// middleware and by tests.
func WithUser(ctx context.Context, u *User, s *Session) context.Context {
	ctx = context.WithValue(ctx, userKey, u)
	return context.WithValue(ctx, sessionKey, s)
}

// CurrentUser returns the authenticated user, if any.
func CurrentUser(ctx context.Context) (*User, bool) {
	u, ok := ctx.Value(userKey).(*User)
	return u, ok && u != nil
}

// CurrentSession returns the active session, if any.
func CurrentSession(ctx context.Context) (*Session, bool) {
	s, ok := ctx.Value(sessionKey).(*Session)
	return s, ok && s != nil
}

// IsImpersonating reports an admin acting as the current user.
func IsImpersonating(ctx context.Context) bool {
	s, ok := CurrentSession(ctx)
	return ok && s.ImpersonatedBy != nil
}

// MiddlewareDeps are the dependencies of the session middleware.
// TODO(wave1-A): add fields as needed (keep construction in router.go
// unchanged: it passes the whole struct).
type MiddlewareDeps struct {
	Pool   *pgxpool.Pool
	Cfg    *config.Config
	Valkey *valkey.Client
}

// Middleware loads the session from the cookie and stores the user in the
// request context. Wave 0 placeholder: loads nothing (everyone is anonymous).
func Middleware(_ MiddlewareDeps) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler { return next } // TODO(wave1-A)
}

// RequireUser redirects anonymous visitors to /login?redirect=<path>.
// TODO(wave1-A): match the Rust require_user behavior exactly (htmx-aware).
func RequireUser(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if _, ok := CurrentUser(r.Context()); !ok {
			// Local path only (r.URL.Path never carries a scheme/host).
			target := "/login?" + url.Values{"redirect": {r.URL.EscapedPath()}}.Encode()
			http.Redirect(w, r, target, http.StatusSeeOther) //nolint:gosec // G710: same-site /login target
			return
		}
		next.ServeHTTP(w, r)
	})
}

// RequireAdmin answers 404 to non-admins (admin pages are not advertised).
// TODO(wave1-A): confirm against src/app/admin.rs guard.
func RequireAdmin(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if u, ok := CurrentUser(r.Context()); !ok || !u.IsAdmin() {
			http.NotFound(w, r)
			return
		}
		next.ServeHTTP(w, r)
	})
}
