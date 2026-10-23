package store

import (
	"context"
	"errors"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/audit"
)

// Returns and exchanges (src/app/store/returns.rs): the customer opens a
// request from /pedido/{id}; the admin walks it through
// requested → approved → received → resolved (refunded | exchanged), or
// rejected at any open step. Every step writes audit_events
// (aggregate_type 'return') and queues the customer mail in the same
// transaction.
//
// Window: CDC art. 49 counts 7 days from receipt (delivered_at, or
// shipped_at + 15 days when never marked delivered); defects get 90 days.
// Refunds: a received devolução/defeito covering every unit is refunded
// through the order FSM; partial returns are settled out of band and only
// recorded here.

// Return windows (days).
const (
	ReturnWindowDays             = 7
	DefectWindowDays             = 90
	UnconfirmedDeliveryGraceDays = 15
	ReturnReasonMaxChars         = 1000
	ReturnSizeMaxChars           = 20
	ReturnAdminTextMaxChars      = 2000
)

// ReturnKind is exchange | return | defect.
type ReturnKind string

// Kinds.
const (
	ReturnExchange ReturnKind = "exchange"
	ReturnReturn   ReturnKind = "return"
	ReturnDefect   ReturnKind = "defect"
)

// AllReturnKinds in form order.
var AllReturnKinds = []ReturnKind{ReturnExchange, ReturnReturn, ReturnDefect}

// ParseReturnKind parses a kind.
func ParseReturnKind(raw string) (ReturnKind, bool) {
	switch k := ReturnKind(raw); k {
	case ReturnExchange, ReturnReturn, ReturnDefect:
		return k, true
	}
	return "", false
}

// Label is the pt-BR label.
func (k ReturnKind) Label() string {
	switch k {
	case ReturnExchange:
		return "Troca de tamanho"
	case ReturnReturn:
		return "Devolução"
	case ReturnDefect:
		return "Defeito"
	}
	return "—"
}

// WindowDays is the request window.
func (k ReturnKind) WindowDays() int {
	if k == ReturnDefect {
		return DefectWindowDays
	}
	return ReturnWindowDays
}

// CanRefund: devoluções and defects end in a refund.
func (k ReturnKind) CanRefund() bool { return k == ReturnReturn || k == ReturnDefect }

// CanExchange: exchanges and defects end in a new piece.
func (k ReturnKind) CanExchange() bool { return k == ReturnExchange || k == ReturnDefect }

// ReturnStatus is the request status.
type ReturnStatus string

// Statuses.
const (
	ReturnRequested ReturnStatus = "requested"
	ReturnApproved  ReturnStatus = "approved"
	ReturnReceived  ReturnStatus = "received"
	ReturnResolved  ReturnStatus = "resolved"
	ReturnRejected  ReturnStatus = "rejected"
)

// ParseReturnStatus parses a status.
func ParseReturnStatus(raw string) (ReturnStatus, bool) {
	switch s := ReturnStatus(raw); s {
	case ReturnRequested, ReturnApproved, ReturnReceived, ReturnResolved, ReturnRejected:
		return s, true
	}
	return "", false
}

// Label is the pt-BR label.
func (s ReturnStatus) Label() string {
	switch s {
	case ReturnRequested:
		return "Em análise"
	case ReturnApproved:
		return "Aprovada — aguardando envio"
	case ReturnReceived:
		return "Peça recebida"
	case ReturnResolved:
		return "Concluída"
	case ReturnRejected:
		return "Recusada"
	}
	return "—"
}

// IsOpen: requested, approved or received.
func (s ReturnStatus) IsOpen() bool {
	return s == ReturnRequested || s == ReturnApproved || s == ReturnReceived
}

// Tone is the admin badge tone.
func (s ReturnStatus) Tone() BadgeTone {
	switch s {
	case ReturnRequested:
		return ToneSecondary
	case ReturnRejected:
		return ToneDestructive
	}
	return TonePrimary
}

// ResolutionLabel labels refunded | exchanged.
func ResolutionLabel(r string) string {
	switch r {
	case "refunded":
		return "Reembolsada"
	case "exchanged":
		return "Trocada"
	}
	return ""
}

// Ineligibility reasons.
var (
	ErrReturnNotShipped = errors.New("A troca ou devolução pode ser solicitada depois que o pedido for enviado.")
	ErrReturnExpired    = errors.New("O prazo para solicitar troca ou devolução deste pedido terminou.")
)

