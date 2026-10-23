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
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
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

// CountLowStockVariants counts variants of active products whose available
// stock (quantity - reserved over active warehouses; none = 0) is at or
// below their own low_stock_threshold.
func CountLowStockVariants(ctx context.Context, q DBTX) (int64, error) {
	return queries(q).StoreAdminLowStockCount(ctx)
}

// LoadAdminDashboard loads the admin home figures.
func LoadAdminDashboard(ctx context.Context, q DBTX) (AdminDashboard, error) {
	var d AdminDashboard
	qs := queries(q)
	var err error
	if d.TotalProducts, err = qs.StoreAdminCountProducts(ctx); err != nil {
		return d, err
	}
	if d.TotalOrders, err = qs.StoreAdminCountOrders(ctx); err != nil {
		return d, err
	}
	if d.TotalRevenue, err = qs.StoreAdminTotalRevenue(ctx); err != nil {
		return d, err
	}
	if d.LowStockCount, err = CountLowStockVariants(ctx, q); err != nil {
		return d, err
	}
	rows, err := qs.StoreAdminRecentOrders(ctx)
	if err != nil {
		return d, err
	}
	d.RecentOrders = mapSlice(rows, func(r sqlc.StoreAdminRecentOrdersRow) AdminOrderRow { return AdminOrderRow(r) })
	return d, nil
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

// LoadStockDirectory searches inventory by product name or SKU.
func LoadStockDirectory(ctx context.Context, q DBTX, query string) (StockDirectory, error) {
	out := StockDirectory{Query: ClampAdminQuery(query, stockQueryMax)}
	qs := queries(q)
	rows, err := qs.StoreAdminStockRows(ctx, db.EscapeLike(out.Query))
	if err != nil {
		return out, err
	}
	out.Rows = mapSlice(rows, func(r sqlc.StoreAdminStockRowsRow) StockRow {
		return StockRow{
			ID: r.ID, VariantID: r.VariantID, WarehouseID: r.WarehouseID, Quantity: r.Quantity, Reserved: r.Reserved,
			ProductName: r.ProductName, SKU: r.Sku, Size: &r.Size, Color: r.Color,
			WarehouseName: r.WarehouseName, WarehouseCode: r.WarehouseCode,
		}
	})
	warehouses, err := qs.StoreAdminWarehouses(ctx)
	if err != nil {
		return out, err
	}
	out.Warehouses = mapSlice(warehouses, func(r sqlc.StoreAdminWarehousesRow) Warehouse { return Warehouse(r) })
	return out, nil
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
	qs := queries(q)
	total, err := qs.StoreAdminOrderCount(ctx, sqlc.StoreAdminOrderCountParams{Query: like, Status: out.Status})
	if err != nil {
		return out, err
	}
	out.Pager = Paginate(page, 20, total)
	rows, err := qs.StoreAdminOrderList(ctx, sqlc.StoreAdminOrderListParams{
		Query: like, Status: out.Status, PageSize: out.Pager.PageSize, PageOffset: out.Pager.Offset(),
	})
	if err != nil {
		return out, err
	}
	out.Orders = mapSlice(rows, func(r sqlc.StoreAdminOrderListRow) AdminOrderRow { return AdminOrderRow(r) })
	return out, nil
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

// LoadAdminOrder loads the admin order page (nil when missing). The
// Mercado Pago payment panel comes from internal/payments.
func LoadAdminOrder(ctx context.Context, q DBTX, id uuid.UUID) (*AdminOrderDetail, error) {
	r, found, err := noRows(queries(q).StoreAdminOrder(ctx, id))
	if err != nil || !found {
		return nil, err
	}
	o := AdminOrderDetail{
		ID: r.ID, Status: r.Status, GuestEmail: deref(r.GuestEmail), SubtotalCents: r.SubtotalCents,
		ShippingCents: r.ShippingCents, DiscountCents: r.DiscountCents, TotalCents: r.TotalCents,
		PaymentProvider: r.PaymentProvider, Address: parseAddress(r.ShippingAddress), Meta: parseMeta(r.PaymentMeta),
		Paid: r.Paid, ReservationExpiresAt: deref(r.ReservationExpiresAt), CreatedAt: r.CreatedAt,
		Carrier: r.Carrier, TrackingCode: r.TrackingCode, ShippedAt: r.ShippedAt, DeliveredAt: r.DeliveredAt,
	}
	if o.Items, err = orderItems(ctx, q, id); err != nil {
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
	provider, _, err := noRows(queries(pool).StoreOrderProvider(ctx, orderID))
	if err != nil {
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

// ListAdminReturns lists requests for a filter (200 max).
func ListAdminReturns(ctx context.Context, q DBTX, filter string) ([]AdminReturnRow, error) {
	rows, err := queries(q).StoreAdminReturns(ctx, sqlc.StoreAdminReturnsParams{Filter: ClampReturnFilter(filter), MaxRows: 200})
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, func(r sqlc.StoreAdminReturnsRow) AdminReturnRow {
		return AdminReturnRow{
			ReturnSummary: ReturnSummary{
				ID: r.ID, OrderID: r.OrderID, Kind: r.Kind, Status: r.Status, Resolution: r.Resolution,
				CreatedAt: r.CreatedAt, RefundCents: r.RefundCents, Instructions: r.Instructions,
				RejectionReason: r.RejectionReason,
			},
			Email: r.Email,
			Units: r.Units,
		}
	}), nil
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
	qs := queries(q)
	r, found, err := noRows(qs.StoreAdminReturn(ctx, returnID))
	if err != nil || !found {
		return nil, err
	}
	d := AdminReturnDetail{
		Summary: ReturnSummary{
			ID: r.ID, OrderID: r.OrderID, Kind: r.Kind, Status: r.Status, Resolution: r.Resolution,
			CreatedAt: r.CreatedAt, RefundCents: r.RefundCents, Instructions: r.Instructions,
			RejectionReason: r.RejectionReason,
		},
		Reason: r.Reason, DesiredSize: r.DesiredSize, Email: r.Email,
		OrderStatus: r.OrderStatus, OrderTotalCents: r.OrderTotalCents,
	}
	items, err := qs.StoreAdminReturnItems(ctx, returnID)
	if err != nil {
		return nil, err
	}
	d.Items = mapSlice(items, func(r sqlc.StoreAdminReturnItemsRow) ReturnItemView { return ReturnItemView(r) })
	audits, err := qs.StoreAdminReturnAudit(ctx, returnID.String())
	if err != nil {
		return nil, err
	}
	for _, a := range audits {
		meta := map[string]any{}
		_ = json.Unmarshal(a.Metadata, &meta)
		action, _ := meta["action"].(string)
		d.Audit = append(d.Audit, ReturnAuditEntry{
			OccurredAt: a.OccurredAt, ActorName: a.ActorName, Action: action,
			Label: ReturnActionLabel(action), Detail: returnAuditDetail(meta),
		})
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
