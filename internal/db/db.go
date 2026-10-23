// Package db owns the Postgres pool, schema migrations (goose, embedded) and
// small SQL helpers. Generated query code lives in internal/db/sqlc.
package db

import (
	"context"
	"errors"
	"fmt"
	"io/fs"
	"log/slog"
	"strings"
	"time"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"
	"github.com/jackc/pgx/v5/stdlib"
	"github.com/pressly/goose/v3"

	dbfiles "github.com/wellington-vell/lindaflor/db"
	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// DBTX is satisfied by *pgxpool.Pool, *pgxpool.Conn and pgx.Tx. Functions
// that must run inside a caller's transaction (audit.Record, mail.Enqueue,
// order FSM...) take a DBTX.
type DBTX = sqlc.DBTX

// PoolOptions configures Open. Zero values fall back to the config defaults.
type PoolOptions struct {
	MaxConns       int32
	ConnectTimeout int // seconds
	IdleTimeout    int // seconds
	// MigrateOnBoot applies pending migrations; otherwise pending
	// migrations abort startup (the migrate job owns the schema).
	MigrateOnBoot bool
}

// OptionsFromConfig maps the app config to PoolOptions.
func OptionsFromConfig(cfg *config.Config) PoolOptions {
	return PoolOptions{
		MaxConns:       cfg.PGMaxConns,
		ConnectTimeout: int(cfg.PGAcquireTimeout.Seconds()),
		IdleTimeout:    int(cfg.PGIdleTimeout.Seconds()),
		MigrateOnBoot:  cfg.MigrateOnBoot,
	}
}

// PendingMigrationsError aborts boot when MIGRATE_ON_BOOT is off and the
// schema is behind.
type PendingMigrationsError struct{ Versions []int64 }

func (e *PendingMigrationsError) Error() string {
	return fmt.Sprintf("pending migrations %v; run the migrate job (`goose up` / `make migrate`) before starting the web process", e.Versions)
}

// Open builds the pool and then migrates (or verifies) the schema.
func Open(ctx context.Context, databaseURL string, opts PoolOptions) (*pgxpool.Pool, error) {
	pcfg, err := pgxpool.ParseConfig(databaseURL)
	if err != nil {
		return nil, fmt.Errorf("parse DATABASE_URL: %w", err)
	}
	if opts.MaxConns > 0 {
		pcfg.MaxConns = opts.MaxConns
	}
	if opts.ConnectTimeout > 0 {
		pcfg.ConnConfig.ConnectTimeout = secs(opts.ConnectTimeout)
	}
	if opts.IdleTimeout > 0 {
		pcfg.MaxConnIdleTime = secs(opts.IdleTimeout)
	}
	pool, err := pgxpool.NewWithConfig(ctx, pcfg)
	if err != nil {
		return nil, fmt.Errorf("connect postgres: %w", err)
	}
	if err := pool.Ping(ctx); err != nil {
		pool.Close()
		return nil, fmt.Errorf("ping postgres: %w", err)
	}
	if opts.MigrateOnBoot {
		err = Migrate(ctx, pool)
	} else {
		err = RequireSchemaCurrent(ctx, pool)
	}
	if err != nil {
		pool.Close()
		return nil, err
	}
	return pool, nil
}

// MigrationsFS is the embedded migrations directory.
func MigrationsFS() fs.FS {
	sub, err := fs.Sub(dbfiles.Migrations, dbfiles.MigrationsDir)
	if err != nil {
		panic(err) // static embed layout
	}
	return sub
}

func newProvider(pool *pgxpool.Pool) (*goose.Provider, func(), error) {
	sqlDB := stdlib.OpenDBFromPool(pool)
	p, err := goose.NewProvider(goose.DialectPostgres, sqlDB, MigrationsFS())
	if err != nil {
		_ = sqlDB.Close()
		return nil, nil, fmt.Errorf("goose provider: %w", err)
	}
	return p, func() { _ = sqlDB.Close() }, nil
}

// Migrate applies every pending migration.
func Migrate(ctx context.Context, pool *pgxpool.Pool) error {
	p, closeFn, err := newProvider(pool)
	if err != nil {
		return err
	}
	defer closeFn()
	results, err := p.Up(ctx)
	if err != nil {
		return fmt.Errorf("migrate: %w", err)
	}
	for _, r := range results {
		slog.InfoContext(ctx, "migration applied", "version", r.Source.Version, "file", r.Source.Path, "duration", r.Duration)
	}
	return nil
}

// RequireSchemaCurrent returns *PendingMigrationsError when the database
// is missing any embedded migration.
func RequireSchemaCurrent(ctx context.Context, pool *pgxpool.Pool) error {
	p, closeFn, err := newProvider(pool)
	if err != nil {
		return err
	}
	defer closeFn()
	statuses, err := p.Status(ctx)
	if err != nil {
		return fmt.Errorf("migration status: %w", err)
	}
	var pending []int64
	for _, s := range statuses {
		if s.State == goose.StatePending {
			pending = append(pending, s.Source.Version)
		}
	}
	if len(pending) > 0 {
		return &PendingMigrationsError{Versions: pending}
	}
	return nil
}

// WithTx runs fn in a transaction, committing when it returns nil and
// rolling back otherwise (including on panic, which is re-raised).
func WithTx(ctx context.Context, pool *pgxpool.Pool, fn func(tx pgx.Tx) error) (err error) {
	tx, err := pool.Begin(ctx)
	if err != nil {
		return fmt.Errorf("begin: %w", err)
	}
	defer func() {
		if p := recover(); p != nil {
			_ = tx.Rollback(ctx)
			panic(p)
		}
		if err != nil {
			if rbErr := tx.Rollback(ctx); rbErr != nil && !errors.Is(rbErr, pgx.ErrTxClosed) {
				err = errors.Join(err, rbErr)
			}
		}
	}()
	if err = fn(tx); err != nil {
		return err
	}
	return tx.Commit(ctx)
}

// EscapeLike escapes %, _ and \ so user input matches literally inside a
// LIKE/ILIKE pattern. Always pair it with an explicit `ESCAPE '\'`.
func EscapeLike(input string) string {
	var b strings.Builder
	b.Grow(len(input))
	for _, r := range input {
		if r == '%' || r == '_' || r == '\\' {
			b.WriteByte('\\')
		}
		b.WriteRune(r)
	}
	return b.String()
}

// LikeContains is `%<escaped>%` for `ILIKE $n ESCAPE '\'`.
func LikeContains(input string) string { return "%" + EscapeLike(input) + "%" }

func secs(n int) time.Duration { return time.Duration(n) * time.Second }
