package store

import (
	"context"
	"fmt"
	"strconv"
	"strings"
	"time"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/web/format"
)

// Admin sales reports (/admin/relatorios) and the orders CSV export
// (src/app/store/reports.rs). Every figure covers Brasília calendar days
// [from, to]. A sale counts on COALESCE(paid_at, created_at) for paid,
// processing, shipped and delivered orders; partial refunds of return
// requests count on their resolution day and are subtracted for the net.
// Abandoned/expired orders and the CSV export key on created_at.

// Report limits.
const (
	MaxRangeDays     = 366
	DefaultRangeDays = 30
	CSVMaxOrders     = 20000
	ReportTopLimit   = 10
)

const saleStatuses = `('paid', 'processing', 'shipped', 'delivered')`

// Date is a calendar day (UTC midnight).
type Date = time.Time

func dateOf(y int, m time.Month, d int) Date { return time.Date(y, m, d, 0, 0, 0, 0, time.UTC) }

// BrasiliaToday is the current Brasília calendar day.
func BrasiliaToday() Date {
	t := time.Now().In(format.BRT)
	return dateOf(t.Year(), t.Month(), t.Day())
}

func parseDate(raw *string) (Date, bool) {
	if raw == nil {
		return Date{}, false
	}
	parts := strings.SplitN(strings.TrimSpace(*raw), "-", 3)
	if len(parts) != 3 {
		return Date{}, false
	}
	y, err1 := strconv.Atoi(parts[0])
	m, err2 := strconv.ParseUint(parts[1], 10, 8)
	d, err3 := strconv.ParseUint(parts[2], 10, 8)
	if err1 != nil || err2 != nil || err3 != nil || y < 2000 || y > 9999 || m < 1 || m > 12 {
		return Date{}, false
	}
	t := dateOf(y, time.Month(m), int(d))
	if t.Day() != int(d) || t.Month() != time.Month(m) {
		return Date{}, false
	}
	return t, true
}

// FormatDateISO is YYYY-MM-DD.
func FormatDateISO(d Date) string { return d.Format("2006-01-02") }

// FormatDateBR is DD/MM/YYYY.
func FormatDateBR(d Date) string { return d.Format("02/01/2006") }

// DateRange is an inclusive range of Brasília calendar days.
type DateRange struct{ From, To Date }

// ParseDateRangeWithToday parses de/ate (YYYY-MM-DD): missing/invalid
// values fall back to the last 30 days ending today, a reversed range is
// swapped and an over-long one cut to 366 days ending at to.
func ParseDateRangeWithToday(from, to *string, today Date) DateRange {
	toDate, ok := parseDate(to)
	if !ok {
		toDate = today
	}
	fromDate, ok := parseDate(from)
	if !ok {
		fromDate = toDate.AddDate(0, 0, -(DefaultRangeDays - 1))
	}
	if fromDate.After(toDate) {
		fromDate, toDate = toDate, fromDate
	}
	earliest := toDate.AddDate(0, 0, -(MaxRangeDays - 1))
	if fromDate.Before(earliest) {
		fromDate = earliest
	}
	return DateRange{From: fromDate, To: toDate}
}

// ParseDateRange uses Brasília today.
func ParseDateRange(from, to *string) DateRange {
	return ParseDateRangeWithToday(from, to, BrasiliaToday())
}

// Days is the inclusive length.
func (r DateRange) Days() int64 { return int64(r.To.Sub(r.From).Hours()/24) + 1 }

// Period is "DD/MM/YYYY a DD/MM/YYYY (N dias, horário de Brasília)".
func (r DateRange) Period() string {
	return fmt.Sprintf("%s a %s (%d dias, horário de Brasília)", FormatDateBR(r.From), FormatDateBR(r.To), r.Days())
}

func (r DateRange) args() []any { return []any{r.From, r.To} }

const (
	rangeStart = `(($1::date)::timestamp AT TIME ZONE 'America/Sao_Paulo')`
	rangeEnd   = `((($2::date) + 1)::timestamp AT TIME ZONE 'America/Sao_Paulo')`
)

