package captcha

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"net/http/httptest"
	"net/url"
	"strings"
	"sync/atomic"
	"testing"
	"time"

	"github.com/wellington-vell/lindaflor/internal/config"
)

const (
	siteKey = "1x00000000000000000000AA"
	secret  = "1x0000000000000000000000000000000AA"
)

// siteverify answers like Cloudflare: success only for token "good".
func siteverify(t *testing.T, got *url.Values) *httptest.Server {
	t.Helper()
	srv := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method != http.MethodPost || !strings.HasPrefix(r.Header.Get("Content-Type"), "application/x-www-form-urlencoded") {
			t.Errorf("siteverify got %s %s", r.Method, r.Header.Get("Content-Type"))
		}
		_ = r.ParseForm()
		if got != nil {
			*got = r.PostForm
		}
		res := map[string]any{"success": true, "hostname": "example.com", "action": "signup", "error-codes": []string{}}
		if r.PostForm.Get("response") != "good" {
			res = map[string]any{"success": false, "error-codes": []string{"invalid-input-response", "timeout-or-duplicate"}}
		}
		_ = json.NewEncoder(w).Encode(res)
	}))
	t.Cleanup(srv.Close)
	return srv
}

func newVerifier(u string, enforced bool, client *http.Client) *Verifier {
	return New(config.CaptchaConfig{SiteKey: siteKey, Secret: secret, Enforced: enforced, VerifyURL: u}, client)
}

func TestNotConfiguredIsNoop(t *testing.T) {
	var calls atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) { calls.Add(1) }))
	defer srv.Close()
	for _, cfg := range []config.CaptchaConfig{
		{Enforced: true, VerifyURL: srv.URL},
		{SiteKey: siteKey, Enforced: true, VerifyURL: srv.URL},
		{Secret: secret, Enforced: true, VerifyURL: srv.URL},
	} {
		v := New(cfg, nil)
		if v.Enabled() || v.Enforced() || v.SiteKey() != "" {
			t.Fatalf("%v: must be disabled", cfg)
		}
		if ok, err := v.Verify(context.Background(), "", "1.2.3.4"); !ok || err != nil {
			t.Fatalf("%v: ok=%v err=%v", cfg, ok, err)
		}
	}
	var nilV *Verifier
	if ok, err := nilV.Verify(context.Background(), "", ""); !ok || err != nil || nilV.SiteKey() != "" {
		t.Fatal("nil verifier must be a no-op")
	}
	if calls.Load() != 0 {
		t.Fatal("siteverify must not be called")
	}
}

func TestVerifySuccessPostsSecretTokenAndIP(t *testing.T) {
	var got url.Values
	v := newVerifier(siteverify(t, &got).URL, true, nil)
	if !v.Enabled() || !v.Enforced() || v.SiteKey() != siteKey {
		t.Fatal("must be enabled and enforced")
	}
	ok, err := v.Verify(context.Background(), " good ", "203.0.113.9")
	if !ok || err != nil {
		t.Fatalf("ok=%v err=%v", ok, err)
	}
	if got.Get("secret") != secret || got.Get("response") != "good" || got.Get("remoteip") != "203.0.113.9" {
		t.Fatalf("form = %v", got)
	}
	// No IP: remoteip is omitted.
	if ok, _ := v.Verify(context.Background(), "good", ""); !ok || got.Has("remoteip") {
		t.Fatalf("remoteip must be omitted: %v", got)
	}
}

func TestVerifyFailureCodes(t *testing.T) {
	srv := siteverify(t, nil)
	ok, err := newVerifier(srv.URL, true, nil).Verify(context.Background(), "bad", "")
	var rej *RejectedError
	if ok || !errors.As(err, &rej) || strings.Join(rej.Codes, ",") != "invalid-input-response,timeout-or-duplicate" {
		t.Fatalf("enforced: ok=%v err=%v", ok, err)
	}
	// Report-only: logged, request passes, err still says why.
	ok, err = newVerifier(srv.URL, false, nil).Verify(context.Background(), "bad", "")
	if !ok || !errors.As(err, &rej) {
		t.Fatalf("not enforced: ok=%v err=%v", ok, err)
	}
}

