package logging

import (
	"bytes"
	"context"
	"encoding/json"
	"log/slog"
	"net/http"
	"net/http/httptest"
	"os"
	"path/filepath"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/go-chi/chi/v5"
	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/httpx"
)

func TestErrorStatusesWin(t *testing.T) {
	for _, s := range []int{400, 500} {
		if DecideSample(s, time.Millisecond, time.Second, 0, 0) != SampleError {
			t.Fatal(s)
		}
	}
}

func TestSlowBoundaryIsInclusive(t *testing.T) {
	if DecideSample(200, time.Second, time.Second, 0, 0) != SampleSlow {
		t.Fatal("slow")
	}
	if DecideSample(200, 999*time.Millisecond, time.Second, 0, 0) != SampleDropped {
		t.Fatal("not slow")
	}
}

func TestRateBounds(t *testing.T) {
	if DecideSample(200, 10*time.Millisecond, time.Second, 0, 0) != SampleDropped {
		t.Fatal("rate 0 drops")
	}
	if DecideSample(200, 10*time.Millisecond, time.Second, 1, 0.5) != SampleSampled {
		t.Fatal("rate 1 keeps")
	}
}

func TestUnknown500HidesInternalText(t *testing.T) {
	if clientSafeMessage(500) != "internal error" || ErrorCodeForStatus(500) != "INTERNAL" {
		t.Fatal("500")
	}
	if clientSafeMessage(404) != "not found" || ErrorCodeForStatus(404) != "NOT_FOUND" {
		t.Fatal("404")
	}
	if ErrorCodeForStatus(429) != "TOO_MANY_REQUESTS" || ErrorCodeForStatus(413) != "CONTENT_TOO_LARGE" {
		t.Fatal("codes")
	}
}

func TestUserAgentIsTruncatedOnACharBoundary(t *testing.T) {
	ua := strings.Repeat("a", 255) + "é" + "tail"
	got := truncateUA(ua)
	if len(got) != 255 || !strings.HasPrefix(ua, got) {
		t.Fatal(len(got))
	}
	if truncateUA("short") != "short" {
		t.Fatal("short")
	}
}

func TestFileTeeRotates(t *testing.T) {
	dir := t.TempDir()
	path := filepath.Join(dir, "sub", "requests.log")
	tee := NewFileTee(path, 20, 2)
	for _, line := range []string{"aaaaaaaaaa\n", "bbbbbbbbbb\n", "cccccccccc\n", "dddddddddd\n"} {
		if err := tee.Write([]byte(line)); err != nil {
			t.Fatal(err)
		}
	}
	read := func(p string) string {
		b, err := os.ReadFile(p)
		if err != nil {
			return "<missing>"
		}
		return string(b)
	}
	if read(path) != "dddddddddd\n" || read(filepath.Join(dir, "sub", "requests.1.log")) != "cccccccccc\n" ||
		read(filepath.Join(dir, "sub", "requests.2.log")) != "bbbbbbbbbb\n" || read(filepath.Join(dir, "sub", "requests.3.log")) != "<missing>" {
		t.Fatal(read(path), read(filepath.Join(dir, "sub", "requests.1.log")))
	}
}

func TestSinkWritesJSONLinesAndLogs(t *testing.T) {
	path := filepath.Join(t.TempDir(), "r.log")
	var buf bytes.Buffer
	sink := NewSink(slog.New(slog.NewJSONHandler(&buf, nil)), path, 0, 0)
	sink(context.Background(), Entry{RequestID: uuid.New(), Method: "GET", Path: "/x", Status: 404, Error: &RequestError{Code: "NOT_FOUND", Message: "not found"}, Sampled: true, SampleReason: SampleError})
	b, err := os.ReadFile(path)
	if err != nil {
		t.Fatal(err)
	}
	var e map[string]any
	if err := json.Unmarshal(bytes.TrimSpace(b), &e); err != nil {
		t.Fatal(err)
	}
	if e["status"] != 404.0 || e["sample_reason"] != "error" || e["error"].(map[string]any)["code"] != "NOT_FOUND" {
		t.Fatal(e)
	}
	if _, ok := e["user_id"]; ok {
		t.Fatal("user fields are omitted when anonymous")
	}
	if !strings.Contains(buf.String(), `"msg":"request"`) || !strings.Contains(buf.String(), `"sample_reason":"error"`) {
		t.Fatal(buf.String())
	}
}

type collector struct {
	mu      sync.Mutex
	entries []Entry
}

func (c *collector) sink(_ context.Context, e Entry) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.entries = append(c.entries, e)
}

func (c *collector) all() []Entry {
	c.mu.Lock()
	defer c.mu.Unlock()
	return append([]Entry(nil), c.entries...)
}

