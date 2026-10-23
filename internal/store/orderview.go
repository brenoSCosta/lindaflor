package store

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// Customer order pages: /pedido/{id} (guest token or owner), status
// refresh, customer cancel, /conta/pedidos, dashboard recent orders, and
// the order timeline (src/app/pedido/id.rs, src/app/store/order_timeline.rs,
// src/app/conta/pedidos.rs, src/app/dashboard.rs).

// TokenOrOwnerAllows: a valid, unexpired guest token (?t=) or the signed-in
// owner may see the order. Expired tokens behave like wrong ones.
func TokenOrOwnerAllows(stored uuid.UUID, tokenExpired bool, orderUserID *uuid.UUID, queryToken *string, sessionUserID *uuid.UUID) bool {
	if !tokenExpired && queryToken != nil {
		if parsed, err := uuid.Parse(strings.TrimSpace(*queryToken)); err == nil && parsed == stored {
			return true
		}
	}
	return orderUserID != nil && sessionUserID != nil && *orderUserID == *sessionUserID
}

// TimelineStepLabels are the happy-path steps.
var TimelineStepLabels = [4]string{"Pedido recebido", "Pagamento confirmado", "Enviado", "Entregue"}

// ReachedStep is the index of the last reached step; ok=false for
// cancelled/refunded orders.
func ReachedStep(status string) (int, bool) {
	switch status {
	case "pending_payment":
		return 0, true
	case "paid", "processing":
		return 1, true
	case "shipped":
		return 2, true
	case "delivered":
		return 3, true
	}
	return 0, false
}

// OrderProgress is the timeline data (dates preformatted DD/MM/YYYY HH:MM).
type OrderProgress struct {
	Status       string  `db:"status"`
	CreatedAt    *string `db:"created_at_fmt"`
	PaidAt       *string `db:"paid_at_fmt"`
	ShippedAt    *string `db:"shipped_at_fmt"`
	DeliveredAt  *string `db:"delivered_at_fmt"`
	Carrier      string  `db:"carrier"`
	TrackingCode *string `db:"tracking_code"`
}

// TimelineStep is one rendered step.
type TimelineStep struct {
	Label string
	Done  bool
	Date  string // "" when not reached / unknown
}

// Steps is the vertical step list (nil for cancelled/refunded orders).
func (p OrderProgress) Steps() []TimelineStep {
	reached, ok := ReachedStep(p.Status)
	if !ok {
		return nil
	}
	dates := [4]*string{p.CreatedAt, p.PaidAt, p.ShippedAt, p.DeliveredAt}
	steps := make([]TimelineStep, 4)
	for i, label := range TimelineStepLabels {
		done := i <= reached
		steps[i] = TimelineStep{Label: label, Done: done}
		if done && dates[i] != nil {
			steps[i].Date = *dates[i]
		}
	}
	return steps
}

// TrackingLink is the carrier tracking URL when known.
func (p OrderProgress) TrackingLink() string {
	if p.TrackingCode == nil {
		return ""
	}
	u, _ := TrackingURL(p.Carrier, *p.TrackingCode)
	return u
}

// ProgressColumns matches OrderProgress for an orders row aliased o.
const ProgressColumns = `o.status::text AS status,
  to_char(o.created_at, 'DD/MM/YYYY HH24:MI') AS created_at_fmt,
  to_char(o.paid_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS paid_at_fmt,
  to_char(o.shipped_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS shipped_at_fmt,
  to_char(o.delivered_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS delivered_at_fmt,
  o.carrier, o.tracking_code`

// OrderItemView is an order line.
type OrderItemView struct {
	ProductName    string `db:"product_name"`
	VariantLabel   string `db:"variant_label"`
	Quantity       int32  `db:"quantity"`
	UnitPriceCents int64  `db:"unit_price_cents"`
}

// LineTotalCents is unit × quantity.
func (i OrderItemView) LineTotalCents() int64 { return i.UnitPriceCents * int64(i.Quantity) }

// ShippingAddress is orders.shipping_address.
type ShippingAddress struct {
	Name         string `json:"name"`
	Street       string `json:"street"`
	Number       string `json:"number"`
	Complement   string `json:"complement"`
	Neighborhood string `json:"neighborhood"`
	City         string `json:"city"`
	State        string `json:"state"`
	ZipCode      string `json:"zip_code"`
	Phone        string `json:"phone"`
}

