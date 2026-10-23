package storage

import (
	"context"
	"errors"
	"fmt"
	"io"
	"net/http"
	"net/http/httptest"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/wellington-vell/lindaflor/internal/config"
)

func TestMemoryPutPresignDelete(t *testing.T) {
	ctx := context.Background()
	st := NewMemory()
	if err := st.Put(ctx, "avatars/a.png", []byte("png-bytes"), "image/png"); err != nil {
		t.Fatal(err)
	}
	url, err := st.PresignGet(ctx, "avatars/a.png")
	if err != nil || url != "memory://avatars/a.png" {
		t.Fatal(url, err)
	}
	if obj, ok := st.Get("avatars/a.png"); !ok || obj.ContentType != "image/png" || string(obj.Body) != "png-bytes" {
		t.Fatal(obj)
	}
	if err := st.Delete(ctx, "avatars/a.png"); err != nil {
		t.Fatal(err)
	}
	if _, err := st.PresignGet(ctx, "avatars/a.png"); !errors.Is(err, ErrNotFound) {
		t.Fatal(err)
	}
}

func TestUnavailableAndFromConfig(t *testing.T) {
	ctx := context.Background()
	var u Store = Unavailable{}
	if !errors.Is(u.Put(ctx, "k", nil, ""), ErrUnavailable) || !errors.Is(u.Delete(ctx, "k"), ErrUnavailable) {
		t.Fatal("unavailable must fail writes")
	}
	if _, err := u.PresignGet(ctx, "k"); !errors.Is(err, ErrUnavailable) {
		t.Fatal(err)
	}
	if u.EnsureBucket(ctx) != nil || u.HealthCheck(ctx) != nil {
		t.Fatal("probes are no-ops")
	}
	if !IsUnavailable(FromConfig(ctx, nil)) || !IsUnavailable(nil) || IsUnavailable(NewMemory()) {
		t.Fatal("IsUnavailable")
	}
}

func testS3(endpoint string) *S3 {
	return NewS3(config.S3Config{Endpoint: endpoint, Region: "us-east-1", AccessKeyID: "test", SecretAccessKey: "testsecret", Bucket: "test-bucket"})
}

func TestS3PresignedURLShape(t *testing.T) {
	st := testS3("http://localhost:9000/")
	url, err := st.PresignGet(context.Background(), "avatars/some-id/image.jpg")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(url, "http://localhost:9000/test-bucket/avatars/some-id/image.jpg?") {
		t.Fatal(url)
	}
	for _, want := range []string{
		"X-Amz-Algorithm=AWS4-HMAC-SHA256", "X-Amz-Credential=test%2F", "X-Amz-Date=",
		"X-Amz-Expires=86400", "X-Amz-SignedHeaders=host", "X-Amz-Signature=",
	} {
		if !strings.Contains(url, want) {
			t.Fatalf("missing %s in %s", want, url)
		}
	}
	for key, path := range map[string]string{"a b": "a%20b", "café": "caf%C3%A9", "a+b=c&d?e": "a%2Bb%3Dc%26d%3Fe"} {
		u, err := st.PresignGet(context.Background(), key)
		if err != nil || !strings.HasPrefix(u, "http://localhost:9000/test-bucket/"+path+"?") {
			t.Fatalf("%q → %s (%v)", key, u, err)
		}
	}
}

func TestS3PresignCacheHitAndRenewal(t *testing.T) {
	st := testS3("http://localhost:9000")
	t0 := time.Unix(1_767_268_800, 0)
	now := t0
	st.now = func() time.Time { return now }
	calls := 0
	st.presignFn = func(_ context.Context, key string) (string, error) {
		calls++
		return fmt.Sprintf("https://s3/%s?sig=%d", key, calls), nil
	}
	ctx := context.Background()
	first, _ := st.PresignGet(ctx, "products/p/a.jpg")
	for _, d := range []time.Duration{5 * time.Minute, 19 * time.Hour} {
		now = t0.Add(d)
		if got, _ := st.PresignGet(ctx, "products/p/a.jpg"); got != first {
			t.Fatalf("+%s: cache miss", d)
		}
	}
	now = t0.Add(21 * time.Hour)
	renewed, _ := st.PresignGet(ctx, "products/p/a.jpg")
	if renewed == first || calls != 2 {
		t.Fatal("past the TTL a new URL is signed")
	}
	now = t0.Add(22 * time.Hour)
	if got, _ := st.PresignGet(ctx, "products/p/a.jpg"); got != renewed {
		t.Fatal("renewed URL is cached")
	}
	// Per key, and Delete evicts.
	now = t0
	b, _ := st.PresignGet(ctx, "b.jpg")
	if b == renewed || st.cache.Len() != 2 {
		t.Fatal("per key")
	}
	st.cache.Remove("b.jpg")
	if _, ok := st.cache.Get("b.jpg", t0); ok || st.cache.Len() != 1 {
		t.Fatal("remove")
	}
}

