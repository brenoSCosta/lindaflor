// Package platform: operational endpoints — /api/health, /api/readyz,
// /metrics, /robots.txt, /sitemap.xml, /api/openapi.json + docs (dev),
// the Mercado Pago webhook.
//
// Owner: wave0 (health, readyz) and wave1-B (the rest).
package platform

import (
	"context"
	"encoding/json"
	"net/http"
	"time"

	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/storage"
)

// Mount registers the platform routes.
func Mount(r chi.Router, d *app.Deps) {
	r.Get("/api/health", Health)
	r.Get("/api/readyz", Readyz(d))
	// TODO(wave1-B): /metrics, /robots.txt, /sitemap.xml, /api/openapi.json,
	// /api/docs, /api/swagger (dev only), POST /api/webhooks/mercadopago.
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