// ReportSummary is the headline block.
type ReportSummary struct {
	RevenueCents       int64 `db:"revenue_cents"`
	Orders             int64 `db:"orders"`
	DiscountCents      int64 `db:"discount_cents"`
	PixDiscountCents   int64 `db:"pix_discount_cents"`
	ShippingCents      int64 `db:"shipping_cents"`
	PendingOrders      int64 `db:"pending_orders"`
	AbandonedOrders    int64 `db:"abandoned_orders"`
	RefundedOrders     int64 `db:"refunded_orders"`
	PartialRefundCents int64 `db:"partial_refund_cents"`
}

// AOVCents is the average order value (0 without orders).
func (s ReportSummary) AOVCents() int64 {
	if s.Orders == 0 {
		return 0
	}
	return s.RevenueCents / s.Orders
}

// NetRevenueCents is revenue minus partial refunds.
func (s ReportSummary) NetRevenueCents() int64 { return s.RevenueCents - s.PartialRefundCents }

var sqlReportSummary = sqlText(`WITH sales AS (
     SELECT COALESCE(SUM(o.total_cents), 0)::bigint AS revenue_cents,
            COUNT(*) AS orders,
            COALESCE(SUM(o.discount_cents), 0)::bigint AS discount_cents,
            COALESCE(SUM(o.pix_discount_cents), 0)::bigint AS pix_discount_cents,
            COALESCE(SUM(o.shipping_cents), 0)::bigint AS shipping_cents
     FROM orders o
     WHERE o.status IN ` + saleStatuses + `
       AND COALESCE(o.paid_at, o.created_at) >= ` + rangeStart + `
       AND COALESCE(o.paid_at, o.created_at) < ` + rangeEnd + `
 ),
 created AS (
     SELECT COUNT(*) FILTER (WHERE o.status = 'pending_payment') AS pending_orders,
            COUNT(*) FILTER (WHERE o.status = 'cancelled' AND o.paid_at IS NULL) AS abandoned_orders,
            COUNT(*) FILTER (WHERE o.status = 'refunded') AS refunded_orders
     FROM orders o
     WHERE o.created_at >= ` + rangeStart + ` AND o.created_at < ` + rangeEnd + `
 ),
 partial AS (
     SELECT COALESCE(SUM(r.refund_cents), 0)::bigint AS partial_refund_cents
     FROM return_requests r
     JOIN orders o ON o.id = r.order_id
     WHERE r.status = 'resolved' AND r.resolution = 'refunded'
       AND o.status <> 'refunded'
       AND r.resolved_at >= ` + rangeStart + ` AND r.resolved_at < ` + rangeEnd + `
 )
 SELECT sales.*, created.*, partial.* FROM sales, created, partial`)

// LoadReportSummary loads the headline figures.
func LoadReportSummary(ctx context.Context, q DBTX, r DateRange) (ReportSummary, error) {
	s, err := collectOne[ReportSummary](q.Query(ctx, sqlReportSummary, r.args()...))
	if err != nil || s == nil {
		return ReportSummary{}, err
	}
	return *s, nil
}

// DayRow is one day of revenue.
type DayRow struct {
	Day          Date  `db:"day"`
	Orders       int64 `db:"orders"`
	RevenueCents int64 `db:"revenue_cents"`
}

var sqlRevenueByDay = sqlText(`WITH days AS (
     SELECT generate_series($1::date, $2::date, interval '1 day')::date AS day
 ),
 sales AS (
     SELECT (COALESCE(o.paid_at, o.created_at) AT TIME ZONE 'America/Sao_Paulo')::date AS day,
            COUNT(*) AS orders,
            SUM(o.total_cents)::bigint AS revenue_cents
     FROM orders o
     WHERE o.status IN ` + saleStatuses + `
       AND COALESCE(o.paid_at, o.created_at) >= ` + rangeStart + `
       AND COALESCE(o.paid_at, o.created_at) < ` + rangeEnd + `
     GROUP BY 1
 )
 SELECT d.day, COALESCE(s.orders, 0) AS orders,
        COALESCE(s.revenue_cents, 0)::bigint AS revenue_cents
 FROM days d LEFT JOIN sales s ON s.day = d.day
 ORDER BY d.day`)

// RevenueByDay is one row per day of the range (zero-filled).
func RevenueByDay(ctx context.Context, q DBTX, r DateRange) ([]DayRow, error) {
	return collect[DayRow](q.Query(ctx, sqlRevenueByDay, r.args()...))
}

// BarPercent scales a bar to the best day (0..100).
func BarPercent(value, maxValue int64) int64 {
	if maxValue <= 0 {
		return 0
	}
	return min(max(value*100/maxValue, 0), 100)
}

