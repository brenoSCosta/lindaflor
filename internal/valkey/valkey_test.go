package valkey

import (
	"context"
	"strings"
	"testing"
	"time"

	"github.com/alicebob/miniredis/v2"
	"github.com/google/uuid"
	"github.com/redis/go-redis/v9"
)

// clients runs each test against the fallback and a miniredis-backed client.
func clients(t *testing.T) map[string]*Client {
	t.Helper()
	mr := miniredis.RunT(t)
	rdb := redis.NewClient(&redis.Options{Addr: mr.Addr()})
	t.Cleanup(func() { _ = rdb.Close() })
	return map[string]*Client{"fallback": NewFallbackOnly(), "valkey": NewWithRedis(rdb)}
}

func TestTotpReserveSecondClaimFails(t *testing.T) {
	ctx := context.Background()
	for name, c := range clients(t) {
		user := uuid.Must(uuid.NewV7())
		if !c.TotpReserve(ctx, user, "123456") {
			t.Fatalf("%s: first claim", name)
		}
		if c.TotpReserve(ctx, user, "123456") {
			t.Fatalf("%s: replay must fail", name)
		}
		if !c.TotpReserve(ctx, user, "654321") {
			t.Fatalf("%s: other code claimable", name)
		}
	}
}

func TestPendingCheckBindingMismatch(t *testing.T) {
	ctx := context.Background()
	for name, c := range clients(t) {
		user := uuid.Must(uuid.NewV7())
		token := "unit-pending-bind-token"
		c.PendingBindEager(ctx, token, user, "1.1.1.1", "ua-a")
		if got := c.PendingCheckBinding(ctx, token, user, "2.2.2.2", "ua-a"); got != BindingMismatch {
			t.Fatalf("%s: got %v", name, got)
		}
		if got := c.PendingCheckBinding(ctx, token, user, "1.1.1.1", "ua-a"); got != BindingMatched {
			t.Fatalf("%s: got %v", name, got)
		}
		other := "fresh-token"
		if got := c.PendingCheckBinding(ctx, other, user, "3.3.3.3", "ua"); got != BindingNewlyBound {
			t.Fatalf("%s: first sight: %v", name, got)
		}
		if got := c.PendingCheckBinding(ctx, other, user, "3.3.3.3", "ua"); got != BindingMatched {
			t.Fatalf("%s: second sight: %v", name, got)
		}
	}
}

func TestPerEmailLockoutThreshold(t *testing.T) {
	ctx := context.Background()
	for name, c := range clients(t) {
		email := "per-email-lockout-" + uuid.NewString() + "@example.com"
		if c.IsAuthEmailLockedOut(ctx, email) {
			t.Fatal(name)
		}
		for range AuthFailEmailMax - 1 {
			c.RecordAuthEmailFail(ctx, email)
		}
		if c.IsAuthEmailLockedOut(ctx, email) {
			t.Fatalf("%s: locked too early", name)
		}
		c.RecordAuthEmailFail(ctx, strings.ToUpper(email))
		if c.AuthEmailFailCount(ctx, email) != AuthFailEmailMax || !c.IsAuthEmailLockedOut(ctx, email) {
			t.Fatalf("%s: case-insensitive threshold", name)
		}
		if c.IsAuthEmailLockedOut(ctx, "per-email-other-"+uuid.NewString()+"@example.com") {
			t.Fatalf("%s: other email affected", name)
		}
		c.ClearAuthEmailFails(ctx, email)
		if c.IsAuthEmailLockedOut(ctx, email) || c.AuthEmailFailCount(ctx, email) != 0 {
			t.Fatalf("%s: clear", name)
		}
	}
}

func TestAuthFailEscalatingWindowAndLockout(t *testing.T) {
	ctx := context.Background()
	mr := miniredis.RunT(t)
	c := NewWithRedis(redis.NewClient(&redis.Options{Addr: mr.Addr()}))
	for i := range AuthFailMax {
		if n := c.RecordAuthFail(ctx, " 1.2.3.4 ", "A@X.com"); n != i+1 {
			t.Fatalf("count %d", n)
		}
	}
	if !c.IsAuthLockedOut(ctx, "1.2.3.4", "a@x.com") {
		t.Fatal("must lock out")
	}
	if ttl := mr.TTL(AuthFailKey("1.2.3.4", "a@x.com")); ttl != AuthFailWindow {
		t.Fatalf("ttl capped at window, got %v", ttl)
	}
	c.ClearAuthFails(ctx, "1.2.3.4", "a@x.com")
	if c.AuthFailCount(ctx, "1.2.3.4", "a@x.com") != 0 {
		t.Fatal("cleared")
	}
}

func TestEnrollmentFlashIsOneTime(t *testing.T) {
	ctx := context.Background()
	for name, c := range clients(t) {
		c.TotpEnrollmentFlashPut(ctx, "sess", `{"secret":"x"}`)
		if v, ok := c.TotpEnrollmentFlashTake(ctx, "sess"); !ok || v != `{"secret":"x"}` {
			t.Fatalf("%s: first take", name)
		}
		if _, ok := c.TotpEnrollmentFlashTake(ctx, "sess"); ok {
			t.Fatalf("%s: second take must be empty", name)
		}
	}
}

func TestTotpLockoutAndPendingAttempts(t *testing.T) {
	ctx := context.Background()
	for name, c := range clients(t) {
		user := uuid.New()
		for range TotpFailMax {
			c.RecordTotpFail(ctx, user)
		}
		if !c.IsTotpLockedOut(ctx, user) {
			t.Fatalf("%s: totp lockout", name)
		}
		for range 3 {
			c.PendingAttempt(ctx, "tok")
		}
		if c.PendingAttempts(ctx, " tok ") != 3 {
			t.Fatalf("%s: attempts (token trimmed)", name)
		}
		c.PendingClear(ctx, "tok")
		if c.PendingAttempts(ctx, "tok") != 0 {
			t.Fatalf("%s: cleared", name)
		}
	}
}

func TestKeysAndHashes(t *testing.T) {
	id := uuid.MustParse("0199c1a0-0000-7000-8000-000000000001")
	if got := TotpReuseKey(id, " 123456 "); got != "totp:used:0199c1a0000070008000000000000001:123456" {
		t.Fatal(got)
	}
	if len(IPUAHash("1.1.1.1", "ua")) != 32 || IPUAHash("a", "b") == IPUAHash("ab", "") {
		t.Fatal("ip/ua hash must separate fields")
	}
}

func TestFallbackExpiry(t *testing.T) {
	f := newFallback()
	f.set("k", "v", time.Millisecond)
	time.Sleep(5 * time.Millisecond)
	if _, ok := f.get("k"); ok {
		t.Fatal("expired entry returned")
	}
	if !f.setNX("k", "v", time.Minute) || f.setNX("k", "w", time.Minute) {
		t.Fatal("setNX")
	}
}
