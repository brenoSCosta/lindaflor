package payments

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/audit"
	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// OrderEvent is a payment-driven order transition requested from the
// order FSM.
type OrderEvent int

// Payment-driven order events (subset of the order FSM events).
const (
	// EventPay: pending_payment → paid (converts the reservation into a sale).
	EventPay OrderEvent = iota + 1
	// EventReopenPay: cancelled (never paid) → paid after a late approval;
	// re-reserves stock, failing with ErrInsufficientStock when it is gone.
	EventReopenPay
	// EventCancel: pending_payment → cancelled (releases the reservation).
	EventCancel
	// EventRefund: paid|processing|shipped|delivered → refunded (restocks).
	EventRefund
)

func (e OrderEvent) String() string {
	switch e {
	case EventPay:
		return "pay"
	case EventReopenPay:
		return "reopen_pay"
	case EventCancel:
		return "cancel"
	case EventRefund:
		return "refund"
	}
	return "unknown"
}

// ErrInsufficientStock is returned by OrderTransitioner.Apply when a
// EventReopenPay cannot re-reserve the stock (match with errors.Is).
var ErrInsufficientStock = errors.New("Estoque insuficiente para reabrir o pedido.") //nolint:staticcheck // user-facing pt-BR copy

// ErrOrdersNotWired is returned when the Service has no OrderTransitioner.
var ErrOrdersNotWired = errors.New("payments: order transitions are not wired")

// Actor attributes a payment-driven transition in the audit trail.
type Actor struct {
	ID   *uuid.UUID
	Name string
}

// MercadoPagoActor: webhook / sweeper transitions are attributed to the PSP.
var MercadoPagoActor = Actor{Name: "Mercado Pago"}

// OrderTransitioner is the narrow view of the order FSM (agent C's
// internal/store) that payments needs. It is the only writer of
// orders.status.
type OrderTransitioner interface {
	// Apply runs ev on the order inside tx (the caller already holds the
	// row lock: SELECT … FOR UPDATE), with its stock/coupon side effects
	// and an audit_events row attributed to actor (ActorID / ActorName),
	// plus the customer mail of the transition. Returns the new status.
	// EventReopenPay without stock must fail with ErrInsufficientStock
	// (errors.Is).
	Apply(ctx context.Context, tx pgx.Tx, orderID uuid.UUID, ev OrderEvent, actor Actor) (sqlc.OrderStatus, error)
	// ExpireOne cancels one expired pending order in its own transaction,
	// re-checking status and deadline under the row lock (no-op when the
	// order was paid/cancelled meanwhile).
	ExpireOne(ctx context.Context, orderID uuid.UUID) error
	// ExpirePendingReservations is the plain reservation sweep (static PIX).
	ExpirePendingReservations(ctx context.Context) error
}

// SweepGrace: past the hold by this much, the sweep cancels an MP order
// even when the MP API cannot be reached (a late approval still lands
// through the webhook as EventReopenPay).
const SweepGrace = time.Hour

// Service orchestrates orders ↔ payments: create the charge for a
// persisted order, sync a PSP payment into the order FSM, admin
// sync/refund and the payment-aware expiry sweep. Amounts, payer and
// expiry are always read from the orders row, never from a request.
type Service struct {
	Pool     *pgxpool.Pool
	Provider Provider
	Orders   OrderTransitioner
}

// OutcomeKind classifies a SyncOutcome.
type OutcomeKind int

// Sync outcomes.
const (
	// OutcomeApplied: the order moved From → To.
	OutcomeApplied OutcomeKind = iota + 1
	// OutcomeUnchanged: already applied, or a non-terminal PSP status.
	OutcomeUnchanged
	// OutcomeUnknownPayment: no order references this payment.
	OutcomeUnknownPayment
	// OutcomeMismatch: the payment's reference or amount disagrees.
	OutcomeMismatch
	// OutcomeNeedsAttention: approved after the order was cancelled and its
	// stock is gone; needs an admin refund.
	OutcomeNeedsAttention
)