// Display is "street, number — neighborhood, city/UF — CEP zip" ("" when
// any part is missing).
func (a *ShippingAddress) Display() string {
	if a == nil || a.Street == "" && a.Number == "" {
		return ""
	}
	return fmt.Sprintf("%s, %s — %s, %s/%s — CEP %s", a.Street, a.Number, a.Neighborhood, a.City, a.State, a.ZipCode)
}

// PaymentMeta is orders.payment_meta.
type PaymentMeta struct {
	PixCopyPaste  *string    `json:"pix_copy_paste"`
	TotalCents    *int64     `json:"total_cents"`
	CouponCode    *string    `json:"coupon_code"`
	CouponID      *uuid.UUID `json:"coupon_id"`
	DiscountCents *int64     `json:"discount_cents"`
}

// Coupon is the applied coupon code, "" when none.
func (m *PaymentMeta) Coupon() string {
	if m == nil || m.CouponCode == nil {
		return ""
	}
	return *m.CouponCode
}

// Pix is the static BR Code, "" when none.
func (m *PaymentMeta) Pix() string {
	if m == nil || m.PixCopyPaste == nil {
		return ""
	}
	return *m.PixCopyPaste
}

func parseAddress(raw []byte) *ShippingAddress {
	if len(raw) == 0 {
		return nil
	}
	var a ShippingAddress
	if json.Unmarshal(raw, &a) != nil {
		return nil
	}
	return &a
}

func parseMeta(raw []byte) *PaymentMeta {
	if len(raw) == 0 {
		return nil
	}
	var m PaymentMeta
	if json.Unmarshal(raw, &m) != nil {
		return nil
	}
	return &m
}

// OrderDetail is the customer order page.
type OrderDetail struct {
	ID               uuid.UUID
	Status           string
	UserID           *uuid.UUID
	AccessToken      uuid.UUID
	TokenExpired     bool
	GuestEmail       string
	SubtotalCents    int64
	ShippingCents    int64
	DiscountCents    int64
	PixDiscountCents int64
	TotalCents       int64
	PaymentProvider  string
	Address          *ShippingAddress
	Meta             *PaymentMeta
	CreatedAt        string
	Progress         OrderProgress
	Items            []OrderItemView
}

// Code is the short order reference.
func (o *OrderDetail) Code() string { return OrderCode(o.ID) }

// StatusLabel is the heading label ("Aguardando pagamento" when pending).
func (o *OrderDetail) StatusLabel() string { return OrderStatusLabel(o.Status) }

// orderItems lists an order's lines.
func orderItems(ctx context.Context, q DBTX, orderID uuid.UUID) ([]OrderItemView, error) {
	rows, err := queries(q).StoreOrderItems(ctx, orderID)
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, func(r sqlc.StoreOrderItemsRow) OrderItemView { return OrderItemView(r) }), nil
}

// GetOrder loads an order regardless of access (nil when missing).
func GetOrder(ctx context.Context, q DBTX, id uuid.UUID) (*OrderDetail, error) {
	qs := queries(q)
	r, found, err := noRows(qs.StoreOrderDetail(ctx, id))
	if err != nil || !found {
		return nil, err
	}
	o := OrderDetail{
		ID: r.ID, Status: r.Status, UserID: r.UserID, AccessToken: r.AccessToken, TokenExpired: r.TokenExpired,
		GuestEmail: deref(r.GuestEmail), SubtotalCents: r.SubtotalCents, ShippingCents: r.ShippingCents,
		DiscountCents: r.DiscountCents, PixDiscountCents: r.PixDiscountCents, TotalCents: r.TotalCents,
		PaymentProvider: r.PaymentProvider, Address: parseAddress(r.ShippingAddress), Meta: parseMeta(r.PaymentMeta),
		CreatedAt: r.CreatedAt,
	}
	if o.Items, err = orderItems(ctx, q, id); err != nil {
		return nil, err
	}
	p, found, err := noRows(qs.StoreOrderProgress(ctx, id))
	if err != nil {
		return nil, err
	}
	if found {
		o.Progress = OrderProgress(p)
	}
	return &o, nil
}

// GetOrderForViewer is GetOrder filtered by TokenOrOwnerAllows (nil =
// "Pedido não encontrado").
func GetOrderForViewer(ctx context.Context, q DBTX, id uuid.UUID, queryToken *string, sessionUserID *uuid.UUID) (*OrderDetail, error) {
	o, err := GetOrder(ctx, q, id)
	if err != nil || o == nil {
		return nil, err
	}
	if !TokenOrOwnerAllows(o.AccessToken, o.TokenExpired, o.UserID, queryToken, sessionUserID) {
		return nil, nil
	}
	return o, nil
}

