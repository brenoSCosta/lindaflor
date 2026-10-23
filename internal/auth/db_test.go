package auth_test

import (
	"context"
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/storage"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

const strong = "Harbor-Violet-Tiger-4821-Lantern!"

func newService(t *testing.T) (*auth.Service, *pgxpool.Pool) {
	t.Helper()
	pool := testutil.NewPool(t)
	cfg := testutil.TestConfig(false)
	return auth.NewService(cfg, pool, nil, storage.NewMemory()), pool
}

func insertCredentialUser(t *testing.T, pool *pgxpool.Pool, email, password string) uuid.UUID {
	t.Helper()
	id := uuid.New()
	hash, err := auth.HashPassword(password)
	if err != nil {
		t.Fatal(err)
	}
	ctx := context.Background()
	if _, err := pool.Exec(ctx, `INSERT INTO users (id, name, email, role) VALUES ($1, 'Hash Test', $2, 'user')`, id, email); err != nil {
		t.Fatal(err)
	}
	if _, err := pool.Exec(ctx, `INSERT INTO accounts (id, account_id, provider_id, user_id, password) VALUES ($1, $2, 'credential', $3, $4)`,
		uuid.New(), id.String(), id, hash); err != nil {
		t.Fatal(err)
	}
	return id
}

func count(t *testing.T, pool *pgxpool.Pool, sql string, args ...any) int64 {
	t.Helper()
	var n int64
	if err := pool.QueryRow(context.Background(), sql, args...).Scan(&n); err != nil {
		t.Fatal(err)
	}
	return n
}

// --- src/auth/service.rs tests ---

func TestHashedResetLookupStoresEmptyValue(t *testing.T) {
	svc, pool := newService(t)
	ctx := context.Background()
	email := testutil.UniqueEmail("hash-reset")
	insertCredentialUser(t, pool, email, "password123")
	if err := svc.RequestPasswordReset(ctx, email, ""); err != nil {
		t.Fatal(err)
	}
	identifier := auth.ResetIdentifierPrefix + email
	var value string
	var hash *string
	if err := pool.QueryRow(ctx, `SELECT value, value_hash FROM verifications WHERE identifier = $1`, identifier).Scan(&value, &hash); err != nil {
		t.Fatal(err)
	}
	if value != "" || hash == nil || len(*hash) != 64 {
		t.Fatal("value must be empty at rest, hash 64 hex")
	}
	raw := "known-raw-reset-secret"
	if _, err := pool.Exec(ctx, `UPDATE verifications SET value_hash = $1 WHERE identifier = $2`, svc.TokenHash(raw), identifier); err != nil {
		t.Fatal(err)
	}
	if err := svc.ResetPassword(ctx, raw, strong); err != nil {
		t.Fatal(err)
	}
	if n := count(t, pool, `SELECT count(*) FROM verifications WHERE identifier = $1`, identifier); n != 0 {
		t.Fatalf("%d tokens left", n)
	}
	// Mail queued (pt-BR, throttled kind).
	if n := count(t, pool, `SELECT count(*) FROM email_outbox WHERE to_address = $1 AND kind = 'password_reset'`, email); n != 1 {
		t.Fatalf("%d reset mails", n)
	}
}

func TestExpiredResetTokenIsRejected(t *testing.T) {
	svc, pool := newService(t)
	ctx := context.Background()
	email := testutil.UniqueEmail("hash-expired")
	insertCredentialUser(t, pool, email, "password123")
	raw := "expired-raw-reset-secret"
	hash := svc.TokenHash(raw)
	q := sqlc.New(pool)
	if err := q.AuthInsertVerification(ctx, sqlc.AuthInsertVerificationParams{
		ID: uuid.New(), Identifier: auth.ResetIdentifierPrefix + email, ValueHash: &hash, ExpiresAt: time.Now().Add(-time.Minute),
	}); err != nil {
		t.Fatal(err)
	}
	err := svc.ResetPassword(ctx, raw, strong)
	if !errors.Is(err, auth.ErrInvalidToken) || !strings.Contains(err.Error(), "invalid or expired") {
		t.Fatalf("got %v", err)
	}
}

func TestRequestResetDeletesPriorTokens(t *testing.T) {
	svc, pool := newService(t)
	ctx := context.Background()
	email := testutil.UniqueEmail("hash-dedupe")
	insertCredentialUser(t, pool, email, "password123")
	for range 2 {
		if err := svc.RequestPasswordReset(ctx, email, ""); err != nil {
			t.Fatal(err)
		}
	}
	if n := count(t, pool, `SELECT count(*) FROM verifications WHERE identifier = $1`, auth.ResetIdentifierPrefix+email); n != 1 {
		t.Fatalf("%d tokens", n)
	}
}

func TestRequestResetUnknownEmailIsSilent(t *testing.T) {
	svc, pool := newService(t)
	if err := svc.RequestPasswordReset(context.Background(), testutil.UniqueEmail("nobody"), ""); err != nil {
		t.Fatal(err)
	}
	if err := svc.RequestPasswordReset(context.Background(), "   ", ""); err != nil {
		t.Fatal(err)
	}
	if n := count(t, pool, `SELECT count(*) FROM verifications`); n != 0 {
		t.Fatalf("%d tokens", n)
	}
}

func TestAuthMailIsCappedPerRecipient(t *testing.T) {
	svc, pool := newService(t)
	ctx := context.Background()
	email := testutil.UniqueEmail("capped")
	insertCredentialUser(t, pool, email, "password123")
	for range 7 {
		if err := svc.RequestPasswordReset(ctx, email, ""); err != nil {
			t.Fatal(err)
		}
	}
	if n := count(t, pool, `SELECT count(*) FROM email_outbox WHERE to_address = $1`, email); n != 5 {
		t.Fatalf("throttled to 5 per hour, got %d", n)
	}
}

// --- src/auth/bootstrap.rs tests ---

func strongBootstrap() *string { s := "correct horse battery staple 42!"; return &s }

func TestCreatesVerifiedAdminWithCredential(t *testing.T) {
	pool := testutil.NewPool(t)
	ctx := context.Background()
	email := testutil.UniqueEmail("Boot")
	out, err := auth.CreateOrPromoteAdmin(ctx, pool, strings.ToUpper(email), "Dona", strongBootstrap())
	if err != nil {
		t.Fatal(err)
	}
	if !out.Created {
		t.Fatal("expected Created")
	}
	var role, stored string
	var verified bool
	if err := pool.QueryRow(ctx, `SELECT role, email_verified, email FROM users WHERE id = $1`, out.UserID).Scan(&role, &verified, &stored); err != nil {
		t.Fatal(err)
	}
	if role != "admin" || !verified || stored != strings.ToLower(email) {
		t.Fatalf("%s %v %s", role, verified, stored)
	}
	var hash string
	if err := pool.QueryRow(ctx, `SELECT password FROM accounts WHERE user_id = $1 AND provider_id = 'credential'`, out.UserID).Scan(&hash); err != nil {
		t.Fatal(err)
	}
	if ok, _ := auth.VerifyPassword(*strongBootstrap(), hash); !ok {
		t.Fatal("credential must verify")
	}
	if n := count(t, pool, `SELECT count(*) FROM audit_events WHERE entity_id = $1 AND actor_name = $2`, out.UserID, auth.BootstrapActor); n != 1 {
		t.Fatalf("%d audit rows", n)
	}
}

func TestPromotesExistingUserAndOptionallyResetsPassword(t *testing.T) {
	pool := testutil.NewPool(t)
	ctx := context.Background()
	email := testutil.UniqueEmail("promote")
	id := uuid.New()
	if _, err := pool.Exec(ctx, `INSERT INTO users (id, name, email, email_verified, role) VALUES ($1, 'Cliente', $2, false, 'user')`, id, email); err != nil {
		t.Fatal(err)
	}
	out, err := auth.CreateOrPromoteAdmin(ctx, pool, email, "Ignorado", nil)
	if err != nil || out.Created || out.UserID != id {
		t.Fatalf("%+v %v", out, err)
	}
	if n := count(t, pool, `SELECT count(*) FROM users WHERE id = $1 AND role = 'admin' AND email_verified`, id); n != 1 {
		t.Fatal("promoted + verified")
	}
	if n := count(t, pool, `SELECT count(*) FROM accounts WHERE user_id = $1`, id); n != 0 {
		t.Fatal("promotion without password leaves credentials alone")
	}
	if _, err := auth.CreateOrPromoteAdmin(ctx, pool, email, "Ignorado", strongBootstrap()); err != nil {
		t.Fatal(err)
	}
	other := "another quite long passphrase 99?"
	if _, err := auth.CreateOrPromoteAdmin(ctx, pool, email, "Ignorado", &other); err != nil {
		t.Fatal(err)
	}
	var hash string
	if err := pool.QueryRow(ctx, `SELECT password FROM accounts WHERE user_id = $1`, id).Scan(&hash); err != nil {
		t.Fatal(err)
	}
	if ok, _ := auth.VerifyPassword(other, hash); !ok {
		t.Fatal("password replaced")
	}
	if n := count(t, pool, `SELECT count(*) FROM accounts WHERE user_id = $1`, id); n != 1 {
		t.Fatalf("%d accounts", n)
	}
}

func TestRejectsWeakPasswordBadEmailAndMissingPassword(t *testing.T) {
	pool := testutil.NewPool(t)
	ctx := context.Background()
	email := testutil.UniqueEmail("reject")
	weak := "admin123"
	if _, err := auth.CreateOrPromoteAdmin(ctx, pool, email, "Dona", &weak); err == nil || !strings.Contains(err.Error(), "password") {
		t.Fatalf("weak: %v", err)
	}
	if _, err := auth.CreateOrPromoteAdmin(ctx, pool, "not-an-email", "Dona", strongBootstrap()); err == nil || !strings.Contains(err.Error(), "invalid email") {
		t.Fatalf("email: %v", err)
	}
	if _, err := auth.CreateOrPromoteAdmin(ctx, pool, email, "Dona", nil); !errors.Is(err, auth.ErrPasswordRequired) {
		t.Fatalf("missing: %v", err)
	}
	if n := count(t, pool, `SELECT count(*) FROM users WHERE email = $1`, email); n != 0 {
		t.Fatal("nothing created")
	}
}