// ReturnDeadline is the end of the window for kind, ok=false while the
// order has not shipped.
func ReturnDeadline(kind ReturnKind, deliveredAt, shippedAt *time.Time) (time.Time, bool) {
	var received time.Time
	switch {
	case deliveredAt != nil:
		received = *deliveredAt
	case shippedAt != nil:
		received = shippedAt.Add(UnconfirmedDeliveryGraceDays * 24 * time.Hour)
	default:
		return time.Time{}, false
	}
	return received.Add(time.Duration(kind.WindowDays()) * 24 * time.Hour), true
}

// CheckReturnEligibility returns the deadline when a kind request can be
// opened for an order in orderStatus at now.
func CheckReturnEligibility(orderStatus string, kind ReturnKind, deliveredAt, shippedAt *time.Time, now time.Time) (time.Time, error) {
	if orderStatus != "shipped" && orderStatus != "delivered" {
		switch orderStatus {
		case "pending_payment", "paid", "processing":
			return time.Time{}, ErrReturnNotShipped
		}
		return time.Time{}, ErrReturnExpired
	}
	deadline, ok := ReturnDeadline(kind, deliveredAt, shippedAt)
	if !ok {
		return time.Time{}, ErrReturnNotShipped
	}
	if now.After(deadline) {
		return time.Time{}, ErrReturnExpired
	}
	return deadline, nil
}

// Return errors.
var (
	ErrReturnNotFound          = errors.New("Pedido não encontrado.")
	ErrReturnAlreadyOpen       = errors.New("Já existe uma solicitação em andamento para este pedido.")
	ErrReturnIllegalTransition = errors.New("Ação inválida para o status atual da solicitação.")
)

// ReturnableItem is an order line with how many units can still be claimed.
type ReturnableItem struct {
	OrderItemID    uuid.UUID `db:"order_item_id"`
	ProductName    string    `db:"product_name"`
	VariantLabel   string    `db:"variant_label"`
	Quantity       int32     `db:"quantity"`
	UnitPriceCents int64     `db:"unit_price_cents"`
	Returnable     int32     `db:"returnable"`
}

var sqlReturnableItems = sqlText(`SELECT oi.id AS order_item_id, oi.product_name, oi.variant_label,
        oi.quantity, oi.unit_price_cents,
        GREATEST(oi.quantity - COALESCE((
            SELECT SUM(ri.quantity)
            FROM return_request_items ri
            JOIN return_requests r ON r.id = ri.return_request_id
            WHERE ri.order_item_id = oi.id AND r.status <> 'rejected'
        ), 0), 0)::int AS returnable
 FROM order_items oi
 WHERE oi.order_id = $1
 ORDER BY oi.created_at, oi.id`)

// ReturnableItems lists the order's lines with claimable units.
func ReturnableItems(ctx context.Context, q DBTX, orderID uuid.UUID) ([]ReturnableItem, error) {
	return collect[ReturnableItem](q.Query(ctx, sqlReturnableItems, orderID))
}

// ReturnLine is (order item, quantity).
type ReturnLine struct {
	OrderItemID uuid.UUID
	Quantity    int32
}

// NewReturn is a customer's submitted request.
type NewReturn struct {
	Kind        ReturnKind
	Reason      string
	DesiredSize *string
	// Lines with zero quantities are ignored.
	Lines []ReturnLine
}

func cleanText(raw string, maxChars int) (string, bool) {
	t := trimSpace(raw)
	if t == "" || runeLen(t) > maxChars {
		return "", false
	}
	return t, true
}

var (
	sqlReturnOrderLock = sqlText(`SELECT status::text AS status, access_token,
        access_token_expires_at <= now() AS token_expired, user_id,
        delivered_at, shipped_at, now() AS now
 FROM orders WHERE id = $1 FOR UPDATE`)
	sqlReturnOpenExists = sqlText(`SELECT EXISTS (SELECT 1 FROM return_requests
                WHERE order_id = $1
                  AND status IN ('requested', 'approved', 'received'))`)
	sqlInsertReturn = sqlText(`INSERT INTO return_requests
     (id, order_id, user_id, kind, reason, desired_size)
 VALUES ($1, $2, $3, $4, $5, $6)`)
	sqlInsertReturnItem = sqlText(`INSERT INTO return_request_items (return_request_id, order_item_id, quantity)
 VALUES ($1, $2, $3)`)
)

