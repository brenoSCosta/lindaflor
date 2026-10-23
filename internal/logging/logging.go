// Package logging: structured logs (log/slog) and the request logging
// middleware (port of src/logging/**).
//
// One entry per request (OPTIONS and /metrics scrapes are silent) with the
// Rust field names: request_id, timestamp, method, path (no query),
// duration_ms, client_ip, user_agent (≤256 bytes), user_id / session_id /
// role (when signed in, see SetRequestUser), status, error {code,message},
// sampled, sample_reason. Errors (status ≥ 400) and slow requests
// (≥ LOG_SLOW_THRESHOLD_MS) are always kept; the rest is sampled at
// LOG_SAMPLE_RATE. Entries go to the process logger and, with
// LOG_FILE_PATH, to a size-rotated JSON-lines file.
package logging

import (
	"context"
	"io"
	"log/slog"
	"math/rand/v2"
	"net/http"
	"os"
	"strings"
	"sync"
	"time"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/httpx"
)

// MetricsPath is never logged (scrapes would drown the log).
const MetricsPath = "/metrics"

const maxUserAgentBytes = 256

// New builds the process logger (JSON to stdout; text in development).
func New(cfg *config.Config) *slog.Logger {
	var w io.Writer = os.Stdout
	opts := &slog.HandlerOptions{Level: slog.LevelInfo}
	if cfg != nil && config.IsDevEnv(cfg.AppEnv) {
		opts.Level = slog.LevelDebug
		return slog.New(slog.NewTextHandler(w, opts))
	}
	return slog.New(slog.NewJSONHandler(w, opts))
}

// SampleReason says why an entry was kept.
type SampleReason string

// Sample reasons.
const (
	SampleError   SampleReason = "error"
	SampleSlow    SampleReason = "slow"
	SampleSampled SampleReason = "sampled"
	SampleDropped SampleReason = "dropped"
)

// DecideSample keeps errors (status ≥ 400), then slow requests
// (duration ≥ slow threshold, inclusive), then a random share (rate).
func DecideSample(status int, duration, slowThreshold time.Duration, rate, random float64) SampleReason {
	switch {
	case status >= 400:
		return SampleError
	case duration >= slowThreshold:
		return SampleSlow
	case random < rate:
		return SampleSampled
	}
	return SampleDropped
}

// RequestError is the client-safe error of a failed request.
type RequestError struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

// Entry is one request log line.
type Entry struct {
	RequestID    uuid.UUID     `json:"request_id"`
	Timestamp    string        `json:"timestamp"`
	Method       string        `json:"method"`
	Path         string        `json:"path"`
	DurationMS   int64         `json:"duration_ms"`
	ClientIP     string        `json:"client_ip"`
	UserAgent    string        `json:"user_agent"`
	UserID       string        `json:"user_id,omitempty"`
	SessionID    string        `json:"session_id,omitempty"`
	Role         string        `json:"role,omitempty"`
	Status       int           `json:"status"`
	Error        *RequestError `json:"error,omitempty"`
	Sampled      bool          `json:"sampled"`
	SampleReason SampleReason  `json:"sample_reason"`
}

// RequestUser identifies the signed-in user of a request.
type RequestUser struct {
	UserID    string
	SessionID string
	Role      string
}

type requestStore struct {
	mu   sync.Mutex
	user *RequestUser
	err  *RequestError
}

type storeKey struct{}

// SetRequestUser enriches this request's log entry with the signed-in
// user (called by the auth middleware once the session resolved).
func SetRequestUser(ctx context.Context, u RequestUser) {
	if s, ok := ctx.Value(storeKey{}).(*requestStore); ok {
		s.mu.Lock()
		s.user = &u
		s.mu.Unlock()
	}
}

// SetRequestError records a client-safe error code/message for this
// request's log entry (otherwise derived from the status).
func SetRequestError(ctx context.Context, code, message string) {
	if s, ok := ctx.Value(storeKey{}).(*requestStore); ok {
		s.mu.Lock()
		s.err = &RequestError{Code: code, Message: message}
		s.mu.Unlock()
	}
}

