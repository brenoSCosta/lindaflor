package storage

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/aws/aws-sdk-go-v2/aws"
	awshttp "github.com/aws/aws-sdk-go-v2/aws/transport/http"
	"github.com/aws/aws-sdk-go-v2/credentials"
	"github.com/aws/aws-sdk-go-v2/service/s3"
	"github.com/aws/aws-sdk-go-v2/service/s3/types"

	"github.com/wellington-vell/lindaflor/internal/config"
)

// Presigned GET tuning (port of src/storage.rs).
const (
	// PresignExpiry is the validity of a presigned GET URL.
	PresignExpiry = 24 * time.Hour
	// PresignCacheTTL is how long one presigned URL is reused: leaves 4h of
	// validity on every URL handed out, so a page rendered just before the
	// entry rolls over still loads its images.
	PresignCacheTTL = 20 * time.Hour
	// PresignCacheMaxEntries bounds the cache (one entry per key).
	PresignCacheMaxEntries = 4096
	httpTimeout            = 10 * time.Second
)

// PresignCache reuses presigned GET URLs per storage key. The SigV4
// signature covers the signing time, so presigning on every render would
// give each page view new image URLs and defeat browser/CDN caches.
// Bounded: expired entries are swept when full, then the oldest is evicted.
type PresignCache struct {
	mu      sync.Mutex
	entries map[string]presignEntry
}

type presignEntry struct {
	signedAt time.Time
	url      string
}

// Get returns the cached URL for key if it was signed less than
// PresignCacheTTL before now.
func (c *PresignCache) Get(key string, now time.Time) (string, bool) {
	c.mu.Lock()
	defer c.mu.Unlock()
	e, ok := c.entries[key]
	if !ok || now.Sub(e.signedAt) >= PresignCacheTTL {
		return "", false
	}
	return e.url, true
}

// Insert stores url for key, signed at signedAt.
func (c *PresignCache) Insert(key string, signedAt time.Time, url string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.entries == nil {
		c.entries = map[string]presignEntry{}
	}
	if _, exists := c.entries[key]; !exists && len(c.entries) >= PresignCacheMaxEntries {
		for k, e := range c.entries {
			if signedAt.Sub(e.signedAt) >= PresignCacheTTL {
				delete(c.entries, k)
			}
		}
		if len(c.entries) >= PresignCacheMaxEntries {
			var oldest string
			var oldestAt time.Time
			first := true
			for k, e := range c.entries {
				if first || e.signedAt.Before(oldestAt) {
					oldest, oldestAt, first = k, e.signedAt, false
				}
			}
			delete(c.entries, oldest)
		}
	}
	c.entries[key] = presignEntry{signedAt: signedAt, url: url}
}

// Remove drops key.
func (c *PresignCache) Remove(key string) {
	c.mu.Lock()
	defer c.mu.Unlock()
	delete(c.entries, key)
}

// Len is the number of cached URLs.
func (c *PresignCache) Len() int {
	c.mu.Lock()
	defer c.mu.Unlock()
	return len(c.entries)
}

// S3 is the S3/RustFS/MinIO Store (path-style addressing, custom endpoint).
type S3 struct {
	client  *s3.Client
	presign *s3.PresignClient
	bucket  string
	region  string
	cache   PresignCache
	// now and presignFn are swappable in tests.
	now       func() time.Time
	presignFn func(ctx context.Context, key string) (string, error)
}

// NewS3 builds the S3 store from cfg (no network I/O; see EnsureBucket).
func NewS3(cfg config.S3Config) *S3 {
	client := s3.New(s3.Options{
		Region:       cfg.Region,
		BaseEndpoint: aws.String(strings.TrimRight(cfg.Endpoint, "/")),
		UsePathStyle: true,
		Credentials:  credentials.NewStaticCredentialsProvider(cfg.AccessKeyID, cfg.SecretAccessKey, ""),
		HTTPClient:   awshttp.NewBuildableClient().WithTimeout(httpTimeout),
		// S3-compatible servers (RustFS, MinIO) do not all implement the
		// newer default CRC checksums.
		RequestChecksumCalculation: aws.RequestChecksumCalculationWhenRequired,
		ResponseChecksumValidation: aws.ResponseChecksumValidationWhenRequired,
	})
	st := &S3{client: client, presign: s3.NewPresignClient(client), bucket: cfg.Bucket, region: cfg.Region, now: time.Now}
	st.presignFn = st.presignNow
	return st
}

