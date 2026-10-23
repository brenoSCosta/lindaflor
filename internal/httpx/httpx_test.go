package httpx

import (
	"io"
	"net/http"
	"net/http/httptest"
	"net/netip"
	"strings"
	"testing"
)

func TestSecurityHeaderValuesMatchSpec(t *testing.T) {
	if !strings.Contains(CSPProduction, "script-src 'self' 'unsafe-inline' 'unsafe-eval';") ||
		!strings.Contains(CSPProduction, "img-src 'self' data: blob: http: https:") {
		t.Fatal("prod CSP")
	}
	for _, cdn := range []string{"unpkg.com", "jsdelivr.net"} {
		if strings.Contains(CSPProduction, cdn) {
			t.Fatalf("prod CSP must not allow %s", cdn)
		}
	}
	if HSTSValue != "max-age=63072000; includeSubDomains" {
		t.Fatal("hsts")
	}
}

func TestDevCSPAllowsLocalPortsAndDocsCDNs(t *testing.T) {
	for _, env := range []string{"development", "dev", "test"} {
		csp := CSPForEnv(env)
		if !DevCSPForEnv(env) || csp != CSPDevelopment {
			t.Fatalf("%s must use dev CSP", env)
		}
		for _, s := range []string{"http://127.0.0.1:*", "http://localhost:*", "https://cdn.jsdelivr.net", "https://unpkg.com", "font-src 'self' data: https://cdn.jsdelivr.net"} {
			if !strings.Contains(csp, s) {
				t.Fatalf("%s CSP missing %s", env, s)
			}
		}
	}
}

func TestCSPAndHSTSGatesFailClosed(t *testing.T) {
	for _, env := range []string{"production", "staging", "prod", "preview"} {
		if DevCSPForEnv(env) || CSPForEnv(env) != CSPProduction || !HSTSEnabledForEnv(env) {
			t.Fatalf("%s must be production-like", env)
		}
	}
	for _, env := range []string{"development", "dev", "test"} {
		if HSTSEnabledForEnv(env) {
			t.Fatalf("%s must skip HSTS", env)
		}
	}
}

func TestSecurityHeadersMiddlewareAppliesOnErrors(t *testing.T) {
	h := SecurityHeaders("production")(http.NotFoundHandler())
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/nope", nil))
	for k, want := range map[string]string{
		"X-Content-Type-Options": "nosniff", "Referrer-Policy": "strict-origin-when-cross-origin",
		"Content-Security-Policy": CSPProduction, "Strict-Transport-Security": HSTSValue,
	} {
		if got := rec.Header().Get(k); got != want {
			t.Fatalf("%s = %q", k, got)
		}
	}
	rec = httptest.NewRecorder()
	SecurityHeaders("development")(http.NotFoundHandler()).ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/", nil))
	if rec.Header().Get("Strict-Transport-Security") != "" {
		t.Fatal("no HSTS in dev")
	}
}

func ip(s string) netip.Addr { return netip.MustParseAddr(s) }

func resolve(t *testing.T, tp TrustedProxies, remote string, xff ...string) string {
	t.Helper()
	var r netip.Addr
	ok := remote != ""
	if ok {
		r = ip(remote)
	}
	a, aok := tp.ClientIP(r, ok, xff)
	if !aok {
		return "<none>"
	}
	return a.String()
}

