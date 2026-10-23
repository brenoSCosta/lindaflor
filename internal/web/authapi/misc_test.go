package authapi_test

import (
	"bytes"
	"net/http"
	"strings"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// update_user_api_rejects_client_supplied_image (tests/avatar.rs)
func TestUpdateUserAPIRejectsClientSuppliedImage(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "avatar-api", "Avatar Api")
	wantStatus(t, a.c.PostJSON("/api/auth/update-user", map[string]any{"image": "products/p/a.jpg"}), http.StatusBadRequest)
	if scalar[*string](t, s, `SELECT image FROM users WHERE id = $1`, a.id) != nil {
		t.Fatal("no image stored")
	}
	exec(t, s, `UPDATE users SET image = 'products/p/a.jpg' WHERE id = $1`, a.id)
	wantStatus(t, a.c.PostJSON("/api/auth/update-user", map[string]any{"image": nil}), http.StatusOK)
	if scalar[*string](t, s, `SELECT image FROM users WHERE id = $1`, a.id) != nil {
		t.Fatal("clearing stays allowed (foreign leftovers nulled)")
	}
}

// default_body_limit_rejects_over_1mb (tests/health.rs)
func TestDefaultBodyLimitRejectsOver1MB(t *testing.T) {
	s := newServer(t)
	res := s.Client(t).Do(http.MethodPost, "/api/auth/sign-in/email", bytes.NewReader(bytes.Repeat([]byte("x"), 1024*1024+1)),
		http.Header{"Content-Type": {"application/json"}})
	wantStatus(t, res, http.StatusRequestEntityTooLarge)
	if strings.Contains(strings.ToLower(res.Body), "sql") || strings.Contains(strings.ToLower(res.Body), "pgx") {
		t.Fatal(res.Body)
	}
}

// security_headers_present_on_404_and_handler_error (handler 401 half,
// tests/security_headers.rs)
func TestSecurityHeadersOnHandler401(t *testing.T) {
	s := newServer(t)
	res := s.Client(t).Get("/api/auth/list-accounts")
	wantStatus(t, res, http.StatusUnauthorized)
	if res.Header.Get("X-Content-Type-Options") != "nosniff" {
		t.Fatal("nosniff on handler errors")
	}
	if res.Header.Get("Content-Security-Policy") != httpx.CSPDevelopment {
		t.Fatalf("csp %q", res.Header.Get("Content-Security-Policy"))
	}
	if res.Header.Get("Strict-Transport-Security") != "" {
		t.Fatal("no HSTS in development")
	}
}

func TestErrorBodiesAreJSON(t *testing.T) {
	s := newServer(t)
	res := s.Client(t).Get("/api/auth/list-sessions")
	wantStatus(t, res, http.StatusUnauthorized)
	j := res.JSON(t)
	if j["code"] != "UNAUTHORIZED" || j["message"] != "unauthorized" || !strings.HasPrefix(res.Header.Get("Content-Type"), "application/json") {
		t.Fatal(res.Body)
	}
	wantStatus(t, s.Client(t).Do(http.MethodPost, "/api/auth/sign-in/email", strings.NewReader("{nope"), nil), http.StatusBadRequest)
	wantStatus(t, s.Client(t).Do(http.MethodPost, "/api/auth/sign-up/email", strings.NewReader(""), nil), http.StatusBadRequest)
	wantStatus(t, s.Client(t).PostJSON("/api/auth/sign-up/email", map[string]any{"email": "x@y.z"}), http.StatusBadRequest)
}

// prod_sign_out_expires_hardened_session_cookie (tests/prod_cookies.rs)
func TestProdSignOutExpiresHardenedSessionCookie(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{Prod: true})
	c := s.Client(t)
	up := c.SignUp("Prod Signout", testutil.UniqueEmail("prod-signout"), strongPW)
	wantStatus(t, up, http.StatusOK)
	line, ok := testutil.FindSetCookie(up.SetCookies(), testutil.ProdSessionCookie)
	if !ok {
		t.Fatal("prod session cookie on sign-up")
	}
	testutil.AssertHardenedCookie(t, line, testutil.ProdSessionCookie)
	if _, bare := testutil.FindSetCookie(up.SetCookies(), testutil.SessionCookie); bare {
		t.Fatal("production must not set the bare session cookie")
	}
	out := c.PostJSON("/api/auth/sign-out", map[string]any{})
	wantStatus(t, out, http.StatusOK)
	cleared, ok := testutil.FindSetCookie(out.SetCookies(), testutil.ProdSessionCookie)
	if !ok {
		t.Fatal("sign-out must expire the session cookie")
	}
	testutil.AssertExpiredHardenedCookie(t, cleared, testutil.ProdSessionCookie)
	if cart, ok := testutil.FindSetCookie(out.SetCookies(), testutil.ProdCartCookie); ok {
		testutil.AssertExpiredHardenedCookie(t, cart, testutil.ProdCartCookie)
	} else {
		t.Fatal("sign-out expires the cart cookie")
	}
}

// The API 2FA challenge carries the pending token in the body only.
func TestProdAPITwoFactorChallengeSetsNoCookie(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{Prod: true})
	a := signUp(t, s, "prod-pending-api", "Prod Pending")
	enable2FA(t, a)
	_, res := signIn(t, s, a.email, strongPW)
	wantStatus(t, res, http.StatusForbidden)
	if len(res.SetCookies()) != 0 {
		t.Fatal("API 2FA challenge sets no cookie (token is in the body)")
	}
}
