// Package valkey wraps the shared Valkey (Redis protocol) connection and the
// security-critical counters stored there: auth failure lockouts, TOTP
// single-use claims, pending-2FA attempt/binding records and the one-time
// TOTP enrollment flash.
//
// Coarse per-IP rate limits are not here (see internal/ratelimit): they are
// a DoS guard, not a security boundary. The counters here must survive
// restarts and be shared across replicas, so they live in Valkey. When no
// connection is configured or Valkey errors, every helper falls back to a
// process-local map — fail closed per process instead of failing open.
package valkey

import (
	"context"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"fmt"
	"log/slog"
	"strconv"
	"strings"
	"sync"
	"time"

	"github.com/google/uuid"
	"github.com/redis/go-redis/v9"
)

// Client is the shared Valkey handle plus its process-local fallback.
// A nil redis client (see NewFallbackOnly) uses the fallback for everything.
type Client struct {
	rdb *redis.Client
	fb  *fallbackStore
}

// Connect opens the client and verifies it with PING, bounding each try to
// 2s and retrying (3 attempts, 200ms·n backoff) so a hung Valkey fails boot
// fast instead of hanging main.
func Connect(ctx context.Context, url string) (*Client, error) {
	opts, err := redis.ParseURL(url)
	if err != nil {
		return nil, fmt.Errorf("parse VALKEY_URL: %w", err)
	}
	const timeout = 2 * time.Second
	opts.DialTimeout = timeout
	var lastErr error
	for attempt := 1; attempt <= 3; attempt++ {
		rdb := redis.NewClient(opts)
		pctx, cancel := context.WithTimeout(ctx, timeout)
		lastErr = rdb.Ping(pctx).Err()
		cancel()
		if lastErr == nil {
			return NewWithRedis(rdb), nil
		}
		_ = rdb.Close()
		if attempt < 3 {
			slog.WarnContext(ctx, "valkey not ready, retrying", "attempt", attempt, "error", lastErr)
			select {
			case <-ctx.Done():
				return nil, ctx.Err()
			case <-time.After(time.Duration(200*attempt) * time.Millisecond):
			}
		}
	}
	return nil, fmt.Errorf("valkey connect/PING: %w", lastErr)
}

// NewWithRedis wraps an existing go-redis client (tests use miniredis).
func NewWithRedis(rdb *redis.Client) *Client { return &Client{rdb: rdb, fb: newFallback()} }

// NewFallbackOnly is a client with no Valkey: every helper uses the
// process-local fallback.
func NewFallbackOnly() *Client { return &Client{fb: newFallback()} }

// Redis exposes the raw client (nil for a fallback-only client).
func (c *Client) Redis() *redis.Client { return c.rdb }

// Ping checks the connection; a fallback-only client reports ErrNoValkey.
func (c *Client) Ping(ctx context.Context) error {
	if c.rdb == nil {
		return ErrNoValkey
	}
	return c.rdb.Ping(ctx).Err()
}

// ErrNoValkey is returned by Ping on a fallback-only client.
var ErrNoValkey = errors.New("valkey: no connection configured")

// Close releases the connection.
func (c *Client) Close() error {
	if c.rdb == nil {
		return nil
	}
	return c.rdb.Close()
}

// IPUAHash is hex(SHA-256(ip | 0x00 | ua)) truncated to 32 chars, stored
// instead of raw IPs/UAs in binding records.
func IPUAHash(ip, ua string) string {
	h := sha256.New()
	h.Write([]byte(ip))
	h.Write([]byte{0})
	h.Write([]byte(ua))
	return hex.EncodeToString(h.Sum(nil))[:32]
}

// PendingTokenHash hashes an opaque pending-2FA token for key names so the
// keyspace / MONITOR never carries a usable secret.
func PendingTokenHash(token string) string {
	sum := sha256.Sum256([]byte(strings.TrimSpace(token)))
	return hex.EncodeToString(sum[:])
}

// simple renders a UUID without hyphens (Rust `as_simple()`).
func simple(id uuid.UUID) string { return strings.ReplaceAll(id.String(), "-", "") }

