package store

import (
	"context"
	"errors"
	"log/slog"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

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
	RemindEveryDays          = 7
	MaxIdleDays              = 7
	abandonedCartSweepBatch  = 100
)

var sqlRememberCheckoutEmail = sqlText(`UPDATE carts SET contact_email = $2
 WHERE id = $1 AND contact_email IS DISTINCT FROM $2`)

// RememberCheckoutEmail stores the address typed on the checkout page for
// a later reminder (not counted as cart activity).
func RememberCheckoutEmail(ctx context.Context, q DBTX, cartID uuid.UUID, emailNorm string) error {
	_, err := q.Exec(ctx, sqlRememberCheckoutEmail, cartID, emailNorm)
	return err
}

var (
	sqlAbandonedCandidates = sqlText(`WITH activity AS (
    SELECT c.id,
           c.user_id,
           c.abandoned_reminded_at,
           GREATEST(
               c.updated_at,
               (SELECT max(ci.updated_at) FROM cart_items ci WHERE ci.cart_id = c.id)
           ) AS last_activity,
           lower(btrim(COALESCE(
               CASE WHEN u.email_verified THEN u.email END,
               c.contact_email
           ))) AS email,
           u.name
    FROM carts c
    LEFT JOIN users u ON u.id = c.user_id
    WHERE EXISTS (
        SELECT 1 FROM cart_items ci
        INNER JOIN product_variants pv ON pv.id = ci.variant_id
        INNER JOIN products p ON p.id = pv.product_id
        WHERE ci.cart_id = c.id AND p.active = true
    )
)
SELECT a.id, a.email, a.name, a.abandoned_reminded_at AS reminded_at
FROM activity a
WHERE a.email IS NOT NULL AND a.email <> ''
  AND a.last_activity < now() - make_interval(mins => $1::int)
  AND a.last_activity > now() - make_interval(days => $2::int)
  AND (
      a.abandoned_reminded_at IS NULL
      OR (a.abandoned_reminded_at < a.last_activity
          AND a.abandoned_reminded_at < now() - make_interval(days => $2::int))
  )
  AND NOT EXISTS (
      SELECT 1 FROM orders o
      WHERE (o.cart_id = a.id OR (a.user_id IS NOT NULL AND o.user_id = a.user_id))
        AND o.created_at >= a.last_activity
  )
  AND NOT EXISTS (
      SELECT 1 FROM newsletter_subscribers ns
      WHERE lower(ns.email) = a.email AND ns.unsubscribed_at IS NOT NULL
  )
  AND NOT EXISTS (
      SELECT 1 FROM email_outbox eo
      WHERE eo.kind = $4 AND lower(eo.to_address) = a.email
        AND eo.created_at > now() - make_interval(days => $2::int)
  )
ORDER BY a.last_activity
LIMIT $3`)
	sqlClaimReminder = sqlText(`UPDATE carts SET abandoned_reminded_at = now()
 WHERE id = $1 AND abandoned_reminded_at IS NOT DISTINCT FROM $2`)
	sqlRecentReminder = sqlText(`SELECT EXISTS (
   SELECT 1 FROM email_outbox
   WHERE kind = $1 AND lower(to_address) = $2
     AND created_at > now() - make_interval(days => $3::int)
 )`)
	sqlReminderLines = sqlText(`SELECT p.name AS product_name, pv.size::text AS size, pv.color,
        ci.quantity,
        COALESCE(pv.price_in_cents, p.price_in_cents)::bigint AS unit_price_cents
 FROM cart_items ci
 INNER JOIN product_variants pv ON pv.id = ci.variant_id
 INNER JOIN products p ON p.id = pv.product_id
 WHERE ci.cart_id = $1 AND p.active = true
 ORDER BY ci.created_at, ci.id`)
)

type reminderCandidate struct {
	ID         uuid.UUID  `db:"id"`
	Email      string     `db:"email"`
	Name       *string    `db:"name"`
	RemindedAt *time.Time `db:"reminded_at"`
}

type reminderLine struct {
	ProductName    string `db:"product_name"`
	Size           string `db:"size"`
	Color          string `db:"color"`
	Quantity       int32  `db:"quantity"`
	UnitPriceCents int64  `db:"unit_price_cents"`
}

// SendAbandonedCartReminders queues one reminder per eligible cart and
// returns how many. Each cart is claimed with a compare-and-set on
// abandoned_reminded_at in its own transaction (replica-safe).
func SendAbandonedCartReminders(ctx context.Context, pool *pgxpool.Pool, afterMinutes int64) (int, error) {
	mins := int32(min(max(afterMinutes, 1), int64(1<<31-1)))
	candidates, err := collect[reminderCandidate](pool.Query(ctx, sqlAbandonedCandidates,
		mins, int32(max(RemindEveryDays, MaxIdleDays)), abandonedCartSweepBatch, mail.KindAbandonedCart))
	if err != nil {
		return 0, err
	}
	sent := 0
	for _, c := range candidates {
		queued := false
		err := withTx(ctx, pool, func(tx pgx.Tx) error {
			tag, err := tx.Exec(ctx, sqlClaimReminder, c.ID, c.RemindedAt)
			if err != nil {
				return err
			}
			var recent bool
			if err := tx.QueryRow(ctx, sqlRecentReminder, mail.KindAbandonedCart, c.Email, int32(RemindEveryDays)).Scan(&recent); err != nil {
				return err
			}
			if tag.RowsAffected() == 0 || recent {
				return errSkip
			}
			rows, err := collect[reminderLine](tx.Query(ctx, sqlReminderLines, c.ID))
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
