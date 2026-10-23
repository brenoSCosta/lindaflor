package store

import (
	"context"
	"encoding/json"
	"errors"
	"strconv"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/audit"
	"github.com/wellington-vell/lindaflor/internal/db"
)

// Admin operations: dashboard stats, stock, orders, returns
// (src/app/admin.rs, estoque.rs, pedidos*.rs, trocas*.rs).

// --- Dashboard ---

// AdminDashboard is /admin.
type AdminDashboard struct {
	TotalProducts int64
	TotalOrders   int64
	TotalRevenue  int64 // paid + shipped + delivered
	LowStockCount int64
	RecentOrders  []AdminOrderRow
}

var (
	sqlCountProducts = sqlText(`SELECT COUNT(*) FROM products`)
	sqlCountOrders   = sqlText(`SELECT COUNT(*) FROM orders`)
	sqlTotalRevenue  = sqlText(`SELECT COALESCE(SUM(total_cents), 0)::bigint FROM orders WHERE status IN ('paid', 'shipped', 'delivered')`)
	sqlLowStock      = sqlText(`SELECT COUNT(*) FROM (
   SELECT v.id
   FROM product_variants v
   INNER JOIN products p ON p.id = v.product_id
   LEFT JOIN inventory i
     ON i.variant_id = v.id
    AND i.warehouse_id IN (SELECT id FROM warehouses WHERE active)
   WHERE p.active
   GROUP BY v.id, v.low_stock_threshold
   HAVING COALESCE(SUM(i.quantity - i.reserved), 0) <= v.low_stock_threshold
 ) low`)
	sqlRecentAdminOrders = sqlText(`SELECT o.id, o.guest_email, o.status::text AS status, o.total_cents, o.created_at,
        o.carrier, o.tracking_code, 0::bigint AS item_count
 FROM orders o ORDER BY o.created_at DESC LIMIT 10`)
)

// CountLowStockVariants counts variants of active products whose available
// stock (quantity - reserved over active warehouses; none = 0) is at or
// below their own low_stock_threshold.
func CountLowStockVariants(ctx context.Context, q DBTX) (int64, error) {
	var n int64
	err := q.QueryRow(ctx, sqlLowStock).Scan(&n)
	return n, err
}

// LoadAdminDashboard loads the admin home figures.
func LoadAdminDashboard(ctx context.Context, q DBTX) (AdminDashboard, error) {
	var d AdminDashboard
	for _, s := range []struct {
		sql string
		dst *int64
	}{{sqlCountProducts, &d.TotalProducts}, {sqlCountOrders, &d.TotalOrders}, {sqlTotalRevenue, &d.TotalRevenue}} {
		if err := q.QueryRow(ctx, s.sql).Scan(s.dst); err != nil {
			return d, err
		}
	}
	var err error
	if d.LowStockCount, err = CountLowStockVariants(ctx, q); err != nil {
		return d, err
	}
	d.RecentOrders, err = collect[AdminOrderRow](q.Query(ctx, sqlRecentAdminOrders))
	return d, err
}

// --- Stock ---

const stockQueryMax = 80

// StockRow is an /admin/estoque row.
type StockRow struct {
	ID            uuid.UUID `db:"id"`
	VariantID     uuid.UUID `db:"variant_id"`
	WarehouseID   uuid.UUID `db:"warehouse_id"`
	Quantity      int32     `db:"quantity"`
	Reserved      int32     `db:"reserved"`
	ProductName   string    `db:"product_name"`
	SKU           string    `db:"sku"`
	Size          *string   `db:"size"`
	Color         string    `db:"color"`
	WarehouseName string    `db:"warehouse_name"`
	WarehouseCode string    `db:"warehouse_code"`
}

// Available is quantity - reserved.
func (r StockRow) Available() int32 { return r.Quantity - r.Reserved }

// Warehouse is a warehouses row.
type Warehouse struct {
	ID        uuid.UUID `db:"id"`
	Code      string    `db:"code"`
	Name      string    `db:"name"`
	IsDefault bool      `db:"is_default"`
}