// SyncOutcome is the result of applying a PSP payment state to its order.
type SyncOutcome struct {
	Kind     OutcomeKind
	OrderID  uuid.UUID
	From, To sqlc.OrderStatus
}

// ChargeView is the PIX instructions for the order page.
type ChargeView struct {
	PaymentID    int64
	QRCode       *string
	QRCodeBase64 *string
	TicketURL    *string
	ExpiresLabel string // "dd/mm/aaaa às HH:MM" (Brasília), "" when unknown
}

// OrderPayment is how /pedido/{id} presents payment.
type OrderPayment struct {
	// MercadoPago false = static PIX (local BR Code).
	MercadoPago bool
	// Charge is nil while the charge is not available yet (PSP down); the
	// page retries on the next load.
	Charge *ChargeView
}

// EventFor maps PSP state to an order event given the order's current
// status; ok=false means "no transition", which makes redelivery safe.
func EventFor(from sqlc.OrderStatus, everPaid bool, status ChargeStatus) (OrderEvent, bool) {
	switch {
	case status == StatusApproved && from == sqlc.OrderStatusPendingPayment:
		return EventPay, true
	case status == StatusApproved && from == sqlc.OrderStatusCancelled && !everPaid:
		return EventReopenPay, true
	case status == StatusFailed && from == sqlc.OrderStatusPendingPayment:
		return EventCancel, true
	case status == StatusRefunded:
		switch from {
		case sqlc.OrderStatusPaid, sqlc.OrderStatusProcessing, sqlc.OrderStatusShipped, sqlc.OrderStatusDelivered:
			return EventRefund, true
		}
	}
	return 0, false
}

var brt = time.FixedZone("BRT", -3*60*60)

// BrasiliaLabel formats t as "dd/mm/aaaa às HH:MM" in Brasília time.
func BrasiliaLabel(t time.Time) string { return t.In(brt).Format("02/01/2006 às 15:04") }

func chargeView(row sqlc.PaymentsChargeRowRow) *ChargeView {
	if row.MpPaymentID == nil {
		return nil
	}
	v := &ChargeView{PaymentID: *row.MpPaymentID, QRCode: row.MpQrCode, QRCodeBase64: row.MpQrCodeBase64, TicketURL: row.MpTicketUrl}
	if row.MpExpiresAt != nil {
		v.ExpiresLabel = BrasiliaLabel(*row.MpExpiresAt)
	}
	return v
}

// EnsureCharge creates the PSP charge for a pending Mercado Pago order
// unless it already has one (nil for other orders). Safe to call
// repeatedly: the idempotency key is the order id.
func (s *Service) EnsureCharge(ctx context.Context, orderID uuid.UUID) (*ChargeView, error) {
	q := sqlc.New(s.Pool)
	row, err := q.PaymentsChargeRow(ctx, orderID)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, nil
	}
	if err != nil {
		return nil, err
	}
	if row.PaymentProvider != string(config.ProviderMercadoPago) {
		return nil, nil
	}
	if v := chargeView(row); v != nil {
		return v, nil
	}
	if row.Status != sqlc.OrderStatusPendingPayment {
		return nil, nil
	}
	expiresAt := time.Now().Add(24 * time.Hour)
	if row.ReservationExpiresAt != nil {
		expiresAt = *row.ReservationExpiresAt
	}
	payer := ""
	if row.GuestEmail != nil {
		payer = *row.GuestEmail
	}
	charge, err := s.Provider.CreateCharge(ctx, NewCharge{
		OrderID: orderID, AmountCents: row.TotalCents, PayerEmail: payer, ExpiresAt: expiresAt,
		Description: "Pedido Linda Flor #" + orderID.String()[:8],
	})
	if err != nil {
		return nil, err
	}
	if err := q.PaymentsStoreCharge(ctx, sqlc.PaymentsStoreChargeParams{
		ID: orderID, MpPaymentID: charge.PaymentID, MpStatus: charge.RawStatus, MpStatusDetail: charge.StatusDetail,
		MpQrCode: charge.QRCode, MpQrCodeBase64: charge.QRCodeBase64, MpTicketUrl: charge.TicketURL, MpExpiresAt: expiresAt,
	}); err != nil {
		return nil, err
	}
	slog.InfoContext(ctx, "mercado pago charge created", "order_id", orderID, "mp_payment_id", charge.PaymentID, "status", charge.RawStatus)
	return &ChargeView{
		PaymentID: charge.PaymentID, QRCode: charge.QRCode, QRCodeBase64: charge.QRCodeBase64,
		TicketURL: charge.TicketURL, ExpiresLabel: BrasiliaLabel(expiresAt),
	}, nil
}

