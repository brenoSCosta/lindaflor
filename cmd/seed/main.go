// Command seed fills a database with the admin user, store settings, the
// default warehouse, the catalog (data/catalog.json), looks and sets
// (port of src/bin/seed). Idempotent: existing rows are left alone.
//
// Env: DATABASE_URL, SEED_ADMIN_EMAIL/PASSWORD/NAME, SEED_PRODUCT_COUNT
// (108), SEED_SET_COUNT (12), SEED_PRICE_MIN/MAX (3990/29990),
// SEED_STOCK_MIN/MAX (5/30), SEED_PIX_KEY, SEED_WHATSAPP_NUMBER,
// SEED_WHATSAPP_MESSAGE_TEMPLATE.
package main

import (
	"context"
	"log/slog"
	"os"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/db"
)

func main() {
	slog.SetDefault(slog.New(slog.NewTextHandler(os.Stderr, nil)))
	if err := run(context.Background()); err != nil {
		slog.Error("seed failed", "error", err)
		os.Exit(1)
	}
}

func run(ctx context.Context) error {
	dbURL, ok := config.ReadEnvOrFile("DATABASE_URL")
	if !ok {
		return &config.MissingVarError{Key: "DATABASE_URL"}
	}
	admin, err := config.SeedAdminFromEnv()
	if err != nil {
		return err
	}
	pool, err := db.Open(ctx, dbURL, db.PoolOptions{MigrateOnBoot: config.MigrateOnBoot()})
	if err != nil {
		return err
	}
	defer pool.Close()
	return seedAll(ctx, pool, admin)
}

func seedAll(ctx context.Context, pool *pgxpool.Pool, admin config.SeedAdmin) error {
	if err := seedAdmin(ctx, pool, admin); err != nil {
		return err
	}
	if err := seedCommerce(ctx, pool); err != nil {
		return err
	}
	slog.InfoContext(ctx, "Seed completed.")
	return nil
}