func recordReturnAudit(ctx context.Context, tx DBTX, returnID uuid.UUID, actor Actor, meta map[string]any) error {
	return audit.Record(ctx, tx, audit.Event{
		AggregateType: "return",
		AggregateID:   returnID.String(),
		EntityType:    "return_request",
		EntityID:      returnID,
		ActorID:       actor.ID,
		ActorName:     actor.Name,
		Metadata:      meta,
	})
}

// CreateReturnRequest opens a request. Access (owner session or valid guest
// token), eligibility and per-item caps are checked under the order row
// lock. Errors: ErrReturnNotFound, ErrReturnNotShipped/ErrReturnExpired,
// ErrReturnAlreadyOpen, *UserError.
func CreateReturnRequest(ctx context.Context, pool *pgxpool.Pool, orderID uuid.UUID, queryToken *string, sessionUserID *uuid.UUID, in NewReturn) (uuid.UUID, error) {
	reason, ok := cleanText(in.Reason, ReturnReasonMaxChars)
	if !ok {
		return uuid.Nil, userErr("Descreva o motivo (até 1000 caracteres).")
	}
	var desired *string
	if in.DesiredSize != nil && trimSpace(*in.DesiredSize) != "" {
		size, ok := cleanText(*in.DesiredSize, ReturnSizeMaxChars)
		if !ok {
			return uuid.Nil, userErr("Tamanho desejado inválido.")
		}
		desired = &size
	}
	returnID := uuid.Must(uuid.NewV7())
	err := withTx(ctx, pool, func(tx pgx.Tx) error {
		var status string
		var token uuid.UUID
		var expired bool
		var owner *uuid.UUID
		var delivered, shipped *time.Time
		var now time.Time
		err := tx.QueryRow(ctx, sqlReturnOrderLock, orderID).Scan(&status, &token, &expired, &owner, &delivered, &shipped, &now)
		if found, err := optional(err); err != nil {
			return err
		} else if !found {
			return ErrReturnNotFound
		}
		if !TokenOrOwnerAllows(token, expired, owner, queryToken, sessionUserID) {
			return ErrReturnNotFound
		}
		if _, err := CheckReturnEligibility(status, in.Kind, delivered, shipped, now); err != nil {
			return err
		}
		var open bool
		if err := tx.QueryRow(ctx, sqlReturnOpenExists, orderID).Scan(&open); err != nil {
			return err
		}
		if open {
			return ErrReturnAlreadyOpen
		}
		items, err := ReturnableItems(ctx, tx, orderID)
		if err != nil {
			return err
		}
		var lines []ReturnLine
		for _, l := range in.Lines {
			if l.Quantity == 0 {
				continue
			}
			var item *ReturnableItem
			for i := range items {
				if items[i].OrderItemID == l.OrderItemID {
					item = &items[i]
				}
			}
			if item == nil {
				return userErr("Item inválido.")
			}
			dup := false
			for _, x := range lines {
				dup = dup || x.OrderItemID == l.OrderItemID
			}
			if l.Quantity < 0 || l.Quantity > item.Returnable || dup {
				return userErr("Quantidade inválida.")
			}
			lines = append(lines, l)
		}
		if len(lines) == 0 {
			return userErr("Escolha pelo menos um item e a quantidade.")
		}
		if _, err := tx.Exec(ctx, sqlInsertReturn, returnID, orderID, sessionUserID, string(in.Kind), reason, desired); err != nil {
			return err
		}
		metaItems := make([]map[string]any, 0, len(lines))
		for _, l := range lines {
			if _, err := tx.Exec(ctx, sqlInsertReturnItem, returnID, l.OrderItemID, l.Quantity); err != nil {
				return err
			}
			metaItems = append(metaItems, map[string]any{"order_item_id": l.OrderItemID, "quantity": l.Quantity})
		}
		if err := recordReturnAudit(ctx, tx, returnID, Actor{ID: sessionUserID, Name: ptr("cliente")}, map[string]any{
			"action": "create", "order_id": orderID, "kind": string(in.Kind),
			"to": string(ReturnRequested), "items": metaItems,
		}); err != nil {
			return err
		}
		return enqueueReturnMail(ctx, tx, returnID, ReturnMailStep{Kind: "requested"})
	})
	if err != nil {
		return uuid.Nil, err
	}
	return returnID, nil
}