// OrderPayment is the payment presentation for /pedido/{id}. It lazily
// (re)tries the charge for a pending MP order that has none yet (e.g. MP
// was down at checkout); only database errors are returned.
func (s *Service) OrderPayment(ctx context.Context, orderID uuid.UUID) (OrderPayment, error) {
	row, err := sqlc.New(s.Pool).PaymentsChargeRow(ctx, orderID)
	if errors.Is(err, pgx.ErrNoRows) {
		return OrderPayment{}, nil
	}
	if err != nil {
		return OrderPayment{}, err
	}
	if row.PaymentProvider != string(config.ProviderMercadoPago) {
		return OrderPayment{}, nil
	}
	if v := chargeView(row); v != nil {
		return OrderPayment{MercadoPago: true, Charge: v}, nil
	}
	if row.Status != sqlc.OrderStatusPendingPayment {
		return OrderPayment{MercadoPago: true}, nil
	}
	v, err := s.EnsureCharge(ctx, orderID)
	if err != nil {
		if !IsPaymentError(err) {
			return OrderPayment{}, err
		}
		slog.WarnContext(ctx, "mercado pago charge not available", "order_id", orderID, "error", err)
		return OrderPayment{MercadoPago: true}, nil
	}
	return OrderPayment{MercadoPago: true, Charge: v}, nil
}

// IsPaymentError reports errors coming from the provider (as opposed to
// database / order-FSM errors).
func IsPaymentError(err error) bool {
	var te *TransientError
	var re *RejectedError
	var de *DecodeError
	return errors.Is(err, ErrUnavailable) || errors.Is(err, ErrNotSupported) ||
		errors.As(err, &te) || errors.As(err, &re) || errors.As(err, &de)
}

// findOrder resolves a payment's order: by mp_payment_id, else by its
// external_reference (our order id, read from the MP API — never from the
// webhook body) when the id was never stored.
func (s *Service) findOrder(ctx context.Context, info PaymentInfo) (uuid.UUID, bool, error) {
	q := sqlc.New(s.Pool)
	pid := info.PaymentID
	id, err := q.PaymentsOrderByMPPaymentID(ctx, &pid)
	if err == nil {
		return id, true, nil
	}
	if !errors.Is(err, pgx.ErrNoRows) {
		return uuid.Nil, false, err
	}
	if info.ExternalReference == nil {
		return uuid.Nil, false, nil
	}
	orderID, perr := uuid.Parse(strings.TrimSpace(*info.ExternalReference))
	if perr != nil {
		return uuid.Nil, false, nil
	}
	id, err = q.PaymentsClaimMPPaymentID(ctx, sqlc.PaymentsClaimMPPaymentIDParams{ID: orderID, MpPaymentID: info.PaymentID})
	if errors.Is(err, pgx.ErrNoRows) {
		return uuid.Nil, false, nil
	}
	return id, err == nil, err
}

// SyncPayment fetches a payment from the PSP and applies it to its order.
func (s *Service) SyncPayment(ctx context.Context, paymentID int64, actor Actor) (SyncOutcome, error) {
	info, err := s.Provider.FetchStatus(ctx, paymentID)
	if err != nil {
		return SyncOutcome{}, err
	}
	return s.ApplyPaymentInfo(ctx, info, actor)
}

