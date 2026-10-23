package mail

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"strings"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// --- Auth hooks (agent A). The raw token only lives in the outbox body
// (scrubbed once delivered), never in logs. Verification, reset and
// change-email go through EnqueueThrottled; a dropped mail is invisible.

// FirstPurchaseCode is the configured welcome coupon ("" when unset).
func FirstPurchaseCode(ctx context.Context, db sqlc.DBTX) (string, error) {
	code, err := sqlc.New(db).MailFirstPurchaseCode(ctx)
	if errors.Is(err, pgx.ErrNoRows) {
		return "", nil
	}
	if err != nil {
		return "", err
	}
	if code == nil {
		return "", nil
	}
	return strings.TrimSpace(*code), nil
}

// SendVerifyEmail queues the account verification mail (with the
// first-purchase coupon, if any: it goes to brand-new accounts).
func SendVerifyEmail(ctx context.Context, db sqlc.DBTX, links Links, to, name, token string) error {
	code, err := FirstPurchaseCode(ctx, db)
	if err != nil {
		return err
	}
	_, err = EnqueueThrottled(ctx, db, VerifyEmail(name, links.TokenURL(VerifyEmailPath, token), code).Msg(KindVerifyEmail, to, ""))
	return err
}

// SendPasswordReset queues the password reset mail.
func SendPasswordReset(ctx context.Context, db sqlc.DBTX, links Links, to, name, token string) error {
	_, err := EnqueueThrottled(ctx, db, PasswordReset(name, links.TokenURL(ResetPasswordPath, token)).Msg(KindPasswordReset, to, ""))
	return err
}

// SendChangeEmail queues the confirmation to the *new* address: following
// the link proves ownership of it.
func SendChangeEmail(ctx context.Context, db sqlc.DBTX, links Links, newEmail, name, token string) error {
	_, err := EnqueueThrottled(ctx, db, ChangeEmail(name, newEmail, links.TokenURL(ChangeEmailPath, token)).Msg(KindChangeEmail, newEmail, ""))
	return err
}

// SendDeleteAccount queues the account deletion confirmation.
func SendDeleteAccount(ctx context.Context, db sqlc.DBTX, links Links, to, name, token string) error {
	return Enqueue(ctx, db, DeleteAccount(name, links.TokenURL(DeleteAccountPath, token)).Msg(KindDeleteAccount, to, ""))
}

// --- Newsletter (agent C/D) ---

// SendNewsletterWelcome queues the welcome mail, deduplicated per address:
// re-subscribing never sends a second copy.
func SendNewsletterWelcome(ctx context.Context, db sqlc.DBTX, links Links, to, name, firstPurchaseCode string) error {
	email := NewsletterWelcome(name, links.AbsoluteURL("/"), firstPurchaseCode, links.UnsubscribeURL(to))
	dedupe := "newsletter-welcome:" + normalizeEmail(to)
	return Enqueue(ctx, db, email.Msg(KindNewsletterWelcome, to, dedupe))
}

// --- Order hooks (agent C's order FSM). Run them inside the order
// transaction so the mail commits (or rolls back) with the state change.

// DefaultCarrier is the carrier with a public tracking page.
const DefaultCarrier = "Correios"

// TrackingURL is the carrier's public tracking page ("" when unknown).
func TrackingURL(carrier, code string) string {
	if !strings.EqualFold(carrier, DefaultCarrier) {
		return ""
	}
	return "https://rastreamento.correios.com.br/app/index.php?objetos=" + code
}

func orderURL(links Links, orderID, accessToken uuid.UUID) string {
	return links.AbsoluteURL(fmt.Sprintf("/pedido/%s?t=%s", orderID, EncodeQueryValue(accessToken.String())))
}

// loadOrderMail returns the recipient and mail data, ok=false when the
// order is missing or has no recipient address.
func loadOrderMail(ctx context.Context, db sqlc.DBTX, links Links, orderID uuid.UUID) (string, OrderMail, bool, error) {
	row, err := sqlc.New(db).MailOrderRecipient(ctx, orderID)
	if errors.Is(err, pgx.ErrNoRows) {
		return "", OrderMail{}, false, nil
	}
	if err != nil {
		return "", OrderMail{}, false, err
	}
	if !strings.Contains(row.Email, "@") {
		slog.WarnContext(ctx, "order has no recipient email", "order_id", orderID)
		return "", OrderMail{}, false, nil
	}
	mercadoPago := row.PaymentProvider == string(config.ProviderMercadoPago)
	tracking := ""
	if row.TrackingCode != nil {
		tracking = strings.TrimSpace(*row.TrackingCode)
	}
	o := OrderMail{
		Code:       OrderCode(orderID),
		Name:       row.Name,
		TotalCents: row.TotalCents,
		URL:        orderURL(links, orderID, row.AccessToken),
		PayUntil:   row.PayUntil,
		// Mercado Pago issues its own (dynamic) QR on the order page; a
		// static BR Code must never reach those customers.
		MercadoPago:  mercadoPago,
		Carrier:      strings.TrimSpace(row.Carrier),
		TrackingCode: tracking,
	}
	if !mercadoPago {
		o.PixCopyPaste = row.Pix
	}
	if tracking != "" {
		o.TrackingURL = TrackingURL(row.Carrier, tracking)
	}
	return row.Email, o, true, nil
}

