package store

import (
	"context"
	"fmt"
	"strings"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/mail"
	"github.com/wellington-vell/lindaflor/internal/store/links"
)

// Back-in-stock alerts ("Avise-me", src/app/store/stock_alerts.rs).
// Delivery is a sweeper pass (NotifyRestocked, run by internal/jobs): any
// pending alert whose variant is sellable now is mailed once and closed in
// the same transaction.

// Stock alert limits.
const (
	MaxPendingAlertsPerEmail = 20
	stockAlertSweepBatch     = 200
	// StockAlertSubscribePerMin is the per-IP budget of POST /avise-me
	// (ratelimit scope "stock-alert").
	StockAlertSubscribePerMin = 5
	ScopeStockAlert           = "stock-alert"
)

// SubscribeOutcome is the result of SubscribeStockAlert.
type SubscribeOutcome int

// Outcomes.
const (
	AlertSubscribed SubscribeOutcome = iota
	AlertAlreadySubscribed
	AlertInStock
	AlertTooMany
	AlertUnknownVariant
)

// StockAlertSubscribedMessage is the same answer for new and repeated
// subscriptions (and the honeypot), so addresses can't be probed.
const StockAlertSubscribedMessage = "Pronto! Avisaremos por e-mail quando esta peça voltar ao estoque."

// Toast is the pt-BR toast (ok=false for error toasts).
func (o SubscribeOutcome) Toast() (string, bool) {
	switch o {
	case AlertSubscribed, AlertAlreadySubscribed:
		return StockAlertSubscribedMessage, true
	case AlertInStock:
		return "Boa notícia: esta variação já está disponível!", true
	case AlertTooMany:
		return "Este e-mail já tem muitos avisos pendentes. Cancele algum pelo link do último aviso e tente de novo.", false
	}
	return "Esta variação não está disponível.", false
}

var (
	sqlVariantStock = sqlText(`SELECT p.active,
        COALESCE((
          SELECT SUM(` + LiveAvailableSQL + `)
          FROM inventory i
          INNER JOIN warehouses w ON w.id = i.warehouse_id
          WHERE i.variant_id = pv.id AND w.active = true
        ), 0)::bigint
 FROM product_variants pv
 INNER JOIN products p ON p.id = pv.product_id
 WHERE pv.id = $1`)
	sqlVariantSlug = sqlText(`SELECT p.slug FROM product_variants pv
 INNER JOIN products p ON p.id = pv.product_id
 WHERE pv.id = $1`)
	sqlPendingAlerts = sqlText(`SELECT count(*) FROM stock_alerts WHERE email_norm = $1 AND notified_at IS NULL`)
	sqlInsertAlert   = sqlText(`INSERT INTO stock_alerts (id, variant_id, email_norm, user_id)
 VALUES ($1, $2, $3, $4)
 ON CONFLICT (variant_id, email_norm) WHERE notified_at IS NULL
 DO NOTHING`)
	sqlCancelAlerts = sqlText(`DELETE FROM stock_alerts WHERE email_norm = $1 AND notified_at IS NULL`)
	sqlDueAlerts    = sqlText(`SELECT a.id, a.email_norm, pv.id AS variant_id,
        p.name AS product_name, p.slug AS product_slug,
        pv.size::text AS size, pv.color
 FROM stock_alerts a
 INNER JOIN product_variants pv ON pv.id = a.variant_id
 INNER JOIN products p ON p.id = pv.product_id
 WHERE a.notified_at IS NULL
   AND p.active = true
   AND COALESCE((
     SELECT SUM(` + LiveAvailableSQL + `)
     FROM inventory i
     INNER JOIN warehouses w ON w.id = i.warehouse_id
     WHERE i.variant_id = pv.id AND w.active = true
   ), 0) > 0
 ORDER BY a.created_at
 LIMIT $1
 FOR UPDATE OF a SKIP LOCKED`)
	sqlCloseAlerts = sqlText(`UPDATE stock_alerts SET notified_at = now() WHERE id = ANY($1)`)
)