// StockDirectory is /admin/estoque (and its /lista fragment).
type StockDirectory struct {
	Query      string
	Rows       []StockRow
	Warehouses []Warehouse
}

var (
	sqlStockRows = sqlText(`SELECT i.id, i.variant_id, i.warehouse_id, i.quantity, i.reserved,
            p.name AS product_name, pv.sku, pv.size::text AS size, pv.color,
            w.name AS warehouse_name, w.code AS warehouse_code
     FROM inventory i
     JOIN product_variants pv ON pv.id = i.variant_id
     JOIN products p ON p.id = pv.product_id
     JOIN warehouses w ON w.id = i.warehouse_id
     WHERE ($1::text = '' OR p.name ILIKE '%' || $1::text || '%' ESCAPE '\' OR pv.sku ILIKE '%' || $1::text || '%' ESCAPE '\')
     ORDER BY p.name, pv.sku`)
	sqlWarehouses = sqlText(`SELECT id, code, name, is_default FROM warehouses ORDER BY name`)
)

// LoadStockDirectory searches inventory by product name or SKU.
func LoadStockDirectory(ctx context.Context, q DBTX, query string) (StockDirectory, error) {
	out := StockDirectory{Query: ClampAdminQuery(query, stockQueryMax)}
	var err error
	if out.Rows, err = collect[StockRow](q.Query(ctx, sqlStockRows, db.EscapeLike(out.Query))); err != nil {
		return out, err
	}
	out.Warehouses, err = collect[Warehouse](q.Query(ctx, sqlWarehouses))
	return out, err
}

// AdjustStockWithAudit validates the adjust form (inventory id, delta,
// reason ≤ 200 chars) and applies it with an audit row. Returns a
// *UserError for invalid input or ErrInsufficientStock (400 in Rust).
func AdjustStockWithAudit(ctx context.Context, pool *pgxpool.Pool, rawInventoryID, rawDelta, rawReason string, actor Actor) error {
	inventoryID, err := uuid.Parse(strings.TrimSpace(rawInventoryID))
	if err != nil {
		return userErr("inventory id inválido")
	}
	delta, err := strconv.ParseInt(strings.TrimSpace(rawDelta), 10, 32)
	if err != nil {
		return userErr("delta inválido")
	}
	reason := strings.TrimSpace(rawReason)
	if reason == "" || runeLen(reason) > 200 {
		return userErr("informe um motivo (até 200 caracteres)")
	}
	return withTx(ctx, pool, func(tx pgx.Tx) error {
		warehouseID, err := AdjustStock(ctx, tx, inventoryID, int32(delta)) //nolint:gosec // ParseInt bitSize 32 bounds it
		if err != nil {
			if errors.Is(err, ErrInsufficientStock) {
				return userErr(err.Error())
			}
			return err
		}
		return audit.Record(ctx, tx, audit.Event{
			AggregateType: "inventory", AggregateID: inventoryID.String(), EntityType: "inventory",
			EntityID: inventoryID, ActorID: actor.ID, ActorName: actor.Name,
			Metadata: map[string]any{"delta": delta, "reason": reason, "warehouse_id": warehouseID},
		})
	})
}

// --- Orders ---

// OrderStatusFilters are the /admin/pedidos filter buttons.
var OrderStatusFilters = [][2]string{
	{"all", "Todos"},
	{"pending_payment", "Pendentes"},
	{"paid", "Pagos"},
	{"processing", "Em separação"},
	{"shipped", "Enviados"},
	{"delivered", "Entregues"},
	{"cancelled", "Cancelados"},
	{"refunded", "Estornados"},
}

// ClampOrderStatus keeps known statuses, else "all".
func ClampOrderStatus(raw string) string {
	if _, ok := ParseOrderStatus(strings.TrimSpace(raw)); ok {
		return strings.TrimSpace(raw)
	}
	return "all"
}