// ReturnSummary is a request as listed on the order page and in the admin.
type ReturnSummary struct {
	ID              uuid.UUID `db:"id"`
	OrderID         uuid.UUID `db:"order_id"`
	Kind            string    `db:"kind"`
	Status          string    `db:"status"`
	Resolution      *string   `db:"resolution"`
	CreatedAt       time.Time `db:"created_at"`
	RefundCents     *int64    `db:"refund_cents"`
	Instructions    *string   `db:"instructions"`
	RejectionReason *string   `db:"rejection_reason"`
}

// KindLabel is the kind label.
func (r ReturnSummary) KindLabel() string {
	if k, ok := ParseReturnKind(r.Kind); ok {
		return k.Label()
	}
	return "—"
}

// StatusLabel is the status, or the resolution once resolved.
func (r ReturnSummary) StatusLabel() string {
	s, ok := ParseReturnStatus(r.Status)
	if !ok {
		return "—"
	}
	if s == ReturnResolved && r.Resolution != nil {
		if l := ResolutionLabel(*r.Resolution); l != "" {
			return l
		}
	}
	return s.Label()
}

// StatusTone is the admin badge tone.
func (r ReturnSummary) StatusTone() BadgeTone {
	s, _ := ParseReturnStatus(r.Status)
	return s.Tone()
}

var sqlRequestsForOrder = sqlText(`SELECT r.id, r.order_id, r.kind, r.status, r.resolution,
  r.created_at, r.refund_cents, r.instructions, r.rejection_reason
 FROM return_requests r
 WHERE r.order_id = $1 ORDER BY r.created_at DESC`)

// RequestsForOrder lists an order's requests, newest first.
func RequestsForOrder(ctx context.Context, q DBTX, orderID uuid.UUID) ([]ReturnSummary, error) {
	return collect[ReturnSummary](q.Query(ctx, sqlRequestsForOrder, orderID))
}

// OpenKind is a kind still inside its window.
type OpenKind struct {
	Kind     ReturnKind
	Deadline time.Time
}

// OrderReturns is what the order page needs.
type OrderReturns struct {
	// Visible: show the section (shipped/delivered orders, or any request).
	Visible   bool
	Requests  []ReturnSummary
	OpenKinds []OpenKind
	// Blocked is why no request can be opened, if any.
	Blocked *string
}

// CanOpen reports whether a new request can be opened.
func (o OrderReturns) CanOpen() bool { return o.Blocked == nil && len(o.OpenKinds) > 0 }

// ReturnDeadline is the devolução deadline when open.
func (o OrderReturns) ReturnDeadline() (time.Time, bool) {
	for _, k := range o.OpenKinds {
		if k.Kind == ReturnReturn {
			return k.Deadline, true
		}
	}
	return time.Time{}, false
}

var sqlOrderReturnState = sqlText(`SELECT status::text, delivered_at, shipped_at, now() FROM orders WHERE id = $1`)

// LoadOrderReturns builds OrderReturns (the caller already checked access).
func LoadOrderReturns(ctx context.Context, q DBTX, orderID uuid.UUID) (OrderReturns, error) {
	var status string
	var delivered, shipped *time.Time
	var now time.Time
	err := q.QueryRow(ctx, sqlOrderReturnState, orderID).Scan(&status, &delivered, &shipped, &now)
	if found, err := optional(err); err != nil || !found {
		return OrderReturns{Requests: []ReturnSummary{}}, err
	}
	requests, err := RequestsForOrder(ctx, q, orderID)
	if err != nil {
		return OrderReturns{}, err
	}
	out := OrderReturns{Requests: requests}
	var firstErr error
	for _, k := range AllReturnKinds {
		deadline, err := CheckReturnEligibility(status, k, delivered, shipped, now)
		if err != nil {
			if firstErr == nil {
				firstErr = err
			}
			continue
		}
		out.OpenKinds = append(out.OpenKinds, OpenKind{Kind: k, Deadline: deadline})
	}
	hasOpen := false
	for _, r := range requests {
		if s, ok := ParseReturnStatus(r.Status); ok && s.IsOpen() {
			hasOpen = true
		}
	}
	switch {
	case hasOpen:
		out.Blocked = ptr("Você já tem uma solicitação em andamento para este pedido.")
	case len(out.OpenKinds) == 0:
		if firstErr != nil {
			out.Blocked = ptr(firstErr.Error())
		}
	default:
		items, err := ReturnableItems(ctx, q, orderID)
		if err != nil {
			return OrderReturns{}, err
		}
		claimable := false
		for _, it := range items {
			claimable = claimable || it.Returnable > 0
		}
		if !claimable {
			out.Blocked = ptr("Todos os itens deste pedido já estão em uma solicitação.")
		}
	}
	out.Visible = len(requests) > 0 || status == "shipped" || status == "delivered"
	return out, nil
}