// TopRow is one best-seller line.
type TopRow struct {
	Name         string `db:"name"`
	Detail       string `db:"detail"` // variant label; "" for products
	Units        int64  `db:"units"`
	RevenueCents int64  `db:"revenue_cents"`
}

// TopBy selects the ranking.
type TopBy int

// Rankings.
const (
	TopVariantUnits TopBy = iota
	TopProductRevenue
)

func topSQL(group, sel, order string) string {
	return `SELECT ` + sel + `,
        SUM(oi.quantity)::bigint AS units,
        SUM(oi.quantity * oi.unit_price_cents)::bigint AS revenue_cents
 FROM orders o
 JOIN order_items oi ON oi.order_id = o.id
 JOIN product_variants v ON v.id = oi.variant_id
 WHERE o.status IN ` + saleStatuses + `
   AND COALESCE(o.paid_at, o.created_at) >= ` + rangeStart + `
   AND COALESCE(o.paid_at, o.created_at) < ` + rangeEnd + `
 GROUP BY ` + group + `
 ORDER BY ` + order + `, name
 LIMIT $3`
}

var (
	sqlTopVariants = sqlText(topSQL("oi.variant_id", "MAX(oi.product_name) AS name, MAX(oi.variant_label) AS detail", "units DESC, revenue_cents DESC"))
	sqlTopProducts = sqlText(topSQL("v.product_id", "MAX(oi.product_name) AS name, ''::text AS detail", "revenue_cents DESC, units DESC"))
)

// TopSellers ranks variants by units or products by item revenue
// (quantity × unit price, before order-level discounts).
func TopSellers(ctx context.Context, q DBTX, r DateRange, by TopBy, limit int64) ([]TopRow, error) {
	query := sqlTopVariants
	if by == TopProductRevenue {
		query = sqlTopProducts
	}
	return collect[TopRow](q.Query(ctx, query, r.From, r.To, limit))
}

// CouponUsageRow is one coupon's use on sales.
type CouponUsageRow struct {
	Code          string `db:"code"`
	Orders        int64  `db:"orders"`
	DiscountCents int64  `db:"discount_cents"`
	RevenueCents  int64  `db:"revenue_cents"`
}

var sqlCouponUsage = sqlText(`SELECT upper(COALESCE(c.code, o.payment_meta->>'coupon_code')) AS code,
        COUNT(*) AS orders,
        SUM(o.discount_cents)::bigint AS discount_cents,
        SUM(o.total_cents)::bigint AS revenue_cents
 FROM orders o
 LEFT JOIN coupons c ON c.id = o.coupon_id
 WHERE o.status IN ` + saleStatuses + `
   AND COALESCE(o.paid_at, o.created_at) >= ` + rangeStart + `
   AND COALESCE(o.paid_at, o.created_at) < ` + rangeEnd + `
   AND COALESCE(c.code, NULLIF(o.payment_meta->>'coupon_code', '')) IS NOT NULL
 GROUP BY 1
 ORDER BY orders DESC, discount_cents DESC, code`)

// CouponUsage lists coupon use on sales in range.
func CouponUsage(ctx context.Context, q DBTX, r DateRange) ([]CouponUsageRow, error) {
	return collect[CouponUsageRow](q.Query(ctx, sqlCouponUsage, r.args()...))
}

// ProviderRow is the revenue split by payment provider.
type ProviderRow struct {
	Provider     string `db:"provider"`
	Orders       int64  `db:"orders"`
	RevenueCents int64  `db:"revenue_cents"`
}

// Label is the provider label.
func (p ProviderRow) Label() string { return ProviderLabel(p.Provider) }

var sqlProviderSplit = sqlText(`SELECT o.payment_provider AS provider, COUNT(*) AS orders,
        SUM(o.total_cents)::bigint AS revenue_cents
 FROM orders o
 WHERE o.status IN ` + saleStatuses + `
   AND COALESCE(o.paid_at, o.created_at) >= ` + rangeStart + `
   AND COALESCE(o.paid_at, o.created_at) < ` + rangeEnd + `
 GROUP BY 1
 ORDER BY revenue_cents DESC`)

