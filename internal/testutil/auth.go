package testutil

import (
	"context"
	"testing"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/auth"
)

// Auth helpers mirroring crates/test-support (verification_token*,
// totp_code). Raw one-time tokens only ever exist in mail bodies, so tests
// plant a known token on the newest matching verifications row.

// VerificationToken plants a fresh raw token on the newest row whose
// identifier starts with prefix and returns it.
func (s *Server) VerificationToken(t testing.TB, prefix string) string {
	t.Helper()
	return PlantVerificationToken(t, s.Pool, s.Deps.Cfg.TokenPepper, prefix+"%")
}

// VerificationTokenLike is VerificationToken with an explicit LIKE pattern
// (e.g. "change-email:%:<new email>") so parallel tests never consume each
// other's rows.
func (s *Server) VerificationTokenLike(t testing.TB, like string) string {
	t.Helper()
	return PlantVerificationToken(t, s.Pool, s.Deps.Cfg.TokenPepper, like)
}

// PlantVerificationToken replaces the hash of the newest verifications row
// matching like with the peppered hash of a new random token, asserts the
// raw value column stays empty, and returns the raw token.
func PlantVerificationToken(t testing.TB, pool *pgxpool.Pool, pepper, like string) string {
	t.Helper()
	token := auth.RandomToken()
	hash := auth.VerificationTokenHashWithPepper(token, pepper)
	var value string
	var stored *string
	err := pool.QueryRow(context.Background(), `
		UPDATE verifications SET value = '', value_hash = $1, updated_at = now()
		WHERE id = (SELECT id FROM verifications WHERE identifier LIKE $2 ORDER BY created_at DESC LIMIT 1)
		RETURNING value, value_hash`, hash, like).Scan(&value, &stored)
	if err != nil {
		t.Fatalf("plant verification token (%s): %v", like, err)
	}
	if value != "" {
		t.Fatalf("raw token must not be stored in value")
	}
	if stored == nil || *stored != hash {
		t.Fatalf("value_hash not planted")
	}
	return token
}

// TOTPCode is the current code of a base32 secret (SHA1/6/30s).
func TOTPCode(t testing.TB, secret string) string {
	t.Helper()
	return TOTPCodeAt(t, secret, time.Now())
}

// TOTPCodeAt is the code at instant at (use time.Now().Add(30*time.Second)
// for a next-step code no replay cache has seen).
func TOTPCodeAt(t testing.TB, secret string, at time.Time) string {
	t.Helper()
	code, err := auth.TOTPCode(secret, at)
	if err != nil {
		t.Fatalf("totp code: %v", err)
	}
	return code
}