// --- Admin side ---

// ReturnActionKind names an admin step.
type ReturnActionKind string

// Admin actions (audit names).
const (
	ReturnActionApprove      ReturnActionKind = "approve"
	ReturnActionReject       ReturnActionKind = "reject"
	ReturnActionReceive      ReturnActionKind = "receive"
	ReturnActionResolveSwap  ReturnActionKind = "resolve_exchanged"
	ReturnActionRefundOrder  ReturnActionKind = "refund_order"
	ReturnActionRecordRefund ReturnActionKind = "record_refund"
	ReturnActionNote         ReturnActionKind = "note"
)

// AdminReturnAction is one admin step on a request.
type AdminReturnAction struct {
	Kind ReturnActionKind
	// Text: approve instructions, reject reason or note.
	Text string
	// RefundCents for RecordRefund.
	RefundCents int64
	// Restock for RefundOrder / RecordRefund.
	Restock bool
}

// AdminReturnTransition is the target status of action from from, ok=false
// when illegal. Note keeps the status.
func AdminReturnTransition(from ReturnStatus, kind ReturnKind, action ReturnActionKind) (ReturnStatus, bool) {
	switch {
	case action == ReturnActionNote:
		return from, true
	case from == ReturnRequested && action == ReturnActionApprove:
		return ReturnApproved, true
	case from.IsOpen() && action == ReturnActionReject:
		return ReturnRejected, true
	case from == ReturnApproved && action == ReturnActionReceive:
		return ReturnReceived, true
	case from == ReturnReceived && action == ReturnActionResolveSwap && kind.CanExchange():
		return ReturnResolved, true
	case from == ReturnReceived && (action == ReturnActionRefundOrder || action == ReturnActionRecordRefund) && kind.CanRefund():
		return ReturnResolved, true
	}
	return "", false
}

var (
	sqlLockReturn    = sqlText(`SELECT order_id, kind, status FROM return_requests WHERE id = $1 FOR UPDATE`)
	sqlReturnedLines = sqlText(`SELECT oi.variant_id, ri.quantity, oi.unit_price_cents
 FROM return_request_items ri
 JOIN order_items oi ON oi.id = ri.order_item_id
 WHERE ri.return_request_id = $1
 ORDER BY oi.variant_id`)
	sqlCoversWholeOrder = sqlText(`SELECT NOT EXISTS (
     SELECT 1 FROM order_items oi
     JOIN return_requests r ON r.order_id = oi.order_id
     LEFT JOIN return_request_items ri
       ON ri.order_item_id = oi.id AND ri.return_request_id = r.id
     WHERE r.id = $1 AND COALESCE(ri.quantity, 0) <> oi.quantity
 )`)
	sqlRefundOptions = sqlText(`SELECT o.status::text, o.payment_provider, o.total_cents,
        COALESCE((SELECT SUM(ri.quantity * oi.unit_price_cents)
                  FROM return_request_items ri
                  JOIN order_items oi ON oi.id = ri.order_item_id
                  WHERE ri.return_request_id = r.id), 0)::bigint
 FROM return_requests r JOIN orders o ON o.id = r.order_id
 WHERE r.id = $1`)
	sqlReturnTouch          = sqlText(`UPDATE return_requests SET updated_at = now() WHERE id = $1`)
	sqlReturnApprove        = sqlText(`UPDATE return_requests SET status = 'approved', instructions = $2, approved_at = now(), updated_at = now() WHERE id = $1`)
	sqlReturnReject         = sqlText(`UPDATE return_requests SET status = 'rejected', rejection_reason = $2, updated_at = now() WHERE id = $1`)
	sqlReturnReceive        = sqlText(`UPDATE return_requests SET status = 'received', received_at = now(), updated_at = now() WHERE id = $1`)
	sqlReturnResolve        = sqlText(`UPDATE return_requests SET status = 'resolved', resolution = $2, refund_cents = $3, resolved_at = now(), updated_at = now() WHERE id = $1`)
	sqlOrderProvider        = sqlText(`SELECT payment_provider FROM orders WHERE id = $1`)
	sqlOrderTotal           = sqlText(`SELECT total_cents FROM orders WHERE id = $1`)
	sqlOrderStatusTotalLock = sqlText(`SELECT status::text, total_cents FROM orders WHERE id = $1 FOR UPDATE`)
)

