package authapi_test

import (
	"net/http"
	"strings"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// Sessions slide (cookie + row) about once per day; impersonation rows
// never slide past their cap.
func TestSessionSlidesAfterADayAndImpersonationStaysCapped(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "slide", "Slide")
	exec(t, s, `UPDATE sessions SET updated_at = now() - interval '25 hours', expires_at = now() + interval '1 hour' WHERE user_id = $1`, a.id)
	res := a.c.Get("/api/auth/get-session")
	wantStatus(t, res, http.StatusOK)
	line, ok := testutil.FindSetCookie(res.SetCookies(), testutil.SessionCookie)
	if !ok || !strings.Contains(line, "Max-Age=2592000") {
		t.Fatalf("refreshed cookie: %v", res.SetCookies())
	}
	if !scalar[bool](t, s, `SELECT expires_at > now() + interval '29 days' FROM sessions WHERE user_id = $1`, a.id) {
		t.Fatal("row slid to 30 days")
	}
	// Fresh sessions do not rewrite the cookie.
	if _, ok := testutil.FindSetCookie(a.c.Get("/api/auth/get-session").SetCookies(), testutil.SessionCookie); ok {
		t.Fatal("no refresh within a day")
	}

	admin := adminAccount(t, s, "slide-admin", "Slide Admin")
	target := signUp(t, s, "slide-target", "Slide Target")
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/impersonate-user", map[string]any{"userId": target.id.String()}), http.StatusOK)
	exec(t, s, `UPDATE sessions SET updated_at = now() - interval '25 hours' WHERE impersonated_by = $1`, admin.id)
	wantStatus(t, admin.c.Get("/api/auth/get-session"), http.StatusOK)
	if !scalar[bool](t, s, `SELECT bool_and(expires_at <= now() + interval '61 minutes') FROM sessions WHERE impersonated_by = $1`, admin.id) {
		t.Fatal("impersonation never slides past its cap")
	}
}

func TestExpiredSessionRowIsAnonymous(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "expired-row", "Expired")
	exec(t, s, `UPDATE sessions SET expires_at = now() - interval '1 second' WHERE user_id = $1`, a.id)
	if sessionEmail(t, a.c) != nil {
		t.Fatal("expired rows never resolve")
	}
	// The stored token is a sha256 hex digest, never the cookie value.
	raw := a.c.Cookie(testutil.SessionCookie)
	if n := scalar[int64](t, s, `SELECT count(*) FROM sessions WHERE token = $1`, raw); n != 0 || len(scalar[string](t, s, `SELECT token FROM sessions WHERE user_id = $1`, a.id)) != 64 {
		t.Fatal("token at rest must be hashed")
	}
}
