// Package store is the commerce domain: catalog queries, cart, inventory
// holds, coupons, pricing, shipping, checkout, the order state machine,
// returns, reviews, reports, wishlist, stock alerts, abandoned carts,
// addresses, store settings and the data side of every admin page.
//
// It is a port of src/app/store/*.rs plus the pure-DB parts of
// src/app/{admin,pedido,conta,...}. No HTML lives here: functions return
// view-ready structs and pt-BR messages copied verbatim from the Rust code.
//
// SQL lives in db/queries/store_*.sql and runs through the sqlc-generated
// internal/db/sqlc package (queries(q) wraps a pool, connection or
// transaction). The only hand-built statement is the catalog listing in
// catalog.go (see the comment there); it is covered by its tests.
//
// Concurrency rules (see inventory.go): inventory rows are always locked in
// one `ORDER BY i.id FOR UPDATE` statement, and an order row is locked
// before its inventory rows. Keep it that way.
package store

import (
	"context"
	"errors"

	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/db"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// DBTX is a pool, a connection or a transaction.
type DBTX = db.DBTX

// UserError is a validation / business-rule failure whose message is shown
// to the customer or admin as-is (pt-BR).
type UserError struct{ Msg string }

func (e *UserError) Error() string { return e.Msg }

// userErr builds a *UserError.
func userErr(msg string) error { return &UserError{Msg: msg} }

// UserMessage returns the pt-BR message of a *UserError (or of a wrapped
// one) and whether err carried one.
func UserMessage(err error) (string, bool) {
	var ue *UserError
	if errors.As(err, &ue) {
		return ue.Msg, true
	}
	return "", false
}

// IsUserError reports whether err is (or wraps) a *UserError.
func IsUserError(err error) bool {
	_, ok := UserMessage(err)
	return ok
}

// queries returns the sqlc query set bound to q (pool, conn or tx).
func queries(q DBTX) *sqlc.Queries { return sqlc.New(q) }

// mapSlice converts every element of in with f (never nil).
func mapSlice[T, U any](in []T, f func(T) U) []U {
	out := make([]U, 0, len(in))
	for _, v := range in {
		out = append(out, f(v))
	}
	return out
}

// noRows maps pgx.ErrNoRows to (found=false, nil).
func noRows[T any](v T, err error) (T, bool, error) {
	if errors.Is(err, pgx.ErrNoRows) {
		var zero T
		return zero, false, nil
	}
	if err != nil {
		var zero T
		return zero, false, err
	}
	return v, true, nil
}

// withTx runs fn in a transaction on pool.
func withTx(ctx context.Context, pool *pgxpool.Pool, fn func(tx pgx.Tx) error) error {
	return db.WithTx(ctx, pool, fn)
}

// collect scans every row of a query into T by column name (`db` tags).
func collect[T any](rows pgx.Rows, err error) ([]T, error) {
	if err != nil {
		return nil, err
	}
	out, err := pgx.CollectRows(rows, pgx.RowToStructByName[T])
	if err != nil {
		return nil, err
	}
	if out == nil {
		out = []T{}
	}
	return out, nil
}

// collectOne scans exactly one row into T, (nil, nil) when there is none.
func collectOne[T any](rows pgx.Rows, err error) (*T, error) {
	if err != nil {
		return nil, err
	}
	v, err := pgx.CollectOneRow(rows, pgx.RowToAddrOfStructByName[T])
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, nil
	}
	return v, err
}

// scalars collects a single-column result.
func scalars[T any](rows pgx.Rows, err error) ([]T, error) {
	if err != nil {
		return nil, err
	}
	out, err := pgx.CollectRows(rows, pgx.RowTo[T])
	if err != nil {
		return nil, err
	}
	if out == nil {
		out = []T{}
	}
	return out, nil
}

// optional maps pgx.ErrNoRows to found=false.
func optional(err error) (found bool, _ error) {
	if errors.Is(err, pgx.ErrNoRows) {
		return false, nil
	}
	if err != nil {
		return false, err
	}
	return true, nil
}

// ptr returns a pointer to v.
func ptr[T any](v T) *T { return &v }

// deref returns *p or the zero value.
func deref[T any](p *T) T {
	var zero T
	if p == nil {
		return zero
	}
	return *p
}

// nonEmpty is nil for "" and &s otherwise.
func nonEmpty(s string) *string {
	if s == "" {
		return nil
	}
	return &s
}

// TRANSITION: removed once every statement moved to sqlc.
var statements []string

func sqlText(s string) string {
	statements = append(statements, s)
	return s
}

// Statements returns the registered static SQL (tests).
func Statements() []string { return append([]string(nil), statements...) }
