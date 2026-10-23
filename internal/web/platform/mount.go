// Package platform: operational endpoints — /api/health, /api/readyz,
// /metrics, /robots.txt, /sitemap.xml, /api/openapi.json + docs (dev),
// the Mercado Pago webhook.
//
// Owner: wave0 (health, readyz) and wave1-B (the rest). The OpenAPI
// document is hand-maintained in openapi.json (generated once from the Rust
// utoipa spec, plus the webhook): update it with every /api route change.
package platform

import (
	"context"
	"encoding/json"
	"net/http"
	"time"

	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/metrics"
	"github.com/wellington-vell/lindaflor/internal/payments"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/storage"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// Mount registers the platform routes.
func Mount(r chi.Router, d *app.Deps) {
	r.Get("/api/health", Health)
	r.Get("/api/readyz", Readyz(d))

	r.Method(http.MethodGet, metrics.Path, metrics.Handler(d.Cfg.MetricsToken, d.Cfg.AppEnv == "production"))

	r.Get(seo.RobotsPath, seo.RobotsHandler(d.Cfg.AppOrigin))
	r.Get(seo.SitemapPath, seo.SitemapHandler(d.Cfg.AppOrigin, d.Q, &seo.SitemapCache{}))

	if config.IsDevEnv(d.Cfg.AppEnv) {
		r.Get("/api/dev/version", DevVersion)
	}

	notFound := func(w http.ResponseWriter, r *http.Request) { render.NotFound(w, r, d) }
	docs := func(h http.HandlerFunc) http.HandlerFunc {
		return func(w http.ResponseWriter, r *http.Request) {
			if !d.Cfg.OpenAPIDocsEnabled {
				notFound(w, r)
				return
			}
			h(w, r)
		}
	}
	r.Get("/api/openapi.json", docs(OpenAPIJSON))
	r.Get("/api/docs", docs(ScalarDocs))
	r.Get("/api/swagger", docs(SwaggerDocs))

	svc := &payments.Service{Pool: d.Pool, Provider: d.Payments, Orders: d.Orders}
	r.With(ratelimit.Limit(ratelimit.ScopeMPWebhook, ratelimit.DefaultMPWebhookPerMin)).
		Post(payments.WebhookPath, svc.WebhookHandler(notFound))
}

// DevVersionResponse is the live-reload build id.
type DevVersionResponse struct {
	Version string `json:"version"`
}

// DevVersion reports the running process build id (development only). The
// browser reloads when it changes, i.e. after an air rebuild.
func DevVersion(w http.ResponseWriter, _ *http.Request) {
	w.Header().Set("Cache-Control", "no-store")
	writeJSON(w, http.StatusOK, DevVersionResponse{Version: app.BuildID})
}

// HealthResponse is the liveness payload.
type HealthResponse struct {
	Status string `json:"status"`
}

// Health is the cheap liveness probe (no backend I/O).
func Health(w http.ResponseWriter, _ *http.Request) {
	writeJSON(w, http.StatusOK, HealthResponse{Status: "ok"})
}

// ReadyResponse reports each backend: ok | degraded | skipped.
type ReadyResponse struct {
	Status   string `json:"status"`
	Database string `json:"database"`
	Valkey   string `json:"valkey"`
	Storage  string `json:"storage"`
}

const readyTimeout = 2 * time.Second

// Readyz checks Postgres (SELECT 1), Valkey (PING) and S3 (HEAD bucket),
// each bounded by 2s: 200 when every configured backend passes, 503 when
// any is degraded. Unconfigured backends report "skipped".
func Readyz(d *app.Deps) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		ctx := r.Context()
		resp := ReadyResponse{
			Database: check(ctx, d.Q != nil, func(ctx context.Context) error {
				_, err := d.Q.Ping(ctx)
				return err
			}),
			Valkey: check(ctx, d.Valkey != nil && d.Valkey.Redis() != nil, func(ctx context.Context) error {
				return d.Valkey.Ping(ctx)
			}),
			Storage: check(ctx, !storage.IsUnavailable(d.Storage), func(ctx context.Context) error {
				return d.Storage.HealthCheck(ctx)
			}),
		}
		status := http.StatusOK
		resp.Status = "ok"
		if resp.Database == "degraded" || resp.Valkey == "degraded" || resp.Storage == "degraded" {
			status = http.StatusServiceUnavailable
			resp.Status = "degraded"
		}
		writeJSON(w, status, resp)
	}
}

// check runs probe with a 2s budget: ok | degraded | skipped (disabled).
func check(ctx context.Context, enabled bool, probe func(ctx context.Context) error) string {
	if !enabled {
		return "skipped"
	}
	ctx, cancel := context.WithTimeout(ctx, readyTimeout)
	defer cancel()
	if err := probe(ctx); err != nil {
		return "degraded"
	}
	return "ok"
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(v)
}