func lockReturn(ctx context.Context, tx DBTX, returnID uuid.UUID) (uuid.UUID, ReturnKind, ReturnStatus, error) {
	var orderID uuid.UUID
	var kindRaw, statusRaw string
	err := tx.QueryRow(ctx, sqlLockReturn, returnID).Scan(&orderID, &kindRaw, &statusRaw)
	if found, err := optional(err); err != nil {
		return uuid.Nil, "", "", err
	} else if !found {
		return uuid.Nil, "", "", ErrReturnNotFound
	}
	kind, ok1 := ParseReturnKind(kindRaw)
	status, ok2 := ParseReturnStatus(statusRaw)
	if !ok1 || !ok2 {
		return uuid.Nil, "", "", ErrReturnNotFound
	}
	return orderID, kind, status, nil
}

func coversWholeOrder(ctx context.Context, q DBTX, returnID uuid.UUID) (bool, error) {
	var whole bool
	err := q.QueryRow(ctx, sqlCoversWholeOrder, returnID).Scan(&whole)
	return whole, err
}

// RefundOptions are the refund paths offered for a received request.
type RefundOptions struct {
	// FullOrder: the request covers the whole order and the order is still
	// refundable ("Estornar pedido" / "Estornar no Mercado Pago").
	FullOrder   bool
	MercadoPago bool
	// MaxManualCents caps RecordRefund (returned units, capped at total).
	MaxManualCents int64
	OrderRefunded  bool
}

// LoadRefundOptions computes RefundOptions.
func LoadRefundOptions(ctx context.Context, q DBTX, returnID uuid.UUID) (RefundOptions, error) {
	var status, provider string
	var total, items int64
	if err := q.QueryRow(ctx, sqlRefundOptions, returnID).Scan(&status, &provider, &total, &items); err != nil {
		return RefundOptions{}, err
	}
	whole, err := coversWholeOrder(ctx, q, returnID)
	if err != nil {
		return RefundOptions{}, err
	}
	return RefundOptions{
		FullOrder:      whole && (status == "shipped" || status == "delivered"),
		MercadoPago:    provider == ProviderMercadoPago,
		MaxManualCents: max(min(items, total), 0),
		OrderRefunded:  status == "refunded",
	}, nil
}

func resolveReturn(ctx context.Context, tx DBTX, returnID uuid.UUID, resolution string, refund *int64) error {
	_, err := tx.Exec(ctx, sqlReturnResolve, returnID, resolution, refund)
	return err
}

// AdminApplyReturn runs one admin step in a transaction: status change,
// side effects (order refund / restock), audit row and customer mail.
// Errors: ErrReturnNotFound, ErrReturnIllegalTransition, *UserError,
// order Apply domain errors.
func AdminApplyReturn(ctx context.Context, pool *pgxpool.Pool, returnID uuid.UUID, action AdminReturnAction, actor Actor) (ReturnStatus, error) {
	var next ReturnStatus
	err := withTx(ctx, pool, func(tx pgx.Tx) error {
		var err error
		next, err = adminApplyReturnTx(ctx, tx, returnID, action, actor)
		return err
	})
	return next, err
}

