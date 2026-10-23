package authapi_test

import (
	"net/http"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/captcha"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func wantCaptchaFailed(t *testing.T, res *testutil.Response) {
	t.Helper()
	if res.StatusCode != http.StatusBadRequest {
		t.Fatalf("status %d: %s", res.StatusCode, res.Body)
	}
	if j := res.JSON(t); j["code"] != captcha.ErrorCode || j["message"] != captcha.Message {
		t.Fatalf("body: %v", j)
	}
}

func countRows(t *testing.T, s *testutil.Server, sql, arg string) int {
	t.Helper()
	var n int
	if err := s.Pool.QueryRow(t.Context(), sql, arg).Scan(&n); err != nil {
		t.Fatal(err)
	}
	return n
}

func TestSignUpEmailCaptcha(t *testing.T) {
	f := testutil.NewFakeTurnstile(t)
	s := testutil.NewServer(t, testutil.Options{Mutate: f.WithTurnstile(true)})
	email := testutil.UniqueEmail("api-captcha")
	body := map[string]string{"name": "Ana", "email": email, "password": strongPW}

	wantCaptchaFailed(t, s.Client(t).PostJSON("/api/auth/sign-up/email", body))
	body["captchaToken"] = "forged"
	wantCaptchaFailed(t, s.Client(t).PostJSON("/api/auth/sign-up/email", body))
	if countRows(t, s, "SELECT count(*) FROM users WHERE email = $1", email) != 0 {
		t.Fatal("no account without a valid CAPTCHA")
	}

	body["captchaToken"] = testutil.TurnstilePass
	if res := s.Client(t).PostJSON("/api/auth/sign-up/email", body); res.StatusCode != http.StatusOK {
		t.Fatalf("valid token (body): %d %s", res.StatusCode, res.Body)
	}

	// The better-auth header works too.
	delete(body, "captchaToken")
	body["email"] = testutil.UniqueEmail("api-captcha-header")
	c := s.Client(t)
	c.Headers.Set(captcha.Header, testutil.TurnstilePass)
	if res := c.PostJSON("/api/auth/sign-up/email", body); res.StatusCode != http.StatusOK {
		t.Fatalf("valid token (header): %d %s", res.StatusCode, res.Body)
	}
	// The CSP lets the widget load.
	if got := c.Get("/api/health").Header.Get("Content-Security-Policy"); got != httpx.CSPDevelopmentTurnstile {
		t.Fatalf("CSP = %q", got)
	}
}

func TestRequestPasswordResetCaptcha(t *testing.T) {
	f := testutil.NewFakeTurnstile(t)
	s := testutil.NewServer(t, testutil.Options{Mutate: f.WithTurnstile(true)})
	email := testutil.UniqueEmail("api-captcha-reset")
	signer := s.Client(t)
	signer.Headers.Set(captcha.Header, testutil.TurnstilePass)
	if res := signer.SignUp("Ana", email, strongPW); res.StatusCode != http.StatusOK {
		t.Fatalf("sign-up: %d %s", res.StatusCode, res.Body)
	}
	u := struct{ email string }{email}
	const resets = "SELECT count(*) FROM verifications WHERE identifier = $1"

	wantCaptchaFailed(t, s.Client(t).PostJSON("/api/auth/request-password-reset", map[string]string{"email": u.email}))
	if countRows(t, s, resets, "reset-password:"+u.email) != 0 {
		t.Fatal("no reset without a valid CAPTCHA")
	}
	res := s.Client(t).PostJSON("/api/auth/request-password-reset", map[string]string{"email": u.email, "captchaToken": testutil.TurnstilePass})
	if res.StatusCode != http.StatusOK || res.JSON(t)["status"] != true {
		t.Fatalf("valid token: %d %s", res.StatusCode, res.Body)
	}
	if countRows(t, s, resets, "reset-password:"+u.email) != 1 {
		t.Fatal("reset requested with a valid CAPTCHA")
	}
}

func TestCaptchaNotConfiguredKeepsAPI(t *testing.T) {
	s := newServer(t)
	u := signUp(t, s, "api-captcha-off", "Ana") // no token, still 200
	res := s.Client(t).PostJSON("/api/auth/request-password-reset", map[string]string{"email": u.email})
	if res.StatusCode != http.StatusOK {
		t.Fatalf("%d %s", res.StatusCode, res.Body)
	}
	if got := res.Header.Get("Content-Security-Policy"); got != httpx.CSPDevelopment {
		t.Fatalf("CSP must stay byte-identical without Turnstile: %q", got)
	}
}

func TestSignInHasNoCaptcha(t *testing.T) {
	f := testutil.NewFakeTurnstile(t)
	s := testutil.NewServer(t, testutil.Options{Mutate: f.WithTurnstile(true)})
	c := s.Client(t)
	c.Headers.Set(captcha.Header, testutil.TurnstilePass)
	email := testutil.UniqueEmail("api-captcha-signin")
	if res := c.SignUp("Ana", email, strongPW); res.StatusCode != http.StatusOK {
		t.Fatalf("sign-up: %d %s", res.StatusCode, res.Body)
	}
	// Sign-in is protected by lockouts, not the CAPTCHA.
	if _, res := signIn(t, s, email, strongPW); res.StatusCode != http.StatusOK {
		t.Fatalf("sign-in: %d %s", res.StatusCode, res.Body)
	}
}
