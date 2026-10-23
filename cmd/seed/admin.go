package main

import (
	"context"
	"errors"
	"log/slog"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/config"
)

// credentialProviderID is accounts.provider_id for email/password.
const credentialProviderID = auth.CredentialProviderID

// hashPassword is auth's argon2id PHC hash (m=19456, t=2, p=1).
func hashPassword(password string) (string, error) { return auth.HashPassword(password) }

func seedAdmin(ctx context.Context, pool *pgxpool.Pool, admin config.SeedAdmin) error {
	var existing uuid.UUID
	err := pool.QueryRow(ctx, `SELECT id FROM users WHERE email = $1`, admin.Email).Scan(&existing)
	if err == nil {
		slog.InfoContext(ctx, "Admin user already exists", "email", admin.Email)
		return nil
	}
	if !errors.Is(err, pgx.ErrNoRows) {
		return err
	}
	if admin.Password == "admin123" {
		slog.WarnContext(ctx, "seeding admin with the documented dev default password; set SEED_ADMIN_PASSWORD for anything shared")
	}
	hash, err := hashPassword(admin.Password)
	if err != nil {
		return err
	}
	userID := uuid.Must(uuid.NewV7())
	accountID := uuid.Must(uuid.NewV7())
	tx, err := pool.Begin(ctx)
	if err != nil {
		return err
	}
	defer func() { _ = tx.Rollback(ctx) }()
	if _, err := tx.Exec(ctx, `INSERT INTO users (id, name, email, email_verified, role, created_at, updated_at)
		VALUES ($1, $2, $3, true, 'admin', now(), now())`, userID, admin.Name, admin.Email); err != nil {
		return err
	}
	if _, err := tx.Exec(ctx, `INSERT INTO accounts (id, account_id, provider_id, user_id, password, created_at, updated_at)
		VALUES ($1, $2, $3, $4, $5, now(), now())`, accountID, userID.String(), credentialProviderID, userID, hash); err != nil {
		return err
	}
	if err := tx.Commit(ctx); err != nil {
		return err
	}
	slog.InfoContext(ctx, "Admin user created", "email", admin.Email)
	return nil
}