func adminApplyReturnTx(ctx context.Context, tx DBTX, returnID uuid.UUID, action AdminReturnAction, actor Actor) (ReturnStatus, error) {
	orderID, kind, from, err := lockReturn(ctx, tx, returnID)
	if err != nil {
		return "", err
	}
	next, ok := AdminReturnTransition(from, kind, action.Kind)
	if !ok {
		return "", ErrReturnIllegalTransition
	}
	meta := map[string]any{
		"action": string(action.Kind), "order_id": orderID,
		"from": string(from), "to": string(next),
	}
	switch action.Kind {
	case ReturnActionNote:
		text, ok := cleanText(action.Text, ReturnAdminTextMaxChars)
		if !ok {
			return "", userErr("Escreva a observação.")
		}
		meta["note"] = text
		if _, err := tx.Exec(ctx, sqlReturnTouch, returnID); err != nil {
			return "", err
		}
		return next, recordReturnAudit(ctx, tx, returnID, actor, meta)
	case ReturnActionApprove:
		instructions, ok := cleanText(action.Text, ReturnAdminTextMaxChars)
		if !ok {
			return "", userErr("Informe as instruções de envio (código de postagem, endereço, prazo).")
		}
		if _, err := tx.Exec(ctx, sqlReturnApprove, returnID, instructions); err != nil {
			return "", err
		}
		if err := enqueueReturnMail(ctx, tx, returnID, ReturnMailStep{Kind: "approved", Instructions: instructions}); err != nil {
			return "", err
		}
	case ReturnActionReject:
		reason, ok := cleanText(action.Text, ReturnAdminTextMaxChars)
		if !ok {
			return "", userErr("Informe o motivo da recusa para o cliente.")
		}
		meta["reason"] = reason
		if _, err := tx.Exec(ctx, sqlReturnReject, returnID, reason); err != nil {
			return "", err
		}
		if err := enqueueReturnMail(ctx, tx, returnID, ReturnMailStep{Kind: "rejected", Reason: reason}); err != nil {
			return "", err
		}
	case ReturnActionReceive:
		if _, err := tx.Exec(ctx, sqlReturnReceive, returnID); err != nil {
			return "", err
		}
	case ReturnActionResolveSwap:
		if err := resolveReturn(ctx, tx, returnID, "exchanged", nil); err != nil {
			return "", err
		}
		if err := enqueueReturnMail(ctx, tx, returnID, ReturnMailStep{Kind: "resolved"}); err != nil {
			return "", err
		}
	case ReturnActionRefundOrder:
		var provider string
		if err := tx.QueryRow(ctx, sqlOrderProvider, orderID).Scan(&provider); err != nil {
			return "", err
		}
		if provider == ProviderMercadoPago {
			return "", userErr(`Pedido do Mercado Pago: use "Estornar no Mercado Pago".`)
		}
		whole, err := coversWholeOrder(ctx, tx, returnID)
		if err != nil {
			return "", err
		}
		if !whole {
			return "", userErr("A solicitação não cobre o pedido inteiro: registre o reembolso parcial feito via PIX.")
		}
		if _, err := ApplyWithAudit(ctx, tx, orderID, EventRefund, WithRestock(action.Restock), actor); err != nil {
			return "", err
		}
		var total int64
		if err := tx.QueryRow(ctx, sqlOrderTotal, orderID).Scan(&total); err != nil {
			return "", err
		}
		meta["restock"] = action.Restock
		meta["refund_cents"] = total
		if err := resolveReturn(ctx, tx, returnID, "refunded", &total); err != nil {
			return "", err
		}
		if err := enqueueReturnMail(ctx, tx, returnID, ReturnMailStep{Kind: "resolved", RefundCents: &total}); err != nil {
			return "", err
		}
	case ReturnActionRecordRefund:
		var status string
		var total int64
		if err := tx.QueryRow(ctx, sqlOrderStatusTotalLock, orderID).Scan(&status, &total); err != nil {
			return "", err
		}
		refunded := status == "refunded"
		if !refunded {
			whole, err := coversWholeOrder(ctx, tx, returnID)
			if err != nil {
				return "", err
			}
			if whole {
				return "", userErr("A solicitação cobre o pedido inteiro: use o estorno do pedido.")
			}
		}
		type returnedLine struct {
			VariantID      uuid.UUID `db:"variant_id"`
			Quantity       int32     `db:"quantity"`
			UnitPriceCents int64     `db:"unit_price_cents"`
		}
		lines, err := collect[returnedLine](tx.Query(ctx, sqlReturnedLines, returnID))
		if err != nil {
			return "", err
		}
		var itemsCents int64
		for _, l := range lines {
			itemsCents += int64(l.Quantity) * l.UnitPriceCents
		}
		limit := min(itemsCents, total)
		if action.RefundCents <= 0 || action.RefundCents > limit {
			return "", userErr("Valor do reembolso inválido (até o valor dos itens devolvidos).")
		}
		// An order refunded elsewhere already decided its stock.
		restocked := action.Restock && !refunded
		if restocked {
			rl := make([]ReserveLine, 0, len(lines))
			for _, l := range lines {
				rl = append(rl, ReserveLine{VariantID: l.VariantID, Quantity: l.Quantity})
			}
			if err := RestockLines(ctx, tx, orderID, rl); err != nil {
				return "", err
			}
		}
		meta["restock"] = restocked
		meta["refund_cents"] = action.RefundCents
		cents := action.RefundCents
		if err := resolveReturn(ctx, tx, returnID, "refunded", &cents); err != nil {
			return "", err
		}
		if err := enqueueReturnMail(ctx, tx, returnID, ReturnMailStep{Kind: "resolved", RefundCents: &cents}); err != nil {
			return "", err
		}
	default:
		return "", ErrReturnIllegalTransition
	}
	return next, recordReturnAudit(ctx, tx, returnID, actor, meta)
}

