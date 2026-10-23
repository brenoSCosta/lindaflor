package httpx

import (
	"errors"
	"io"
	"net/http"
	"sync"
)

// DefaultBodyLimit is the global request body cap (1 MiB).
const DefaultBodyLimit int64 = 1 << 20

// limitedBody enforces a byte limit that inner route middleware may raise
// (BodyLimit) before the handler reads the body.
type limitedBody struct {
	mu    sync.Mutex
	rc    io.ReadCloser
	limit int64
	read  int64
	cl    int64 // declared Content-Length, -1 when unknown
}

func (b *limitedBody) Read(p []byte) (int, error) {
	b.mu.Lock()
	defer b.mu.Unlock()
	if b.cl > b.limit {
		return 0, &http.MaxBytesError{Limit: b.limit}
	}
	if b.read > b.limit {
		return 0, &http.MaxBytesError{Limit: b.limit}
	}
	if int64(len(p)) > b.limit-b.read+1 {
		p = p[:b.limit-b.read+1]
	}
	n, err := b.rc.Read(p)
	b.read += int64(n)
	if b.read > b.limit {
		return n, &http.MaxBytesError{Limit: b.limit}
	}
	return n, err
}

func (b *limitedBody) Close() error { return b.rc.Close() }

// GlobalBodyLimit caps every request body at limit bytes. Reads past the
// limit fail with *http.MaxBytesError (see IsBodyTooLarge); routes that
// accept larger bodies wrap themselves in BodyLimit.
func GlobalBodyLimit(limit int64) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if r.Body != nil && r.Body != http.NoBody {
				r.Body = &limitedBody{rc: r.Body, limit: limit, cl: r.ContentLength}
			}
			next.ServeHTTP(w, r)
		})
	}
}

// BodyLimit overrides the global cap for one route or group, e.g.
//
//	r.With(httpx.BodyLimit(2<<20)).Post("/settings/avatar", h)
//	r.With(httpx.BodyLimit(20<<20)).Post("/admin/produtos/create", h)
func BodyLimit(limit int64) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			if lb, ok := r.Body.(*limitedBody); ok {
				lb.mu.Lock()
				lb.limit = limit
				lb.mu.Unlock()
			} else if r.Body != nil && r.Body != http.NoBody {
				r.Body = &limitedBody{rc: r.Body, limit: limit, cl: r.ContentLength}
			}
			next.ServeHTTP(w, r)
		})
	}
}

// IsBodyTooLarge reports a body-limit error from ParseForm/Read/Decode.
// Handlers answer it with 413 (see TooLarge).
func IsBodyTooLarge(err error) bool {
	var mbe *http.MaxBytesError
	return errors.As(err, &mbe)
}

// TooLarge writes a 413.
func TooLarge(w http.ResponseWriter) {
	http.Error(w, "Corpo da requisição muito grande.", http.StatusRequestEntityTooLarge)
}
