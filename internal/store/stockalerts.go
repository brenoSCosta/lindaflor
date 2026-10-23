package store

import (
	"context"
	"fmt"
	"strings"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/mail"
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

// VariantSlug is the product slug of a variant ("" when unknown).
func VariantSlug(ctx context.Context, q DBTX, variantID uuid.UUID) (string, error) {
	slug, _, err := noRows(queries(q).StoreVariantSlug(ctx, variantID))
	return slug, err
}

// ProductVariantPath is /produtos/{slug}?variant={id} (redirect target).
func ProductVariantPath(slug string, variantID uuid.UUID) string {
	return fmt.Sprintf("/produtos/%s?variant=%s", mail.EncodeQueryValue(slug), variantID)
}

// SubscribeStockAlert registers an alert. emailNorm must already be
// validated (ValidateEmail).
func SubscribeStockAlert(ctx context.Context, q DBTX, variantID uuid.UUID, emailNorm string, userID *uuid.UUID) (SubscribeOutcome, error) {
	qs := queries(q)
	stock, found, err := noRows(qs.StoreVariantStock(ctx, variantID))
	switch {
	case err != nil:
		return 0, err
	case !found || !stock.Active:
		return AlertUnknownVariant, nil
	case stock.Available > 0:
		return AlertInStock, nil
	}
	pending, err := qs.StorePendingAlerts(ctx, emailNorm)
	if err != nil {
		return 0, err
	}
	if pending >= MaxPendingAlertsPerEmail {
		return AlertTooMany, nil
	}
	inserted, err := qs.StoreInsertAlert(ctx, sqlc.StoreInsertAlertParams{
		ID: uuid.Must(uuid.NewV7()), VariantID: variantID, EmailNorm: emailNorm, UserID: userID,
	})
	if err != nil {
		return 0, err
	}
	if inserted > 0 {
		return AlertSubscribed, nil
	}
	return AlertAlreadySubscribed, nil
}

// CancelStockAlerts drops every pending alert of the address (signed cancel
// link) and returns how many were removed.
func CancelStockAlerts(ctx context.Context, q DBTX, email string) (int64, error) {
	return queries(q).StoreCancelAlerts(ctx, strings.ToLower(strings.TrimSpace(email)))
}

// StockAlertCancelURL is the absolute signed cancel link of an address.
func StockAlertCancelURL(email string) string { return MailLinks().StockAlertCancelURL(email) }

// VerifyStockAlertCancelToken checks the cancel link token.
func VerifyStockAlertCancelToken(email, token string) bool {
	return MailLinks().VerifyStockAlertCancelToken(email, token)
}

// NotifyRestocked is the sweeper pass: mail every pending alert whose
// variant is sellable again (active product, live available > 0) and close
// it. Returns the number mailed.
func NotifyRestocked(ctx context.Context, pool *pgxpool.Pool) (int, error) {
	sent := 0
	err := withTx(ctx, pool, func(tx pgx.Tx) error {
		q := queries(tx)
		due, err := q.StoreDueAlerts(ctx, stockAlertSweepBatch)
		if err != nil || len(due) == 0 {
			return err
		}
		ids := make([]uuid.UUID, 0, len(due))
		l := MailLinks()
		for _, a := range due {
			productURL := l.AbsoluteURL(ProductVariantPath(a.ProductSlug, a.VariantID))
			e := mail.BackInStock(a.ProductName, VariantLabel(a.Size, a.Color), productURL, l.StockAlertCancelURL(a.EmailNorm))
			if err := mail.Enqueue(ctx, tx, e.Msg(mail.KindBackInStock, a.EmailNorm, "stock-alert:"+a.ID.String())); err != nil {
				return err
			}
			ids = append(ids, a.ID)
		}
		if err := q.StoreCloseAlerts(ctx, ids); err != nil {
			return err
		}
		sent = len(ids)
		return nil
	})
	return sent, err
}
