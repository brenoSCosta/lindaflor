package testutil

import (
	"context"
	"net/http"
	"testing"
)

// No TestMain here on purpose: the postmaster must still die with the test
// binary (Pdeathsig).
func TestPoolAndServerHelpers(t *testing.T) {
	pool := NewPool(t)
	var n int
	if err := pool.QueryRow(context.Background(), "SELECT count(*) FROM goose_db_version WHERE version_id > 0").Scan(&n); err != nil || n == 0 {
		t.Fatalf("template not migrated: %v", err)
	}
	s := NewServer(t, Options{Prod: true})
	c := s.Client(t)
	res := c.Get("/api/health")
	if res.StatusCode != http.StatusOK || c.SessionCookieName() != ProdSessionCookie {
		t.Fatal(res.StatusCode)
	}
	if a, b := UniqueEmail("x"), UniqueEmail("x"); a == b {
		t.Fatal("unique")
	}
}

func TestCookieAssertions(t *testing.T) {
	line := "__Host-session=abc; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax"
	AssertExpiredHardenedCookie(t, line, "__Host-session")
	AssertExpiredHardenedCookieDev(t, "session=; Path=/; Max-Age=0; HttpOnly; SameSite=Lax", "session")
	if RedactSetCookie(line) != "__Host-session=***; Path=/; Max-Age=0; HttpOnly; Secure; SameSite=Lax" {
		t.Fatal(RedactSetCookie(line))
	}
	if _, ok := FindSetCookie([]string{line}, "__Host-session"); !ok {
		t.Fatal("find")
	}
}
