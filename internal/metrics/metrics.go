// Package metrics: Prometheus request metrics and the bearer-protected
// /metrics endpoint (port of src/logging/metrics*.rs).
//
// Series: http_requests_total{method,route,status} and the histogram
// http_request_duration_ms{method,route,status} (ms buckets 5…5000), with
// route labels normalized by NormalizeRouteLabel. OPTIONS requests and
// /metrics scrapes are not recorded.
package metrics

import (
	"crypto/subtle"
	"net/http"
	"strconv"
	"strings"
	"time"

	"github.com/prometheus/client_golang/prometheus"
	"github.com/prometheus/client_golang/prometheus/promhttp"
)

// Path is the scrape endpoint.
const Path = "/metrics"

// Buckets are the duration histogram bounds in milliseconds.
var Buckets = []float64{5, 10, 25, 50, 100, 250, 500, 1000, 2500, 5000}

var (
	// Registry holds only the application series.
	Registry = prometheus.NewRegistry()

	requests = prometheus.NewCounterVec(prometheus.CounterOpts{
		Name: "http_requests_total",
		Help: "Total HTTP requests processed",
	}, []string{"method", "route", "status"})

	duration = prometheus.NewHistogramVec(prometheus.HistogramOpts{
		Name:    "http_request_duration_ms",
		Help:    "HTTP request duration in milliseconds",
		Buckets: Buckets,
	}, []string{"method", "route", "status"})
)

func init() {
	Registry.MustRegister(requests, duration)
}

// RecordRequest adds one request to the series.
func RecordRequest(method, route string, status int, d time.Duration) {
	labels := prometheus.Labels{"method": method, "route": route, "status": strconv.Itoa(status)}
	requests.With(labels).Inc()
	duration.With(labels).Observe(float64(d) / float64(time.Millisecond))
}

type statusWriter struct {
	http.ResponseWriter
	status int
}

func (s *statusWriter) WriteHeader(code int) {
	if s.status == 0 {
		s.status = code
	}
	s.ResponseWriter.WriteHeader(code)
}

func (s *statusWriter) Write(b []byte) (int, error) {
	if s.status == 0 {
		s.status = http.StatusOK
	}
	return s.ResponseWriter.Write(b)
}

// Unwrap lets http.ResponseController reach the underlying writer.
func (s *statusWriter) Unwrap() http.ResponseWriter { return s.ResponseWriter }

// Middleware records per-request metrics.
func Middleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if r.Method == http.MethodOptions || r.URL.Path == Path {
			next.ServeHTTP(w, r)
			return
		}
		start := time.Now()
		sw := &statusWriter{ResponseWriter: w}
		defer func() {
			status := sw.status
			if status == 0 {
				status = http.StatusOK
			}
			if rec := recover(); rec != nil {
				RecordRequest(r.Method, NormalizeRouteLabel(r.URL.Path), http.StatusInternalServerError, time.Since(start))
				panic(rec)
			}
			RecordRequest(r.Method, NormalizeRouteLabel(r.URL.Path), status, time.Since(start))
		}()
		next.ServeHTTP(sw, r)
	})
}

// PresentedBearerToken extracts the token of "Bearer <token>" ("" when absent).
func PresentedBearerToken(authorization string) (string, bool) {
	rest, ok := strings.CutPrefix(authorization, "Bearer ")
	if !ok {
		return "", false
	}
	return strings.TrimSpace(rest), true
}

// RequestIsAuthorized: an unset (empty) token means open metrics, except
// in production where it means closed — /metrics profiles traffic per
// route and must never be public (boot also requires METRICS_TOKEN).
func RequestIsAuthorized(authorization, token string, production bool) bool {
	if token == "" {
		return !production
	}
	presented, ok := PresentedBearerToken(authorization)
	return ok && subtle.ConstantTimeCompare([]byte(presented), []byte(token)) == 1
}

// Handler serves GET /metrics behind the bearer token.
func Handler(token string, production bool) http.Handler {
	h := promhttp.HandlerFor(Registry, promhttp.HandlerOpts{})
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		if !RequestIsAuthorized(r.Header.Get("Authorization"), token, production) {
			http.Error(w, "unauthorized", http.StatusUnauthorized)
			return
		}
		h.ServeHTTP(w, r)
	})
}