// CheckMPRefund validates a "Estornar no Mercado Pago" request before the
// PSP call (received, refundable kind, whole order, MP order) and returns
// the order id.
func CheckMPRefund(ctx context.Context, pool *pgxpool.Pool, returnID uuid.UUID) (uuid.UUID, error) {
	var orderID uuid.UUID
	var kind ReturnKind
	var status ReturnStatus
	err := withTx(ctx, pool, func(tx pgx.Tx) error {
		var err error
		orderID, kind, status, err = lockReturn(ctx, tx, returnID)
		return err
	})
	if err != nil {
		return uuid.Nil, err
	}
	if _, ok := AdminReturnTransition(status, kind, ReturnActionRefundOrder); !ok {
		return uuid.Nil, ErrReturnIllegalTransition
	}
	opts, err := LoadRefundOptions(ctx, pool, returnID)
	if err != nil {
		return uuid.Nil, err
	}
	if !opts.MercadoPago {
		return uuid.Nil, userErr("Este pedido não é do Mercado Pago.")
	}
	if !opts.FullOrder {
		return uuid.Nil, userErr("A solicitação não cobre o pedido inteiro: faça o reembolso parcial no painel do Mercado Pago e registre o valor.")
	}
	return orderID, nil
}

// FinishMPRefund resolves the request after a successful Mercado Pago
// refund (the payments service already moved the order to refunded),
// optionally restocking the order's units.
func FinishMPRefund(ctx context.Context, pool *pgxpool.Pool, returnID uuid.UUID, restock bool, actor Actor) error {
	return withTx(ctx, pool, func(tx pgx.Tx) error {
		orderID, kind, from, err := lockReturn(ctx, tx, returnID)
		if err != nil {
			return err
		}
		next, ok := AdminReturnTransition(from, kind, ReturnActionRefundOrder)
		if !ok {
			return ErrReturnIllegalTransition
		}
		var status string
		var total int64
		if err := tx.QueryRow(ctx, sqlOrderStatusTotalLock, orderID).Scan(&status, &total); err != nil {
			return err
		}
		if status != "refunded" {
			return userErr(`O pedido ainda não consta como estornado. Use "Sincronizar com Mercado Pago" no pedido.`)
		}
		if restock {
			if err := RestockSoldItems(ctx, tx, orderID); err != nil {
				return err
			}
		}
		if err := resolveReturn(ctx, tx, returnID, "refunded", &total); err != nil {
			return err
		}
		if err := enqueueReturnMail(ctx, tx, returnID, ReturnMailStep{Kind: "resolved", RefundCents: &total}); err != nil {
			return err
		}
		return recordReturnAudit(ctx, tx, returnID, actor, map[string]any{
			"action": "mp_refund", "order_id": orderID, "from": string(from), "to": string(next),
			"restock": restock, "refund_cents": total,
		})
	})
}

// ReturnErrorMessage is the admin/customer toast for a return error ("" for
// internal errors, which callers log and replace with a generic message).
func ReturnErrorMessage(err error) string {
	if msg, ok := UserMessage(err); ok {
		return msg
	}
	switch {
	case errors.Is(err, ErrReturnNotFound), errors.Is(err, ErrReturnAlreadyOpen),
		errors.Is(err, ErrReturnIllegalTransition), errors.Is(err, ErrReturnNotShipped),
		errors.Is(err, ErrReturnExpired), isApplyDomainError(err):
		return err.Error()
	}
	return ""
}