// OrderWhatsappURL is the "confirm payment on WhatsApp" link of the order
// page ("" without a store number).
func OrderWhatsappURL(s StoreSettings, orderID uuid.UUID, totalCents int64) string {
	number := s.WhatsappNumberTrimmed()
	if number == "" {
		return ""
	}
	short := orderID.String()[:8]
	total := FormatPrice(totalCents)
	fallback := fmt.Sprintf("Olá! Fiz o pedido %s no valor de %s e quero confirmar o pagamento via PIX.", short, total)
	msg := RenderWhatsappTemplate(s.WhatsappMessageTemplate, [][2]string{{"order_id", short}, {"total", total}}, fallback)
	return WhatsappLink(number, msg)
}

// orderAccessRow is an order's status and access columns.
type orderAccessRow = sqlc.StoreStatusAccessRow

// statusAccess reads (or, with lock, locks) an order's access columns.
func statusAccess(ctx context.Context, q DBTX, orderID uuid.UUID, lock bool) (orderAccessRow, bool, error) {
	if lock {
		r, found, err := noRows(queries(q).StoreStatusAccessLock(ctx, orderID))
		return orderAccessRow(r), found, err
	}
	return noRows(queries(q).StoreStatusAccess(ctx, orderID))
}

// RefreshOrderStatus is the "Atualizar status" button: the current status,
// or "unknown" for an unknown order / denied viewer.
func RefreshOrderStatus(ctx context.Context, q DBTX, rawOrderID, accessToken string, sessionUserID *uuid.UUID) (string, error) {
	id, err := uuid.Parse(strings.TrimSpace(rawOrderID))
	if err != nil {
		return "unknown", nil
	}
	a, found, err := statusAccess(ctx, q, id, false)
	if err != nil {
		return "", err
	} else if !found {
		return "unknown", nil
	}
	var t *string
	if v := strings.TrimSpace(accessToken); v != "" {
		t = &v
	}
	if !TokenOrOwnerAllows(a.AccessToken, a.TokenExpired, a.UserID, t, sessionUserID) {
		return "unknown", nil
	}
	return a.Status, nil
}

// CustomerCancelOutcome is the result of CancelUnpaidOrder.
type CustomerCancelOutcome int

// Outcomes.
const (
	CustomerCancelled CustomerCancelOutcome = iota
	// CustomerCancelNotFound: unknown order or the caller may not see it.
	CustomerCancelNotFound
	// CustomerCancelNotPending: only the store can cancel a paid order.
	CustomerCancelNotPending
)

// Message is the pt-BR toast (ok=false for the error toasts).
func (o CustomerCancelOutcome) Message() (string, bool) {
	switch o {
	case CustomerCancelled:
		return "Pedido cancelado.", true
	case CustomerCancelNotFound:
		return "Pedido não encontrado.", false
	}
	return "Este pedido já foi pago. Fale conosco no WhatsApp para cancelar.", false
}

// CancelUnpaidOrder is the customer cancel (POST /pedido/{id}/cancelar):
// access and status are checked under the same row lock as the transition.
func CancelUnpaidOrder(ctx context.Context, pool *pgxpool.Pool, orderID uuid.UUID, queryToken *string, sessionUserID *uuid.UUID) (CustomerCancelOutcome, error) {
	out := CustomerCancelNotFound
	err := withTx(ctx, pool, func(tx pgx.Tx) error {
		a, found, err := statusAccess(ctx, tx, orderID, true)
		if err != nil || !found {
			return err
		}
		if !TokenOrOwnerAllows(a.AccessToken, a.TokenExpired, a.UserID, queryToken, sessionUserID) {
			return nil
		}
		if a.Status != string(StatusPendingPayment) {
			out = CustomerCancelNotPending
			return nil
		}
		if _, err := ApplyWithAudit(ctx, tx, orderID, EventCancel, ApplyOptions{}, Actor{ID: sessionUserID, Name: ptr("cliente")}); err != nil {
			return err
		}
		out = CustomerCancelled
		return nil
	})
	return out, err
}

// OrderAccess checks guest/owner access to an order (troca page).
func OrderAccess(ctx context.Context, q DBTX, orderID uuid.UUID, queryToken *string, sessionUserID *uuid.UUID) (bool, error) {
	a, found, err := statusAccess(ctx, q, orderID, false)
	if err != nil || !found {
		return false, err
	}
	return TokenOrOwnerAllows(a.AccessToken, a.TokenExpired, a.UserID, queryToken, sessionUserID), nil
}

