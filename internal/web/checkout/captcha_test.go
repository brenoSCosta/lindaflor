package checkout_test

import (
	"net/http"
	"strings"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/captcha"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func orders(t *testing.T, s *testutil.Server) int64 {
	t.Helper()
	return scalar[int64](t, s.Pool, `SELECT COUNT(*) FROM orders`)
}

func TestCheckoutCaptchaEnforced(t *testing.T) {
	f := testutil.NewFakeTurnstile(t)
	s := testutil.NewServer(t, testutil.Options{Mutate: f.WithTurnstile(true)})
	pixSettings(t, s.Pool)
	c := s.Client(t)
	cartFor(t, s, c, seedVariant(t, s.Pool, "Biquíni Captcha", 5), 1)

	page := c.Get("/checkout")
	wantContains(t, page, `class="cf-turnstile`, `data-sitekey="`+testutil.TurnstileSiteKey+`"`, "Pagar com PIX")

	// Coupon preview (intent=apply) never needs the CAPTCHA.
	apply := payForm("cliente@example.com")
	apply.Set("intent", "apply")
	wantStatus(t, c.PostForm("/checkout", apply), http.StatusOK)

	res := c.PostForm("/checkout", payForm("cliente@example.com"))
	wantStatus(t, res, http.StatusUnprocessableEntity)
	wantContains(t, res, captcha.Message, "Maria Silva", `class="cf-turnstile`)
	forged := payForm("cliente@example.com")
	forged.Set(captcha.FormField, "forged")
	wantStatus(t, c.PostForm("/checkout", forged), http.StatusUnprocessableEntity)
	if n := orders(t, s); n != 0 {
		t.Fatalf("created %d orders without a valid CAPTCHA", n)
	}

	ok := payForm("cliente@example.com")
	ok.Set(captcha.FormField, testutil.TurnstilePass)
	res = c.PostForm("/checkout", ok)
	wantStatus(t, res, http.StatusSeeOther)
	if !strings.HasPrefix(res.Header.Get("Location"), "/pedido/") || orders(t, s) != 1 {
		t.Fatalf("order with a valid CAPTCHA: %q", res.Header.Get("Location"))
	}
}

func TestCheckoutCaptchaNotConfigured(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	pixSettings(t, s.Pool)
	c := s.Client(t)
	cartFor(t, s, c, seedVariant(t, s.Pool, "Biquíni Livre", 5), 1)
	if strings.Contains(c.Get("/checkout").Body, `class="cf-turnstile`) {
		t.Fatal("no widget without TURNSTILE_* config")
	}
	wantStatus(t, c.PostForm("/checkout", payForm("cliente@example.com")), http.StatusSeeOther)
}