// ProviderSplit splits sales by provider.
func ProviderSplit(ctx context.Context, q DBTX, r DateRange) ([]ProviderRow, error) {
	return collect[ProviderRow](q.Query(ctx, sqlProviderSplit, r.args()...))
}

// ReportPage is everything /admin/relatorios renders.
type ReportPage struct {
	Range       DateRange
	Summary     ReportSummary
	Days        []DayRow
	BestDay     int64
	TopVariants []TopRow
	TopProducts []TopRow
	Coupons     []CouponUsageRow
	Providers   []ProviderRow
	// CouponOrders / CouponDiscount are the coupon table totals.
	CouponOrders   int64
	CouponDiscount int64
	// CSVQuery is "de=…&ate=…" for the export link.
	CSVQuery string
}

// LoadReportPage loads the whole report for de/ate.
func LoadReportPage(ctx context.Context, q DBTX, de, ate *string) (ReportPage, error) {
	r := ParseDateRange(de, ate)
	p := ReportPage{Range: r, CSVQuery: "de=" + FormatDateISO(r.From) + "&ate=" + FormatDateISO(r.To)}
	var err error
	if p.Summary, err = LoadReportSummary(ctx, q, r); err != nil {
		return p, err
	}
	if p.Days, err = RevenueByDay(ctx, q, r); err != nil {
		return p, err
	}
	for _, d := range p.Days {
		p.BestDay = max(p.BestDay, d.RevenueCents)
	}
	if p.TopVariants, err = TopSellers(ctx, q, r, TopVariantUnits, ReportTopLimit); err != nil {
		return p, err
	}
	if p.TopProducts, err = TopSellers(ctx, q, r, TopProductRevenue, ReportTopLimit); err != nil {
		return p, err
	}
	if p.Coupons, err = CouponUsage(ctx, q, r); err != nil {
		return p, err
	}
	for _, c := range p.Coupons {
		p.CouponOrders += c.Orders
		p.CouponDiscount += c.DiscountCents
	}
	p.Providers, err = ProviderSplit(ctx, q, r)
	return p, err
}

// --- CSV export ---

// CSVOrder is one exported order (any status, created in range).
type CSVOrder struct {
	ID               uuid.UUID `db:"id"`
	CreatedAt        string    `db:"created_at"`
	PaidAt           *string   `db:"paid_at"`
	Status           string    `db:"status"`
	Email            *string   `db:"email"`
	UF               *string   `db:"uf"`
	City             *string   `db:"city"`
	Items            *string   `db:"items"`
	Units            int64     `db:"units"`
	SubtotalCents    int64     `db:"subtotal_cents"`
	ShippingCents    int64     `db:"shipping_cents"`
	DiscountCents    int64     `db:"discount_cents"`
	PixDiscountCents int64     `db:"pix_discount_cents"`
	TotalCents       int64     `db:"total_cents"`
	Provider         string    `db:"provider"`
	Coupon           *string   `db:"coupon"`
	Carrier          string    `db:"carrier"`
	TrackingCode     *string   `db:"tracking_code"`
	ShippedAt        *string   `db:"shipped_at"`
	DeliveredAt      *string   `db:"delivered_at"`
}

const csvTS = `'YYYY-MM-DD HH24:MI'`

var sqlCSVOrders = sqlText(`SELECT o.id,
        to_char(o.created_at AT TIME ZONE 'America/Sao_Paulo', ` + csvTS + `) AS created_at,
        to_char(o.paid_at AT TIME ZONE 'America/Sao_Paulo', ` + csvTS + `) AS paid_at,
        o.status::text AS status,
        COALESCE(NULLIF(btrim(o.guest_email), ''), u.email) AS email,
        o.shipping_address->>'state' AS uf,
        o.shipping_address->>'city' AS city,
        items.summary AS items,
        COALESCE(items.units, 0)::bigint AS units,
        o.subtotal_cents, o.shipping_cents, o.discount_cents,
        o.pix_discount_cents, o.total_cents,
        o.payment_provider AS provider,
        COALESCE(c.code, NULLIF(o.payment_meta->>'coupon_code', '')) AS coupon,
        o.carrier, o.tracking_code,
        to_char(o.shipped_at AT TIME ZONE 'America/Sao_Paulo', ` + csvTS + `) AS shipped_at,
        to_char(o.delivered_at AT TIME ZONE 'America/Sao_Paulo', ` + csvTS + `) AS delivered_at
 FROM orders o
 LEFT JOIN users u ON u.id = o.user_id
 LEFT JOIN coupons c ON c.id = o.coupon_id
 LEFT JOIN LATERAL (
     SELECT string_agg(
                oi.quantity || 'x ' || oi.product_name || ' (' || oi.variant_label || ')',
                '; ' ORDER BY oi.created_at, oi.id) AS summary,
            SUM(oi.quantity) AS units
     FROM order_items oi WHERE oi.order_id = o.id
 ) items ON true
 WHERE o.created_at >= ` + rangeStart + ` AND o.created_at < ` + rangeEnd + `
 ORDER BY o.created_at, o.id
 LIMIT $3`)