// ---------------------------------------------------------------------------
// Tiny Valkey wrappers. ok=false means "no connection or error": callers use
// the fallback.
// ---------------------------------------------------------------------------

func (c *Client) vIncr(ctx context.Context, key string) (uint64, bool) {
	if c.rdb == nil {
		return 0, false
	}
	n, err := c.rdb.Incr(ctx, key).Result()
	if err != nil || n < 0 {
		return 0, false
	}
	return uint64(n), true
}

func (c *Client) vExpire(ctx context.Context, key string, ttl time.Duration) {
	if c.rdb != nil {
		_ = c.rdb.Expire(ctx, key, ttl).Err()
	}
}

// vGet returns ok=false on missing key too (Rust `Option` flattening), so
// callers consult the fallback in that case.
func (c *Client) vGet(ctx context.Context, key string) (string, bool) {
	if c.rdb == nil {
		return "", false
	}
	v, err := c.rdb.Get(ctx, key).Result()
	if err != nil {
		return "", false
	}
	return v, true
}

func (c *Client) vDel(ctx context.Context, key string) {
	if c.rdb != nil {
		_ = c.rdb.Del(ctx, key).Err()
	}
}

// vSetNXEX is atomic SET key val NX EX ttl. claimed=true: claimed;
// claimed=false: already held; ok=false: no connection or error.
func (c *Client) vSetNXEX(ctx context.Context, key, val string, ttl time.Duration) (claimed, ok bool) {
	if c.rdb == nil {
		return false, false
	}
	err := c.rdb.SetArgs(ctx, key, val, redis.SetArgs{Mode: "NX", TTL: ttl}).Err()
	switch {
	case err == nil:
		return true, true
	case errors.Is(err, redis.Nil):
		return false, true
	default:
		return false, false
	}
}

func (c *Client) vSetEX(ctx context.Context, key, val string, ttl time.Duration) {
	if c.rdb != nil {
		_ = c.rdb.Set(ctx, key, val, ttl).Err()
	}
}

func (c *Client) vGetDel(ctx context.Context, key string) (string, bool) {
	if c.rdb == nil {
		return "", false
	}
	v, err := c.rdb.GetDel(ctx, key).Result()
	if err != nil {
		return "", false
	}
	return v, true
}

func parseCount(v string) uint64 {
	n, err := strconv.ParseUint(v, 10, 64)
	if err != nil {
		return 0
	}
	return n
}

// ---------------------------------------------------------------------------
// Process-local fallback: key -> (value, deadline)
// ---------------------------------------------------------------------------

type fbEntry struct {
	val      string
	deadline time.Time
}

type fallbackStore struct {
	mu sync.Mutex
	m  map[string]fbEntry
}

func newFallback() *fallbackStore { return &fallbackStore{m: make(map[string]fbEntry)} }

func (f *fallbackStore) pruneLocked(now time.Time) {
	if len(f.m) <= 4096 {
		return
	}
	for k, e := range f.m {
		if !e.deadline.After(now) {
			delete(f.m, k)
		}
	}
}

func (f *fallbackStore) incr(key string, ttl time.Duration) uint64 {
	f.mu.Lock()
	defer f.mu.Unlock()
	now := time.Now()
	f.pruneLocked(now)
	count := uint64(1)
	if e, ok := f.m[key]; ok && e.deadline.After(now) {
		count = parseCount(e.val) + 1
	}
	f.m[key] = fbEntry{val: strconv.FormatUint(count, 10), deadline: now.Add(ttl)}
	return count
}

func (f *fallbackStore) get(key string) (string, bool) {
	f.mu.Lock()
	defer f.mu.Unlock()
	if e, ok := f.m[key]; ok && e.deadline.After(time.Now()) {
		return e.val, true
	}
	delete(f.m, key)
	return "", false
}

func (f *fallbackStore) setNX(key, val string, ttl time.Duration) bool {
	f.mu.Lock()
	defer f.mu.Unlock()
	now := time.Now()
	if e, ok := f.m[key]; ok && e.deadline.After(now) {
		return false
	}
	f.m[key] = fbEntry{val: val, deadline: now.Add(ttl)}
	return true
}

