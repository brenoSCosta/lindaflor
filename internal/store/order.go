package store

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"strings"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/audit"
	"github.com/wellington-vell/lindaflor/internal/payments"
)

// The order state machine (src/app/store/order.rs). Apply is the only
// writer of orders.status: payments (webhook, sync, sweeper), the admin,
// the customer cancel and checkout reclaim all go through it.

// OrderStatus mirrors the order_status enum.
type OrderStatus string

// Statuses.
const (
	StatusPendingPayment OrderStatus = "pending_payment"
	StatusPaid           OrderStatus = "paid"
	StatusProcessing     OrderStatus = "processing"
	StatusShipped        OrderStatus = "shipped"
	StatusDelivered      OrderStatus = "delivered"
	StatusCancelled      OrderStatus = "cancelled"
	StatusRefunded       OrderStatus = "refunded"
)

// ParseOrderStatus parses an order_status value.
func ParseOrderStatus(raw string) (OrderStatus, bool) {
	switch s := OrderStatus(raw); s {
	case StatusPendingPayment, StatusPaid, StatusProcessing, StatusShipped,
		StatusDelivered, StatusCancelled, StatusRefunded:
		return s, true
	}
	return "", false
}

// Label is the pt-BR label.
func (s OrderStatus) Label() string { return OrderStatusLabel(string(s)) }

// OrderEvent is an FSM input.
type OrderEvent string

// Events (values are the audit action names).
const (
	EventPay       OrderEvent = "pay"
	EventProcess   OrderEvent = "process"
	EventShip      OrderEvent = "ship"
	EventDeliver   OrderEvent = "deliver"
	EventCancel    OrderEvent = "cancel"
	EventRefund    OrderEvent = "refund"
	EventReopen    OrderEvent = "reopen"
	EventReopenPay OrderEvent = "reopen_pay"
)

// ParseOrderEvent accepts the event names and the target-status aliases
// the admin forms post.
func ParseOrderEvent(raw string) (OrderEvent, bool) {
	switch raw {
	case "paid", "pay":
		return EventPay, true
	case "processing", "process":
		return EventProcess, true
	case "shipped", "ship":
		return EventShip, true
	case "delivered", "deliver":
		return EventDeliver, true
	case "cancelled", "cancel":
		return EventCancel, true
	case "refunded", "refund":
		return EventRefund, true
	case "reopen":
		return EventReopen, true
	case "reopen_pay", "reopen_and_pay":
		return EventReopenPay, true
	}
	return "", false
}

// ActionName is the stable audit name.
func (e OrderEvent) ActionName() string { return string(e) }

// Apply errors.
var (
	ErrOrderNotFound = errors.New("Pedido não encontrado.")
	// ErrReopenInsufficient is payments.ErrInsufficientStock so the payments
	// service can match a late approval without stock (errors.Is).
	ErrReopenInsufficient = payments.ErrInsufficientStock
	ErrTrackingRequired   = errors.New("Informe o código de rastreio para marcar como enviado.")
)

// IllegalTransitionError is an edge the FSM does not have.
type IllegalTransitionError struct {
	From  OrderStatus
	Event OrderEvent
}

func (e *IllegalTransitionError) Error() string {
	return fmt.Sprintf("Transição inválida: %s + %s.", e.From, eventDebugName(e.Event))
}

func eventDebugName(e OrderEvent) string {
	switch e {
	case EventPay:
		return "Pay"
	case EventProcess:
		return "Process"
	case EventShip:
		return "Ship"
	case EventDeliver:
		return "Deliver"
	case EventCancel:
		return "Cancel"
	case EventRefund:
		return "Refund"
	case EventReopen:
		return "Reopen"
	case EventReopenPay:
		return "ReopenPay"
	}
	return string(e)
}

// InvalidTrackingError is a malformed carrier or code.
type InvalidTrackingError struct{ Msg string }