// VariantSlug is the product slug of a variant ("" when unknown).
func VariantSlug(ctx context.Context, q DBTX, variantID uuid.UUID) (string, error) {
	var slug string
	err := q.QueryRow(ctx, sqlVariantSlug, variantID).Scan(&slug)
	if found, err := optional(err); err != nil || !found {
		return "", err
	}
	return slug, nil
}

// ProductVariantPath is /produtos/{slug}?variant={id} (redirect target).
func ProductVariantPath(slug string, variantID uuid.UUID) string {
	return fmt.Sprintf("/produtos/%s?variant=%s", links.EncodeQueryValue(slug), variantID)
}

// SubscribeStockAlert registers an alert. emailNorm must already be
// validated (ValidateEmail).
func SubscribeStockAlert(ctx context.Context, q DBTX, variantID uuid.UUID, emailNorm string, userID *uuid.UUID) (SubscribeOutcome, error) {
	var active bool
	var available int64
	err := q.QueryRow(ctx, sqlVariantStock, variantID).Scan(&active, &available)
	found, err := optional(err)
	switch {
	case err != nil:
		return 0, err
	case !found || !active:
		return AlertUnknownVariant, nil
	case available > 0:
		return AlertInStock, nil
	}
	var pending int64
	if err := q.QueryRow(ctx, sqlPendingAlerts, emailNorm).Scan(&pending); err != nil {
		return 0, err
	}
	if pending >= MaxPendingAlertsPerEmail {
		return AlertTooMany, nil
	}
	tag, err := q.Exec(ctx, sqlInsertAlert, uuid.Must(uuid.NewV7()), variantID, emailNorm, userID)
	if err != nil {
		return 0, err
	}
	if tag.RowsAffected() > 0 {
		return AlertSubscribed, nil
	}
	return AlertAlreadySubscribed, nil
}

// CancelStockAlerts drops every pending alert of the address (signed cancel
// link) and returns how many were removed.
func CancelStockAlerts(ctx context.Context, q DBTX, email string) (int64, error) {
	tag, err := q.Exec(ctx, sqlCancelAlerts, strings.ToLower(strings.TrimSpace(email)))
	if err != nil {
		return 0, err
	}
	return tag.RowsAffected(), nil
}

// StockAlertCancelURL is the absolute signed cancel link of an address.
func StockAlertCancelURL(email string) string { return links.StockAlertCancelURL(email) }

// VerifyStockAlertCancelToken checks the cancel link token.
func VerifyStockAlertCancelToken(email, token string) bool {
	return links.VerifyStockAlertCancelToken(email, token)
}

type dueAlert struct {
	ID          uuid.UUID `db:"id"`
	EmailNorm   string    `db:"email_norm"`
	VariantID   uuid.UUID `db:"variant_id"`
	ProductName string    `db:"product_name"`
	ProductSlug string    `db:"product_slug"`
	Size        string    `db:"size"`
	Color       string    `db:"color"`
}

// NotifyRestocked is the sweeper pass: mail every pending alert whose
// variant is sellable again (active product, live available > 0) and close
// it. Returns the number mailed.
func NotifyRestocked(ctx context.Context, pool *pgxpool.Pool) (int, error) {
	sent := 0
	err := withTx(ctx, pool, func(tx pgx.Tx) error {
		due, err := collect[dueAlert](tx.Query(ctx, sqlDueAlerts, stockAlertSweepBatch))
		if err != nil || len(due) == 0 {
			return err
		}
		ids := make([]uuid.UUID, 0, len(due))
		for _, a := range due {
			productURL := links.AbsoluteURL(fmt.Sprintf("/produtos/%s?variant=%s", links.EncodeQueryValue(a.ProductSlug), a.VariantID))
			e := renderer.BackInStock(a.ProductName, VariantLabel(a.Size, a.Color), productURL, links.StockAlertCancelURL(a.EmailNorm))
			if err := enqueueEmail(ctx, tx, mail.KindBackInStock, a.EmailNorm, e, "stock-alert:"+a.ID.String()); err != nil {
				return err
			}
			ids = append(ids, a.ID)
		}
		if _, err := tx.Exec(ctx, sqlCloseAlerts, ids); err != nil {
			return err
		}
		sent = len(ids)
		return nil
	})
	return sent, err
}
