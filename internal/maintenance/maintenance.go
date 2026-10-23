// Package maintenance is the periodic data hygiene run by the background
// sweeper (port of src/maintenance.rs). Guest carts and verification rows
// can be created without an account, so nothing else ever deletes most of
// them. Each pass deletes at most SweepBatch rows per table so a backlog
// drains over a few ticks instead of one long statement.
package maintenance

import (
	"context"

	"github.com/wellington-vell/lindaflor/internal/db"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// SweepBatch is the max rows deleted per table per pass.
const SweepBatch = 1000

// SweepOutcome counts the rows removed by one pass.
type SweepOutcome struct {
	Carts         int64
	Verifications int64
}

// SweepStaleRows deletes guest carts untouched for 30 days (the cart
// cookie lifetime) or empty and untouched for a day (cart_items cascade),
// and expired verification rows. Signed-in users' carts are never swept.
func SweepStaleRows(ctx context.Context, q db.DBTX) (SweepOutcome, error) {
	qs := sqlc.New(q)
	carts, err := qs.MaintenanceSweepCarts(ctx, SweepBatch)
	if err != nil {
		return SweepOutcome{}, err
	}
	verifications, err := qs.MaintenanceSweepVerifications(ctx, SweepBatch)
	if err != nil {
		return SweepOutcome{}, err
	}
	return SweepOutcome{Carts: carts, Verifications: verifications}, nil
}
