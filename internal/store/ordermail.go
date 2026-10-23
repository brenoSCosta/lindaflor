package store

import (
	"context"
	"fmt"
	"log/slog"
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/mail"
	"github.com/wellington-vell/lindaflor/internal/store/links"
)

// Commerce mail hooks (src/mail/orders.rs, src/mail/returns.rs). They run
// inside the caller's transaction so the mail commits (or rolls back) with
// the state change.

func enqueueEmail(ctx context.Context, tx DBTX, kind, to string, e Email, dedupe string) error {
	return mail.Enqueue(ctx, tx, mail.Msg{
		Kind: kind, To: to, Subject: e.Subject, HTML: e.HTML, Text: e.Text, DedupeKey: dedupe,
	})
}

var sqlOrderMailLoad = sqlText(`SELECT o.access_token,
        o.total_cents::bigint AS total_cents,
        COALESCE(NULLIF(btrim(o.guest_email), ''), u.email) AS email,
        COALESCE(NULLIF(btrim(o.shipping_address->>'name'), ''), u.name) AS name,
        o.payment_meta->>'pix_copy_paste' AS pix,
        to_char(o.reservation_expires_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS pay_until,
        o.payment_provider, o.carrier, o.tracking_code
 FROM orders o
 LEFT JOIN users u ON u.id = o.user_id
 WHERE o.id = $1`)

type orderMailRow struct {
	AccessToken     uuid.UUID `db:"access_token"`
	TotalCents      int64     `db:"total_cents"`
	Email           *string   `db:"email"`
	Name            *string   `db:"name"`
	Pix             *string   `db:"pix"`
	PayUntil        *string   `db:"pay_until"`
	PaymentProvider string    `db:"payment_provider"`
	Carrier         string    `db:"carrier"`
	TrackingCode    *string   `db:"tracking_code"`
}

// loadOrderMail returns (recipient, data, ok).
func loadOrderMail(ctx context.Context, tx DBTX, orderID uuid.UUID) (string, OrderMail, bool, error) {
	row, err := collectOne[orderMailRow](tx.Query(ctx, sqlOrderMailLoad, orderID))
	if err != nil || row == nil {
		return "", OrderMail{}, false, err
	}
	to := deref(row.Email)
	if !strings.Contains(to, "@") {
		slog.WarnContext(ctx, "order has no recipient email", "order_id", orderID)
		return "", OrderMail{}, false, nil
	}
	mp := row.PaymentProvider == ProviderMercadoPago
	var pix *string
	if row.Pix != nil && *row.Pix != "" && !mp {
		pix = row.Pix
	}
	var tracking, trackingURL *string
	if row.TrackingCode != nil {
		if code := strings.TrimSpace(*row.TrackingCode); code != "" {
			tracking = &code
			if u, ok := TrackingURL(row.Carrier, code); ok {
				trackingURL = &u
			}
		}
	}
	return to, OrderMail{
		Code:         OrderCode(orderID),
		Name:         deref(row.Name),
		TotalCents:   row.TotalCents,
		URL:          links.OrderURL(orderID, row.AccessToken),
		PixCopyPaste: pix,
		PayUntil:     row.PayUntil,
		MercadoPago:  mp,
		Carrier:      strings.TrimSpace(row.Carrier),
		TrackingCode: tracking,
		TrackingURL:  trackingURL,
	}, true, nil
}

// EnqueueOrderCreated queues "Pedido recebido" with PIX instructions.
// Call after payment_meta is written, inside the checkout transaction.
func EnqueueOrderCreated(ctx context.Context, tx DBTX, orderID uuid.UUID) error {
	to, data, ok, err := loadOrderMail(ctx, tx, orderID)
	if err != nil || !ok {
		return err
	}
	return enqueueEmail(ctx, tx, mail.KindOrderCreated, to, renderer.OrderCreated(data), fmt.Sprintf("order:%s:created", orderID))
}

