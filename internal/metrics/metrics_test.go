package metrics

import (
	"io"
	"net/http"
	"net/http/httptest"
	"slices"
	"strings"
	"testing"
	"time"

	"github.com/google/uuid"
)

func TestKnownTemplatesAreSorted(t *testing.T) {
	if !slices.IsSorted(knownTemplates) {
		t.Fatal("knownTemplates must stay sorted (binary search)")
	}
}

func TestRouteLabels(t *testing.T) {
	for path, want := range map[string]string{
		"/":                       "/",
		"/api/auth/sign-in/email": AuthRouteLabel,
		"/api/auth/get-session":   AuthRouteLabel,
		"/pedido/0199c1a0-0000-7000-8000-000000000001": "/pedido/{id}",
		"/produtos/rosa-vermelha":                      "/produtos/{slug}",
		"/admin/pedidos/abc":                           UnknownRouteLabel,
		"/admin/pedidos/42":                            "/admin/pedidos/{id}",
		"/wp-admin.php":                                UnknownRouteLabel,
		"/some/unknown/deep/path":                      UnknownRouteLabel,
		"/favicon.ico":                                 UnknownRouteLabel,
		"/api/health":                                  "/api/health",
		"/login":                                       "/login",
		"/checkout":                                    "/checkout",
		"/robots.txt":                                  "/robots.txt",
		"/sitemap.xml":                                 "/sitemap.xml",
		"/sets/verao":                                  "/sets/{slug}",
	} {
		if got := NormalizeRouteLabel(path); got != want {
			t.Errorf("%s → %s, want %s", path, got, want)
		}
	}
}

func TestUnsetOrEmptyTokenIsOpenOutsideProduction(t *testing.T) {
	if !RequestIsAuthorized("", "", false) || !RequestIsAuthorized("Bearer x", "", false) {
		t.Fatal("open")
	}
}

func TestUnsetTokenIsClosedInProduction(t *testing.T) {
	if RequestIsAuthorized("", "", true) || RequestIsAuthorized("Bearer x", "", true) || RequestIsAuthorized("Bearer ", "", true) {
		t.Fatal("closed")
	}
	if !RequestIsAuthorized("Bearer secret", "secret", true) {
		t.Fatal("good bearer")
	}
}

func TestBearerChecks(t *testing.T) {
	if RequestIsAuthorized("Bearer other", "secret", false) {
		t.Fatal("wrong token")
	}
	if !RequestIsAuthorized("Bearer secret", "secret", false) || !RequestIsAuthorized("Bearer  secret  ", "secret", false) {
		t.Fatal("good bearer")
	}
	if RequestIsAuthorized("", "secret", false) || RequestIsAuthorized("secret", "secret", false) {
		t.Fatal("missing scheme")
	}
}

func scrape(t *testing.T, token, auth string, production bool) (*http.Response, string) {
	t.Helper()
	srv := httptest.NewServer(Handler(token, production))
	defer srv.Close()
	req, _ := http.NewRequest(http.MethodGet, srv.URL+Path, nil) //nolint:noctx // test
	if auth != "" {
		req.Header.Set("Authorization", auth)
	}
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	defer func() { _ = res.Body.Close() }()
	b, _ := io.ReadAll(res.Body)
	return res, string(b)
}

func TestFormatIncludesRecordedSeriesAndBuckets(t *testing.T) {
	route := "/metrics-test-" + uuid.NewString()
	RecordRequest("GET", route, 200, 12*time.Millisecond)
	RecordRequest("GET", route, 200, 3*time.Second)
	res, body := scrape(t, "", "", false)
	if res.StatusCode != http.StatusOK || !strings.Contains(res.Header.Get("Content-Type"), "0.0.4") {
		t.Fatal(res.StatusCode, res.Header.Get("Content-Type"))
	}
	base := `method="GET",route="` + route + `",status="200"`
	for _, want := range []string{
		"# TYPE http_requests_total counter",
		"http_requests_total{" + base + "} 2",
		"# TYPE http_request_duration_ms histogram",
		"http_request_duration_ms_bucket{" + base + `,le="25"} 1`,
		"http_request_duration_ms_bucket{" + base + `,le="5000"} 2`,
		"http_request_duration_ms_count{" + base + "} 2",
	} {
		if !strings.Contains(body, want) {
			t.Fatalf("missing %q in\n%s", want, body)
		}
	}
}

func TestMetricsEndpointRequiresBearer(t *testing.T) {
	if res, _ := scrape(t, "tok", "", false); res.StatusCode != http.StatusUnauthorized {
		t.Fatal(res.StatusCode)
	}
	if res, _ := scrape(t, "tok", "Bearer tok", true); res.StatusCode != http.StatusOK {
		t.Fatal(res.StatusCode)
	}
	if res, _ := scrape(t, "", "", true); res.StatusCode != http.StatusUnauthorized {
		t.Fatal("production without token is closed")
	}
}

func TestMiddlewareSkipsOptionsAndScrapes(t *testing.T) {
	h := Middleware(http.HandlerFunc(func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(http.StatusTeapot) }))
	for _, m := range []string{http.MethodOptions, http.MethodGet} {
		path := Path
		if m == http.MethodOptions {
			path = "/login"
		}
		h.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(m, path, nil))
	}
	h.ServeHTTP(httptest.NewRecorder(), httptest.NewRequest(http.MethodGet, "/termos", nil))
	_, body := scrape(t, "", "", false)
	if strings.Contains(body, `route="/metrics"`) || strings.Contains(body, `method="OPTIONS"`) {
		t.Fatal("scrapes / OPTIONS must not be recorded")
	}
	if !strings.Contains(body, `http_requests_total{method="GET",route="/termos",status="418"}`) {
		t.Fatal(body)
	}
}