// CSVOrders lists orders created in range, oldest first, at most limit.
func CSVOrders(ctx context.Context, q DBTX, r DateRange, limit int64) ([]CSVOrder, error) {
	return collect[CSVOrder](q.Query(ctx, sqlCSVOrders, r.From, r.To, limit))
}

// CSVField quotes one field (RFC 4180) and neutralises spreadsheet
// formulas: a cell starting with = + - @ tab or CR gets a leading '.
func CSVField(raw string) string {
	v := raw
	if raw != "" && strings.ContainsRune("=+-@\t\r", rune(raw[0])) {
		v = "'" + raw
	}
	return `"` + strings.ReplaceAll(v, `"`, `""`) + `"`
}

func csvMoney(cents int64) string {
	c := cents % 100
	if c < 0 {
		c = -c
	}
	return fmt.Sprintf("%d.%02d", cents/100, c)
}

// CSVHeader is the export header.
var CSVHeader = []string{
	"pedido", "id", "criado_em", "pago_em", "status", "email", "uf", "cidade", "itens",
	"unidades", "subtotal", "frete", "desconto_cupom", "desconto_pix", "total",
	"pagamento", "cupom", "transportadora", "rastreio", "enviado_em", "entregue_em",
}

// RenderCSV renders the export with a UTF-8 BOM, every field quoted.
func RenderCSV(orders []CSVOrder) string {
	var b strings.Builder
	b.WriteString("\xEF\xBB\xBF")
	writeRow := func(fields []string) {
		for i, f := range fields {
			if i > 0 {
				b.WriteByte(',')
			}
			b.WriteString(CSVField(f))
		}
		b.WriteString("\r\n")
	}
	writeRow(CSVHeader)
	for _, o := range orders {
		writeRow([]string{
			OrderCode(o.ID), o.ID.String(), o.CreatedAt, deref(o.PaidAt), o.Status,
			deref(o.Email), deref(o.UF), deref(o.City), deref(o.Items), strconv.FormatInt(o.Units, 10),
			csvMoney(o.SubtotalCents), csvMoney(o.ShippingCents), csvMoney(o.DiscountCents),
			csvMoney(o.PixDiscountCents), csvMoney(o.TotalCents), o.Provider, deref(o.Coupon),
			o.Carrier, deref(o.TrackingCode), deref(o.ShippedAt), deref(o.DeliveredAt),
		})
	}
	return b.String()
}

// OrdersCSVExport is the /admin/relatorios/pedidos.csv body. TooMany is
// set (and Body empty) past CSVMaxOrders: the UI toasts "Mais de 20000
// pedidos no período. Reduza o intervalo." and redirects back.
type OrdersCSVExport struct {
	Range    DateRange
	Filename string
	Body     string
	TooMany  bool
}

// TooManyMessage is the toast for an oversized export.
func (e OrdersCSVExport) TooManyMessage() string {
	return fmt.Sprintf("Mais de %d pedidos no período. Reduza o intervalo.", CSVMaxOrders)
}

// ExportOrdersCSV builds the export for de/ate.
func ExportOrdersCSV(ctx context.Context, q DBTX, de, ate *string) (OrdersCSVExport, error) {
	r := ParseDateRange(de, ate)
	out := OrdersCSVExport{Range: r, Filename: fmt.Sprintf("pedidos-%s_%s.csv", FormatDateISO(r.From), FormatDateISO(r.To))}
	orders, err := CSVOrders(ctx, q, r, CSVMaxOrders+1)
	if err != nil {
		return out, err
	}
	if len(orders) > CSVMaxOrders {
		out.TooMany = true
		return out, nil
	}
	out.Body = RenderCSV(orders)
	return out, nil
}