// EnqueueOrderTransition queues the customer mail of an order edge, if it
// has one. expired marks a pending_payment → cancelled caused by the
// reservation deadline rather than an admin.
func EnqueueOrderTransition(ctx context.Context, tx DBTX, orderID uuid.UUID, from, next OrderStatus, expired bool) error {
	var kind string
	switch next {
	case StatusPaid:
		kind = mail.KindPaymentConfirmed
	case StatusShipped:
		kind = mail.KindOrderShipped
	case StatusCancelled:
		if expired && from == StatusPendingPayment {
			kind = mail.KindOrderExpired
		} else {
			kind = mail.KindOrderCancelled
		}
	case StatusRefunded:
		kind = mail.KindOrderRefunded
	default:
		return nil
	}
	to, data, ok, err := loadOrderMail(ctx, tx, orderID)
	if err != nil || !ok {
		return err
	}
	var e Email
	switch kind {
	case mail.KindPaymentConfirmed:
		e = renderer.PaymentConfirmed(data)
	case mail.KindOrderShipped:
		e = renderer.OrderShipped(data)
	case mail.KindOrderExpired:
		e = renderer.OrderCancelled(data, true)
	case mail.KindOrderRefunded:
		e = renderer.OrderRefunded(data)
	default:
		e = renderer.OrderCancelled(data, false)
	}
	return enqueueEmail(ctx, tx, kind, to, e, "")
}

// ReturnMailStep selects the return-request mail.
type ReturnMailStep struct {
	Kind         string // requested | approved | rejected | resolved
	Instructions string
	Reason       string
	RefundCents  *int64 // resolved: refund amount, nil = exchanged
}

var sqlReturnMailLoad = sqlText(`SELECT o.id AS order_id, o.access_token, r.kind,
        COALESCE(NULLIF(btrim(o.guest_email), ''), u.email) AS email,
        COALESCE(NULLIF(btrim(o.shipping_address->>'name'), ''), u.name) AS name
 FROM return_requests r
 JOIN orders o ON o.id = r.order_id
 LEFT JOIN users u ON u.id = o.user_id
 WHERE r.id = $1`)

var sqlReturnMailItems = sqlText(`SELECT ri.quantity, oi.product_name, oi.variant_label
 FROM return_request_items ri
 JOIN order_items oi ON oi.id = ri.order_item_id
 WHERE ri.return_request_id = $1
 ORDER BY oi.created_at, oi.id`)

type returnMailRow struct {
	OrderID     uuid.UUID `db:"order_id"`
	AccessToken uuid.UUID `db:"access_token"`
	Kind        string    `db:"kind"`
	Email       *string   `db:"email"`
	Name        *string   `db:"name"`
}

type returnMailItem struct {
	Quantity     int32  `db:"quantity"`
	ProductName  string `db:"product_name"`
	VariantLabel string `db:"variant_label"`
}

// enqueueReturnMail queues one return-request step mail (deduplicated per
// request and step).
func enqueueReturnMail(ctx context.Context, tx DBTX, returnID uuid.UUID, step ReturnMailStep) error {
	row, err := collectOne[returnMailRow](tx.Query(ctx, sqlReturnMailLoad, returnID))
	if err != nil || row == nil {
		return err
	}
	to := deref(row.Email)
	if !strings.Contains(to, "@") {
		slog.WarnContext(ctx, "return request has no recipient email", "return_id", returnID)
		return nil
	}
	items, err := collect[returnMailItem](tx.Query(ctx, sqlReturnMailItems, returnID))
	if err != nil {
		return err
	}
	kindLabel := "Troca/devolução"
	if k, ok := ParseReturnKind(row.Kind); ok {
		kindLabel = k.Label()
	}
	lines := make([]string, 0, len(items))
	for _, it := range items {
		lines = append(lines, fmt.Sprintf("%dx %s (%s)", it.Quantity, it.ProductName, it.VariantLabel))
	}
	data := ReturnMail{
		OrderCode: OrderCode(row.OrderID),
		Name:      deref(row.Name),
		KindLabel: kindLabel,
		Items:     lines,
		URL:       links.OrderURL(row.OrderID, row.AccessToken),
	}
	var kind string
	var e Email
	switch step.Kind {
	case "requested":
		kind, e = mail.KindReturnRequested, renderer.ReturnRequested(data)
	case "approved":
		kind, e = mail.KindReturnApproved, renderer.ReturnApproved(data, step.Instructions)
	case "rejected":
		kind, e = mail.KindReturnRejected, renderer.ReturnRejected(data, step.Reason)
	default:
		kind, e = mail.KindReturnResolved, renderer.ReturnResolved(data, step.RefundCents)
	}
	return enqueueEmail(ctx, tx, kind, to, e, fmt.Sprintf("return:%s:%s", returnID, step.Kind))
}
