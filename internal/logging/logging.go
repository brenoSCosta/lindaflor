// Package logging: structured JSON logs (log/slog) and the request logging
// middleware.
//
// Wave 0: logger construction + a minimal request logger.
// TODO(wave1-B): port src/logging (sampling LOG_SAMPLE_RATE, slow-request
// threshold, rotating LOG_FILE_PATH sink, field names, error mapping).
package logging

import (
	"io"
	"log/slog"
	"net/http"
	"os"
	"time"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/httpx"
)

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

type statusWriter struct {
	http.ResponseWriter
	status int
	bytes  int
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
	n, err := s.ResponseWriter.Write(b)
	s.bytes += n
	return n, err
}

// Unwrap lets http.ResponseController reach the underlying writer.
func (s *statusWriter) Unwrap() http.ResponseWriter { return s.ResponseWriter }

// Middleware logs one line per request.
func Middleware(logger *slog.Logger) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			start := time.Now()
			sw := &statusWriter{ResponseWriter: w}
			next.ServeHTTP(sw, r)
			if sw.status == 0 {
				sw.status = http.StatusOK
			}
			level := slog.LevelInfo
			if sw.status >= 500 {
				level = slog.LevelError
			}
			logger.LogAttrs(r.Context(), level, "request",
				slog.String("request_id", httpx.RequestIDFrom(r.Context()).String()),
				slog.String("method", r.Method),
				slog.String("path", r.URL.Path),
				slog.Int("status", sw.status),
				slog.Int("bytes", sw.bytes),
				slog.Duration("duration", time.Since(start)),
				slog.String("ip", httpx.ClientIP(r)),
			)
		})
	}
}
