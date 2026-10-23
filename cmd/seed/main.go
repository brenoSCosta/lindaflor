// Command seed fills a development database (catalog, admin user, store
// settings, warehouse...).
//
// TODO(wave1-C): port src/bin/seed (catalog.json, commerce, admin seeders).
package main

import (
	"context"
	"fmt"
	"log/slog"
	"os"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/db"
)

func main() {
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
	fmt.Printf("seed: not implemented yet (would seed admin %s)\n", admin.Email)
	return nil
}