// ApplyPaymentInfo applies PSP payment state (as fetched from the API) to
// its order in one transaction under the order row lock. Idempotent: the
// event derives from the order's current status.
func (s *Service) ApplyPaymentInfo(ctx context.Context, info PaymentInfo, actor Actor) (out SyncOutcome, err error) {
	if s.Orders == nil {
		return SyncOutcome{}, ErrOrdersNotWired
	}
	orderID, found, err := s.findOrder(ctx, info)
	if err != nil {
		return SyncOutcome{}, err
	}
	if !found {
		slog.WarnContext(ctx, "mercado pago payment has no matching order", "mp_payment_id", info.PaymentID)
		return SyncOutcome{Kind: OutcomeUnknownPayment}, nil
	}
	tx, err := s.Pool.Begin(ctx)
	if err != nil {
		return SyncOutcome{}, err
	}
	committed := false
	defer func() {
		if !committed {
			_ = tx.Rollback(ctx)
		}
	}()
	commit := func() error {
		committed = true
		return tx.Commit(ctx)
	}
	q := sqlc.New(tx)
	locked, err := q.PaymentsLockOrder(ctx, orderID)
	if err != nil {
		return SyncOutcome{}, err
	}
	from := locked.Status
	if info.ExternalReference != nil && strings.TrimSpace(*info.ExternalReference) != orderID.String() {
		slog.ErrorContext(ctx, "mercado pago external_reference does not match the order", "order_id", orderID, "mp_payment_id", info.PaymentID)
		return SyncOutcome{Kind: OutcomeMismatch, OrderID: orderID}, nil
	}
	setStatus := sqlc.PaymentsSetMPStatusParams{ID: orderID, MpStatus: info.RawStatus, MpStatusDetail: info.StatusDetail}
	if err := q.PaymentsSetMPStatus(ctx, setStatus); err != nil {
		return SyncOutcome{}, err
	}
	ev, ok := EventFor(from, locked.EverPaid, info.Status)
	if !ok {
		return SyncOutcome{Kind: OutcomeUnchanged, OrderID: orderID}, commit()
	}
	// Never mark paid on an amount we did not ask for.
	if (ev == EventPay || ev == EventReopenPay) && (info.AmountCents == nil || *info.AmountCents != locked.TotalCents) {
		slog.ErrorContext(ctx, "mercado pago approved amount does not match the order total", "order_id", orderID, "mp_payment_id", info.PaymentID)
		return SyncOutcome{Kind: OutcomeMismatch, OrderID: orderID}, commit()
	}
	to, err := s.Orders.Apply(ctx, tx, orderID, ev, actor)
	if errors.Is(err, ErrInsufficientStock) {
		committed = true
		_ = tx.Rollback(ctx)
		if err := sqlc.New(s.Pool).PaymentsSetMPStatus(ctx, setStatus); err != nil {
			return SyncOutcome{}, err
		}
		slog.ErrorContext(ctx, "late mercado pago approval but stock is gone: refund required", "order_id", orderID, "mp_payment_id", info.PaymentID)
		return SyncOutcome{Kind: OutcomeNeedsAttention, OrderID: orderID}, nil
	}
	if err != nil {
		return SyncOutcome{}, err
	}
	if err := commit(); err != nil {
		return SyncOutcome{}, err
	}
	slog.InfoContext(ctx, "mercado pago payment applied", "order_id", orderID, "mp_payment_id", info.PaymentID, "new_status", string(to))
	return SyncOutcome{Kind: OutcomeApplied, OrderID: orderID, From: from, To: to}, nil
}

// AdminPaymentInfo is the admin view of an order's payment.
type AdminPaymentInfo struct {
	Provider       config.ProviderKind
	MPPaymentID    *int64
	MPStatus       *string
	MPStatusDetail *string
	SyncedAt       string // BrasiliaLabel, "" when never synced
}