func TestPresignCacheIsBounded(t *testing.T) {
	var c PresignCache
	t0 := time.Unix(1_767_268_800, 0)
	for i := range PresignCacheMaxEntries {
		c.Insert(fmt.Sprintf("k%d", i), t0.Add(time.Duration(i)*time.Second), fmt.Sprintf("u%d", i))
	}
	late := t0.Add(time.Hour)
	c.Insert("new", late, "u-new")
	if c.Len() != PresignCacheMaxEntries {
		t.Fatal(c.Len())
	}
	if _, ok := c.Get("k0", late); ok {
		t.Fatal("oldest must be evicted")
	}
	if u, ok := c.Get("k1", late); !ok || u != "u1" {
		t.Fatal("k1")
	}
	if u, ok := c.Get("new", late); !ok || u != "u-new" {
		t.Fatal("new")
	}
	c.Insert("fresh", t0.Add(48*time.Hour), "u-fresh")
	if c.Len() != 1 {
		t.Fatalf("expired sweep: %d", c.Len())
	}
}

// fakeS3 is a minimal path-style S3 endpoint recording requests.
type fakeS3 struct {
	mu        sync.Mutex
	headCode  int
	reqs      []string
	bodies    map[string]string
	deleteErr int
}

func (f *fakeS3) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	f.mu.Lock()
	defer f.mu.Unlock()
	body, _ := io.ReadAll(r.Body)
	f.reqs = append(f.reqs, r.Method+" "+r.URL.Path)
	if f.bodies == nil {
		f.bodies = map[string]string{}
	}
	if !strings.HasPrefix(r.Header.Get("Authorization"), "AWS4-HMAC-SHA256 Credential=test/") {
		w.WriteHeader(http.StatusForbidden)
		return
	}
	switch {
	case r.Method == http.MethodHead && r.URL.Path == "/test-bucket":
		w.WriteHeader(f.headCode)
	case r.Method == http.MethodPut && r.URL.Path == "/test-bucket":
		f.bodies["create"] = string(body)
		f.headCode = http.StatusOK
	case r.Method == http.MethodPut:
		f.bodies[r.URL.Path] = r.Header.Get("Content-Type") + ":" + string(body)
	case r.Method == http.MethodDelete:
		if f.deleteErr != 0 {
			w.WriteHeader(f.deleteErr)
			return
		}
		w.WriteHeader(http.StatusNoContent)
	default:
		w.WriteHeader(http.StatusNotFound)
	}
}

func TestS3EnsureBucketCreatesWhenMissing(t *testing.T) {
	for _, code := range []int{http.StatusNotFound, http.StatusForbidden} {
		f := &fakeS3{headCode: code}
		srv := httptest.NewServer(f)
		st := testS3(srv.URL)
		ctx := context.Background()
		if err := st.HealthCheck(ctx); err == nil {
			t.Fatal("health check must fail while the bucket is missing")
		}
		if err := st.EnsureBucket(ctx); err != nil {
			t.Fatal(code, err)
		}
		if err := st.HealthCheck(ctx); err != nil {
			t.Fatal(err)
		}
		if strings.Join(f.reqs, ",") != "HEAD /test-bucket,HEAD /test-bucket,PUT /test-bucket,HEAD /test-bucket" {
			t.Fatal(f.reqs)
		}
		if f.bodies["create"] != "" {
			t.Fatal("us-east-1 sends no LocationConstraint")
		}
		srv.Close()
	}
	f := &fakeS3{headCode: http.StatusOK}
	srv := httptest.NewServer(f)
	defer srv.Close()
	if err := testS3(srv.URL).EnsureBucket(context.Background()); err != nil || len(f.reqs) != 1 {
		t.Fatal(err, f.reqs)
	}
	f.headCode = http.StatusInternalServerError
	if err := testS3(srv.URL).EnsureBucket(context.Background()); err == nil {
		t.Fatal("5xx must not create")
	}
}

func TestS3EnsureBucketSendsLocationConstraintOutsideUSEast1(t *testing.T) {
	f := &fakeS3{headCode: http.StatusNotFound}
	srv := httptest.NewServer(f)
	defer srv.Close()
	st := NewS3(config.S3Config{Endpoint: srv.URL, Region: "sa-east-1", AccessKeyID: "test", SecretAccessKey: "s", Bucket: "test-bucket"})
	if err := st.EnsureBucket(context.Background()); err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(f.bodies["create"], "<LocationConstraint>sa-east-1</LocationConstraint>") {
		t.Fatal(f.bodies["create"])
	}
}

func TestS3PutAndDelete(t *testing.T) {
	f := &fakeS3{headCode: http.StatusOK}
	srv := httptest.NewServer(f)
	defer srv.Close()
	st := testS3(srv.URL)
	ctx := context.Background()
	if err := st.Put(ctx, "avatars/x/a b.png", []byte("img"), "image/png"); err != nil {
		t.Fatal(err)
	}
	if f.bodies["/test-bucket/avatars/x/a b.png"] != "image/png:img" {
		t.Fatal(f.bodies)
	}
	if err := st.Delete(ctx, "avatars/x/a b.png"); err != nil {
		t.Fatal(err)
	}
	f.deleteErr = http.StatusNotFound
	if err := st.Delete(ctx, "missing"); err != nil {
		t.Fatal("idempotent delete", err)
	}
	f.deleteErr = http.StatusInternalServerError
	if err := st.Delete(ctx, "x"); err == nil {
		t.Fatal("5xx delete must fail")
	}
}