// RecentOrder is a dashboard / order history row.
type RecentOrder struct {
	ID           uuid.UUID `db:"id"`
	GuestEmail   *string   `db:"guest_email"`
	Status       string    `db:"status"`
	TotalCents   int64     `db:"total_cents"`
	CreatedAt    time.Time `db:"created_at"`
	Carrier      string    `db:"carrier"`
	TrackingCode *string   `db:"tracking_code"`
}

// StatusLabel is the pt-BR status.
func (o RecentOrder) StatusLabel() string { return OrderStatusLabel(o.Status) }

// StatusTone is the customer badge tone.
func (o RecentOrder) StatusTone() BadgeTone { return CustomerOrderStatusTone(o.Status) }

// TrackingLink is the carrier page ("" when unknown).
func (o RecentOrder) TrackingLink() string {
	if o.TrackingCode == nil {
		return ""
	}
	u, _ := TrackingURL(o.Carrier, *o.TrackingCode)
	return u
}

// LoadRecentOrders is the dashboard's last 5 orders.
func LoadRecentOrders(ctx context.Context, q DBTX, userID uuid.UUID) ([]RecentOrder, error) {
	rows, err := queries(q).StoreRecentOrders(ctx, userID)
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, func(r sqlc.StoreRecentOrdersRow) RecentOrder { return RecentOrder(r) }), nil
}

// Pagination is a computed page window.
type Pagination struct {
	Page       int64
	PageSize   int64
	Total      int64
	TotalPages int64
}

// HasPrev / HasNext / Show / Label mirror the Rust pagers.
func (p Pagination) HasPrev() bool { return p.Page > 1 }

// HasNext reports a next page.
func (p Pagination) HasNext() bool { return p.TotalPages > 0 && p.Page < p.TotalPages }

// Show reports whether a pager is needed.
func (p Pagination) Show() bool { return p.TotalPages > 1 }

// Label is "Página X de Y".
func (p Pagination) Label() string {
	if p.TotalPages == 0 {
		return "Página 1 de 1"
	}
	return fmt.Sprintf("Página %d de %d", p.Page, p.TotalPages)
}

// Offset is the SQL offset.
func (p Pagination) Offset() int64 { return (p.Page - 1) * p.PageSize }

// ClampPage clamps a page number to 1..=10000.
func ClampPage(page int64) int64 { return min(max(page, 1), 10_000) }

// ParsePage parses ?page= (positive integers; else 1).
func ParsePage(raw string) int64 {
	var n int64
	if _, err := fmt.Sscan(strings.TrimSpace(raw), &n); err != nil || n <= 0 {
		return 1
	}
	return n
}

// Paginate computes the window, pulling an out-of-range page back to the
// last one.
func Paginate(page, pageSize, total int64) Pagination {
	p := Pagination{Page: ClampPage(page), PageSize: pageSize, Total: total}
	if total > 0 {
		p.TotalPages = (total + pageSize - 1) / pageSize
	}
	if p.TotalPages > 0 && p.Page > p.TotalPages {
		p.Page = p.TotalPages
	}
	return p
}

// MyOrdersPage is /conta/pedidos.
type MyOrdersPage struct {
	Orders []RecentOrder
	Pager  Pagination
	// Reviewable: received orders with something left to review.
	Reviewable map[uuid.UUID]bool
}

// LoadMyOrders loads one page (20 per page) of the user's orders.
func LoadMyOrders(ctx context.Context, q DBTX, userID uuid.UUID, page int64) (MyOrdersPage, error) {
	qs := queries(q)
	total, err := qs.StoreUserOrderCount(ctx, userID)
	if err != nil {
		return MyOrdersPage{}, err
	}
	pager := Paginate(page, 20, total)
	rows, err := qs.StoreUserOrdersPage(ctx, sqlc.StoreUserOrdersPageParams{UserID: userID, PageSize: pager.PageSize, PageOffset: pager.Offset()})
	if err != nil {
		return MyOrdersPage{}, err
	}
	orders := mapSlice(rows, func(r sqlc.StoreUserOrdersPageRow) RecentOrder { return RecentOrder(r) })
	reviewable, err := ReviewableOrderIDs(ctx, q, userID)
	if err != nil {
		return MyOrdersPage{}, err
	}
	return MyOrdersPage{Orders: orders, Pager: pager, Reviewable: reviewable}, nil
}
