package storefront_test

import (
	"strings"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/captcha"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func captchaServer(t *testing.T, enforced bool) *testutil.Server {
	t.Helper()
	ratelimit.Reset()
	t.Cleanup(ratelimit.Reset)
	f := testutil.NewFakeTurnstile(t)
	return testutil.NewServer(t, testutil.Options{Mutate: f.WithTurnstile(enforced)})
}

func subscribers(t *testing.T, s *testutil.Server, email string) int64 {
	t.Helper()
	return scalar[int64](t, s.Pool, `SELECT count(*) FROM newsletter_subscribers WHERE email = $1`, email)
}

func TestNewsletterCaptcha(t *testing.T) {
	s := captchaServer(t, true)
	c := s.Client(t)
	res := c.PostForm("/newsletter", form("email", "cap@example.com"))
	wantRedirect(t, res, "/")
	wantToast(t, flash(t, res), toast.ErrorType, captcha.Message)
	res = c.PostForm("/newsletter", form("email", "cap@example.com", captcha.FormField, "forged"))
	wantToast(t, flash(t, res), toast.ErrorType, captcha.Message)
	if subscribers(t, s, "cap@example.com") != 0 {
		t.Fatal("no subscription without a valid CAPTCHA")
	}
	res = c.PostForm("/newsletter", form("email", "cap@example.com", captcha.FormField, testutil.TurnstilePass))
	wantToast(t, flash(t, res), toast.SuccessType, "Inscrição realizada! Você vai receber nossas novidades por e-mail.")
	if subscribers(t, s, "cap@example.com") != 1 {
		t.Fatal("subscribed with a valid CAPTCHA")
	}
}

func TestNewsletterCaptchaNotConfigured(t *testing.T) {
	s := newServer(t)
	res := s.Client(t).PostForm("/newsletter", form("email", "off@example.com"))
	wantToast(t, flash(t, res), toast.SuccessType, "Inscrição realizada! Você vai receber nossas novidades por e-mail.")
}

func TestStockAlertCaptcha(t *testing.T) {
	s := captchaServer(t, true)
	p := seedProduct(t, s.Pool, "Saída Captcha", 8990, 0)
	c := s.Client(t)

	page := c.Get("/produtos/" + p.Slug)
	wantContains(t, page.Body, `action="/avise-me"`, `class="cf-turnstile`, `data-sitekey="`+testutil.TurnstileSiteKey+`"`)

	res := c.PostForm("/avise-me", form("variant_id", p.VariantID.String(), "email", "ana@example.com"))
	if !strings.Contains(res.Header.Get("Location"), "variant="+p.VariantID.String()) {
		t.Fatalf("back to the product: %q", res.Header.Get("Location"))
	}
	wantToast(t, flash(t, res), toast.ErrorType, captcha.Message)
	if len(alerts(t, s.Pool, p.VariantID)) != 0 {
		t.Fatal("no alert without a valid CAPTCHA")
	}
	c.PostForm("/avise-me", form("variant_id", p.VariantID.String(), "email", "ana@example.com", captcha.FormField, testutil.TurnstilePass))
	if got := alerts(t, s.Pool, p.VariantID); len(got) != 1 || got[0] != "ana@example.com" {
		t.Fatalf("alert stored with a valid CAPTCHA: %v", got)
	}
}

func TestStockAlertCaptchaNotConfigured(t *testing.T) {
	s := newServer(t)
	p := seedProduct(t, s.Pool, "Saída Sem Captcha", 8990, 0)
	c := s.Client(t)
	wantNotContains(t, c.Get("/produtos/"+p.Slug).Body, `class="cf-turnstile`)
	c.PostForm("/avise-me", form("variant_id", p.VariantID.String(), "email", "bia@example.com"))
	if got := alerts(t, s.Pool, p.VariantID); len(got) != 1 {
		t.Fatalf("alert stored without CAPTCHA config: %v", got)
	}
}
