// Package app holds the dependency container shared by every handler.
package app

import (
	"log/slog"
	"net/http"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/mail"
	"github.com/wellington-vell/lindaflor/internal/payments"
	"github.com/wellington-vell/lindaflor/internal/storage"
	"github.com/wellington-vell/lindaflor/internal/valkey"
	"github.com/wellington-vell/lindaflor/internal/web/layout"
)

// ShellLoader loads the per-request shell data (store settings, nav sets,
// cart count, avatar...). The storefront package provides it.
type ShellLoader func(r *http.Request) (layout.ShellData, error)

// Deps is built once in cmd/server (and testutil) and passed to every
// feature package's Mount. Fields are never nil after construction
// (Pool may be nil only in unit tests that do not touch the database).
type Deps struct {
	Cfg      *config.Config
	Pool     *pgxpool.Pool
	Q        *sqlc.Queries
	Valkey   *valkey.Client
	Mailer   mail.Mailer // nil until wave1-B wires a transport
	Storage  storage.Store
	Payments payments.Provider
	Logger   *slog.Logger
	Shell    ShellLoader
}