// AdminOrderRow is an order list row.
type AdminOrderRow struct {
	ID           uuid.UUID `db:"id"`
	GuestEmail   *string   `db:"guest_email"`
	Status       string    `db:"status"`
	TotalCents   int64     `db:"total_cents"`
	CreatedAt    time.Time `db:"created_at"`
	Carrier      string    `db:"carrier"`
	TrackingCode *string   `db:"tracking_code"`
	ItemCount    int64     `db:"item_count"`
}

// StatusLabel / StatusTone for the admin badges.
func (o AdminOrderRow) StatusLabel() string { return OrderStatusLabel(o.Status) }

// StatusTone is the admin tone.
func (o AdminOrderRow) StatusTone() BadgeTone { return AdminOrderStatusTone(o.Status) }

// Code is the short reference.
func (o AdminOrderRow) Code() string { return OrderCode(o.ID) }

var (
	sqlAdminOrderCount = sqlText(`SELECT COUNT(*) FROM orders o WHERE ($1::text = '' OR o.guest_email ILIKE '%' || $1::text || '%' ESCAPE '\' OR o.tracking_code ILIKE '%' || $1::text || '%' ESCAPE '\') AND ($2::text = 'all' OR o.status::text = $2::text)`)
	sqlAdminOrderList  = sqlText(`SELECT o.id, o.guest_email, o.status::text AS status, o.total_cents, o.created_at,
        o.carrier, o.tracking_code,
        (SELECT COUNT(*) FROM order_items oi WHERE oi.order_id = o.id) AS item_count
 FROM orders o
 WHERE ($1::text = '' OR o.guest_email ILIKE '%' || $1::text || '%' ESCAPE '\'
        OR o.tracking_code ILIKE '%' || $1::text || '%' ESCAPE '\')
   AND ($2::text = 'all' OR o.status::text = $2::text)
 ORDER BY o.created_at DESC
 LIMIT $3 OFFSET $4`)
)

// AdminOrderDirectory is /admin/pedidos.
type AdminOrderDirectory struct {
	Query  string
	Status string
	Orders []AdminOrderRow
	Pager  Pagination
}

// LoadAdminOrders searches by guest email or tracking code (literal), with
// a status filter, 20 per page.
func LoadAdminOrders(ctx context.Context, q DBTX, query, status string, page int64) (AdminOrderDirectory, error) {
	out := AdminOrderDirectory{Query: ClampAdminQuery(query, 80), Status: ClampOrderStatus(status)}
	like := db.EscapeLike(out.Query)
	var total int64
	if err := q.QueryRow(ctx, sqlAdminOrderCount, like, out.Status).Scan(&total); err != nil {
		return out, err
	}
	out.Pager = Paginate(page, 20, total)
	var err error
	out.Orders, err = collect[AdminOrderRow](q.Query(ctx, sqlAdminOrderList, like, out.Status, out.Pager.PageSize, out.Pager.Offset()))
	return out, err
}

// AdminOrderDetail is /admin/pedidos/{id}.
type AdminOrderDetail struct {
	ID                   uuid.UUID
	Status               string
	GuestEmail           string
	SubtotalCents        int64
	ShippingCents        int64
	DiscountCents        int64
	TotalCents           int64
	PaymentProvider      string
	Address              *ShippingAddress
	Meta                 *PaymentMeta
	Paid                 bool
	ReservationExpiresAt string
	CreatedAt            string
	Carrier              string
	TrackingCode         *string
	ShippedAt            *string
	DeliveredAt          *string
	Items                []OrderItemView
	Returns              []ReturnSummary
}

// Code is the short reference (also the manual-override confirmation).
func (o *AdminOrderDetail) Code() string { return OrderCode(o.ID) }

// MercadoPago reports an MP order (PSP-backed money actions only).
func (o *AdminOrderDetail) MercadoPago() bool { return o.PaymentProvider == ProviderMercadoPago }

// Reopenable: cancelled while still unpaid.
func (o *AdminOrderDetail) Reopenable() bool { return o.Status == "cancelled" && !o.Paid }

