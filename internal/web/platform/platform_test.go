package platform_test

import (
	"context"
	"encoding/json"
	"net/http"
	"net/url"
	"strings"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/testutil"
	"github.com/wellington-vell/lindaflor/internal/web/platform"
)

func TestMain(m *testing.M) { testutil.Main(m) }

// --- health (tests/health.rs) ---

func TestHealthCheckReturnsOK(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	res := s.Client(t).Get("/api/health")
	if res.StatusCode != http.StatusOK || res.JSON(t)["status"] != "ok" {
		t.Fatal(res.StatusCode, res.Body)
	}
}

func TestReadyzReportsReadyWhenBackendsHealthy(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	res := s.Client(t).Get("/api/readyz")
	j := res.JSON(t)
	if res.StatusCode != http.StatusOK || j["status"] != "ok" || j["database"] != "ok" || j["storage"] != "ok" {
		t.Fatal(res.StatusCode, j)
	}
}

func TestReadyzReturns503WhenDatabaseDegraded(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	s.Pool.Close()
	res := s.Client(t).Get("/api/readyz")
	j := res.JSON(t)
	if res.StatusCode != http.StatusServiceUnavailable || j["status"] != "degraded" || j["database"] != "degraded" {
		t.Fatal(res.StatusCode, j)
	}
}

func TestReadyzSkipsUnconfiguredStorage(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{MutateDeps: func(d *app.Deps) { d.Storage = nil }})
	if j := s.Client(t).Get("/api/readyz").JSON(t); j["storage"] != "skipped" || j["status"] != "ok" {
		t.Fatal(j)
	}
}

// --- robots / sitemap (tests/seo.rs) ---

func TestRobotsTxtDisallowsPrivateAreasAndLinksSitemap(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	res := s.Client(t).Get("/robots.txt")
	if res.StatusCode != http.StatusOK || !strings.HasPrefix(res.Header.Get("Content-Type"), "text/plain") || !strings.HasPrefix(res.Body, "User-agent: *\n") {
		t.Fatal(res.StatusCode, res.Body)
	}
	for _, p := range []string{"/admin", "/conta", "/checkout", "/carrinho", "/pedido", "/api", "/settings"} {
		if !strings.Contains(res.Body, "Disallow: "+p+"\n") {
			t.Fatal(p)
		}
	}
	host := strings.TrimPrefix(s.URL, "http://")
	if !strings.Contains(res.Body, "Sitemap: http://"+host+"/sitemap.xml") {
		t.Fatal(res.Body)
	}
}

func TestSitemapListsActiveProductsAndSetsOnly(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{Prod: true})
	ctx := context.Background()
	tag := strings.ReplaceAll(uuid.NewString(), "-", "")
	active, inactive := "seo-ativo-"+tag, "seo-inativo-"+tag
	for slug, on := range map[string]bool{active: true, inactive: false} {
		if _, err := s.Pool.Exec(ctx, `INSERT INTO products (id, name, slug, price_in_cents, category, active, featured, created_at, updated_at)
			VALUES ($1, 'Maiô SEO', $2, 12990, 'maio', $3, false, '2026-10-01 12:00:00', '2026-10-20 08:30:00')`, uuid.New(), slug, on); err != nil {
			t.Fatal(err)
		}
	}
	activeSet, inactiveSet := "seo-set-ativo-"+tag, "seo-set-inativo-"+tag
	for slug, on := range map[string]bool{activeSet: true, inactiveSet: false} {
		if _, err := s.Pool.Exec(ctx, `INSERT INTO sets (id, name, slug, active, updated_at)
			VALUES ($1, 'Conjunto SEO', $2, $3, '2026-10-21 10:00:00+00')`, uuid.New(), slug, on); err != nil {
			t.Fatal(err)
		}
	}
	res := s.Client(t).Get("/sitemap.xml")
	if res.StatusCode != http.StatusOK || !strings.HasPrefix(res.Header.Get("Content-Type"), "application/xml") || !strings.HasPrefix(res.Body, "<?xml") {
		t.Fatal(res.StatusCode, res.Body)
	}
	origin := testutil.ProdAppOrigin
	for _, loc := range []string{"/", "/produtos", "/sets", "/termos", "/politica-privacidade", "/trocas-devolucoes", "/guia-de-medidas", "/produtos?category=maio"} {
		if !strings.Contains(res.Body, "<loc>"+origin+loc+"</loc>") {
			t.Fatal(loc, res.Body)
		}
	}
	for _, want := range []string{
		"<loc>" + origin + "/produtos/" + active + "</loc><lastmod>2026-10-20</lastmod>",
		"<loc>" + origin + "/sets/" + activeSet + "</loc><lastmod>2026-10-21</lastmod>",
	} {
		if !strings.Contains(res.Body, want) {
			t.Fatal(want, res.Body)
		}
	}
	for _, no := range []string{inactive, inactiveSet, "/admin"} {
		if strings.Contains(res.Body, no) {
			t.Fatal(no)
		}
	}
}