func (f *fallbackStore) del(key string) {
	f.mu.Lock()
	defer f.mu.Unlock()
	delete(f.m, key)
}

func (f *fallbackStore) set(key, val string, ttl time.Duration) {
	f.mu.Lock()
	defer f.mu.Unlock()
	now := time.Now()
	f.pruneLocked(now)
	f.m[key] = fbEntry{val: val, deadline: now.Add(ttl)}
}

func (f *fallbackStore) take(key string) (string, bool) {
	f.mu.Lock()
	defer f.mu.Unlock()
	e, ok := f.m[key]
	delete(f.m, key)
	if ok && e.deadline.After(time.Now()) {
		return e.val, true
	}
	return "", false
}

// ---------------------------------------------------------------------------
// TOTP enrollment flash
// ---------------------------------------------------------------------------

// EnrollmentFlashTTL matches the pending-2FA TTL (10 min).
const EnrollmentFlashTTL = 10 * time.Minute

func enrollmentFlashKey(sessionHash string) string { return "2fa:enroll-flash:" + sessionHash }

// TotpEnrollmentFlashPut stores the enable response (secret / otpauth URI /
// backup codes JSON) for the next GET of the setup page, keyed by session
// token hash.
func (c *Client) TotpEnrollmentFlashPut(ctx context.Context, sessionHash, payload string) {
	key := enrollmentFlashKey(sessionHash)
	c.vSetEX(ctx, key, payload, EnrollmentFlashTTL)
	c.fb.set(key, payload, EnrollmentFlashTTL)
}

// TotpEnrollmentFlashTake consumes the flash; a second take returns false.
func (c *Client) TotpEnrollmentFlashTake(ctx context.Context, sessionHash string) (string, bool) {
	key := enrollmentFlashKey(sessionHash)
	if v, ok := c.vGetDel(ctx, key); ok {
		c.fb.del(key)
		return v, true
	}
	return c.fb.take(key)
}

// ---------------------------------------------------------------------------
// Password failure lockouts
// ---------------------------------------------------------------------------

// Per-(ip, email) failure budget: AuthFailMax within AuthFailWindow.
const (
	AuthFailMax    uint64 = 10
	AuthFailWindow        = 15 * time.Minute
)

// AuthFailKey is the per-(ip, email) counter key.
func AuthFailKey(ip, email string) string {
	return "auth:fail:" + strings.TrimSpace(ip) + ":" + strings.ToLower(strings.TrimSpace(email))
}

// RecordAuthFail counts a failed password attempt. The window escalates
// (60s << count, capped at 15 min) so persistent attackers stay tracked
// longer while a typo followed by success (which clears) is unaffected.
func (c *Client) RecordAuthFail(ctx context.Context, ip, email string) uint64 {
	key := AuthFailKey(ip, email)
	if n, ok := c.vIncr(ctx, key); ok {
		ttl := time.Duration(60<<min(n, 4)) * time.Second
		c.vExpire(ctx, key, min(ttl, AuthFailWindow))
		return n
	}
	return c.fb.incr(key, AuthFailWindow)
}

func (c *Client) count(ctx context.Context, key string) uint64 {
	if v, ok := c.vGet(ctx, key); ok {
		return parseCount(v)
	}
	if v, ok := c.fb.get(key); ok {
		return parseCount(v)
	}
	return 0
}

// AuthFailCount is the current per-(ip, email) failure count.
func (c *Client) AuthFailCount(ctx context.Context, ip, email string) uint64 {
	return c.count(ctx, AuthFailKey(ip, email))
}

// IsAuthLockedOut reports whether the (ip, email) pair hit AuthFailMax.
func (c *Client) IsAuthLockedOut(ctx context.Context, ip, email string) bool {
	return c.AuthFailCount(ctx, ip, email) >= AuthFailMax
}

