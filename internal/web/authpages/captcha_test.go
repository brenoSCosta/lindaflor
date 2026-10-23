package authpages_test

import (
	"net/http"
	"net/url"
	"strings"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/captcha"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

const widget = `class="cf-turnstile`

func userExists(t *testing.T, s *testutil.Server, email string) bool {
	t.Helper()
	var n int
	if err := s.Pool.QueryRow(t.Context(), "SELECT count(*) FROM users WHERE email = $1", email).Scan(&n); err != nil {
		t.Fatal(err)
	}
	return n > 0
}

func resetRequested(t *testing.T, s *testutil.Server, email string) bool {
	t.Helper()
	var n int
	if err := s.Pool.QueryRow(t.Context(), "SELECT count(*) FROM verifications WHERE identifier = $1", "reset-password:"+email).Scan(&n); err != nil {
		t.Fatal(err)
	}
	return n > 0
}

func TestCaptchaWidgetRendersOnlyWhenConfigured(t *testing.T) {
	off := testutil.NewServer(t, testutil.Options{}).Client(t)
	for _, p := range []string{"/login?mode=signup", "/forgot-password", "/login"} {
		if strings.Contains(off.Get(p).Body, widget) {
			t.Fatalf("%s: no widget without TURNSTILE_* config", p)
		}
	}
	f := testutil.NewFakeTurnstile(t)
	on := testutil.NewServer(t, testutil.Options{Mutate: f.WithTurnstile(true)}).Client(t)
	for _, p := range []string{"/login?mode=signup", "/forgot-password"} {
		body := on.Get(p).Body
		for _, want := range []string{widget, `data-sitekey="` + testutil.TurnstileSiteKey + `"`, `data-theme="auto"`, `data-language="pt-br"`} {
			if !strings.Contains(body, want) {
				t.Fatalf("%s: missing %q", p, want)
			}
		}
	}
	// No CAPTCHA on sign-in (lockouts cover it).
	if strings.Contains(on.Get("/login").Body, widget) {
		t.Fatal("sign-in must not render the widget")
	}
}

func TestSignupCaptchaEnforced(t *testing.T) {
	f := testutil.NewFakeTurnstile(t)
	s := testutil.NewServer(t, testutil.Options{Mutate: f.WithTurnstile(true)})
	c := s.Client(t)
	email := testutil.UniqueEmail("captcha-signup")
	form := url.Values{"name": {"Ana"}, "email": {email}, "password": {pw}}

	missing := c.PostForm("/signup", form)
	wantRedirect(t, missing, "/login?")
	wantToast(t, missing, toast.ErrorType, captcha.Message)
	if l := missing.Header.Get("Location"); !strings.Contains(l, "mode=signup") || !strings.Contains(l, "name=Ana") {
		t.Fatalf("keeps the form context: %s", l)
	}

	form.Set(captcha.FormField, "forged")
	wantToast(t, c.PostForm("/signup", form), toast.ErrorType, captcha.Message)

	// htmx: toast header + HX-Location back to the form (widget re-renders).
	hx := s.Client(t).HTMX().PostForm("/signup", form)
	wantStatus(t, hx, http.StatusOK)
	if !strings.HasPrefix(hx.Header.Get("HX-Location"), "/login?") || hx.Header.Get("X-Toast") == "" {
		t.Fatalf("htmx answer: %v", hx.Header)
	}
	if userExists(t, s, email) {
		t.Fatal("no account without a valid CAPTCHA")
	}

	form.Set(captcha.FormField, testutil.TurnstilePass)
	ok := c.PostForm("/signup", form)
	wantRedirect(t, ok, "/dashboard")
	wantToast(t, ok, toast.SuccessType, "Conta criada.")
	if !userExists(t, s, email) {
		t.Fatal("account created with a valid CAPTCHA")
	}
	reqs := f.Requests()
	// Sign-in has no CAPTCHA (lockouts cover it).
	wantRedirect(t, s.Client(t).PostForm("/login", url.Values{"email": {email}, "password": {pw}}), "/dashboard")
	last := reqs[len(reqs)-1]
	if last.Secret != testutil.TurnstileSecret || last.Response != testutil.TurnstilePass || last.RemoteIP == "" {
		t.Fatalf("siteverify form: %+v", last)
	}
}

func TestSignupCaptchaReportOnly(t *testing.T) {
	f := testutil.NewFakeTurnstile(t)
	s := testutil.NewServer(t, testutil.Options{Mutate: f.WithTurnstile(false)})
	email := testutil.UniqueEmail("captcha-report")
	res := s.Client(t).PostForm("/signup", url.Values{"name": {"Ana"}, "email": {email}, "password": {pw}, captcha.FormField: {"forged"}})
	wantRedirect(t, res, "/dashboard")
	if !userExists(t, s, email) || len(f.Requests()) != 1 {
		t.Fatal("not enforced: verified, logged, accepted")
	}
}

func TestForgotPasswordCaptcha(t *testing.T) {
	f := testutil.NewFakeTurnstile(t)
	s := testutil.NewServer(t, testutil.Options{Mutate: f.WithTurnstile(true)})
	signer := s.Client(t)
	signer.Headers.Set(captcha.Header, testutil.TurnstilePass)
	u := struct{ email string }{testutil.UniqueEmail("captcha-forgot")}
	wantStatus(t, signer.SignUp("Ana", u.email, pw), http.StatusOK)
	c := s.Client(t)

	missing := c.PostForm("/forgot-password", url.Values{"email": {u.email}})
	wantRedirect(t, missing, "/forgot-password")
	if strings.HasPrefix(missing.Header.Get("Location"), "/forgot-password/sent") {
		t.Fatal("must not claim the link was sent")
	}
	wantToast(t, missing, toast.ErrorType, captcha.Message)
	if resetRequested(t, s, u.email) {
		t.Fatal("no reset without a valid CAPTCHA")
	}

	ok := c.PostForm("/forgot-password", url.Values{"email": {u.email}, captcha.FormField: {testutil.TurnstilePass}})
	wantRedirect(t, ok, "/forgot-password/sent")
	if !resetRequested(t, s, u.email) {
		t.Fatal("reset requested with a valid CAPTCHA")
	}
}

func TestCaptchaNotConfiguredKeepsForms(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	email := testutil.UniqueEmail("captcha-off")
	wantRedirect(t, s.Client(t).PostForm("/signup", url.Values{"name": {"Ana"}, "email": {email}, "password": {pw}}), "/dashboard")
	wantRedirect(t, s.Client(t).PostForm("/forgot-password", url.Values{"email": {email}}), "/forgot-password/sent")
	if !resetRequested(t, s, email) {
		t.Fatal("reset requested without CAPTCHA config")
	}
}