// CanShip: paid or processing.
func (o *AdminOrderDetail) CanShip() bool { return o.Status == "paid" || o.Status == "processing" }

// TrackingLink is the carrier page ("" when unknown).
func (o *AdminOrderDetail) TrackingLink() string {
	if o.TrackingCode == nil {
		return ""
	}
	u, _ := TrackingURL(o.Carrier, *o.TrackingCode)
	return u
}

// ManualOverride is the one local money transition offered under "Ações
// manuais (avançado)" for an MP order: (form status, label, offers restock).
func (o *AdminOrderDetail) ManualOverride() (status, label string, restock, ok bool) {
	return ManualOverrideAction(o.Status, o.Reopenable())
}

var sqlAdminOrder = sqlText(`SELECT id, status::text AS status, guest_email, subtotal_cents, shipping_cents,
        discount_cents, total_cents, payment_provider, shipping_address, payment_meta,
        paid_at IS NOT NULL,
        to_char(reservation_expires_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS reservation_expires_at,
        to_char(created_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS created_at,
        carrier, tracking_code,
        to_char(shipped_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS shipped_at,
        to_char(delivered_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS delivered_at
 FROM orders WHERE id = $1`)

// LoadAdminOrder loads the admin order page (nil when missing). The
// Mercado Pago payment panel comes from internal/payments.
func LoadAdminOrder(ctx context.Context, q DBTX, id uuid.UUID) (*AdminOrderDetail, error) {
	var o AdminOrderDetail
	var guest, expires *string
	var addr, meta []byte
	err := q.QueryRow(ctx, sqlAdminOrder, id).Scan(&o.ID, &o.Status, &guest, &o.SubtotalCents, &o.ShippingCents,
		&o.DiscountCents, &o.TotalCents, &o.PaymentProvider, &addr, &meta, &o.Paid, &expires, &o.CreatedAt,
		&o.Carrier, &o.TrackingCode, &o.ShippedAt, &o.DeliveredAt)
	if found, err := optional(err); err != nil || !found {
		return nil, err
	}
	o.GuestEmail, o.ReservationExpiresAt = deref(guest), deref(expires)
	o.Address, o.Meta = parseAddress(addr), parseMeta(meta)
	if o.Items, err = collect[OrderItemView](q.Query(ctx, sqlOrderItems, id)); err != nil {
		return nil, err
	}
	o.Returns, err = RequestsForOrder(ctx, q, id)
	return &o, err
}

// ManualMoneyEventBlocked: MP orders refuse local Pay/ReopenPay/Refund
// (the PSP is the source of truth).
func ManualMoneyEventBlocked(mercadoPago bool, e OrderEvent) bool {
	return mercadoPago && (e == EventPay || e == EventReopenPay || e == EventRefund)
}

// MPManualBlockedMessage refuses a manual money transition on an MP order.
const MPManualBlockedMessage = `Pedido do Mercado Pago: use "Sincronizar com Mercado Pago" ou "Estornar no Mercado Pago". A baixa manual não movimenta dinheiro.`

// CheckManualOverride validates the typed order code and the reason
// (10..=500 chars) and returns the trimmed reason.
func CheckManualOverride(orderID uuid.UUID, confirm, reason string) (string, error) {
	typed := strings.TrimLeft(strings.TrimSpace(confirm), "#")
	if !strings.EqualFold(typed, OrderCode(orderID)) {
		return "", userErr("Confirmação incorreta: digite o código do pedido exatamente como aparece.")
	}
	r := strings.TrimSpace(reason)
	switch n := runeLen(r); {
	case n < 10:
		return "", userErr("Informe o motivo da ação manual (mínimo de 10 caracteres).")
	case n > 500:
		return "", userErr("O motivo deve ter no máximo 500 caracteres.")
	}
	return r, nil
}