func wrap(err error) error { return fmt.Errorf("object storage error: %w", err) }

// statusCode is the HTTP status of an S3 error (0 when none).
func statusCode(err error) int {
	var re *awshttp.ResponseError
	if errors.As(err, &re) {
		return re.HTTPStatusCode()
	}
	return 0
}

// Put implements Store.
func (s *S3) Put(ctx context.Context, key string, body []byte, contentType string) error {
	_, err := s.client.PutObject(ctx, &s3.PutObjectInput{
		Bucket:        aws.String(s.bucket),
		Key:           aws.String(key),
		Body:          bytes.NewReader(body),
		ContentLength: aws.Int64(int64(len(body))),
		ContentType:   aws.String(contentType),
	})
	if err != nil {
		slog.ErrorContext(ctx, "s3 request failed", "op", "put", "status", statusCode(err))
		return wrap(err)
	}
	return nil
}

// Delete implements Store. A missing key is not an error.
func (s *S3) Delete(ctx context.Context, key string) error {
	s.cache.Remove(key)
	_, err := s.client.DeleteObject(ctx, &s3.DeleteObjectInput{Bucket: aws.String(s.bucket), Key: aws.String(key)})
	if err != nil && statusCode(err) != http.StatusNotFound {
		slog.ErrorContext(ctx, "s3 request failed", "op", "delete", "status", statusCode(err))
		return wrap(err)
	}
	return nil
}

func (s *S3) presignNow(ctx context.Context, key string) (string, error) {
	req, err := s.presign.PresignGetObject(ctx, &s3.GetObjectInput{Bucket: aws.String(s.bucket), Key: aws.String(key)},
		s3.WithPresignExpires(PresignExpiry))
	if err != nil {
		return "", wrap(err)
	}
	return req.URL, nil
}

// PresignGet implements Store, reusing the cached URL while it is younger
// than PresignCacheTTL.
func (s *S3) PresignGet(ctx context.Context, key string) (string, error) {
	now := s.now()
	if url, ok := s.cache.Get(key, now); ok {
		return url, nil
	}
	url, err := s.presignFn(ctx, key)
	if err != nil {
		return "", err
	}
	s.cache.Insert(key, now, url)
	return url, nil
}

// EnsureBucket implements Store: HeadBucket, CreateBucket when missing
// (403/404/NoSuchBucket — S3-likes differ).
func (s *S3) EnsureBucket(ctx context.Context) error {
	_, err := s.client.HeadBucket(ctx, &s3.HeadBucketInput{Bucket: aws.String(s.bucket)})
	if err == nil {
		return nil
	}
	var nsb *types.NoSuchBucket
	var nf *types.NotFound
	code := statusCode(err)
	if code != http.StatusForbidden && code != http.StatusNotFound && !errors.As(err, &nsb) && !errors.As(err, &nf) {
		return wrap(err)
	}
	in := &s3.CreateBucketInput{Bucket: aws.String(s.bucket)}
	// AWS rejects a LocationConstraint in us-east-1; other regions need one.
	if !strings.EqualFold(s.region, "us-east-1") {
		in.CreateBucketConfiguration = &types.CreateBucketConfiguration{LocationConstraint: types.BucketLocationConstraint(s.region)}
	}
	if _, err := s.client.CreateBucket(ctx, in); err != nil {
		return wrap(err)
	}
	return nil
}

// HealthCheck implements Store: read-only HEAD bucket (never creates).
func (s *S3) HealthCheck(ctx context.Context) error {
	if _, err := s.client.HeadBucket(ctx, &s3.HeadBucketInput{Bucket: aws.String(s.bucket)}); err != nil {
		return wrap(err)
	}
	return nil
}
