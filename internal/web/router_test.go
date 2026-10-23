package web_test

import (
	"net/http"
	"net/url"
	"strings"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/testutil"
	"github.com/wellington-vell/lindaflor/web/static"
)

func TestMain(m *testing.M) { testutil.Main(m) }

func TestHealth(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	res := s.Client(t).Get("/api/health")
	if res.StatusCode != http.StatusOK || res.JSON(t)["status"] != "ok" {
		t.Fatalf("%d %s", res.StatusCode, res.Body)
	}
	if res.Header.Get("X-Request-Id") == "" {
		t.Fatal("missing request id")
	}
}

func TestReadyz(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	c := s.Client(t)
	res := c.Get("/api/readyz")
	j := res.JSON(t)
	if res.StatusCode != http.StatusOK || j["status"] != "ok" || j["database"] != "ok" || j["valkey"] != "ok" || j["storage"] != "ok" {
		t.Fatalf("%d %v", res.StatusCode, j)
	}
	s.Redis.Close()
	res = c.Get("/api/readyz")
	j = res.JSON(t)
	if res.StatusCode != http.StatusServiceUnavailable || j["valkey"] != "degraded" || j["status"] != "degraded" {
		t.Fatalf("valkey down: %d %v", res.StatusCode, j)
	}
}

func TestNotFoundIsBrandedAndNoindex(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	res := s.Client(t).Get("/nao-existe")
	if res.StatusCode != http.StatusNotFound {
		t.Fatal(res.StatusCode)
	}
	for _, want := range []string{"Página não encontrada", `<html lang="pt-BR">`, `noindex, nofollow`, "Voltar ao início", `name="htmx-config"`, static.URL("css/app.css")} {
		if !strings.Contains(res.Body, want) {
			t.Fatalf("missing %q", want)
		}
	}
	if res.Header.Get("Content-Security-Policy") != httpx.CSPDevelopment {
		t.Fatal("dev CSP on 404")
	}
}

func TestFavicon(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	c := s.Client(t)
	page := c.Get("/")
	if !strings.Contains(page.Body, `<link rel="icon" type="image/svg+xml" href="`+static.URL(static.FaviconName)+`"`) {
		t.Fatal("missing icon link")
	}
	for _, path := range []string{"/favicon.ico", static.URL(static.FaviconName)} {
		res := c.Get(path)
		if res.StatusCode != http.StatusOK || res.Header.Get("Content-Type") != "image/svg+xml" || !strings.HasPrefix(res.Body, "<svg") {
			t.Fatalf("%s: %d %q", path, res.StatusCode, res.Header.Get("Content-Type"))
		}
	}
	req, err := http.NewRequestWithContext(t.Context(), http.MethodHead, s.URL+"/favicon.ico", nil)
	if err != nil {
		t.Fatal(err)
	}
	head, err := s.Server.Client().Do(req)
	if err != nil {
		t.Fatal(err)
	}
	_ = head.Body.Close()
	if head.StatusCode != http.StatusOK {
		t.Fatalf("HEAD /favicon.ico: %d", head.StatusCode)
	}
}

func TestProdHeadersAndOriginCheck(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{Prod: true})
	c := s.Client(t)
	res := c.Get("/api/health")
	if res.Header.Get("Strict-Transport-Security") != httpx.HSTSValue || res.Header.Get("Content-Security-Policy") != httpx.CSPProduction {
		t.Fatalf("prod headers: %v", res.Header)
	}
	res = c.Do(http.MethodPost, "/carrinho", strings.NewReader(""), http.Header{"Sec-Fetch-Site": {"cross-site"}, "Origin": {"https://evil.example"}})
	if res.StatusCode != http.StatusForbidden {
		t.Fatalf("cross-site POST: %d", res.StatusCode)
	}
	res = c.PostForm("/api/webhooks/mercadopago", url.Values{})
	if res.StatusCode == http.StatusForbidden {
		t.Fatal("webhook must bypass the origin check")
	}
}

func TestStaticAssetsAreCacheBusted(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	c := s.Client(t)
	res := c.Get(static.URL("vendor/htmx.min.js"))
	if res.StatusCode != http.StatusOK || !strings.Contains(res.Header.Get("Cache-Control"), "immutable") {
		t.Fatalf("%d %v", res.StatusCode, res.Header)
	}
	res = c.Get("/static/vendor/htmx.min.js?v=stale")
	if res.Header.Get("Cache-Control") != "no-cache" {
		t.Fatal("stale hash must revalidate")
	}
	if c.Get("/static/").StatusCode != http.StatusNotFound || c.Get("/static/nope.js").StatusCode != http.StatusNotFound {
		t.Fatal("listing / missing file")
	}
}

func TestHomePlaceholderRendersStorefrontShell(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	res := s.Client(t).Get("/")
	for _, want := range []string{`id="toaster"`, `id="cart-count"`, `id="cart-sheet"`, "FRETE GRÁTIS ACIMA DE R$ 299,00", "Trocas e devoluções"} {
		if !strings.Contains(res.Body, want) {
			t.Fatalf("missing %q", want)
		}
	}
}