func TestVerifyEmptyOrOversizedToken(t *testing.T) {
	var calls atomic.Int32
	srv := httptest.NewServer(http.HandlerFunc(func(http.ResponseWriter, *http.Request) { calls.Add(1) }))
	defer srv.Close()
	for _, tok := range []string{"", "   "} {
		if ok, err := newVerifier(srv.URL, true, nil).Verify(context.Background(), tok, ""); ok || !errors.Is(err, ErrMissingToken) {
			t.Fatalf("enforced %q: ok=%v err=%v", tok, ok, err)
		}
		if ok, err := newVerifier(srv.URL, false, nil).Verify(context.Background(), tok, ""); !ok || !errors.Is(err, ErrMissingToken) {
			t.Fatalf("not enforced %q: ok=%v err=%v", tok, ok, err)
		}
	}
	if ok, err := newVerifier(srv.URL, true, nil).Verify(context.Background(), strings.Repeat("a", 2049), ""); ok || !errors.Is(err, ErrInvalidToken) {
		t.Fatalf("oversized: ok=%v err=%v", ok, err)
	}
	if calls.Load() != 0 {
		t.Fatal("siteverify must not be called without a usable token")
	}
}

func TestVerifyUnavailable(t *testing.T) {
	slow := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		select {
		case <-r.Context().Done():
		case <-time.After(time.Second):
		}
	}))
	defer slow.Close()
	broken := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		http.Error(w, "oops", http.StatusInternalServerError)
	}))
	defer broken.Close()
	garbage := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		_, _ = w.Write([]byte("<html>"))
	}))
	defer garbage.Close()
	closed := httptest.NewServer(http.NotFoundHandler())
	closedURL := closed.URL
	closed.Close()

	cases := map[string]struct {
		url    string
		client *http.Client
	}{
		"timeout":     {slow.URL, &http.Client{Timeout: 50 * time.Millisecond}},
		"status 500":  {broken.URL, nil},
		"bad json":    {garbage.URL, nil},
		"unreachable": {closedURL, nil},
	}
	for name, c := range cases {
		ok, err := newVerifier(c.url, true, c.client).Verify(context.Background(), "good", "")
		if ok || !errors.Is(err, ErrUnavailable) {
			t.Fatalf("%s enforced: ok=%v err=%v", name, ok, err)
		}
		ok, err = newVerifier(c.url, false, c.client).Verify(context.Background(), "good", "")
		if !ok || !errors.Is(err, ErrUnavailable) {
			t.Fatalf("%s not enforced must fail open: ok=%v err=%v", name, ok, err)
		}
	}
}

func TestVerifyHonorsContextDeadline(t *testing.T) {
	slow := httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		select {
		case <-r.Context().Done():
		case <-time.After(time.Second):
		}
	}))
	defer slow.Close()
	ctx, cancel := context.WithTimeout(context.Background(), 50*time.Millisecond)
	defer cancel()
	start := time.Now()
	ok, err := newVerifier(slow.URL, true, nil).Verify(ctx, "good", "")
	if ok || !errors.Is(err, ErrUnavailable) || time.Since(start) > Timeout {
		t.Fatalf("ok=%v err=%v after %s", ok, err, time.Since(start))
	}
}

func TestToken(t *testing.T) {
	r := httptest.NewRequest(http.MethodPost, "/", strings.NewReader(FormField+"=form-tok"))
	r.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	r.Header.Set(Header, "header-tok")
	if err := r.ParseForm(); err != nil {
		t.Fatal(err)
	}
	if got := Token(r, " body-tok "); got != "body-tok" {
		t.Fatal(got)
	}
	if got := Token(r, ""); got != "form-tok" {
		t.Fatal(got)
	}
	r = httptest.NewRequest(http.MethodPost, "/", nil)
	r.Header.Set("x-captcha-response", "header-tok")
	if got := Token(r, ""); got != "header-tok" {
		t.Fatal(got)
	}
}

func TestCheckUsesClientIPAndPolicy(t *testing.T) {
	var got url.Values
	v := newVerifier(siteverify(t, &got).URL, true, nil)
	r := httptest.NewRequest(http.MethodPost, "/", strings.NewReader(FormField+"=good"))
	r.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	_ = r.ParseForm()
	if !v.CheckForm(r) {
		t.Fatal("good token must pass")
	}
	if got.Get("response") != "good" {
		t.Fatalf("form = %v", got)
	}
	r = httptest.NewRequest(http.MethodPost, "/", nil)
	if v.CheckForm(r) {
		t.Fatal("missing token must fail when enforced")
	}
}

func TestConfigSecretIsRedacted(t *testing.T) {
	cfg := config.CaptchaConfig{SiteKey: siteKey, Secret: secret}
	for _, s := range []string{cfg.String(), cfg.GoString()} {
		if strings.Contains(s, secret) {
			t.Fatalf("secret leaked: %s", s)
		}
	}
}