func (e *InvalidTrackingError) Error() string { return e.Msg }

// isApplyDomainError reports a non-database Apply error (a business-rule
// refusal: illegal edge, stock, tracking, not found).
func isApplyDomainError(err error) bool {
	var ill *IllegalTransitionError
	var inv *InvalidTrackingError
	return errors.Is(err, ErrOrderNotFound) || errors.Is(err, ErrReopenInsufficient) ||
		errors.Is(err, ErrTrackingRequired) || errors.As(err, &ill) || errors.As(err, &inv)
}

// IsApplyDomainError is isApplyDomainError for callers (show err.Error()
// to the admin; anything else is a 500-class failure).
func IsApplyDomainError(err error) bool { return isApplyDomainError(err) }

// Tracking is the carrier + code typed by the admin.
type Tracking struct {
	Carrier string
	Code    string
}

// ApplyOptions are per-apply knobs. Restock nil uses the status default
// (restock when cancelling/refunding a paid/processing order, keep stock
// sold once goods left the warehouse); non-nil is an explicit admin choice.
// Tracking is required by Ship and ignored otherwise.
type ApplyOptions struct {
	Restock  *bool
	Tracking *Tracking
}

// WithRestock is ApplyOptions{Restock: &restock}.
func WithRestock(restock bool) ApplyOptions { return ApplyOptions{Restock: &restock} }

func (o ApplyOptions) resolveRestock(from OrderStatus) bool {
	if o.Restock != nil {
		return *o.Restock
	}
	return from == StatusPaid || from == StatusProcessing
}

// DefaultCarrier is the default orders.carrier.
const DefaultCarrier = "Correios"

const (
	carrierMaxChars  = 40
	trackingMaxChars = 40
)

func isCorreios(carrier string) bool { return strings.EqualFold(carrier, DefaultCarrier) }

// NormalizeTracking validates a tracking code and returns (carrier, code)
// normalized. Correios codes must look like AA123456789BR (spaces, dashes
// and dots ignored); other carriers accept short alphanumeric codes.
func NormalizeTracking(carrier, code string) (string, string, error) {
	carrier = strings.TrimSpace(carrier)
	switch {
	case carrier == "" || isCorreios(carrier):
		carrier = DefaultCarrier
	case runeLen(carrier) > carrierMaxChars:
		return "", "", &InvalidTrackingError{"Transportadora inválida."}
	}
	var b strings.Builder
	for _, r := range code {
		if isWhitespace(r) || r == '-' || r == '.' {
			continue
		}
		if r >= 'a' && r <= 'z' {
			r -= 'a' - 'A'
		}
		b.WriteRune(r)
	}
	norm := b.String()
	if norm == "" {
		return "", "", ErrTrackingRequired
	}
	if isCorreios(carrier) {
		ok := len(norm) == 13
		for i := 0; ok && i < 13; i++ {
			c := norm[i]
			switch {
			case i < 2 || i >= 11:
				ok = c >= 'A' && c <= 'Z'
			default:
				ok = c >= '0' && c <= '9'
			}
		}
		if !ok {
			return "", "", &InvalidTrackingError{"Código de rastreio dos Correios inválido (ex.: AA123456789BR)."}
		}
	} else {
		ok := runeLen(norm) <= trackingMaxChars
		for _, r := range norm {
			if !isASCIIAlnum(r) {
				ok = false
			}
		}
		if !ok {
			return "", "", &InvalidTrackingError{"Código de rastreio inválido (use apenas letras e números)."}
		}
	}
	return carrier, norm, nil
}

func isWhitespace(r rune) bool {
	switch r {
	case ' ', '\t', '\n', '\r', '\v', '\f', 0x85, 0xA0, 0x1680, 0x2028, 0x2029, 0x202F, 0x205F, 0x3000:
		return true
	}
	return r >= 0x2000 && r <= 0x200A
}