// ClearAuthFails resets the (ip, email) counter (successful sign-in).
func (c *Client) ClearAuthFails(ctx context.Context, ip, email string) {
	key := AuthFailKey(ip, email)
	c.vDel(ctx, key)
	c.fb.del(key)
}

// Per-email failure budget, independent of IP, so distributed guessing
// against one account still locks out.
const (
	AuthFailEmailMax    uint64 = 30
	AuthFailEmailWindow        = 15 * time.Minute
)

// AuthFailEmailKey is the per-email counter key.
func AuthFailEmailKey(email string) string {
	return "auth:fail-email:" + strings.ToLower(strings.TrimSpace(email))
}

// RecordAuthEmailFail counts a failure against the email alone (fixed window).
func (c *Client) RecordAuthEmailFail(ctx context.Context, email string) uint64 {
	key := AuthFailEmailKey(email)
	if n, ok := c.vIncr(ctx, key); ok {
		if n == 1 {
			c.vExpire(ctx, key, AuthFailEmailWindow)
		}
		return n
	}
	return c.fb.incr(key, AuthFailEmailWindow)
}

// AuthEmailFailCount is the per-email failure count.
func (c *Client) AuthEmailFailCount(ctx context.Context, email string) uint64 {
	return c.count(ctx, AuthFailEmailKey(email))
}

// IsAuthEmailLockedOut reports whether the email hit AuthFailEmailMax.
func (c *Client) IsAuthEmailLockedOut(ctx context.Context, email string) bool {
	return c.AuthEmailFailCount(ctx, email) >= AuthFailEmailMax
}

// ClearAuthEmailFails resets the per-email counter.
func (c *Client) ClearAuthEmailFails(ctx context.Context, email string) {
	key := AuthFailEmailKey(email)
	c.vDel(ctx, key)
	c.fb.del(key)
}

// ---------------------------------------------------------------------------
// TOTP single use + second-factor failure budget
// ---------------------------------------------------------------------------

// TotpReuseKey claims one TOTP code for one user.
func TotpReuseKey(userID uuid.UUID, code string) string {
	return "totp:used:" + simple(userID) + ":" + strings.TrimSpace(code)
}

// totpReuseTTL covers the skew=1 window (prev/current/next 30s step).
const totpReuseTTL = 90 * time.Second

// TotpReserve atomically claims (user, code). False means the code was
// already used in the window: reject as replay.
func (c *Client) TotpReserve(ctx context.Context, userID uuid.UUID, code string) bool {
	key := TotpReuseKey(userID, code)
	if claimed, ok := c.vSetNXEX(ctx, key, "1", totpReuseTTL); ok {
		return claimed
	}
	return c.fb.setNX(key, "1", totpReuseTTL)
}

// Wrong TOTP/backup codes tolerated per user (across every pending token,
// login and step-up path) inside TotpFailWindow.
const (
	TotpFailMax    uint64 = 10
	TotpFailWindow        = 15 * time.Minute
)

func totpFailKey(userID uuid.UUID) string { return "2fa:fail:" + userID.String() }

// RecordTotpFail counts one wrong second-factor code (fixed window from the
// first failure).
func (c *Client) RecordTotpFail(ctx context.Context, userID uuid.UUID) uint64 {
	key := totpFailKey(userID)
	if n, ok := c.vIncr(ctx, key); ok {
		if n == 1 {
			c.vExpire(ctx, key, TotpFailWindow)
		}
		return n
	}
	return c.fb.incr(key, TotpFailWindow)
}

// IsTotpLockedOut reports whether the user exhausted the second-factor budget.
func (c *Client) IsTotpLockedOut(ctx context.Context, userID uuid.UUID) bool {
	return c.count(ctx, totpFailKey(userID)) >= TotpFailMax
}

// ---------------------------------------------------------------------------
// Pending 2FA tokens: attempts + request binding
// ---------------------------------------------------------------------------

// Failed verifications allowed per pending token, and its lifetime.
const (
	PendingMaxAttempts uint64 = 5
	PendingTTL                = 10 * time.Minute
)

