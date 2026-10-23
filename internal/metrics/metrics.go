// Package metrics: Prometheus metrics and the bearer-protected /metrics
// endpoint.
//
// TODO(wave1-B): prometheus/client_golang registry, request counters /
// histograms with route labels (see src/logging/metrics.rs and
// route_label.rs), METRICS_TOKEN auth, /metrics handler mounted by
// internal/web/platform.
package metrics

import "net/http"

// Middleware records per-request metrics. Wave 0 no-op.
func Middleware(next http.Handler) http.Handler { return next } // TODO(wave1-B)