// ManualOverrideAction is the MP escape hatch per status.
func ManualOverrideAction(status string, reopenable bool) (string, string, bool, bool) {
	switch status {
	case "pending_payment":
		return "paid", "Marcar como pago manualmente", false, true
	case "paid", "processing", "shipped", "delivered":
		return "refunded", "Marcar como estornado (não estorna no Mercado Pago)", true, true
	case "cancelled":
		if reopenable {
			return "reopen_pay", "Recebido após expirar (manual)", false, true
		}
	}
	return "", "", false, false
}

// ApplyMPManualOverride applies a money event on an MP order with its
// usual audit row plus an mp_manual_override row (reason), atomically.
func ApplyMPManualOverride(ctx context.Context, pool *pgxpool.Pool, orderID uuid.UUID, event OrderEvent, restock bool, reason string, actor Actor) (OrderStatus, error) {
	var next OrderStatus
	err := withTx(ctx, pool, func(tx pgx.Tx) error {
		var err error
		next, err = ApplyWithAudit(ctx, tx, orderID, event, WithRestock(restock), actor)
		if err != nil {
			return err
		}
		return audit.Record(ctx, tx, audit.Event{
			AggregateType: "order", AggregateID: orderID.String(), EntityType: "order", EntityID: orderID,
			ActorID: actor.ID, ActorName: actor.Name,
			Metadata: map[string]any{
				"action": "mp_manual_override", "event": event.ActionName(), "to": string(next),
				"restock": restock, "reason": reason, "provider": ProviderMercadoPago,
			},
		})
	})
	return next, err
}

// AdminStatusInput is the order status form.
type AdminStatusInput struct {
	Status          string
	Restock         bool
	Carrier         string
	TrackingCode    string
	OverrideConfirm *string
	OverrideReason  *string
}

// AdminUpdateOrder applies an admin status form (after the UI handled the
// payments admin actions "sync"/"refund_mp" through internal/payments).
// Returns the success toast, or an error: *UserError / FSM domain errors
// are shown as-is; anything else maps to "Não foi possível atualizar o
// pedido. Tente novamente.".
func AdminUpdateOrder(ctx context.Context, pool *pgxpool.Pool, orderID uuid.UUID, in AdminStatusInput, actor Actor) (string, error) {
	event, ok := ParseOrderEvent(strings.TrimSpace(in.Status))
	if !ok {
		return "", userErr("Status inválido.")
	}
	var provider string
	err := pool.QueryRow(ctx, sqlOrderProvider, orderID).Scan(&provider)
	if _, err := optional(err); err != nil {
		return "", err
	}
	if ManualMoneyEventBlocked(provider == ProviderMercadoPago, event) {
		if in.OverrideConfirm == nil || in.OverrideReason == nil {
			return "", userErr(MPManualBlockedMessage)
		}
		reason, err := CheckManualOverride(orderID, *in.OverrideConfirm, *in.OverrideReason)
		if err != nil {
			return "", err
		}
		if _, err := ApplyMPManualOverride(ctx, pool, orderID, event, in.Restock, reason, actor); err != nil {
			return "", err
		}
		return "Ação manual registrada no pedido.", nil
	}
	opts := WithRestock(in.Restock)
	opts.Tracking = &Tracking{Carrier: in.Carrier, Code: in.TrackingCode}
	err = withTx(ctx, pool, func(tx pgx.Tx) error {
		_, err := ApplyWithAudit(ctx, tx, orderID, event, opts, actor)
		return err
	})
	if err != nil {
		return "", err
	}
	return "Pedido atualizado com sucesso.", nil
}

// AdminOrderErrorMessage maps AdminUpdateOrder errors to toasts.
func AdminOrderErrorMessage(err error) string {
	if msg, ok := UserMessage(err); ok {
		return msg
	}
	if isApplyDomainError(err) {
		return err.Error()
	}
	return "Não foi possível atualizar o pedido. Tente novamente."
}

// --- Returns ---