// Refundable: paid-through-delivered orders, and an approved payment on a
// cancelled order (late approval without stock).
func (a AdminPaymentInfo) Refundable(orderStatus sqlc.OrderStatus) bool {
	if a.MPPaymentID == nil {
		return false
	}
	switch orderStatus {
	case sqlc.OrderStatusPaid, sqlc.OrderStatusProcessing, sqlc.OrderStatusShipped, sqlc.OrderStatusDelivered:
		return true
	case sqlc.OrderStatusCancelled:
		return a.MPStatus != nil && *a.MPStatus == "approved"
	}
	return false
}

// AdminPaymentInfo loads the admin payment view (ok=false: no order).
func (s *Service) AdminPaymentInfo(ctx context.Context, orderID uuid.UUID) (AdminPaymentInfo, bool, error) {
	row, err := sqlc.New(s.Pool).PaymentsAdminInfo(ctx, orderID)
	if errors.Is(err, pgx.ErrNoRows) {
		return AdminPaymentInfo{}, false, nil
	}
	if err != nil {
		return AdminPaymentInfo{}, false, err
	}
	info := AdminPaymentInfo{
		Provider: config.ProviderStaticPix, MPPaymentID: row.MpPaymentID, MPStatus: row.MpStatus, MPStatusDetail: row.MpStatusDetail,
	}
	if row.PaymentProvider == string(config.ProviderMercadoPago) {
		info.Provider = config.ProviderMercadoPago
	}
	if row.MpSyncedAt != nil {
		info.SyncedAt = BrasiliaLabel(*row.MpSyncedAt)
	}
	return info, true, nil
}

// AdminAction is an admin payment button, posted through the existing
// POST /admin/pedidos/{id} form as status=mp_sync|mp_refund.
type AdminAction int

// Admin actions.
const (
	AdminSync AdminAction = iota + 1
	AdminRefund
)

// ParseAdminAction parses "mp_sync" / "mp_refund".
func ParseAdminAction(raw string) (AdminAction, bool) {
	switch raw {
	case "mp_sync":
		return AdminSync, true
	case "mp_refund":
		return AdminRefund, true
	}
	return 0, false
}

func toastErr(msg string) error { return errors.New(msg) }

// RunAdminAction runs an admin payment action (caller already checked the
// admin role). The success string and the error message are the pt-BR
// toast texts.
func (s *Service) RunAdminAction(ctx context.Context, orderID uuid.UUID, action AdminAction, actor Actor) (string, error) {
	unavailable := toastErr("Não foi possível falar com o Mercado Pago.")
	info, found, err := s.AdminPaymentInfo(ctx, orderID)
	if err != nil {
		return "", unavailable
	}
	if !found {
		return "", toastErr("Pedido não encontrado.")
	}
	if info.MPPaymentID == nil {
		return "", toastErr("Pedido sem pagamento no Mercado Pago.")
	}
	paymentID := *info.MPPaymentID
	if !IsMercadoPago(s.Provider) {
		return "", toastErr("Mercado Pago não está configurado.")
	}
	switch action {
	case AdminSync:
		out, err := s.SyncPayment(ctx, paymentID, actor)
		if err != nil {
			if IsPaymentError(err) {
				return "", toastErr(err.Error())
			}
			slog.ErrorContext(ctx, "mercado pago sync failed", "order_id", orderID, "error", err)
			return "", unavailable
		}
		switch out.Kind {
		case OutcomeApplied:
			return "Pedido sincronizado com o Mercado Pago.", nil
		case OutcomeUnchanged:
			return "Pedido já está em dia com o Mercado Pago.", nil
		case OutcomeNeedsAttention:
			return "", toastErr("Pagamento aprovado, mas sem estoque para reabrir o pedido. Estorne o pagamento.")
		case OutcomeMismatch:
			return "", toastErr("O pagamento não confere com o pedido (referência ou valor). Verifique no Mercado Pago.")
		default:
			return "", toastErr("Pagamento não encontrado.")
		}
	case AdminRefund:
		status, err := sqlc.New(s.Pool).PaymentsOrderStatus(ctx, orderID)
		if err != nil && !errors.Is(err, pgx.ErrNoRows) {
			return "", unavailable
		}
		if !info.Refundable(status) {
			return "", toastErr("Este pedido não pode ser estornado.")
		}
		if err := s.Provider.Refund(ctx, paymentID); err != nil {
			return "", toastErr(err.Error())
		}
		if err := s.recordRefund(ctx, orderID, paymentID, actor); err != nil {
			slog.ErrorContext(ctx, "refund recorded at MP but not locally", "order_id", orderID, "error", err)
			return "", toastErr("Estorno feito no Mercado Pago, mas o pedido não foi atualizado. Use Sincronizar.")
		}
		return "Pagamento estornado no Mercado Pago.", nil
	}
	return "", toastErr("Ação inválida.")
}