// --- metrics ---

func TestMetricsOpenInDevAndBearerInProduction(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	c := s.Client(t)
	c.Get("/termos")
	res := c.Get("/metrics")
	if res.StatusCode != http.StatusOK || !strings.Contains(res.Header.Get("Content-Type"), "0.0.4") || !strings.Contains(res.Body, "http_requests_total") {
		t.Fatal(res.StatusCode, res.Body)
	}
	if strings.Contains(res.Body, `route="/metrics"`) {
		t.Fatal("scrapes must not be recorded")
	}

	p := testutil.NewServer(t, testutil.Options{Prod: true})
	pc := p.Client(t)
	if res := pc.Get("/metrics"); res.StatusCode != http.StatusUnauthorized {
		t.Fatal(res.StatusCode)
	}
	if res := pc.Do(http.MethodGet, "/metrics", nil, http.Header{"Authorization": {"Bearer test-metrics-token"}}); res.StatusCode != http.StatusOK {
		t.Fatal(res.StatusCode)
	}
}

// --- OpenAPI ---

func TestOpenAPIDocsAreDevOnly(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	c := s.Client(t)
	for _, p := range []string{"/api/openapi.json", "/api/docs", "/api/swagger"} {
		if res := c.Get(p); res.StatusCode != http.StatusNotFound {
			t.Fatal(p, res.StatusCode)
		}
	}
	dev := testutil.NewServer(t, testutil.Options{Mutate: func(c *config.Config) { c.OpenAPIDocsEnabled = true }})
	dc := dev.Client(t)
	res := dc.Get("/api/openapi.json")
	if res.StatusCode != http.StatusOK || res.Header.Get("Content-Type") != "application/json" {
		t.Fatal(res.StatusCode)
	}
	var spec struct {
		OpenAPI string                    `json:"openapi"`
		Info    map[string]any            `json:"info"`
		Paths   map[string]map[string]any `json:"paths"`
	}
	if err := json.Unmarshal([]byte(res.Body), &spec); err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(spec.OpenAPI, "3.") || spec.Info["title"] != "Linda Flor API" {
		t.Fatal(spec.OpenAPI, spec.Info)
	}
	for path, method := range map[string]string{
		"/api/health": "get", "/api/readyz": "get", "/api/auth/sign-in/email": "post", "/api/auth/sign-up/email": "post",
		"/api/auth/get-session": "get", "/api/auth/two-factor/verify-totp": "post", "/api/auth/admin/list-users": "get",
		"/api/webhooks/mercadopago": "post",
	} {
		if _, ok := spec.Paths[path][method]; !ok {
			t.Errorf("spec misses %s %s", method, path)
		}
	}
	if res := dc.Get("/api/docs"); res.StatusCode != http.StatusOK || !strings.Contains(res.Body, "Scalar.createApiReference") || !strings.Contains(res.Body, `url: "/api/openapi.json"`) {
		t.Fatal(res.StatusCode)
	}
	if res := dc.Get("/api/swagger"); res.StatusCode != http.StatusOK || !strings.Contains(res.Body, "SwaggerUIBundle") {
		t.Fatal(res.StatusCode)
	}
	if !json.Valid(platform.OpenAPISpec()) {
		t.Fatal("embedded spec must be valid JSON")
	}
}

// --- MP webhook ---

func TestStaticPixProviderHasNoWebhook(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	res := s.Client(t).Do(http.MethodPost, "/api/webhooks/mercadopago?data.id=1&type=payment", strings.NewReader(`{}`),
		http.Header{"Content-Type": {"application/json"}, "Origin": {"https://www.mercadopago.com.br"}, "Sec-Fetch-Site": {"cross-site"}})
	if res.StatusCode != http.StatusNotFound {
		t.Fatal(res.StatusCode)
	}
}

// --- security headers (tests/security_headers.rs) ---