// ReturnFilters are the /admin/trocas filter buttons.
var ReturnFilters = [][2]string{
	{"abertas", "Em aberto"},
	{"requested", "Em análise"},
	{"approved", "Aprovadas"},
	{"received", "Recebidas"},
	{"resolved", "Concluídas"},
	{"rejected", "Recusadas"},
	{"todas", "Todas"},
}

// ClampReturnFilter keeps a known filter, else "abertas".
func ClampReturnFilter(raw string) string {
	r := strings.TrimSpace(raw)
	for _, f := range ReturnFilters {
		if f[0] == r {
			return r
		}
	}
	return "abertas"
}

// AdminReturnRow is a /admin/trocas row.
type AdminReturnRow struct {
	ReturnSummary
	Email *string
	Units int64
}

var sqlAdminReturns = sqlText(`SELECT r.id, r.order_id, r.kind, r.status, r.resolution, r.created_at,
        r.refund_cents, r.instructions, r.rejection_reason,
        COALESCE(NULLIF(btrim(o.guest_email), ''), u.email) AS email,
        COALESCE((SELECT SUM(ri.quantity) FROM return_request_items ri
                  WHERE ri.return_request_id = r.id), 0)::bigint AS units
 FROM return_requests r
 JOIN orders o ON o.id = r.order_id
 LEFT JOIN users u ON u.id = o.user_id
 WHERE CASE $1::text
         WHEN 'todas' THEN true
         WHEN 'abertas' THEN r.status IN ('requested', 'approved', 'received')
         ELSE r.status = $1::text
       END
 ORDER BY r.created_at DESC
 LIMIT $2`)

// ListAdminReturns lists requests for a filter (200 max).
func ListAdminReturns(ctx context.Context, q DBTX, filter string) ([]AdminReturnRow, error) {
	type row struct {
		ReturnSummary
		Email *string `db:"email"`
		Units int64   `db:"units"`
	}
	rows, err := collect[row](q.Query(ctx, sqlAdminReturns, ClampReturnFilter(filter), 200))
	if err != nil {
		return nil, err
	}
	out := make([]AdminReturnRow, 0, len(rows))
	for _, r := range rows {
		out = append(out, AdminReturnRow(r))
	}
	return out, nil
}

// ReturnItemView is a returned line.
type ReturnItemView struct {
	ProductName    string `db:"product_name"`
	VariantLabel   string `db:"variant_label"`
	Quantity       int32  `db:"quantity"`
	Ordered        int32  `db:"ordered"`
	UnitPriceCents int64  `db:"unit_price_cents"`
}

// ReturnAuditEntry is a timeline entry.
type ReturnAuditEntry struct {
	OccurredAt time.Time
	ActorName  *string
	Action     string
	Label      string
	Detail     string
}

// AdminReturnDetail is /admin/trocas/{id}.
type AdminReturnDetail struct {
	Summary         ReturnSummary
	Reason          string
	DesiredSize     *string
	Email           *string
	OrderStatus     string
	OrderTotalCents int64
	Items           []ReturnItemView
	Audit           []ReturnAuditEntry
	// Refund is set for received refundable requests.
	Refund *RefundOptions
}

// Kind / Status helpers for the action forms.
func (d AdminReturnDetail) kind() (ReturnKind, bool) { return ParseReturnKind(d.Summary.Kind) }

func (d AdminReturnDetail) status() ReturnStatus {
	s, _ := ParseReturnStatus(d.Summary.Status)
	return s
}

// CanApprove / CanReceive / CanReject / CanExchange / CanRefund drive the
// buttons.
func (d AdminReturnDetail) CanApprove() bool { return d.status() == ReturnRequested }

// CanReceive reports approved.
func (d AdminReturnDetail) CanReceive() bool { return d.status() == ReturnApproved }

// CanReject reports an open request.
func (d AdminReturnDetail) CanReject() bool { return d.status().IsOpen() }

// CanExchange reports received + exchangeable kind.
func (d AdminReturnDetail) CanExchange() bool {
	k, ok := d.kind()
	return d.status() == ReturnReceived && ok && k.CanExchange()
}