// TrackingURL is the carrier's public tracking page, when known.
func TrackingURL(carrier, code string) (string, bool) {
	if !isCorreios(carrier) {
		return "", false
	}
	return "https://rastreamento.correios.com.br/app/index.php?objetos=" + code, true
}

func transition(from OrderStatus, event OrderEvent) (OrderStatus, error) {
	switch {
	case from == StatusPendingPayment && event == EventPay:
		return StatusPaid, nil
	case from == StatusPendingPayment && event == EventCancel:
		return StatusCancelled, nil
	case from == StatusPaid && event == EventProcess:
		return StatusProcessing, nil
	case (from == StatusPaid || from == StatusProcessing) && event == EventShip:
		return StatusShipped, nil
	case from == StatusShipped && event == EventDeliver:
		return StatusDelivered, nil
	case (from == StatusPaid || from == StatusProcessing || from == StatusShipped) && event == EventCancel:
		return StatusCancelled, nil
	case (from == StatusPaid || from == StatusProcessing || from == StatusShipped || from == StatusDelivered) && event == EventRefund:
		return StatusRefunded, nil
	case from == StatusCancelled && event == EventReopen:
		return StatusPendingPayment, nil
	case from == StatusCancelled && event == EventReopenPay:
		return StatusPaid, nil
	}
	return "", &IllegalTransitionError{From: from, Event: event}
}

// Transition exposes the edge table (UI: which buttons to offer).
func Transition(from OrderStatus, event OrderEvent) (OrderStatus, bool) {
	next, err := transition(from, event)
	return next, err == nil
}

type applyOutcome struct {
	from, next OrderStatus
	restocked  bool
	tracking   *Tracking
}

var (
	sqlApplyLock = sqlText(`SELECT status::text, paid_at IS NOT NULL,
        COALESCE(reservation_expires_at < now(), false)
 FROM orders WHERE id = $1 FOR UPDATE`)
	sqlOrderLines    = sqlText(`SELECT variant_id, quantity FROM order_items WHERE order_id = $1`)
	sqlSetStatusPaid = sqlText(`UPDATE orders SET status = $2::text::order_status, paid_at = now(), updated_at = now() WHERE id = $1`)
	sqlSetStatus     = sqlText(`UPDATE orders SET status = $2::text::order_status, updated_at = now() WHERE id = $1`)
	sqlSetStatusShip = sqlText(`UPDATE orders SET status = $2::text::order_status, carrier = $3, tracking_code = $4,
        shipped_at = now(),
        access_token_expires_at = GREATEST(access_token_expires_at, now() + interval '90 days'),
        updated_at = now()
 WHERE id = $1`)
	sqlSetStatusDeliver = sqlText(`UPDATE orders SET status = $2::text::order_status, delivered_at = now(),
        access_token_expires_at = GREATEST(access_token_expires_at, now() + interval '90 days'),
        updated_at = now()
 WHERE id = $1`)
)

func orderLines(ctx context.Context, tx DBTX, orderID uuid.UUID) ([]ReserveLine, error) {
	items, err := collect[itemQuantity](tx.Query(ctx, sqlOrderLines, orderID))
	if err != nil {
		return nil, err
	}
	lines := make([]ReserveLine, 0, len(items))
	for _, it := range items {
		lines = append(lines, ReserveLine(it))
	}
	return lines, nil
}

func reserveForReopen(ctx context.Context, tx DBTX, orderID uuid.UUID) error {
	lines, err := orderLines(ctx, tx, orderID)
	if err != nil {
		return err
	}
	if err := ReserveForOrder(ctx, tx, orderID, lines); err != nil {
		if errors.Is(err, ErrInsufficientStock) {
			return ErrReopenInsufficient
		}
		return err
	}
	return nil
}

