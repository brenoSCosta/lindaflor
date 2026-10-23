// Package app holds the dependency container shared by every handler.
package app

import (
	"log/slog"
	"net/http"
	"strconv"
	"time"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/captcha"
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
	Mailer   mail.Mailer // log or SMTP transport (the outbox worker sends)
	Storage  storage.Store
	Payments payments.Provider
	// Orders is the order FSM seen by payments (webhook, sweep, admin
	// sync/refund): store.OrderFSM in production; nil makes payment syncs
	// fail with payments.ErrOrdersNotWired.
	Orders payments.OrderTransitioner
	Logger *slog.Logger
	Shell  ShellLoader
	// Captcha verifies Turnstile tokens on the abuse-prone forms (nil or
	// unconfigured = no-op; NewRouter builds it from Cfg.Captcha).
	Captcha *captcha.Verifier
}

// BuildID identifies this server process. Development renders it into the
// page so the live-reload script (static/js/dev-reload.js) notices an air
// rebuild and reloads the browser. It is stable for the process lifetime.
var BuildID = strconv.FormatInt(time.Now().UnixNano(), 36)
