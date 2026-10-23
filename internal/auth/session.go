package auth

import (
	"context"
	"crypto/rand"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"errors"
	"fmt"
	"net/http"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/httpx"
)

// Session lifetimes.
const (
	// SessionMaxAge is the sliding session lifetime (cookie Max-Age and
	// sessions.expires_at).
	SessionMaxAge = 30 * 24 * time.Hour
	// SessionUpdateAge: the expiry slides at most once per day.
	SessionUpdateAge = 24 * time.Hour
	// ImpersonationTTL caps an admin impersonation session (the DB row
	// expires first even though the cookie keeps the normal lifetime).
	ImpersonationTTL = time.Hour
)

// Cookie names of the auth flows (dev drops Secure and the __Host- prefix).
const (
	Pending2FACookieDev    = "lindaflor_pending_2fa"
	Pending2FACookieSecure = "__Host-lindaflor_pending_2fa"
	OAuthBindCookieDev     = "oauth_bind"
	OAuthBindCookieSecure  = "__Host-oauth_bind"
	// Cart cookie (owned by the store); auth only expires it on logout.
	CartCookieDev    = "lindaflor_cart"
	CartCookieSecure = "__Host-lindaflor_cart"
)

func cookieName(secure bool, dev, prod string) string {
	if secure {
		return prod
	}
	return dev
}

// SetCookie writes a hardened cookie: HttpOnly, SameSite=Lax, Path=/,
// Secure (+ __Host- name chosen by the caller) when secure.
func SetCookie(w http.ResponseWriter, name, value string, maxAge time.Duration, secure bool) {
	setCookieReplacing(w, &http.Cookie{ //nolint:gosec // G124: Secure is false only in development (plain-HTTP localhost)
		Name: name, Value: value, Path: "/", MaxAge: int(maxAge / time.Second),
		HttpOnly: true, Secure: secure, SameSite: http.SameSiteLaxMode,
	})
}

// ClearCookie expires a cookie with the same attributes it was written with.
func ClearCookie(w http.ResponseWriter, name string, secure bool) {
	setCookieReplacing(w, &http.Cookie{ //nolint:gosec // G124: Secure is false only in development (plain-HTTP localhost)
		Name: name, Value: "", Path: "/", MaxAge: -1, Expires: time.Unix(0, 0),
		HttpOnly: true, Secure: secure, SameSite: http.SameSiteLaxMode,
	})
}

// setCookieReplacing adds c, dropping an earlier Set-Cookie line for the
// same name in this response (stop + start in one request keeps only the
// final cookie).
func setCookieReplacing(w http.ResponseWriter, c *http.Cookie) {
	h := w.Header()
	prefix := c.Name + "="
	var kept []string
	for _, v := range h.Values("Set-Cookie") {
		if !strings.HasPrefix(v, prefix) {
			kept = append(kept, v)
		}
	}
	h.Del("Set-Cookie")
	for _, v := range kept {
		h.Add("Set-Cookie", v)
	}
	if v := c.String(); v != "" {
		h.Add("Set-Cookie", v)
	}
}

func readCookie(r *http.Request, name string) string {
	c, err := r.Cookie(name)
	if err != nil {
		return ""
	}
	return strings.TrimSpace(c.Value)
}

// newSessionToken returns a raw 32-byte token (base64url) and its at-rest
// hash.
func newSessionToken() (raw, hash string) {
	b := make([]byte, 32)
	if _, err := rand.Read(b); err != nil {
		panic("crypto/rand failed: " + err.Error())
	}
	raw = base64.RawURLEncoding.EncodeToString(b)
	return raw, TokenHashHex(raw)
}

// TokenHashHex is hex(sha256(raw cookie token)) — the sessions.token value.
func TokenHashHex(raw string) string {
	sum := sha256.Sum256([]byte(raw))
	return hex.EncodeToString(sum[:])
}

// SessionTokenHash is the at-rest hash of the request's session cookie
// ("" without one). It identifies the current session even when the user
// could not be loaded (expired, banned).
func SessionTokenHash(r *http.Request, secure bool) string {
	raw := readCookie(r, SessionCookieName(secure))
	if raw == "" {
		return ""
	}
	return TokenHashHex(raw)
}

func setSessionCookie(w http.ResponseWriter, raw string, secure bool) {
	SetCookie(w, SessionCookieName(secure), raw, SessionMaxAge, secure)
}

// IsCurrentlyBanned is the single ban rule: permanent bans (no expiry)
// stay banned, temporary bans lift once ban_expires passes.
func IsCurrentlyBanned(banned bool, banExpires *time.Time) bool {
	if !banned {
		return false
	}
	return banExpires == nil || banExpires.After(time.Now())
}

