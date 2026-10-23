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
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// credentialProviderID is accounts.provider_id for email/password.
const credentialProviderID = auth.CredentialProviderID

// hashPassword is auth's argon2id PHC hash (m=19456, t=2, p=1).
func hashPassword(password string) (string, error) { return auth.HashPassword(password) }

func seedAdmin(ctx context.Context, pool *pgxpool.Pool, admin config.SeedAdmin) error {
	_, err := sqlc.New(pool).SeedUserIDByEmail(ctx, admin.Email)
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
	q := sqlc.New(tx)
	if err := q.SeedInsertAdminUser(ctx, sqlc.SeedInsertAdminUserParams{ID: userID, Name: admin.Name, Email: admin.Email}); err != nil {
		return err
	}
	if err := q.SeedInsertCredentialAccount(ctx, sqlc.SeedInsertCredentialAccountParams{
		ID: accountID, AccountID: userID.String(), ProviderID: credentialProviderID, UserID: userID, Password: &hash,
	}); err != nil {
		return err
	}
	if err := tx.Commit(ctx); err != nil {
		return err
	}
	slog.InfoContext(ctx, "Admin user created", "email", admin.Email)
	return nil
}