func routerWith(opts Options) http.Handler {
	r := chi.NewRouter()
	r.Use(httpx.RequestID, httpx.ClientIPMiddleware(httpx.TrustedProxies{}), NewMiddleware(opts))
	r.Get("/api/health", func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(http.StatusOK) })
	r.Get("/metrics", func(w http.ResponseWriter, _ *http.Request) { w.WriteHeader(http.StatusOK) })
	r.Get("/api/auth/get-session", func(w http.ResponseWriter, r *http.Request) {
		SetRequestUser(r.Context(), RequestUser{UserID: "u1", SessionID: "s1", Role: "admin"})
		w.WriteHeader(http.StatusOK)
	})
	return r
}

func send(h http.Handler, method, target string) *httptest.ResponseRecorder {
	req := httptest.NewRequest(method, target, nil)
	req.Header.Set("User-Agent", "test-agent")
	rec := httptest.NewRecorder()
	h.ServeHTTP(rec, req)
	return rec
}

func TestHealthLogsCorrelatedRequestID(t *testing.T) {
	c := &collector{}
	h := routerWith(Options{SampleRate: 1, SlowThreshold: time.Second, Sink: c.sink})
	rec := send(h, http.MethodGet, "/api/health?foo=bar")
	id, err := uuid.Parse(rec.Header().Get("X-Request-Id"))
	if rec.Code != http.StatusOK || err != nil || id.Version() != 7 {
		t.Fatal(rec.Code, err)
	}
	got := c.all()
	if len(got) != 1 {
		t.Fatal(got)
	}
	e := got[0]
	if e.RequestID != id || e.Method != "GET" || e.Path != "/api/health" || e.Status != 200 || !e.Sampled ||
		e.SampleReason != SampleSampled || e.UserAgent != "test-agent" || e.ClientIP == "" || e.Error != nil {
		t.Fatalf("%+v", e)
	}
	if _, err := time.Parse("2006-01-02T15:04:05.000Z", e.Timestamp); err != nil {
		t.Fatal(e.Timestamp)
	}
}

func TestOptionsAndMetricsAreSilent(t *testing.T) {
	c := &collector{}
	h := routerWith(Options{SampleRate: 1, SlowThreshold: time.Second, Sink: c.sink})
	send(h, http.MethodOptions, "/api/health")
	send(h, http.MethodGet, "/metrics")
	if len(c.all()) != 0 {
		t.Fatal(c.all())
	}
}

func TestNotFoundIsKeptAsError(t *testing.T) {
	c := &collector{}
	h := routerWith(Options{SampleRate: 0, SlowThreshold: time.Minute, Sink: c.sink})
	rec := send(h, http.MethodGet, "/no-such-route")
	if rec.Code != http.StatusNotFound || rec.Header().Get("X-Request-Id") == "" {
		t.Fatal(rec.Code)
	}
	got := c.all()
	if len(got) != 1 || got[0].Status != 404 || got[0].SampleReason != SampleError || got[0].Error == nil || got[0].Error.Code != "NOT_FOUND" {
		t.Fatalf("%+v", got)
	}
}

func TestSampleRateZeroDropsOKRequests(t *testing.T) {
	c := &collector{}
	h := routerWith(Options{SampleRate: 0, SlowThreshold: time.Minute, Sink: c.sink})
	if send(h, http.MethodGet, "/api/health").Code != http.StatusOK || len(c.all()) != 0 {
		t.Fatal(c.all())
	}
}

func TestSignedInRequestEnrichesUserFields(t *testing.T) {
	c := &collector{}
	h := routerWith(Options{SampleRate: 1, SlowThreshold: time.Second, Sink: c.sink})
	send(h, http.MethodGet, "/api/auth/get-session")
	got := c.all()
	if len(got) != 1 || got[0].UserID != "u1" || got[0].SessionID != "s1" || got[0].Role != "admin" {
		t.Fatalf("%+v", got)
	}
}

func TestExplicitRequestErrorWins(t *testing.T) {
	c := &collector{}
	r := chi.NewRouter()
	r.Use(httpx.RequestID, NewMiddleware(Options{SampleRate: 1, SlowThreshold: time.Second, Sink: c.sink}))
	r.Get("/x", func(w http.ResponseWriter, r *http.Request) {
		SetRequestError(r.Context(), "INVALID_TOKEN", "Token inválido")
		w.WriteHeader(http.StatusBadRequest)
	})
	send(r, http.MethodGet, "/x")
	if got := c.all(); len(got) != 1 || got[0].Error.Code != "INVALID_TOKEN" {
		t.Fatalf("%+v", got)
	}
}
