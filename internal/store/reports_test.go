package store_test

import (
	"strings"
	"testing"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// Port of src/app/store/reports/tests.rs.

func date(y int, m time.Month, d int) time.Time { return time.Date(y, m, d, 0, 0, 0, 0, time.UTC) }

func TestRangeParsingDefaultsSwapsAndClamps(t *testing.T) {
	today := date(2026, time.October, 22)
	def := store.ParseDateRangeWithToday(nil, nil, today)
	if !def.To.Equal(today) || def.Days() != store.DefaultRangeDays || !def.From.Equal(date(2026, time.September, 23)) {
		t.Fatalf("%+v", def)
	}
	if inv := store.ParseDateRangeWithToday(str("2026-02-30"), str("lixo"), today); inv != def {
		t.Fatalf("%+v", inv)
	}
	sw := store.ParseDateRangeWithToday(str("2026-03-10"), str("2026-03-01"), today)
	if !sw.From.Equal(date(2026, time.March, 1)) || !sw.To.Equal(date(2026, time.March, 10)) {
		t.Fatalf("%+v", sw)
	}
	long := store.ParseDateRangeWithToday(str("2020-01-01"), str("2026-01-31"), today)
	if !long.To.Equal(date(2026, time.January, 31)) || long.Days() != store.MaxRangeDays {
		t.Fatalf("%+v %d", long, long.Days())
	}
}

func TestCSVCellsAreQuotedAndFormulaSafe(t *testing.T) {
	if store.CSVField("plain") != `"plain"` || store.CSVField(`a"b,c`) != `"a""b,c"` {
		t.Fatal("quoting")
	}
	for raw, want := range map[string]string{
		"=1+1": `"'=1+1"`, "+55 11": `"'+55 11"`, "-2": `"'-2"`, "@SUM(A1)": `"'@SUM(A1)"`,
		"\t=x": "\"'\t=x\"", "\r=x": "\"'\r=x\"", "a=b": `"a=b"`,
	} {
		if got := store.CSVField(raw); got != want {
			t.Errorf("%q → %s, want %s", raw, got, want)
		}
	}
}

func TestRenderedCSVNeutralisesInjectedFields(t *testing.T) {
	o := store.CSVOrder{
		ID: newID(), CreatedAt: "2026-01-10 12:00", Status: "paid",
		Email: str(`=HYPERLINK("http://evil")@x.com`), UF: str("SP"), City: str("@cidade"),
		Items: str("-1x Biquíni (M / azul)"), Units: 1, SubtotalCents: 1000, TotalCents: 1000,
		Provider: "static_pix", Coupon: str("+VERAO"), Carrier: "Correios",
	}
	csv := store.RenderCSV([]store.CSVOrder{o})
	if !strings.HasPrefix(csv, "\xEF\xBB\xBF") {
		t.Fatal("BOM")
	}
	lines := strings.Split(strings.TrimPrefix(csv, "\xEF\xBB\xBF"), "\r\n")
	if n := len(strings.Split(lines[0], ",")); n != len(store.CSVHeader) {
		t.Fatal(n)
	}
	row := lines[1]
	for _, want := range []string{`"'=HYPERLINK(""http://evil"")@x.com"`, `"'@cidade"`, `"'-1x Biquíni (M / azul)"`, `"'+VERAO"`, `"10.00"`} {
		if !strings.Contains(row, want) {
			t.Errorf("row lacks %s: %s", want, row)
		}
	}
	if strings.Contains(row, ",=") {
		t.Fatal(row)
	}
}

type reportSeed struct {
	status, provider, at string
	paid                 bool
	total, discount      int64
	couponID             *uuid.UUID
	items                []reportItem
}

type reportItem struct {
	variant uuid.UUID
	name    string
	qty     int32
	price   int64
}

func reportOrder(t *testing.T, pool *pgxpool.Pool, s reportSeed) uuid.UUID {
	t.Helper()
	if s.status == "" {
		s.status = "paid"
	}
	if s.provider == "" {
		s.provider = "static_pix"
	}
	if s.at == "" {
		s.at = "2026-01-10 12:00"
	}
	id := newID()
	exec(t, pool, `INSERT INTO orders (id, guest_email, status, access_token, payment_provider, subtotal_cents, discount_cents, total_cents, coupon_id, shipping_address, created_at, paid_at)
		VALUES ($1, 'r@example.com', $2::text::order_status, $3, $4, $5, $6, $7, $8, jsonb_build_object('state', 'SP', 'city', 'Santos'),
		($9::timestamp AT TIME ZONE 'America/Sao_Paulo'), CASE WHEN $10 THEN ($9::timestamp AT TIME ZONE 'America/Sao_Paulo') END)`,
		id, s.status, uuid.New(), s.provider, s.total+s.discount, s.discount, s.total, s.couponID, s.at, s.paid)
	for _, it := range s.items {
		exec(t, pool, `INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
			VALUES ($1, $2, $3, $4, 'M / azul', $5, $6)`, newID(), id, it.variant, it.name, it.qty, it.price)
	}
	return id
}

func TestReportMathOnFixture(t *testing.T) {
	pool := testutil.NewPool(t)
	x := insertVariant(t, pool, insertProduct(t, pool, "Biquíni Flor", 1000, true), "m", "azul")
	y := insertVariant(t, pool, insertProduct(t, pool, "Maiô Sol", 1000, true), "m", "azul")
	couponID := newID()
	exec(t, pool, `INSERT INTO coupons (id, code, discount_type, discount_value) VALUES ($1, 'verao', 'fixed', 1000)`, couponID)
	reportOrder(t, pool, reportSeed{
		paid: true, total: 10000, discount: 1000, couponID: &couponID,
		items: []reportItem{{x, "Biquíni Flor", 2, 4000}, {y, "Maiô Sol", 1, 3000}},
	})
	b := reportOrder(t, pool, reportSeed{
		status: "delivered", provider: "mercadopago", at: "2026-01-11 23:30", paid: true, total: 5000,
		items: []reportItem{{x, "Biquíni Flor", 1, 5000}},
	})
	reportOrder(t, pool, reportSeed{status: "processing", at: "2026-01-12 10:00", total: 2000, items: []reportItem{{y, "Maiô Sol", 1, 2000}}})
	reportOrder(t, pool, reportSeed{status: "refunded", at: "2026-01-10 09:00", paid: true, total: 7000, items: []reportItem{{x, "Biquíni Flor", 1, 7000}}})
	reportOrder(t, pool, reportSeed{status: "cancelled", at: "2026-01-10 10:00", total: 3000, items: []reportItem{{y, "Maiô Sol", 1, 3000}}})
	reportOrder(t, pool, reportSeed{status: "pending_payment", provider: "mercadopago", at: "2026-01-11 08:00", total: 4000, items: []reportItem{{y, "Maiô Sol", 1, 4000}}})
	for at, total := range map[string]int64{"2026-01-13 00:00": 9999, "2026-01-09 23:59": 8888} {
		reportOrder(t, pool, reportSeed{at: at, paid: true, total: total, items: []reportItem{{x, "Biquíni Flor", 9, 1111}}})
	}
	exec(t, pool, `INSERT INTO return_requests (id, order_id, kind, status, resolution, reason, refund_cents, resolved_at)
		VALUES ($1, $2, 'defect', 'resolved', 'refunded', 'Costura solta', 500, '2026-01-12 15:00'::timestamp AT TIME ZONE 'America/Sao_Paulo')`, newID(), b)

	r := store.DateRange{From: date(2026, time.January, 10), To: date(2026, time.January, 12)}
	s, err := store.LoadReportSummary(ctx, pool, r)
	mustNoErr(t, err)
	want := store.ReportSummary{RevenueCents: 17000, Orders: 3, DiscountCents: 1000, PendingOrders: 1, AbandonedOrders: 1, RefundedOrders: 1, PartialRefundCents: 500}
	if s != want || s.AOVCents() != 5666 || s.NetRevenueCents() != 16500 {
		t.Fatalf("%+v", s)
	}
	days, err := store.RevenueByDay(ctx, pool, r)
	mustNoErr(t, err)
	if len(days) != 3 || days[0].Day.Day() != 10 || days[0].RevenueCents != 10000 || days[1].RevenueCents != 5000 || days[2].RevenueCents != 2000 || days[2].Orders != 1 {
		t.Fatalf("%+v", days)
	}
	variants, err := store.TopSellers(ctx, pool, r, store.TopVariantUnits, 10)
	mustNoErr(t, err)
	if len(variants) != 2 || variants[0].Name != "Biquíni Flor" || variants[0].Units != 3 || variants[0].RevenueCents != 13000 ||
		variants[1].Name != "Maiô Sol" || variants[1].Units != 2 || variants[1].RevenueCents != 5000 {
		t.Fatalf("%+v", variants)
	}
	products, err := store.TopSellers(ctx, pool, r, store.TopProductRevenue, 1)
	mustNoErr(t, err)
	if len(products) != 1 || products[0].Name != "Biquíni Flor" || products[0].RevenueCents != 13000 {
		t.Fatalf("%+v", products)
	}
	coupons, err := store.CouponUsage(ctx, pool, r)
	mustNoErr(t, err)
	if len(coupons) != 1 || coupons[0] != (store.CouponUsageRow{Code: "VERAO", Orders: 1, DiscountCents: 1000, RevenueCents: 10000}) {
		t.Fatalf("%+v", coupons)
	}
	providers, err := store.ProviderSplit(ctx, pool, r)
	mustNoErr(t, err)
	if len(providers) != 2 || providers[0].Provider != "static_pix" || providers[0].Orders != 2 || providers[0].RevenueCents != 12000 ||
		providers[1].Provider != "mercadopago" || providers[1].RevenueCents != 5000 {
		t.Fatalf("%+v", providers)
	}
	rows, err := store.CSVOrders(ctx, pool, r, 100)
	mustNoErr(t, err)
	if len(rows) != 6 || rows[0].CreatedAt != "2026-01-10 09:00" {
		t.Fatalf("%d %+v", len(rows), rows[0])
	}
	var withCoupon *store.CSVOrder
	for i := range rows {
		if rows[i].UF == nil || *rows[i].UF != "SP" {
			t.Fatal("uf")
		}
		if rows[i].Coupon != nil {
			withCoupon = &rows[i]
		}
	}
	if withCoupon == nil || *withCoupon.Items != "2x Biquíni Flor (M / azul); 1x Maiô Sol (M / azul)" || withCoupon.Units != 3 {
		t.Fatalf("%+v", withCoupon)
	}
	if limited, _ := store.CSVOrders(ctx, pool, r, 2); len(limited) != 2 {
		t.Fatal(len(limited))
	}
	empty := store.DateRange{From: date(2025, time.June, 1), To: date(2025, time.June, 2)}
	if s, _ := store.LoadReportSummary(ctx, pool, empty); s != (store.ReportSummary{}) {
		t.Fatalf("%+v", s)
	}
	if d, _ := store.RevenueByDay(ctx, pool, empty); len(d) != 2 {
		t.Fatal(len(d))
	}
	page, err := store.LoadReportPage(ctx, pool, str("2026-01-10"), str("2026-01-12"))
	mustNoErr(t, err)
	if page.BestDay != 10000 || page.CouponOrders != 1 || page.CSVQuery != "de=2026-01-10&ate=2026-01-12" ||
		page.Range.Period() != "10/01/2026 a 12/01/2026 (3 dias, horário de Brasília)" {
		t.Fatalf("%+v", page)
	}
	export, err := store.ExportOrdersCSV(ctx, pool, str("2026-01-10"), str("2026-01-12"))
	mustNoErr(t, err)
	if export.TooMany || export.Filename != "pedidos-2026-01-10_2026-01-12.csv" || strings.Count(export.Body, "\r\n") != 7 {
		t.Fatalf("%+v", export.Filename)
	}
}

func TestBarsScaleToBestDay(t *testing.T) {
	if store.BarPercent(50, 200) != 25 || store.BarPercent(200, 200) != 100 || store.BarPercent(10, 0) != 0 {
		t.Fatal("bars")
	}
}