// ErrorCodeForStatus maps a status to the log error code.
func ErrorCodeForStatus(status int) string {
	switch status {
	case 400:
		return "BAD_REQUEST"
	case 401:
		return "UNAUTHORIZED"
	case 403:
		return "FORBIDDEN"
	case 404:
		return "NOT_FOUND"
	case 405:
		return "METHOD_NOT_ALLOWED"
	case 413:
		return "CONTENT_TOO_LARGE"
	case 429:
		return "TOO_MANY_REQUESTS"
	case 503:
		return "SERVICE_UNAVAILABLE"
	}
	return "INTERNAL"
}

// clientSafeMessage never echoes internal error text (SQL, stack traces).
func clientSafeMessage(status int) string {
	if status >= 500 && status != http.StatusServiceUnavailable {
		return "internal error"
	}
	return strings.ToLower(http.StatusText(status))
}

func truncateUA(ua string) string {
	if len(ua) <= maxUserAgentBytes {
		return ua
	}
	end := maxUserAgentBytes
	for end > 0 && ua[end]&0xC0 == 0x80 {
		end--
	}
	return ua[:end]
}

// Sink receives every kept entry.
type Sink func(ctx context.Context, e Entry)

// Options configure the request logger.
type Options struct {
	SampleRate    float64
	SlowThreshold time.Duration
	Sink          Sink
	// Random returns [0,1) (tests); nil uses math/rand.
	Random func() float64
}

// OptionsFromEnv reads LOG_SAMPLE_RATE, LOG_SLOW_THRESHOLD_MS and the
// optional LOG_FILE_PATH sink (LOG_FILE_MAX_BYTES / LOG_FILE_MAX_FILES),
// emitting entries through logger.
func OptionsFromEnv(logger *slog.Logger) Options {
	return Options{
		SampleRate:    config.LogSampleRate(),
		SlowThreshold: time.Duration(min(config.LogSlowThresholdMS(), 1<<40)) * time.Millisecond,
		Sink:          NewSink(logger, config.LogFilePath(), config.LogFileMaxBytes(), config.LogFileMaxFiles()),
	}
}

// Middleware logs one entry per request with the env options (router.go).
func Middleware(logger *slog.Logger) func(http.Handler) http.Handler {
	return NewMiddleware(OptionsFromEnv(logger))
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

// NewMiddleware builds the request logger.
func NewMiddleware(opts Options) func(http.Handler) http.Handler {
	random := opts.Random
	if random == nil {
		random = rand.Float64 //nolint:gosec // sampling, not security
	}
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if r.Method == http.MethodOptions || r.URL.Path == MetricsPath {
				next.ServeHTTP(w, r)
				return
			}
			start := time.Now()
			store := &requestStore{}
			sw := &statusWriter{ResponseWriter: w}
			ctx := context.WithValue(r.Context(), storeKey{}, store)
			panicked := true
			defer func(ctx context.Context) {
				status := sw.status
				if status == 0 {
					status = http.StatusOK
				}
				if panicked {
					status = http.StatusInternalServerError
				}
				elapsed := time.Since(start)
				reason := DecideSample(status, elapsed, opts.SlowThreshold, opts.SampleRate, random())
				if reason == SampleDropped || opts.Sink == nil {
					return
				}
				store.mu.Lock()
				user, reqErr := store.user, store.err
				store.mu.Unlock()
				if reqErr == nil && status >= 400 {
					reqErr = &RequestError{Code: ErrorCodeForStatus(status), Message: clientSafeMessage(status)}
				}
				e := Entry{
					RequestID:    httpx.RequestIDFrom(ctx),
					Timestamp:    start.UTC().Format("2006-01-02T15:04:05.000Z"),
					Method:       r.Method,
					Path:         r.URL.Path,
					DurationMS:   elapsed.Milliseconds(),
					ClientIP:     httpx.ClientIP(r),
					UserAgent:    truncateUA(httpx.UserAgent(r)),
					Status:       status,
					Error:        reqErr,
					Sampled:      true,
					SampleReason: reason,
				}
				if user != nil {
					e.UserID, e.SessionID, e.Role = user.UserID, user.SessionID, user.Role
				}
				opts.Sink(ctx, e)
			}(ctx)
			next.ServeHTTP(sw, r.WithContext(ctx))
			panicked = false
		})
	}
}
