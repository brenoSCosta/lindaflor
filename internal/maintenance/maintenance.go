// Package maintenance is the periodic data hygiene run by the background
// sweeper (port of src/maintenance.rs). Guest carts and verification rows
// can be created without an account, so nothing else ever deletes most of
// them. Each pass deletes at most SweepBatch rows per table so a backlog
// drains over a few ticks instead of one long statement.
package maintenance

import (
	"context"

	"github.com/wellington-vell/lindaflor/internal/db"
)

// SweepBatch is the max rows deleted per table per pass.
const SweepBatch = 1000

// SweepOutcome counts the rows removed by one pass.
type SweepOutcome struct {
	Carts         int64
	Verifications int64
}

const (
	sqlSweepCarts = `DELETE FROM carts WHERE id IN (
   SELECT c.id FROM carts c
   WHERE c.user_id IS NULL
     AND (
       c.updated_at < now() - interval '30 days'
       OR (
         c.updated_at < now() - interval '1 day'
         AND NOT EXISTS (SELECT 1 FROM cart_items i WHERE i.cart_id = c.id)
       )
     )
   LIMIT $1
 )`
	sqlSweepVerifications = `DELETE FROM verifications WHERE id IN (
   SELECT id FROM verifications WHERE expires_at < now() LIMIT $1
 )`
)

// SweepStaleRows deletes guest carts untouched for 30 days (the cart
// cookie lifetime) or empty and untouched for a day (cart_items cascade),
// and expired verification rows. Signed-in users' carts are never swept.
func SweepStaleRows(ctx context.Context, q db.DBTX) (SweepOutcome, error) {
	carts, err := q.Exec(ctx, sqlSweepCarts, SweepBatch)
	if err != nil {
		return SweepOutcome{}, err
	}
	verifications, err := q.Exec(ctx, sqlSweepVerifications, SweepBatch)
	if err != nil {
		return SweepOutcome{}, err
	}
	return SweepOutcome{Carts: carts.RowsAffected(), Verifications: verifications.RowsAffected()}, nil
}
