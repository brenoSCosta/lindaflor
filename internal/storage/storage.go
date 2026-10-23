// Package storage: object storage for avatars and product images (S3 /
// RustFS, path-style, presigned GETs).
//
// Wave 0: Store contract + in-memory and unavailable implementations.
// TODO(wave1-B): S3 implementation (aws-sdk-go-v2, presign cache,
// EnsureBucket/HeadBucket) and avatar rules from src/storage.rs.
package storage

import (
	"context"
	"errors"
	"net/url"
	"sync"
)

// Errors.
var (
	ErrUnavailable = errors.New("object storage is unavailable")
	ErrNotFound    = errors.New("object not found")
)

// Store is the object storage contract.
type Store interface {
	Put(ctx context.Context, key string, body []byte, contentType string) error
	Delete(ctx context.Context, key string) error
	// PresignGet returns a short-lived URL to read key.
	PresignGet(ctx context.Context, key string) (string, error)
	// EnsureBucket creates the bucket when missing (boot).
	EnsureBucket(ctx context.Context) error
	// HealthCheck is the read-only readiness probe (HEAD bucket).
	HealthCheck(ctx context.Context) error
}

// Object is a stored blob (Memory only).
type Object struct {
	Body        []byte
	ContentType string
}

// Memory is an in-process Store for tests and S3-less development.
type Memory struct {
	mu   sync.RWMutex
	objs map[string]Object
}

// NewMemory returns an empty Memory store.
func NewMemory() *Memory { return &Memory{objs: map[string]Object{}} }

// Put implements Store.
func (m *Memory) Put(_ context.Context, key string, body []byte, ct string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.objs[key] = Object{Body: append([]byte(nil), body...), ContentType: ct}
	return nil
}

// Delete implements Store.
func (m *Memory) Delete(_ context.Context, key string) error {
	m.mu.Lock()
	defer m.mu.Unlock()
	delete(m.objs, key)
	return nil
}

// PresignGet implements Store with a fake memory:// URL.
func (m *Memory) PresignGet(_ context.Context, key string) (string, error) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	if _, ok := m.objs[key]; !ok {
		return "", ErrNotFound
	}
	return "memory://" + url.PathEscape(key), nil
}

// Get returns a stored object (tests).
func (m *Memory) Get(key string) (Object, bool) {
	m.mu.RLock()
	defer m.mu.RUnlock()
	o, ok := m.objs[key]
	return o, ok
}

// EnsureBucket implements Store.
func (m *Memory) EnsureBucket(context.Context) error { return nil }

// HealthCheck implements Store.
func (m *Memory) HealthCheck(context.Context) error { return nil }

// Unavailable is the Store used when S3 is not configured.
type Unavailable struct{}

// Put implements Store.
func (Unavailable) Put(context.Context, string, []byte, string) error { return ErrUnavailable }

// Delete implements Store.
func (Unavailable) Delete(context.Context, string) error { return ErrUnavailable }

// PresignGet implements Store.
func (Unavailable) PresignGet(context.Context, string) (string, error) { return "", ErrUnavailable }

// EnsureBucket implements Store.
func (Unavailable) EnsureBucket(context.Context) error { return nil }

// HealthCheck implements Store.
func (Unavailable) HealthCheck(context.Context) error { return nil }

// IsUnavailable reports the no-storage implementation (readyz "skipped").
func IsUnavailable(s Store) bool {
	_, ok := s.(Unavailable)
	return ok || s == nil
}