// CanRefund reports received + refundable kind.
func (d AdminReturnDetail) CanRefund() bool {
	k, ok := d.kind()
	return d.status() == ReturnReceived && ok && k.CanRefund()
}

// FullRefund: the whole-order refund path is offered.
func (d AdminReturnDetail) FullRefund() bool { return d.Refund != nil && d.Refund.FullOrder }

// PartialRefund: the manual partial refund path is offered.
func (d AdminReturnDetail) PartialRefund() bool {
	return d.Refund != nil && (!d.Refund.FullOrder || d.Refund.OrderRefunded)
}

// DefaultReturnInstructions pre-fills the approve form.
const DefaultReturnInstructions = "Poste a peça nos Correios em até 7 dias usando o código de postagem: \nEnvie na embalagem original, com etiquetas, e inclua o número do pedido."

var (
	sqlAdminReturn = sqlText(`SELECT r.id, r.order_id, r.kind, r.status, r.resolution, r.created_at,
        r.refund_cents, r.instructions, r.rejection_reason, r.reason,
        r.desired_size,
        COALESCE(NULLIF(btrim(o.guest_email), ''), u.email) AS email,
        o.status::text AS order_status, o.total_cents AS order_total_cents
 FROM return_requests r
 JOIN orders o ON o.id = r.order_id
 LEFT JOIN users u ON u.id = o.user_id
 WHERE r.id = $1`)
	sqlAdminReturnItems = sqlText(`SELECT oi.product_name, oi.variant_label, ri.quantity,
        oi.quantity AS ordered, oi.unit_price_cents
 FROM return_request_items ri
 JOIN order_items oi ON oi.id = ri.order_item_id
 WHERE ri.return_request_id = $1
 ORDER BY oi.created_at, oi.id`)
	sqlAdminReturnAudit = sqlText(`SELECT occurred_at, actor_name, metadata FROM audit_events
 WHERE aggregate_type = 'return' AND aggregate_id = $1
 ORDER BY occurred_at, id`)
)

// ReturnActionLabel labels an audit action.
func ReturnActionLabel(action string) string {
	switch action {
	case "create":
		return "Solicitação aberta"
	case "approve":
		return "Aprovada"
	case "reject":
		return "Recusada"
	case "receive":
		return "Peça recebida"
	case "resolve_exchanged":
		return "Troca concluída"
	case "refund_order":
		return "Pedido estornado"
	case "record_refund":
		return "Reembolso registrado"
	case "mp_refund":
		return "Estornado no Mercado Pago"
	case "note":
		return "Observação"
	}
	return "Evento"
}

func returnAuditDetail(meta map[string]any) string {
	var parts []string
	if s, ok := meta["note"].(string); ok {
		parts = append(parts, s)
	} else if s, ok := meta["reason"].(string); ok {
		parts = append(parts, s)
	}
	if c, ok := meta["refund_cents"].(float64); ok {
		parts = append(parts, FormatPrice(int64(c)))
	}
	if b, ok := meta["restock"].(bool); ok && b {
		parts = append(parts, "devolvido ao estoque")
	}
	return strings.Join(parts, " · ")
}

