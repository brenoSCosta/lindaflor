package httpx

import (
	"log/slog"
	"net/http"
	"runtime/debug"
	"strings"
)

// CSPProduction is the enforcing Content-Security-Policy. Every script is
// self-hosted under /static (htmx, Alpine), so no third-party origin is
// listed. 'unsafe-eval' is required by Alpine's standard build and htmx
// hx-on/trigger filters (both compile expressions with new Function).
// img-src allows http: so local object storage (RustFS on :4203) can serve
// images; production S3 must be https (mixed content is blocked anyway).
const CSPProduction = "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval'; style-src 'self' 'unsafe-inline'; img-src 'self' data: blob: http: https:; font-src 'self' data:; connect-src 'self' ws: wss:; frame-ancestors 'none'; base-uri 'self'; form-action 'self'"

// CSPDevelopment additionally allows any local port (live-reload tools)
// and the CDNs used by the dev-only API docs (Scalar/Swagger).
const CSPDevelopment = "default-src 'self'; script-src 'self' 'unsafe-inline' 'unsafe-eval' http://127.0.0.1:* http://localhost:* https://cdn.jsdelivr.net https://unpkg.com; style-src 'self' 'unsafe-inline' http://127.0.0.1:* http://localhost:* https://cdn.jsdelivr.net https://unpkg.com; img-src 'self' data: blob: http: https:; font-src 'self' data: https://cdn.jsdelivr.net https://unpkg.com; connect-src 'self' ws: wss: http://127.0.0.1:* http://localhost:*; frame-ancestors 'none'; base-uri 'self'; form-action 'self'"

// HSTSValue is the production Strict-Transport-Security (two years).
const HSTSValue = "max-age=63072000; includeSubDomains"

// DevCSPForEnv reports the local/test envs that get the dev CSP. Unknown
// envs (staging, preview, typos) stay on the production CSP (fail closed).
func DevCSPForEnv(env string) bool {
	switch env {
	case "development", "dev", "test":
		return true
	}
	return false
}

// HSTSEnabledForEnv: HSTS everywhere except local/test environments.
func HSTSEnabledForEnv(env string) bool { return !DevCSPForEnv(env) }

// CSPForEnv picks the CSP value for an APP_ENV.
func CSPForEnv(env string) string {
	if DevCSPForEnv(env) {
		return CSPDevelopment
	}
	return CSPProduction
}

// SecurityHeaders stamps X-Content-Type-Options, Referrer-Policy, CSP and
// (outside dev/test) HSTS on every response, before the handler runs, so
// errors, 404s and 429s carry them too.
func SecurityHeaders(appEnv string) func(http.Handler) http.Handler {
	csp := CSPForEnv(appEnv)
	hsts := HSTSEnabledForEnv(appEnv)
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			h := w.Header()
			h.Set("X-Content-Type-Options", "nosniff")
			h.Set("Referrer-Policy", "strict-origin-when-cross-origin")
			h.Set("Content-Security-Policy", csp)
			if hsts {
				h.Set("Strict-Transport-Security", HSTSValue)
			}
			next.ServeHTTP(w, r)
		})
	}
}

// Recover turns a panic into a logged 500 (http.ErrAbortHandler is re-raised).
func Recover(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		defer func() { //nolint:contextcheck // logs with the request context
			rec := recover()
			if rec == nil {
				return
			}
			if rec == http.ErrAbortHandler { //nolint:errorlint // sentinel comparison by design
				panic(rec)
			}
			slog.ErrorContext(r.Context(), "panic serving request",
				"panic", rec, "method", r.Method, "path", r.URL.Path,
				"request_id", RequestIDFrom(r.Context()).String(),
				"stack", string(debug.Stack()))
			http.Error(w, "Erro interno do servidor.", http.StatusInternalServerError)
		}()
		next.ServeHTTP(w, r)
	})
}

// OriginExemptPaths skip the cross-origin check: the Mercado Pago webhook
// (cookie-less, HMAC-signed server-to-server POST) and the newsletter
// one-click unsubscribe (RFC 8058 POSTs from mail providers; the signed
// token is the authorization).
var OriginExemptPaths = []string{"/api/webhooks/mercadopago", "/newsletter/cancelar"}

// OriginProtection rejects cross-origin unsafe requests (net/http
// CrossOriginProtection: Sec-Fetch-Site, else Origin vs Host) with 403.
// appOrigin, when set, is trusted explicitly.
func OriginProtection(appOrigin string) (func(http.Handler) http.Handler, error) {
	cop := http.NewCrossOriginProtection()
	if appOrigin != "" {
		if err := cop.AddTrustedOrigin(appOrigin); err != nil {
			return nil, err
		}
	}
	for _, p := range OriginExemptPaths {
		cop.AddInsecureBypassPattern(p)
		cop.AddInsecureBypassPattern(strings.TrimSuffix(p, "/") + "/{rest...}")
	}
	cop.SetDenyHandler(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) {
		http.Error(w, "Origem não permitida.", http.StatusForbidden)
	}))
	return cop.Handler, nil
}
