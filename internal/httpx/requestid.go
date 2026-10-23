// Package httpx holds the cross-cutting HTTP middleware and helpers shared
// by every feature package: request ids, panic recovery, security headers,
// body limits, origin checks, client IP resolution, safe redirects, htmx
// helpers and the toast flash system.
package httpx

import (
	"context"
	"net/http"

	"github.com/google/uuid"
)

type ctxKey int

const (
	requestIDKey ctxKey = iota
	clientIPKey
)

// RequestIDHeader is echoed on every response.
const RequestIDHeader = "X-Request-Id"

// RequestID assigns a fresh UUIDv7 per request (client-supplied ids are
// ignored so logs cannot be forged), stores it in the context and sets the
// X-Request-Id response header.
func RequestID(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		id, err := uuid.NewV7()
		if err != nil {
			id = uuid.New()
		}
		w.Header().Set(RequestIDHeader, id.String())
		next.ServeHTTP(w, r.WithContext(context.WithValue(r.Context(), requestIDKey, id)))
	})
}

// RequestIDFrom returns the request id (uuid.Nil outside RequestID).
func RequestIDFrom(ctx context.Context) uuid.UUID {
	id, _ := ctx.Value(requestIDKey).(uuid.UUID)
	return id
}