// recordRefund runs after a successful PSP refund: applies EventRefund
// (stock/coupon side effects) and audits the PSP call itself.
func (s *Service) recordRefund(ctx context.Context, orderID uuid.UUID, paymentID int64, actor Actor) error {
	if s.Orders == nil {
		return ErrOrdersNotWired
	}
	tx, err := s.Pool.Begin(ctx)
	if err != nil {
		return err
	}
	defer func() { _ = tx.Rollback(ctx) }()
	q := sqlc.New(tx)
	from, err := q.PaymentsLockOrderStatus(ctx, orderID)
	if err != nil {
		return err
	}
	if _, ok := EventFor(from, true, StatusRefunded); ok {
		if _, err := s.Orders.Apply(ctx, tx, orderID, EventRefund, actor); err != nil {
			return err
		}
	}
	if err := q.PaymentsMarkRefunded(ctx, orderID); err != nil {
		return err
	}
	name := actor.Name
	if err := audit.Record(ctx, tx, audit.Event{
		AggregateType: "order", AggregateID: orderID.String(), EntityType: "order", EntityID: orderID,
		ActorID: actor.ID, ActorName: &name,
		Metadata: map[string]any{"action": "mp_refund", "mp_payment_id": paymentID},
	}); err != nil {
		return err
	}
	return tx.Commit(ctx)
}

// ExpirePendingReservations is the payment-aware reservation sweep. With
// Mercado Pago, every expired pending MP order is re-fetched first: an
// approval that never reached us (lost webhook) marks it paid instead of
// cancelling. If MP cannot be reached the order is kept until SweepGrace
// past its hold. Everything else is cancelled one order per transaction
// (OrderTransitioner.ExpireOne).
func (s *Service) ExpirePendingReservations(ctx context.Context) error {
	if s.Orders == nil {
		return ErrOrdersNotWired
	}
	if !IsMercadoPago(s.Provider) {
		return s.Orders.ExpirePendingReservations(ctx)
	}
	rows, err := sqlc.New(s.Pool).PaymentsExpiredPending(ctx, SweepGrace.Seconds())
	if err != nil {
		return err
	}
	var firstErr error
	for _, row := range rows {
		if row.MpPaymentID != 0 {
			if _, err := s.SyncPayment(ctx, row.MpPaymentID, MercadoPagoActor); err != nil {
				if !row.PastGrace {
					slog.WarnContext(ctx, "expiry deferred: mercado pago unreachable", "order_id", row.ID, "error", err)
					continue
				}
				slog.WarnContext(ctx, "expiring without mercado pago confirmation", "order_id", row.ID, "error", err)
			}
		}
		if err := s.Orders.ExpireOne(ctx, row.ID); err != nil {
			slog.ErrorContext(ctx, "reservation expiry failed", "order_id", row.ID, "error", err)
			if firstErr == nil {
				firstErr = fmt.Errorf("expire order %s: %w", row.ID, err)
			}
		}
	}
	return firstErr
}