func TestTrustedProxiesResolution(t *testing.T) {
	if got := resolve(t, TrustedProxies{}, "203.0.113.9", "198.51.100.1"); got != "203.0.113.9" {
		t.Fatalf("no trust: %s", got)
	}
	n1 := TrustedProxies{Nearest: 1}
	if got := resolve(t, n1, "203.0.113.9", "198.51.100.1"); got != "198.51.100.1" {
		t.Fatalf("nearest 1: %s", got)
	}
	if got := resolve(t, n1, "", "198.51.100.1"); got != "198.51.100.1" {
		t.Fatalf("unix peer: %s", got)
	}
	n2 := TrustedProxies{Nearest: 2}
	if got := resolve(t, n2, "203.0.113.9", "1.1.1.1, 198.51.100.1, 192.0.2.7"); got != "198.51.100.1" {
		t.Fatalf("nearest 2: %s", got)
	}
	for _, v := range []string{"198.51.100.1, unknown", "198.51.100.1, _lb1"} {
		if got := resolve(t, n2, "203.0.113.9", v); got != "198.51.100.1" {
			t.Fatalf("positional hop: %s", got)
		}
	}
	if got := resolve(t, n2, "203.0.113.9", "unknown"); got != "<none>" {
		t.Fatalf("unreadable client: %s", got)
	}
	combo, err := NewTrustedProxies(1, []string{"192.0.2.0/24"})
	if err != nil {
		t.Fatal(err)
	}
	if got := resolve(t, combo, "203.0.113.9", "198.51.100.1, 192.0.2.7"); got != "198.51.100.1" {
		t.Fatalf("combine: %s", got)
	}
	nets, _ := NewTrustedProxies(0, []string{"10.0.0.0/8"})
	if got := resolve(t, nets, "10.0.0.1", "198.51.100.1", "10.0.0.5, 10.0.0.3"); got != "198.51.100.1" {
		t.Fatalf("multi-line: %s", got)
	}
	if got := resolve(t, n1, "203.0.113.9", "[2001:db8::1]:443"); got != "2001:db8::1" {
		t.Fatalf("brackets/ports: %s", got)
	}
	if _, err := NewTrustedProxies(0, []string{"proxy.internal"}); err == nil {
		t.Fatal("invalid network must error")
	}
}

func TestBodyLimitOverride(t *testing.T) {
	read := func(h http.Handler, n int) int {
		rec := httptest.NewRecorder()
		req := httptest.NewRequest(http.MethodPost, "/", strings.NewReader(strings.Repeat("a", n)))
		h.ServeHTTP(rec, req)
		return rec.Code
	}
	handler := http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if _, err := io.ReadAll(r.Body); IsBodyTooLarge(err) {
			TooLarge(w)
			return
		}
		w.WriteHeader(http.StatusNoContent)
	})
	global := GlobalBodyLimit(10)
	if read(global(handler), 10) != http.StatusNoContent || read(global(handler), 11) != http.StatusRequestEntityTooLarge {
		t.Fatal("global limit")
	}
	raised := global(BodyLimit(100)(handler))
	if read(raised, 50) != http.StatusNoContent || read(raised, 101) != http.StatusRequestEntityTooLarge {
		t.Fatal("override")
	}
}

func TestOriginProtection(t *testing.T) {
	mw, err := OriginProtection("https://loja.example.com")
	if err != nil {
		t.Fatal(err)
	}
	h := mw(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(http.StatusNoContent) }))
	do := func(path, site, origin string) int {
		req := httptest.NewRequest(http.MethodPost, path, nil)
		if site != "" {
			req.Header.Set("Sec-Fetch-Site", site)
		}
		if origin != "" {
			req.Header.Set("Origin", origin)
		}
		rec := httptest.NewRecorder()
		h.ServeHTTP(rec, req)
		return rec.Code
	}
	if do("/carrinho", "cross-site", "https://evil.example") != http.StatusForbidden {
		t.Fatal("cross-site POST must be rejected")
	}
	if do("/carrinho", "same-origin", "") != http.StatusNoContent {
		t.Fatal("same-origin POST allowed")
	}
	if do("/carrinho", "", "https://loja.example.com") != http.StatusNoContent {
		t.Fatal("trusted origin allowed")
	}
	if do("/api/webhooks/mercadopago", "cross-site", "https://mp.example") != http.StatusNoContent {
		t.Fatal("webhook exempt")
	}
	if do("/newsletter/cancelar", "cross-site", "https://mail.example") != http.StatusNoContent {
		t.Fatal("unsubscribe exempt")
	}
}

func TestRedirectHTMXAware(t *testing.T) {
	rec := httptest.NewRecorder()
	req := httptest.NewRequest(http.MethodPost, "/x", nil)
	Redirect(rec, req, "/y")
	if rec.Code != http.StatusSeeOther || rec.Header().Get("Location") != "/y" {
		t.Fatal("plain")
	}
	rec = httptest.NewRecorder()
	req.Header.Set("HX-Request", "true")
	req.Header.Set("HX-Current-URL", "http://h/produtos?x=1")
	Redirect(rec, req, "/y")
	if rec.Code != http.StatusOK || rec.Header().Get("HX-Location") != "/y" || CurrentPath(req) != "/produtos" {
		t.Fatal("htmx")
	}
}