// Apply runs one order event under SELECT … FOR UPDATE inside tx. Side
// effects (stock, coupon restore, paid_at, tracking, customer mail) run in
// the same transaction.
func Apply(ctx context.Context, tx DBTX, orderID uuid.UUID, event OrderEvent, opts ApplyOptions) (OrderStatus, error) {
	out, err := applyInner(ctx, tx, orderID, event, opts)
	if err != nil {
		return "", err
	}
	return out.next, nil
}

func applyInner(ctx context.Context, tx DBTX, orderID uuid.UUID, event OrderEvent, opts ApplyOptions) (applyOutcome, error) {
	var raw string
	var paid, expired bool
	// expired is read before side effects: releasing the hold clears
	// reservation_expires_at.
	err := tx.QueryRow(ctx, sqlApplyLock, orderID).Scan(&raw, &paid, &expired)
	if found, err := optional(err); err != nil {
		return applyOutcome{}, err
	} else if !found {
		return applyOutcome{}, ErrOrderNotFound
	}
	from, ok := ParseOrderStatus(raw)
	if !ok {
		return applyOutcome{}, ErrOrderNotFound
	}
	// Reopen is admin-only recovery for orders cancelled while unpaid.
	if (event == EventReopen || event == EventReopenPay) && (from != StatusCancelled || paid) {
		return applyOutcome{}, &IllegalTransitionError{From: from, Event: event}
	}
	next, err := transition(from, event)
	if err != nil {
		return applyOutcome{}, err
	}
	out := applyOutcome{from: from, next: next}

	switch {
	case from == StatusPendingPayment && event == EventPay:
		if err := ConvertReservation(ctx, tx, orderID); err != nil {
			return out, err
		}
		_, err = tx.Exec(ctx, sqlSetStatusPaid, orderID, string(next))
	case from == StatusCancelled && event == EventReopen:
		if err := reserveForReopen(ctx, tx, orderID); err != nil {
			return out, err
		}
		if err := ReclaimRedemptionForOrder(ctx, tx, orderID); err != nil {
			return out, err
		}
		_, err = tx.Exec(ctx, sqlSetStatus, orderID, string(next))
	case from == StatusCancelled && event == EventReopenPay:
		if err := reserveForReopen(ctx, tx, orderID); err != nil {
			return out, err
		}
		if err := ConvertReservation(ctx, tx, orderID); err != nil {
			return out, err
		}
		if err := ReclaimRedemptionForOrder(ctx, tx, orderID); err != nil {
			return out, err
		}
		_, err = tx.Exec(ctx, sqlSetStatusPaid, orderID, string(next))
	case from == StatusPendingPayment && event == EventCancel:
		if err := ReleaseReservation(ctx, tx, orderID); err != nil {
			return out, err
		}
		if err := RestoreRedemptionForOrder(ctx, tx, orderID); err != nil {
			return out, err
		}
		_, err = tx.Exec(ctx, sqlSetStatus, orderID, string(next))
	case event == EventCancel || event == EventRefund:
		out.restocked = opts.resolveRestock(from)
		if out.restocked {
			if err := RestockSoldItems(ctx, tx, orderID); err != nil {
				return out, err
			}
		}
		if err := RestoreRedemptionForOrder(ctx, tx, orderID); err != nil {
			return out, err
		}
		_, err = tx.Exec(ctx, sqlSetStatus, orderID, string(next))
	case event == EventShip:
		if opts.Tracking == nil {
			return out, ErrTrackingRequired
		}
		carrier, code, terr := NormalizeTracking(opts.Tracking.Carrier, opts.Tracking.Code)
		if terr != nil {
			return out, terr
		}
		// The guest link keeps working for 90 days after shipping.
		_, err = tx.Exec(ctx, sqlSetStatusShip, orderID, string(next), carrier, code)
		out.tracking = &Tracking{Carrier: carrier, Code: code}
	case event == EventDeliver:
		_, err = tx.Exec(ctx, sqlSetStatusDeliver, orderID, string(next))
	default:
		_, err = tx.Exec(ctx, sqlSetStatus, orderID, string(next))
	}
	if err != nil {
		return out, err
	}
	// Customer mail rides the same transaction (outbox).
	if err := EnqueueOrderTransition(ctx, tx, orderID, from, next, expired); err != nil {
		return out, err
	}
	return out, nil
}

