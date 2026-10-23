package testutil

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"sync"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/config"
)

// Turnstile test values. Cloudflare's published test keys are used for
// realism, but the fake never talks to Cloudflare.
const (
	TurnstileSiteKey = "1x00000000000000000000AA"
	TurnstileSecret  = "1x0000000000000000000000000000000AA"
	// TurnstilePass is the only token the fake siteverify accepts.
	TurnstilePass = "test-pass-token"
)

// TurnstileRequest is one form posted to the fake siteverify.
type TurnstileRequest struct{ Secret, Response, RemoteIP string }

// FakeTurnstile is an httptest siteverify: success for TurnstilePass with
// the right secret, else success=false with invalid-input-response.
type FakeTurnstile struct {
	*httptest.Server
	mu   sync.Mutex
	reqs []TurnstileRequest
}

// NewFakeTurnstile starts the fake (closed at cleanup).
func NewFakeTurnstile(t testing.TB) *FakeTurnstile {
	t.Helper()
	f := &FakeTurnstile{}
	f.Server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if err := r.ParseForm(); err != nil {
			http.Error(w, "bad form", http.StatusBadRequest)
			return
		}
		req := TurnstileRequest{Secret: r.PostForm.Get("secret"), Response: r.PostForm.Get("response"), RemoteIP: r.PostForm.Get("remoteip")}
		f.mu.Lock()
		f.reqs = append(f.reqs, req)
		f.mu.Unlock()
		body := map[string]any{"success": true, "error-codes": []string{}, "hostname": "example.com", "action": ""}
		switch {
		case req.Secret != TurnstileSecret:
			body = map[string]any{"success": false, "error-codes": []string{"invalid-input-secret"}}
		case req.Response != TurnstilePass:
			body = map[string]any{"success": false, "error-codes": []string{"invalid-input-response"}}
		}
		w.Header().Set("Content-Type", "application/json")
		_ = json.NewEncoder(w).Encode(body)
	}))
	t.Cleanup(f.Close)
	return f
}

// Requests are the forms received so far.
func (f *FakeTurnstile) Requests() []TurnstileRequest {
	f.mu.Lock()
	defer f.mu.Unlock()
	return append([]TurnstileRequest(nil), f.reqs...)
}

// Config is a configured Turnstile pointing at the fake.
func (f *FakeTurnstile) Config(enforced bool) config.CaptchaConfig {
	return config.CaptchaConfig{SiteKey: TurnstileSiteKey, Secret: TurnstileSecret, Enforced: enforced, VerifyURL: f.URL}
}

// WithTurnstile is an Options.Mutate that enables Turnstile against the fake.
func (f *FakeTurnstile) WithTurnstile(enforced bool) func(*config.Config) {
	return func(c *config.Config) { c.Captcha = f.Config(enforced) }
}