func userFromSessionRow(row sqlc.AuthSessionWithUserRow) *User {
	return &User{
		ID: row.UserID, Name: row.Name, Email: row.Email, EmailVerified: row.EmailVerified,
		Image: row.Image, TwoFactorEnabled: row.TwoFactorEnabled, Role: row.Role,
		Banned: row.Banned, BanReason: row.BanReason, BanExpires: row.BanExpires,
	}
}

// UserFromRow converts a users row.
func UserFromRow(row sqlc.User) *User {
	return &User{
		ID: row.ID, Name: row.Name, Email: row.Email, EmailVerified: row.EmailVerified,
		Image: row.Image, TwoFactorEnabled: row.TwoFactorEnabled, Role: row.Role,
		Banned: row.Banned, BanReason: row.BanReason, BanExpires: row.BanExpires,
	}
}

// loadSession resolves the request's session (unexpired, user not
// banned), sliding its expiry about once per day.
func loadSession(ctx context.Context, w http.ResponseWriter, r *http.Request, q *sqlc.Queries, secure bool) (*User, *Session, error) {
	raw := readCookie(r, SessionCookieName(secure))
	if raw == "" {
		return nil, nil, nil
	}
	hash := TokenHashHex(raw)
	row, err := q.AuthSessionWithUser(ctx, hash)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, nil, nil
	}
	if err != nil {
		return nil, nil, fmt.Errorf("load session: %w", err)
	}
	if IsCurrentlyBanned(row.Banned, row.BanExpires) {
		return nil, nil, nil
	}
	expires := row.ExpiresAt
	if time.Since(row.UpdatedAt) >= SessionUpdateAge {
		next := time.Now().Add(SessionMaxAge)
		if err := q.AuthSlideSessionExpiry(ctx, sqlc.AuthSlideSessionExpiryParams{ExpiresAt: next, Token: hash}); err != nil {
			return nil, nil, fmt.Errorf("slide session: %w", err)
		}
		setSessionCookie(w, raw, secure)
		if row.ImpersonatedBy == nil || next.Before(expires) {
			expires = next
		}
	}
	s := &Session{ID: row.SessionID, UserID: row.UserID, ExpiresAt: expires, ImpersonatedBy: row.ImpersonatedBy, TokenHash: hash}
	return userFromSessionRow(row), s, nil
}

// insertSession starts a new session for userID: fresh token, DB row
// (impersonation capped at ImpersonationTTL) and the cookie.
func insertSession(ctx context.Context, w http.ResponseWriter, r *http.Request, q *sqlc.Queries, secure bool, userID uuid.UUID, impersonatedBy *uuid.UUID) (*Session, error) {
	raw, hash := newSessionToken()
	id, err := uuid.NewV7()
	if err != nil {
		return nil, err
	}
	expires := time.Now().Add(SessionMaxAge)
	if impersonatedBy != nil {
		if c := time.Now().Add(ImpersonationTTL); c.Before(expires) {
			expires = c
		}
	}
	ip, ua := httpx.ClientIP(r), httpx.UserAgent(r)
	var ipPtr, uaPtr *string
	if ip != "" && ip != "unknown" {
		ipPtr = &ip
	}
	if ua != "" {
		uaPtr = &ua
	}
	if err := q.AuthInsertSession(ctx, sqlc.AuthInsertSessionParams{
		ID: id, ExpiresAt: expires, Token: hash, IpAddress: ipPtr, UserAgent: uaPtr,
		UserID: userID, ImpersonatedBy: impersonatedBy,
	}); err != nil {
		return nil, fmt.Errorf("insert session: %w", err)
	}
	setSessionCookie(w, raw, secure)
	return &Session{ID: id, UserID: userID, ExpiresAt: expires, ImpersonatedBy: impersonatedBy, TokenHash: hash}, nil
}

// stopSession deletes the request's session row (if any) and expires the
// cookie. Returns the stopped token hash ("" when there was no cookie).
func stopSession(ctx context.Context, w http.ResponseWriter, r *http.Request, q *sqlc.Queries, secure bool) (string, error) {
	hash := SessionTokenHash(r, secure)
	ClearCookie(w, SessionCookieName(secure), secure)
	if hash == "" {
		return "", nil
	}
	if err := q.AuthDeleteSessionByToken(ctx, hash); err != nil {
		return hash, fmt.Errorf("delete session: %w", err)
	}
	return hash, nil
}

// ClearCartCookie expires the store's cart cookie (logout must not leave
// the previous owner's cart attached to the browser).
func ClearCartCookie(w http.ResponseWriter, secure bool) {
	ClearCookie(w, cookieName(secure, CartCookieDev, CartCookieSecure), secure)
}