func pendingAttemptsKey(tokenHash string) string { return "2fa:attempt:" + tokenHash }
func pendingBindingKey(tokenHash string) string  { return "2fa:bind:" + tokenHash }

// PendingAttempt increments and returns the attempt counter of a token.
func (c *Client) PendingAttempt(ctx context.Context, token string) uint64 {
	key := pendingAttemptsKey(PendingTokenHash(token))
	if n, ok := c.vIncr(ctx, key); ok {
		if n == 1 {
			c.vExpire(ctx, key, PendingTTL)
		}
		return n
	}
	return c.fb.incr(key, PendingTTL)
}

// PendingAttempts reads the attempt counter.
func (c *Client) PendingAttempts(ctx context.Context, token string) uint64 {
	return c.count(ctx, pendingAttemptsKey(PendingTokenHash(token)))
}

// PendingClear drops attempt + binding state after a terminal outcome.
func (c *Client) PendingClear(ctx context.Context, token string) {
	h := PendingTokenHash(token)
	for _, key := range []string{pendingAttemptsKey(h), pendingBindingKey(h)} {
		c.vDel(ctx, key)
		c.fb.del(key)
	}
}

// PendingBindingOutcome is the result of PendingCheckBinding.
type PendingBindingOutcome int

// Binding outcomes.
const (
	// BindingMatched: request IP/UA matches the stored binding.
	BindingMatched PendingBindingOutcome = iota
	// BindingNewlyBound: no binding yet; bound to this request now (first
	// legitimate verifier wins the race).
	BindingNewlyBound
	// BindingMismatch: a different binding exists → reject.
	BindingMismatch
)

func (o PendingBindingOutcome) String() string {
	return [...]string{"Matched", "NewlyBound", "Mismatch"}[o]
}

func bindingValue(userID uuid.UUID, ip, ua string) string {
	return simple(userID) + "|" + IPUAHash(ip, ua)
}

// PendingBindEager binds a freshly minted pending token to (user, ip, ua).
func (c *Client) PendingBindEager(ctx context.Context, token string, userID uuid.UUID, ip, ua string) {
	key := pendingBindingKey(PendingTokenHash(token))
	val := bindingValue(userID, ip, ua)
	c.vSetEX(ctx, key, val, PendingTTL)
	c.fb.del(key)
	_ = c.fb.setNX(key, val, PendingTTL)
}

// PendingCheckBinding verifies the request against the stored binding, or
// binds on first sight.
func (c *Client) PendingCheckBinding(ctx context.Context, token string, userID uuid.UUID, ip, ua string) PendingBindingOutcome {
	key := pendingBindingKey(PendingTokenHash(token))
	want := bindingValue(userID, ip, ua)
	stored, ok := c.vGet(ctx, key)
	if !ok {
		stored, ok = c.fb.get(key)
	}
	if ok {
		if stored == want {
			return BindingMatched
		}
		return BindingMismatch
	}
	claimed, vok := c.vSetNXEX(ctx, key, want, PendingTTL)
	if !vok {
		claimed = c.fb.setNX(key, want, PendingTTL)
	}
	if claimed {
		return BindingNewlyBound
	}
	// Lost the race: re-read and compare.
	if v, ok := c.fb.get(key); ok && v == want {
		return BindingMatched
	}
	return BindingMismatch
}

// ---------------------------------------------------------------------------
// CAPTCHA gate (config only; siteverify is a documented follow-up)
// ---------------------------------------------------------------------------

var warnCaptchaOnce sync.Once

// VerifyCaptchaToken fails open until the Turnstile siteverify integration
// lands: when the gate is not required it returns true; when required, a
// non-blank token is accepted (logged once) and a blank one rejected.
// TODO(wave1-A): real siteverify (POST challenges.cloudflare.com, 5s timeout).
func VerifyCaptchaToken(required bool, token string) bool {
	if !required {
		return true
	}
	if strings.TrimSpace(token) != "" {
		warnCaptchaOnce.Do(func() {
			slog.Warn("TURNSTILE_ENFORCE=1 but server-side siteverify is not implemented; accepting token presence only")
		})
		return true
	}
	return false
}