// Actor attributes a privileged mutation in the audit trail.
type Actor struct {
	ID   *uuid.UUID
	Name *string
}

// NamedActor is an Actor with only a display name ("cliente", "Mercado Pago").
func NamedActor(name string) Actor { return Actor{Name: &name} }

// UserActor is an Actor for a signed-in user.
func UserActor(id uuid.UUID, name string) Actor { return Actor{ID: &id, Name: &name} }

// ApplyWithAudit is Apply plus the audit_events row, in the same
// transaction. Every admin/customer/PSP status change must use it.
func ApplyWithAudit(ctx context.Context, tx DBTX, orderID uuid.UUID, event OrderEvent, opts ApplyOptions, actor Actor) (OrderStatus, error) {
	out, err := applyInner(ctx, tx, orderID, event, opts)
	if err != nil {
		return "", err
	}
	meta := map[string]any{
		"action":  event.ActionName(),
		"from":    string(out.from),
		"to":      string(out.next),
		"restock": out.restocked,
	}
	if out.tracking != nil {
		meta["carrier"] = out.tracking.Carrier
		meta["tracking_code"] = out.tracking.Code
	}
	if err := audit.Record(ctx, tx, audit.Event{
		AggregateType: "order",
		AggregateID:   orderID.String(),
		EntityType:    "order",
		EntityID:      orderID,
		ActorID:       actor.ID,
		ActorName:     actor.Name,
		Metadata:      meta,
	}); err != nil {
		return "", err
	}
	return out.next, nil
}

var (
	sqlExpiredPending = sqlText(`SELECT id FROM orders
WHERE status = 'pending_payment'
  AND reservation_expires_at IS NOT NULL
  AND reservation_expires_at < now()
ORDER BY id`)
	sqlClaimExpiredOne = sqlText(`SELECT id FROM orders
WHERE id = $1
  AND status = 'pending_payment'
  AND reservation_expires_at < now()
FOR UPDATE SKIP LOCKED`)
)

// ExpiredPendingOrderIDs lists pending orders past their hold (sweeper scan).
func ExpiredPendingOrderIDs(ctx context.Context, pool DBTX) ([]uuid.UUID, error) {
	return scalars[uuid.UUID](pool.Query(ctx, sqlExpiredPending))
}

// ExpirePendingReservations is the store-only sweep: cancel every pending
// order whose hold expired, one transaction per order (several in one
// transaction would break the inventory lock order). Orders another
// transaction holds are skipped and picked up next run. The payment-aware
// sweep (Mercado Pago re-check) lives in internal/payments and calls
// ExpireOne per order.
func ExpirePendingReservations(ctx context.Context, pool *pgxpool.Pool) error {
	ids, err := ExpiredPendingOrderIDs(ctx, pool)
	if err != nil {
		return err
	}
	var first error
	for _, id := range ids {
		if err := ExpireOne(ctx, pool, id); err != nil {
			slog.ErrorContext(ctx, "reservation expiry failed", "order_id", id, "error", err)
			if first == nil {
				first = err
			}
		}
	}
	return first
}

// ExpireOne cancels one expired pending order (no-op unless it is still
// pending and past its hold).
func ExpireOne(ctx context.Context, pool *pgxpool.Pool, orderID uuid.UUID) error {
	return withTx(ctx, pool, func(tx pgx.Tx) error {
		var id uuid.UUID
		err := tx.QueryRow(ctx, sqlClaimExpiredOne, orderID).Scan(&id)
		if found, err := optional(err); err != nil || !found {
			return err
		}
		_, err = Apply(ctx, tx, orderID, EventCancel, ApplyOptions{})
		return err
	})
}