func assertHeaders(t *testing.T, res *testutil.Response, csp string, hsts bool) {
	t.Helper()
	if res.Header.Get("X-Content-Type-Options") != "nosniff" || res.Header.Get("Referrer-Policy") != "strict-origin-when-cross-origin" ||
		res.Header.Get("Content-Security-Policy") != csp {
		t.Fatalf("%d %v", res.StatusCode, res.Header)
	}
	if got := res.Header.Get("Strict-Transport-Security"); (got != "") != hsts {
		t.Fatalf("HSTS %q", got)
	}
}

func TestSecurityHeadersPresentOnOKAnd404(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	c := s.Client(t)
	ok := c.Get("/api/health")
	if ok.StatusCode != http.StatusOK {
		t.Fatal(ok.StatusCode)
	}
	assertHeaders(t, ok, httpx.CSPDevelopment, false)
	nf := c.Get("/no-such-route-3-4")
	if nf.StatusCode != http.StatusNotFound {
		t.Fatal(nf.StatusCode)
	}
	assertHeaders(t, nf, httpx.CSPDevelopment, false)
	prod := testutil.NewServer(t, testutil.Options{Prod: true}).Client(t).Get("/no-such-route-3-4")
	assertHeaders(t, prod, httpx.CSPProduction, true)
	if prod.Header.Get("Strict-Transport-Security") != httpx.HSTSValue {
		t.Fatal("hsts value")
	}
}

func TestSecurityHeadersPresentOn429(t *testing.T) {
	ratelimit.Reset()
	t.Setenv("RATE_LIMIT_GLOBAL_PER_MIN", "1")
	s := testutil.NewServer(t, testutil.Options{KeepRateLimits: true})
	c := s.Client(t)
	if res := c.Get("/no-such-route-429"); res.StatusCode != http.StatusNotFound {
		t.Fatal(res.StatusCode)
	}
	res := c.Get("/no-such-route-429")
	if res.StatusCode != http.StatusTooManyRequests || res.Header.Get("Retry-After") != "60" {
		t.Fatal(res.StatusCode)
	}
	assertHeaders(t, res, httpx.CSPDevelopment, false)
	ratelimit.Reset()
}

func TestCrossOriginPostRejectedSameOriginPasses(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	c := s.Client(t)
	evil := c.Do(http.MethodPost, "/api/auth/sign-up/email", strings.NewReader(`{}`),
		http.Header{"Origin": {"https://evil.test"}, "Sec-Fetch-Site": {"cross-site"}, "Content-Type": {"application/json"}})
	if evil.StatusCode != http.StatusForbidden {
		t.Fatal(evil.StatusCode)
	}
	same := c.Do(http.MethodPost, "/api/auth/sign-up/email", strings.NewReader(`{}`),
		http.Header{"Sec-Fetch-Site": {"same-origin"}, "Content-Type": {"application/json"}})
	if same.StatusCode == http.StatusForbidden {
		t.Fatal("same-origin POST must reach the router")
	}
	// The webhook and one-click unsubscribe bypass the origin check.
	if res := c.PostForm("/newsletter/cancelar", url.Values{}); res.StatusCode == http.StatusForbidden {
		t.Fatal("unsubscribe must bypass the origin check")
	}
}

func TestAppPagesEmitOnlySelfHostedScripts(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	html := s.Client(t).Get("/pagina-inexistente").Body
	var external []string
	scripts := 0
	for _, tag := range []struct{ open, attr string }{{"<script", `src="`}, {"<link", `href="`}} {
		for _, chunk := range strings.Split(html, tag.open)[1:] {
			open, _, _ := strings.Cut(chunk, ">")
			if tag.open == "<link" && !strings.Contains(open, "stylesheet") {
				continue
			}
			_, rest, ok := strings.Cut(open, tag.attr)
			if !ok {
				continue
			}
			u, _, _ := strings.Cut(rest, `"`)
			if tag.open == "<script" {
				scripts++
			}
			if !strings.HasPrefix(u, "/") || strings.HasPrefix(u, "//") {
				external = append(external, u)
			}
		}
	}
	if scripts == 0 || len(external) > 0 {
		t.Fatal(scripts, external)
	}
	for _, cdn := range []string{"unpkg.com", "jsdelivr.net"} {
		if strings.Contains(httpx.CSPProduction, cdn) {
			t.Fatal(cdn)
		}
	}
	if !strings.Contains(httpx.CSPProduction, "'unsafe-eval'") {
		t.Fatal("htmx/Alpine need 'unsafe-eval'")
	}
}