// LoadAdminReturn loads the request page (nil when missing).
func LoadAdminReturn(ctx context.Context, q DBTX, returnID uuid.UUID) (*AdminReturnDetail, error) {
	var d AdminReturnDetail
	s := &d.Summary
	err := q.QueryRow(ctx, sqlAdminReturn, returnID).Scan(&s.ID, &s.OrderID, &s.Kind, &s.Status, &s.Resolution, &s.CreatedAt,
		&s.RefundCents, &s.Instructions, &s.RejectionReason, &d.Reason, &d.DesiredSize, &d.Email, &d.OrderStatus, &d.OrderTotalCents)
	if found, err := optional(err); err != nil || !found {
		return nil, err
	}
	if d.Items, err = collect[ReturnItemView](q.Query(ctx, sqlAdminReturnItems, returnID)); err != nil {
		return nil, err
	}
	rows, err := q.Query(ctx, sqlAdminReturnAudit, returnID.String())
	if err != nil {
		return nil, err
	}
	var at time.Time
	var actor *string
	var raw []byte
	_, err = pgx.ForEachRow(rows, []any{&at, &actor, &raw}, func() error {
		meta := map[string]any{}
		_ = json.Unmarshal(raw, &meta)
		action, _ := meta["action"].(string)
		d.Audit = append(d.Audit, ReturnAuditEntry{
			OccurredAt: at, ActorName: actor, Action: action,
			Label: ReturnActionLabel(action), Detail: returnAuditDetail(meta),
		})
		return nil
	})
	if err != nil {
		return nil, err
	}
	if d.CanRefund() {
		opts, err := LoadRefundOptions(ctx, q, returnID)
		if err != nil {
			return nil, err
		}
		d.Refund = &opts
	}
	return &d, nil
}

// ReturnFormAction is the parsed admin return form: either a local action
// or the Mercado Pago refund (handled with internal/payments, then
// FinishMPRefund).
type ReturnFormAction struct {
	Local     *AdminReturnAction
	MPRefund  bool
	MPRestock bool
}

// ParseReturnForm parses action/text/amount/restock.
func ParseReturnForm(action, text, amount string, restock bool) (ReturnFormAction, error) {
	a := AdminReturnAction{Text: text, Restock: restock}
	switch strings.TrimSpace(action) {
	case "approve":
		a.Kind = ReturnActionApprove
	case "reject":
		a.Kind = ReturnActionReject
	case "receive":
		a.Kind = ReturnActionReceive
	case "exchange":
		a.Kind = ReturnActionResolveSwap
	case "refund_order":
		a.Kind = ReturnActionRefundOrder
	case "record_refund":
		cents, err := ParseProductPrice(amount)
		if err != nil {
			return ReturnFormAction{}, err
		}
		a.Kind, a.RefundCents = ReturnActionRecordRefund, cents
	case "note":
		a.Kind = ReturnActionNote
	case "mp_refund":
		return ReturnFormAction{MPRefund: true, MPRestock: restock}, nil
	default:
		return ReturnFormAction{}, userErr("Ação inválida.")
	}
	return ReturnFormAction{Local: &a}, nil
}

// ReturnAdminErrorMessage is the admin toast of a return error.
func ReturnAdminErrorMessage(err error) string {
	if m := ReturnErrorMessage(err); m != "" {
		return m
	}
	return "Não foi possível atualizar a solicitação. Tente novamente."
}

// ParseCustomerReturnForm parses POST /pedido/{id}/troca pairs: t, kind,
// reason, desired_size and qty_<order_item_id>. A nil request means "Escolha
// o tipo de solicitação.".
func ParseCustomerReturnForm(pairs []QueryPair) (token *string, req *NewReturn) {
	var kind *ReturnKind
	var reason string
	var desired *string
	var lines []ReturnLine
	for _, p := range pairs {
		switch p.Key {
		case "t":
			if t := strings.TrimSpace(p.Value); t != "" {
				token = &t
			}
		case "kind":
			if k, ok := ParseReturnKind(strings.TrimSpace(p.Value)); ok {
				kind = &k
			} else {
				kind = nil
			}
		case "reason":
			reason = p.Value
		case "desired_size":
			v := p.Value
			desired = &v
		default:
			raw, ok := strings.CutPrefix(p.Key, "qty_")
			if !ok {
				continue
			}
			id, err := uuid.Parse(raw)
			if err != nil {
				continue
			}
			n, err := strconv.ParseInt(strings.TrimSpace(p.Value), 10, 32)
			if err != nil {
				n = -1
			}
			lines = append(lines, ReturnLine{OrderItemID: id, Quantity: int32(n)})
		}
	}
	if kind == nil {
		return token, nil
	}
	return token, &NewReturn{Kind: *kind, Reason: reason, DesiredSize: desired, Lines: lines}
}