// EnqueueOrderCreated queues "Pedido recebido" with the PIX instructions.
// Call after payment_meta is written, inside the checkout transaction.
func EnqueueOrderCreated(ctx context.Context, tx sqlc.DBTX, links Links, orderID uuid.UUID) error {
	to, o, ok, err := loadOrderMail(ctx, tx, links, orderID)
	if err != nil || !ok {
		return err
	}
	return Enqueue(ctx, tx, OrderCreated(o).Msg(KindOrderCreated, to, fmt.Sprintf("order:%s:created", orderID)))
}

// EnqueueOrderTransition queues the customer mail of an order transition,
// if that edge has one. expired marks a pending_payment → cancelled caused
// by the reservation deadline (the sweep) rather than an admin.
func EnqueueOrderTransition(ctx context.Context, tx sqlc.DBTX, links Links, orderID uuid.UUID, from, next sqlc.OrderStatus, expired bool) error {
	var kind string
	switch next {
	case sqlc.OrderStatusPaid:
		kind = KindPaymentConfirmed
	case sqlc.OrderStatusShipped:
		kind = KindOrderShipped
	case sqlc.OrderStatusCancelled:
		kind = KindOrderCancelled
		if expired && from == sqlc.OrderStatusPendingPayment {
			kind = KindOrderExpired
		}
	case sqlc.OrderStatusRefunded:
		kind = KindOrderRefunded
	default:
		return nil
	}
	to, o, ok, err := loadOrderMail(ctx, tx, links, orderID)
	if err != nil || !ok {
		return err
	}
	var email Email
	switch kind {
	case KindPaymentConfirmed:
		email = PaymentConfirmed(o)
	case KindOrderShipped:
		email = OrderShipped(o)
	case KindOrderExpired:
		email = OrderCancelled(o, true)
	case KindOrderRefunded:
		email = OrderRefunded(o)
	default:
		email = OrderCancelled(o, false)
	}
	return Enqueue(ctx, tx, email.Msg(kind, to, ""))
}

// --- Return hooks (agent C's returns flow) ---

// ReturnStep selects the return mail.
type ReturnStep struct {
	kind         string
	instructions string
	reason       string
	refundCents  *int64
}

// Return steps.
var ReturnRequestedStep = ReturnStep{kind: KindReturnRequested}

// ReturnApprovedStep carries the reverse-logistics instructions.
func ReturnApprovedStep(instructions string) ReturnStep {
	return ReturnStep{kind: KindReturnApproved, instructions: instructions}
}

// ReturnRejectedStep carries the reason given to the customer.
func ReturnRejectedStep(reason string) ReturnStep {
	return ReturnStep{kind: KindReturnRejected, reason: reason}
}

// ReturnResolvedStep: refundCents non-nil for a refund, nil for an exchange.
func ReturnResolvedStep(refundCents *int64) ReturnStep {
	return ReturnStep{kind: KindReturnResolved, refundCents: refundCents}
}

// ReturnKindLabel is the pt-BR label of a return_requests.kind.
func ReturnKindLabel(kind string) string {
	switch kind {
	case "exchange":
		return "Troca de tamanho"
	case "return":
		return "Devolução"
	case "defect":
		return "Defeito"
	}
	return "Troca/devolução"
}

// EnqueueReturn queues the mail of one return-request step inside the
// caller's transaction. Each step is sent at most once per request.
func EnqueueReturn(ctx context.Context, tx sqlc.DBTX, links Links, returnID uuid.UUID, step ReturnStep) error {
	q := sqlc.New(tx)
	row, err := q.MailReturnRecipient(ctx, returnID)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil
	}
	if err != nil {
		return err
	}
	if !strings.Contains(row.Email, "@") {
		slog.WarnContext(ctx, "return request has no recipient email", "return_id", returnID)
		return nil
	}
	items, err := q.MailReturnItems(ctx, returnID)
	if err != nil {
		return err
	}
	data := ReturnMail{
		OrderCode: OrderCode(row.OrderID),
		Name:      row.Name,
		KindLabel: ReturnKindLabel(row.Kind),
		URL:       orderURL(links, row.OrderID, row.AccessToken),
	}
	for _, it := range items {
		data.Items = append(data.Items, fmt.Sprintf("%dx %s (%s)", it.Quantity, it.ProductName, it.VariantLabel))
	}
	var email Email
	var stepName string
	switch step.kind {
	case KindReturnApproved:
		email, stepName = ReturnApproved(data, step.instructions), "approved"
	case KindReturnRejected:
		email, stepName = ReturnRejected(data, step.reason), "rejected"
	case KindReturnResolved:
		email, stepName = ReturnResolved(data, step.refundCents), "resolved"
	default:
		email, stepName = ReturnRequested(data), "requested"
	}
	kind := step.kind
	if kind == "" {
		kind = KindReturnRequested
	}
	return Enqueue(ctx, tx, email.Msg(kind, row.Email, fmt.Sprintf("return:%s:%s", returnID, stepName)))
}
