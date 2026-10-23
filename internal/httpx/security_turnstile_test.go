package httpx

import (
	"net/http"
	"net/http/httptest"
	"strings"
	"testing"
)

func TestCSPWithoutTurnstileIsUnchanged(t *testing.T) {
	for _, env := range []string{"production", "staging", "development", "test"} {
		if CSPFor(env, false) != CSPForEnv(env) {
			t.Fatalf("%s: CSPFor(env, false) must equal CSPForEnv", env)
		}
	}
	for _, csp := range []string{CSPProduction, CSPDevelopment} {
		if strings.Contains(csp, "challenges.cloudflare.com") || strings.Contains(csp, "frame-src") {
			t.Fatalf("base CSP must not mention Turnstile: %s", csp)
		}
	}
}

func TestCSPTurnstileVariant(t *testing.T) {
	cases := map[string]struct{ base, turnstile string }{
		"production":  {CSPProduction, CSPProductionTurnstile},
		"staging":     {CSPProduction, CSPProductionTurnstile},
		"development": {CSPDevelopment, CSPDevelopmentTurnstile},
		"test":        {CSPDevelopment, CSPDevelopmentTurnstile},
	}
	for env, c := range cases {
		csp := CSPFor(env, true)
		if csp != c.turnstile {
			t.Fatalf("%s: wrong Turnstile CSP", env)
		}
		dirs := directives(csp)
		for _, d := range []string{"script-src", "connect-src", "frame-src"} {
			if !strings.Contains(dirs[d], TurnstileOrigin) {
				t.Fatalf("%s: %s must allow %s: %q", env, d, TurnstileOrigin, dirs[d])
			}
		}
		// Every other directive is byte-identical to the base CSP, and the
		// Turnstile directives only gain the Cloudflare origin.
		base := directives(c.base)
		for name, v := range base {
			want := v
			if name == "script-src" || name == "connect-src" {
				want = v + " " + TurnstileOrigin
			}
			if dirs[name] != want {
				t.Fatalf("%s: %s = %q, want %q", env, name, dirs[name], want)
			}
		}
		if dirs["frame-src"] != "'self' "+TurnstileOrigin || len(dirs) != len(base)+1 {
			t.Fatalf("%s: frame-src = %q (%d directives)", env, dirs["frame-src"], len(dirs))
		}
		if strings.Contains(csp, "unpkg.com") != strings.Contains(c.base, "unpkg.com") {
			t.Fatalf("%s: CDN allowances must not change", env)
		}
	}
}

func TestSecurityHeadersWithTurnstile(t *testing.T) {
	for env, want := range map[string]string{"production": CSPProductionTurnstile, "development": CSPDevelopmentTurnstile} {
		rec := httptest.NewRecorder()
		SecurityHeadersWith(env, true)(http.NotFoundHandler()).ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/", nil))
		if got := rec.Header().Get("Content-Security-Policy"); got != want {
			t.Fatalf("%s: CSP = %q", env, got)
		}
		rec = httptest.NewRecorder()
		SecurityHeadersWith(env, false)(http.NotFoundHandler()).ServeHTTP(rec, httptest.NewRequest(http.MethodGet, "/", nil))
		if got := rec.Header().Get("Content-Security-Policy"); got != CSPForEnv(env) {
			t.Fatalf("%s: CSP without Turnstile = %q", env, got)
		}
	}
}

// directives splits a CSP into name → value.
func directives(csp string) map[string]string {
	out := map[string]string{}
	for part := range strings.SplitSeq(csp, ";") {
		name, value, _ := strings.Cut(strings.TrimSpace(part), " ")
		out[name] = value
	}
	return out
}
