package store

import (
	"context"
	"errors"
	"log/slog"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/mail"
)

// Abandoned-cart reminders (src/app/store/abandoned_cart.rs), sent by the
// sweeper. Eligible carts hold an active product line, were last touched
// more than ABANDONED_CART_AFTER_MINUTES ago but within 7 days, have a
// known address (verified account email, else the checkout email), no
// order since that activity, and the address has not unsubscribed. At most
// one reminder per abandonment, per cart and per address every 7 days.

// Reminder windows.
const (
	RemindEveryDays         = 7
	MaxIdleDays             = 7
	abandonedCartSweepBatch = 100
)

// RememberCheckoutEmail stores the address typed on the checkout page for
// a later reminder (not counted as cart activity).
func RememberCheckoutEmail(ctx context.Context, q DBTX, cartID uuid.UUID, emailNorm string) error {
	return queries(q).StoreRememberCheckoutEmail(ctx, sqlc.StoreRememberCheckoutEmailParams{ID: cartID, ContactEmail: emailNorm})
}

// SendAbandonedCartReminders queues one reminder per eligible cart and
// returns how many. Each cart is claimed with a compare-and-set on
// abandoned_reminded_at in its own transaction (replica-safe).
func SendAbandonedCartReminders(ctx context.Context, pool *pgxpool.Pool, afterMinutes int64) (int, error) {
	mins := int32(min(max(afterMinutes, 1), int64(1<<31-1)))
	candidates, err := queries(pool).StoreAbandonedCandidates(ctx, sqlc.StoreAbandonedCandidatesParams{
		AfterMinutes: mins, WindowDays: int32(max(RemindEveryDays, MaxIdleDays)),
		MaxRows: abandonedCartSweepBatch, Kind: mail.KindAbandonedCart,
	})
	if err != nil {
		return 0, err
	}
	sent := 0
	for _, c := range candidates {
		queued := false
		err := withTx(ctx, pool, func(tx pgx.Tx) error {
			q := queries(tx)
			claimed, err := q.StoreClaimReminder(ctx, sqlc.StoreClaimReminderParams{ID: c.ID, RemindedAt: c.RemindedAt})
			if err != nil {
				return err
			}
			recent, err := q.StoreRecentReminder(ctx, sqlc.StoreRecentReminderParams{
				Kind: mail.KindAbandonedCart, Email: c.Email, Days: int32(RemindEveryDays),
			})
			if err != nil {
				return err
			}
			if claimed == 0 || recent {
				return errSkip
			}
			rows, err := q.StoreReminderLines(ctx, c.ID)
			if err != nil {
				return err
			}
			if len(rows) == 0 {
				return errSkip
			}
			lines := make([]mail.CartReminderLine, 0, len(rows))
			for _, r := range rows {
				lines = append(lines, mail.CartReminderLine{
					ProductName: r.ProductName, VariantLabel: VariantLabel(r.Size, r.Color),
					Quantity: r.Quantity, UnitPriceCents: r.UnitPriceCents,
				})
			}
			l := MailLinks()
			e := mail.AbandonedCart(deref(c.Name), lines, l.AbsoluteURL("/checkout"), l.UnsubscribeURL(c.Email))
			if err := mail.Enqueue(ctx, tx, e.Msg(mail.KindAbandonedCart, c.Email, "")); err != nil {
				return err
			}
			queued = true
			return nil
		})
		if err != nil && !errors.Is(err, errSkip) {
			return sent, err
		}
		if queued {
			slog.DebugContext(ctx, "abandoned-cart reminder queued", "cart_id", c.ID, "to", mail.RedactAddress(c.Email))
			sent++
		}
	}
	return sent, nil
}

// errSkip rolls a claim transaction back without failing the pass.
var errSkip = skipError{}

type skipError struct{}

func (skipError) Error() string { return "skip" }
