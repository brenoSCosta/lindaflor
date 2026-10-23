// Package auth: users, sessions, passwords, TOTP, one-time tokens, Google
// OAuth and the /api/auth JSON API.
//
// The contract other packages code against: User/Session, CurrentUser /
// CurrentSession readers, Middleware (wired in internal/web/router.go),
// RequireUser / RequireAdmin. The behaviour lives in Service (service*.go):
// sign-up/in, sessions, one-time tokens, TOTP, step-up, Google OAuth,
// impersonation and admin user management. HTTP surfaces live in
// internal/web/authapi (/api/auth/*) and internal/web/authpages (HTML).
//
// Extension points for other packages: RegisterLoginHook (guest-cart merge
// on login) and RateAllow (in-handler rate-limit budget, ratelimit.Check).
package auth

import (
	"context"
	"log/slog"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/logging"
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
type MiddlewareDeps struct {
	Pool   *pgxpool.Pool
	Cfg    *config.Config
	Valkey *valkey.Client
}

// Middleware loads the session from the cookie (`session` /
// `__Host-session`), resolves the user (unexpired session, not currently
// banned), slides the expiry about once per day and stores both in the
// request context. Lookup failures are logged and treated as anonymous.
func Middleware(d MiddlewareDeps) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		if d.Pool == nil || d.Cfg == nil {
			return next
		}
		q := sqlc.New(d.Pool)
		secure := d.Cfg.SecureCookies
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			u, s, err := loadSession(r.Context(), w, r, q, secure)
			switch {
			case err == nil:
			case r.Context().Err() != nil:
				// The client went away mid-lookup (navigation, aborted
				// fetch): not a server fault, the response is never read.
				slog.DebugContext(r.Context(), "auth: load session: request cancelled", "error", err)
			default:
				slog.ErrorContext(r.Context(), "auth: load session", "error", err)
			}
			if u != nil {
				role := ""
				if u.Role != nil {
					role = *u.Role
				}
				logging.SetRequestUser(r.Context(), logging.RequestUser{
					UserID: u.ID.String(), SessionID: s.ID.String(), Role: role,
				})
				r = r.WithContext(WithUser(r.Context(), u, s))
			}
			next.ServeHTTP(w, r)
		})
	}
}

// LoginPath is where RequireUser sends anonymous visitors.
const LoginPath = "/login"

// RequireUser redirects anonymous visitors to /login?next=<path> (303, or
// HX-Location for htmx requests).
func RequireUser(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if _, ok := CurrentUser(r.Context()); !ok {
			target := LoginPath
			if p := r.URL.RequestURI(); p != "" && p != "/" {
				if safe, ok := httpx.SafeLocalRedirect(p); ok {
					target += "?" + url.Values{"next": {safe}}.Encode()
				}
			}
			httpx.Redirect(w, r, target)
			return
		}
		next.ServeHTTP(w, r)
	})
}

// RequireAdmin mirrors src/app/auth_helpers.rs require_admin: anonymous →
// /login (RequireUser), signed-in non-admins → /dashboard.
func RequireAdmin(next http.Handler) http.Handler {
	return RequireUser(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if u, _ := CurrentUser(r.Context()); !u.IsAdmin() {
			httpx.Redirect(w, r, "/dashboard")
			return
		}
		next.ServeHTTP(w, r)
	}))
}
