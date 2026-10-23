package store_test

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/payments"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// Port of the src/app/store/order.rs tests, plus the stock assertions of
// the webhook-driven transitions (src/payments/tests.rs) run through
// payments.Service with the real store.OrderFSM.

func shipOpts() store.ApplyOptions {
	return store.ApplyOptions{Tracking: &store.Tracking{Carrier: "Correios", Code: "AA123456789BR"}}
}

// seedOrderWithStock: a pending order holding 1 unit of a variant with qty
// on hand (reserved through ReserveForOrder).
func seedOrderWithStock(t *testing.T, pool *pgxpool.Pool, qty int32) (orderID, variantID uuid.UUID) {
	t.Helper()
	_, variantID, _ = seedVariant(t, pool, 1000, qty)
	orderID = insertPendingOrder(t, pool, store.ReserveLine{VariantID: variantID, Quantity: 1})
	tx := begin(t, pool)
	mustNoErr(t, store.ReserveForOrder(ctx, tx, orderID, []store.ReserveLine{{VariantID: variantID, Quantity: 1}}))
	commit(t, tx)
	return orderID, variantID
}

func apply(t *testing.T, tx store.DBTX, orderID uuid.UUID, ev store.OrderEvent, opts store.ApplyOptions) store.OrderStatus {
	t.Helper()
	next, err := store.Apply(ctx, tx, orderID, ev, opts)
	if err != nil {
		t.Fatalf("apply %s: %v", ev, err)
	}
	return next
}

func TestApplyRejectsIllegalTransition(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, _ := seedOrderWithStock(t, pool, 2)
	tx := begin(t, pool)
	apply(t, tx, orderID, store.EventPay, store.ApplyOptions{})
	if _, err := store.Apply(ctx, tx, orderID, store.EventDeliver, store.ApplyOptions{}); err == nil {
		t.Fatal("paid → deliver must be illegal")
	}
	var ill *store.IllegalTransitionError
	if _, err := store.Apply(ctx, tx, orderID, store.EventPay, store.ApplyOptions{}); !errors.As(err, &ill) {
		t.Fatalf("pay twice: %v", err)
	}
}

func TestPaidShippedDelivered(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, _ := seedOrderWithStock(t, pool, 2)
	tx := begin(t, pool)
	for _, step := range []struct {
		ev   store.OrderEvent
		opts store.ApplyOptions
		want store.OrderStatus
	}{
		{store.EventPay, store.ApplyOptions{}, store.StatusPaid},
		{store.EventShip, shipOpts(), store.StatusShipped},
		{store.EventDeliver, store.ApplyOptions{}, store.StatusDelivered},
	} {
		if got := apply(t, tx, orderID, step.ev, step.opts); got != step.want {
			t.Fatalf("%s → %s, want %s", step.ev, got, step.want)
		}
	}
	commit(t, tx)
}

func TestPaidProcessingShippedDelivered(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, _ := seedOrderWithStock(t, pool, 2)
	tx := begin(t, pool)
	apply(t, tx, orderID, store.EventPay, store.ApplyOptions{})
	if got := apply(t, tx, orderID, store.EventProcess, store.ApplyOptions{}); got != store.StatusProcessing {
		t.Fatal(got)
	}
	if got := apply(t, tx, orderID, store.EventShip, shipOpts()); got != store.StatusShipped {
		t.Fatal(got)
	}
	if got := apply(t, tx, orderID, store.EventDeliver, store.ApplyOptions{}); got != store.StatusDelivered {
		t.Fatal(got)
	}
	commit(t, tx)
}

func TestParseCoversNewEvents(t *testing.T) {
	for raw, want := range map[string]store.OrderEvent{
		"processing": store.EventProcess, "process": store.EventProcess, "reopen": store.EventReopen,
		"reopen_pay": store.EventReopenPay, "ship": store.EventShip,
	} {
		if got, ok := store.ParseOrderEvent(raw); !ok || got != want {
			t.Errorf("parse %q = %q", raw, got)
		}
	}
	if _, ok := store.ParseOrderEvent("unknown"); ok {
		t.Error("unknown parsed")
	}
}

func TestPaidRefundRestocks(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, variantID := seedOrderWithStock(t, pool, 3)
	tx := begin(t, pool)
	apply(t, tx, orderID, store.EventPay, store.ApplyOptions{})
	if q, _ := stockOf(t, tx, variantID); q != 2 {
		t.Fatalf("after pay %d", q)
	}
	apply(t, tx, orderID, store.EventRefund, store.ApplyOptions{})
	if q, _ := stockOf(t, tx, variantID); q != 3 {
		t.Fatalf("after refund %d", q)
	}
}

func TestRestockFlagIsExplicit(t *testing.T) {
	pool := testutil.NewPool(t)
	// Paid refund with an explicit "don't restock" keeps the sale consumed.
	orderID, variantID := seedOrderWithStock(t, pool, 3)
	tx := begin(t, pool)
	apply(t, tx, orderID, store.EventPay, store.ApplyOptions{})
	apply(t, tx, orderID, store.EventRefund, store.WithRestock(false))
	if q, _ := stockOf(t, tx, variantID); q != 2 {
		t.Fatalf("no-restock refund: %d", q)
	}
	if s := statusOf(t, tx, orderID); s != "refunded" {
		t.Fatal(s)
	}
	commit(t, tx)

	// Shipped refund defaults to NOT restocking.
	orderID, variantID = seedOrderWithStock(t, pool, 3)
	tx = begin(t, pool)
	apply(t, tx, orderID, store.EventPay, store.ApplyOptions{})
	apply(t, tx, orderID, store.EventShip, shipOpts())
	apply(t, tx, orderID, store.EventRefund, store.ApplyOptions{})
	if q, _ := stockOf(t, tx, variantID); q != 2 {
		t.Fatalf("shipped refund default: %d", q)
	}
	commit(t, tx)

	// ...unless the admin explicitly asks to restock.
	orderID, variantID = seedOrderWithStock(t, pool, 3)
	tx = begin(t, pool)
	apply(t, tx, orderID, store.EventPay, store.ApplyOptions{})
	apply(t, tx, orderID, store.EventShip, shipOpts())
	apply(t, tx, orderID, store.EventRefund, store.WithRestock(true))
	if q, _ := stockOf(t, tx, variantID); q != 3 {
		t.Fatalf("explicit restock: %d", q)
	}
	commit(t, tx)
}

func TestProcessingCancelRestocksByDefault(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, variantID := seedOrderWithStock(t, pool, 3)
	tx := begin(t, pool)
	apply(t, tx, orderID, store.EventPay, store.ApplyOptions{})
	apply(t, tx, orderID, store.EventProcess, store.ApplyOptions{})
	apply(t, tx, orderID, store.EventCancel, store.ApplyOptions{})
	if s := statusOf(t, tx, orderID); s != "cancelled" {
		t.Fatal(s)
	}
	if q, r := stockOf(t, tx, variantID); q != 3 || r != 0 {
		t.Fatalf("(%d, %d)", q, r)
	}
}

func TestDeliveredCanBeRefundedWithoutRestock(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, variantID := seedOrderWithStock(t, pool, 3)
	tx := begin(t, pool)
	apply(t, tx, orderID, store.EventPay, store.ApplyOptions{})
	apply(t, tx, orderID, store.EventShip, shipOpts())
	apply(t, tx, orderID, store.EventDeliver, store.ApplyOptions{})
	if got := apply(t, tx, orderID, store.EventRefund, store.ApplyOptions{}); got != store.StatusRefunded {
		t.Fatal(got)
	}
	if q, _ := stockOf(t, tx, variantID); q != 2 {
		t.Fatalf("%d", q)
	}
}

func TestReopenExpiredOrderThenPay(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, variantID := seedOrderWithStock(t, pool, 3)
	tx := begin(t, pool)
	apply(t, tx, orderID, store.EventCancel, store.ApplyOptions{})
	if q, r := stockOf(t, tx, variantID); q != 3 || r != 0 {
		t.Fatalf("after cancel (%d, %d)", q, r)
	}
	if got := apply(t, tx, orderID, store.EventReopen, store.ApplyOptions{}); got != store.StatusPendingPayment {
		t.Fatal(got)
	}
	if _, r := stockOf(t, tx, variantID); r != 1 {
		t.Fatalf("reopen reserved %d", r)
	}
	if got := apply(t, tx, orderID, store.EventPay, store.ApplyOptions{}); got != store.StatusPaid {
		t.Fatal(got)
	}
	if q, r := stockOf(t, tx, variantID); q != 2 || r != 0 {
		t.Fatalf("after pay (%d, %d)", q, r)
	}
}

func TestReopenPayMarksPaidInOneStep(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, variantID := seedOrderWithStock(t, pool, 3)
	tx := begin(t, pool)
	apply(t, tx, orderID, store.EventCancel, store.ApplyOptions{})
	if got := apply(t, tx, orderID, store.EventReopenPay, store.ApplyOptions{}); got != store.StatusPaid {
		t.Fatal(got)
	}
	if q, r := stockOf(t, tx, variantID); q != 2 || r != 0 {
		t.Fatalf("(%d, %d)", q, r)
	}
	if !scalar[bool](t, tx, `SELECT paid_at IS NOT NULL FROM orders WHERE id = $1`, orderID) {
		t.Fatal("paid_at not set")
	}
	if got := apply(t, tx, orderID, store.EventShip, shipOpts()); got != store.StatusShipped {
		t.Fatal(got)
	}
}

func TestReopenFailsWhenStockGone(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, variantID := seedOrderWithStock(t, pool, 1)
	tx := begin(t, pool)
	apply(t, tx, orderID, store.EventCancel, store.ApplyOptions{})
	exec(t, tx, `UPDATE inventory SET quantity = 0 WHERE variant_id = $1`, variantID)
	_, err := store.Apply(ctx, tx, orderID, store.EventReopen, store.ApplyOptions{})
	if !errors.Is(err, store.ErrReopenInsufficient) || !errors.Is(err, payments.ErrInsufficientStock) {
		t.Fatalf("want insufficient stock, got %v", err)
	}
	if s := statusOf(t, tx, orderID); s != "cancelled" {
		t.Fatal(s)
	}
}

func TestReopenRejectedOncePaid(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, _ := seedOrderWithStock(t, pool, 2)
	tx := begin(t, pool)
	apply(t, tx, orderID, store.EventPay, store.ApplyOptions{})
	apply(t, tx, orderID, store.EventCancel, store.ApplyOptions{})
	var ill *store.IllegalTransitionError
	for _, ev := range []store.OrderEvent{store.EventReopen, store.EventReopenPay} {
		if _, err := store.Apply(ctx, tx, orderID, ev, store.ApplyOptions{}); !errors.As(err, &ill) {
			t.Fatalf("%s: %v", ev, err)
		}
	}
}

func TestReopenReclaimsCoupon(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, _ := seedOrderWithStock(t, pool, 2)
	couponID := newID()
	exec(t, pool, `INSERT INTO coupons (id, code, discount_type, discount_value) VALUES ($1, $2, 'fixed', 100)`, couponID, "reabrir-"+couponID.String())
	exec(t, pool, `INSERT INTO coupon_redemptions (id, coupon_id, order_id, guest_email) VALUES ($1, $2, $3, 'a@b.co')`, newID(), couponID, orderID)
	tx := begin(t, pool)
	apply(t, tx, orderID, store.EventCancel, store.ApplyOptions{})
	if !scalar[bool](t, tx, `SELECT refunded_at IS NOT NULL FROM coupon_redemptions WHERE order_id = $1`, orderID) {
		t.Fatal("cancel must restore the redemption")
	}
	apply(t, tx, orderID, store.EventReopen, store.ApplyOptions{})
	if scalar[bool](t, tx, `SELECT refunded_at IS NOT NULL FROM coupon_redemptions WHERE order_id = $1`, orderID) {
		t.Fatal("reopen must consume the coupon again")
	}
}

func TestApplyWithAuditRecordsTrail(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, _ := seedOrderWithStock(t, pool, 2)
	actorID := insertUser(t, pool, "Admin")
	tx := begin(t, pool)
	actor := store.UserActor(actorID, "Admin")
	if _, err := store.ApplyWithAudit(ctx, tx, orderID, store.EventPay, store.ApplyOptions{}, actor); err != nil {
		t.Fatal(err)
	}
	if _, err := store.ApplyWithAudit(ctx, tx, orderID, store.EventRefund, store.WithRestock(true), actor); err != nil {
		t.Fatal(err)
	}
	rows, err := tx.Query(ctx, `SELECT aggregate_type, entity_type, metadata FROM audit_events WHERE entity_id = $1 ORDER BY occurred_at, id`, orderID)
	mustNoErr(t, err)
	type row struct {
		agg, ent string
		meta     map[string]any
	}
	var got []row
	for rows.Next() {
		var r row
		var raw []byte
		mustNoErr(t, rows.Scan(&r.agg, &r.ent, &raw))
		mustNoErr(t, json.Unmarshal(raw, &r.meta))
		got = append(got, r)
	}
	rows.Close()
	if len(got) != 2 || got[0].agg != "order" || got[0].ent != "order" {
		t.Fatalf("%+v", got)
	}
	want := []map[string]any{
		{"action": "pay", "from": "pending_payment", "to": "paid", "restock": false},
		{"action": "refund", "from": "paid", "to": "refunded", "restock": true},
	}
	for i, w := range want {
		for k, v := range w {
			if got[i].meta[k] != v {
				t.Errorf("row %d %s = %v, want %v", i, k, got[i].meta[k], v)
			}
		}
	}
}

func outboxKinds(t *testing.T, q store.DBTX, orderID uuid.UUID) []string {
	t.Helper()
	rows, err := q.Query(ctx, `SELECT kind FROM email_outbox WHERE text_body LIKE $1 ORDER BY id`, "%/pedido/"+orderID.String()+"?t=%")
	mustNoErr(t, err)
	defer rows.Close()
	out := []string{}
	for rows.Next() {
		var k string
		mustNoErr(t, rows.Scan(&k))
		out = append(out, k)
	}
	return out
}

func TestTransitionsEnqueueCustomerMailInTx(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, _ := seedOrderWithStock(t, pool, 2)
	tx := begin(t, pool)
	for _, ev := range []store.OrderEvent{store.EventPay, store.EventProcess, store.EventShip} {
		apply(t, tx, orderID, ev, shipOpts())
	}
	// Processing has no customer mail; pay and ship do.
	if got := strings.Join(outboxKinds(t, tx, orderID), ","); got != "payment_confirmed,order_shipped" {
		t.Fatal(got)
	}
	if to := scalar[string](t, tx, `SELECT to_address FROM email_outbox WHERE text_body LIKE $1 LIMIT 1`, "%/pedido/"+orderID.String()+"?t=%"); to != "a@b.co" {
		t.Fatal(to)
	}
	shipped := scalar[string](t, tx, `SELECT text_body FROM email_outbox WHERE text_body LIKE $1 AND kind = 'order_shipped'`, "%/pedido/"+orderID.String()+"?t=%")
	for _, want := range []string{"Correios", "AA123456789BR", "rastreamento.correios.com.br/app/index.php?objetos=AA123456789BR"} {
		if !strings.Contains(shipped, want) {
			t.Errorf("shipped mail lacks %q:\n%s", want, shipped)
		}
	}
	mustNoErr(t, tx.Rollback(ctx))
	if got := outboxKinds(t, pool, orderID); len(got) != 0 {
		t.Fatalf("rolled-back mail leaked: %v", got)
	}
}

func TestExpiryAndRefundEnqueueDistinctMail(t *testing.T) {
	pool := testutil.NewPool(t)
	expiredID, _ := seedOrderWithStock(t, pool, 2)
	exec(t, pool, `UPDATE orders SET reservation_expires_at = now() - interval '1 hour' WHERE id = $1`, expiredID)
	refundID, _ := seedOrderWithStock(t, pool, 2)
	tx := begin(t, pool)
	apply(t, tx, expiredID, store.EventCancel, store.ApplyOptions{})
	apply(t, tx, refundID, store.EventPay, store.ApplyOptions{})
	apply(t, tx, refundID, store.EventRefund, store.ApplyOptions{})
	commit(t, tx)
	if got := strings.Join(outboxKinds(t, pool, expiredID), ","); got != "order_expired" {
		t.Fatal(got)
	}
	if got := strings.Join(outboxKinds(t, pool, refundID), ","); got != "payment_confirmed,order_refunded" {
		t.Fatal(got)
	}
}

func TestRevenueSumsPaidThroughDeliveredOnly(t *testing.T) {
	pool := testutil.NewPool(t)
	var ids []uuid.UUID
	for _, o := range []struct {
		status string
		cents  int64
	}{{"pending_payment", 99_999}, {"cancelled", 50_000}, {"refunded", 20_000}, {"paid", 1_000}, {"shipped", 300}, {"delivered", 400}} {
		id := newID()
		exec(t, pool, `INSERT INTO orders (id, guest_email, status, access_token, total_cents) VALUES ($1, 'a@b.co', $2::text::order_status, $3, $4)`, id, o.status, newID(), o.cents)
		ids = append(ids, id)
	}
	if got := scalar[int64](t, pool, `SELECT COALESCE(SUM(total_cents), 0)::bigint FROM orders WHERE id = ANY($1) AND status IN ('paid', 'shipped', 'delivered')`, ids); got != 1700 {
		t.Fatal(got)
	}
	d, err := store.LoadAdminDashboard(ctx, pool)
	mustNoErr(t, err)
	if d.TotalRevenue != 1700 || d.TotalOrders != 6 {
		t.Fatalf("%+v", d)
	}
}

func TestTrackingNormalization(t *testing.T) {
	carrier, code, err := store.NormalizeTracking("", " aa 123.456.789-br ")
	if err != nil || carrier != "Correios" || code != "AA123456789BR" {
		t.Fatalf("%q %q %v", carrier, code, err)
	}
	if _, code, _ := store.NormalizeTracking("correios", "QB987654321US"); code != "QB987654321US" {
		t.Fatal(code)
	}
	if _, _, err := store.NormalizeTracking("Correios", "  "); !errors.Is(err, store.ErrTrackingRequired) {
		t.Fatal(err)
	}
	var inv *store.InvalidTrackingError
	if _, _, err := store.NormalizeTracking("Correios", "123456789"); !errors.As(err, &inv) {
		t.Fatal(err)
	}
	carrier, code, err = store.NormalizeTracking(" Jadlog ", "1234-5678 90")
	if err != nil || carrier != "Jadlog" || code != "1234567890" {
		t.Fatalf("%q %q %v", carrier, code, err)
	}
	if _, _, err := store.NormalizeTracking("Jadlog", "<script>"); !errors.As(err, &inv) {
		t.Fatal(err)
	}
	if _, ok := store.TrackingURL("Correios", "AA123456789BR"); !ok {
		t.Fatal("correios url")
	}
	if _, ok := store.TrackingURL("Jadlog", "123"); ok {
		t.Fatal("jadlog url")
	}
}

func TestShipRequiresTrackingAndStoresIt(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, _ := seedOrderWithStock(t, pool, 2)
	tx := begin(t, pool)
	apply(t, tx, orderID, store.EventPay, store.ApplyOptions{})
	if _, err := store.Apply(ctx, tx, orderID, store.EventShip, store.ApplyOptions{}); !errors.Is(err, store.ErrTrackingRequired) {
		t.Fatal(err)
	}
	var inv *store.InvalidTrackingError
	bad := store.ApplyOptions{Tracking: &store.Tracking{Carrier: "Correios", Code: "nope"}}
	if _, err := store.Apply(ctx, tx, orderID, store.EventShip, bad); !errors.As(err, &inv) {
		t.Fatal(err)
	}
	if s := statusOf(t, tx, orderID); s != "paid" {
		t.Fatal(s)
	}
	exec(t, tx, `UPDATE orders SET access_token_expires_at = now() + interval '1 day' WHERE id = $1`, orderID)
	apply(t, tx, orderID, store.EventShip, store.ApplyOptions{Tracking: &store.Tracking{Code: "aa123456789br"}})
	var carrier string
	var code *string
	var shipped, extended bool
	mustNoErr(t, tx.QueryRow(ctx, `SELECT carrier, tracking_code, shipped_at IS NOT NULL,
		access_token_expires_at > now() + interval '80 days' FROM orders WHERE id = $1`, orderID).Scan(&carrier, &code, &shipped, &extended))
	if carrier != "Correios" || code == nil || *code != "AA123456789BR" || !shipped || !extended {
		t.Fatalf("%s %v %v %v", carrier, code, shipped, extended)
	}
	apply(t, tx, orderID, store.EventDeliver, store.ApplyOptions{})
	if !scalar[bool](t, tx, `SELECT delivered_at IS NOT NULL FROM orders WHERE id = $1`, orderID) {
		t.Fatal("delivered_at")
	}
}

func TestShipAuditRecordsTracking(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, _ := seedOrderWithStock(t, pool, 2)
	tx := begin(t, pool)
	apply(t, tx, orderID, store.EventPay, store.ApplyOptions{})
	if _, err := store.ApplyWithAudit(ctx, tx, orderID, store.EventShip, shipOpts(), store.NamedActor("Admin")); err != nil {
		t.Fatal(err)
	}
	var raw []byte
	mustNoErr(t, tx.QueryRow(ctx, `SELECT metadata FROM audit_events WHERE entity_id = $1`, orderID).Scan(&raw))
	var meta map[string]any
	mustNoErr(t, json.Unmarshal(raw, &meta))
	if meta["tracking_code"] != "AA123456789BR" || meta["carrier"] != "Correios" {
		t.Fatalf("%v", meta)
	}
}

func TestExpirePendingReservationsCancelsExpiredOnly(t *testing.T) {
	pool := testutil.NewPool(t)
	expired, v1 := seedOrderWithStock(t, pool, 2)
	live, _ := seedOrderWithStock(t, pool, 2)
	exec(t, pool, `UPDATE orders SET reservation_expires_at = now() - interval '1 hour' WHERE id = $1`, expired)
	mustNoErr(t, store.ExpirePendingReservations(ctx, pool))
	if s := statusOf(t, pool, expired); s != "cancelled" {
		t.Fatal(s)
	}
	if s := statusOf(t, pool, live); s != "pending_payment" {
		t.Fatal(s)
	}
	if _, r := stockOf(t, pool, v1); r != 0 {
		t.Fatalf("expired hold not released: %d", r)
	}
	// ExpireOne is a no-op on a live order.
	mustNoErr(t, store.OrderFSM{Pool: pool}.ExpireOne(ctx, live))
	if s := statusOf(t, pool, live); s != "pending_payment" {
		t.Fatal(s)
	}
}

// --- webhook-driven transitions (src/payments/tests.rs stock assertions) ---

// seedMPOrder: a pending Mercado Pago order holding 1 unit of a variant
// with stock units, charged as MP payment paymentID.
func seedMPOrder(t *testing.T, pool *pgxpool.Pool, stock int32, paymentID int64) (uuid.UUID, uuid.UUID) {
	t.Helper()
	orderID, variantID := seedOrderWithStock(t, pool, stock)
	exec(t, pool, `UPDATE orders SET payment_provider = 'mercadopago', total_cents = 12345, mp_payment_id = $2 WHERE id = $1`, orderID, paymentID)
	return orderID, variantID
}

func mpInfo(orderID uuid.UUID, paymentID int64, status payments.ChargeStatus, raw string, cents int64) payments.PaymentInfo {
	ref := orderID.String()
	return payments.PaymentInfo{PaymentID: paymentID, Status: status, RawStatus: raw, ExternalReference: &ref, AmountCents: &cents}
}

func paymentService(pool *pgxpool.Pool) *payments.Service {
	return &payments.Service{Pool: pool, Provider: payments.StaticPix{}, Orders: store.OrderFSM{Pool: pool}}
}

func TestWebhookApprovedMarksPaidAndConvertsReservation(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, variantID := seedMPOrder(t, pool, 3, 9001)
	if q, r := stockOf(t, pool, variantID); q != 3 || r != 1 {
		t.Fatalf("(%d, %d)", q, r)
	}
	svc := paymentService(pool)
	out, err := svc.ApplyPaymentInfo(ctx, mpInfo(orderID, 9001, payments.StatusApproved, "approved", 12345), payments.MercadoPagoActor)
	mustNoErr(t, err)
	if out.Kind != payments.OutcomeApplied || statusOf(t, pool, orderID) != "paid" {
		t.Fatalf("%+v %s", out, statusOf(t, pool, orderID))
	}
	if q, r := stockOf(t, pool, variantID); q != 2 || r != 0 {
		t.Fatalf("(%d, %d)", q, r)
	}
	// Redelivery applies once.
	out, err = svc.ApplyPaymentInfo(ctx, mpInfo(orderID, 9001, payments.StatusApproved, "approved", 12345), payments.MercadoPagoActor)
	mustNoErr(t, err)
	if out.Kind != payments.OutcomeUnchanged {
		t.Fatalf("%+v", out)
	}
	if q, r := stockOf(t, pool, variantID); q != 2 || r != 0 {
		t.Fatalf("stock moved twice: (%d, %d)", q, r)
	}
	if got := scalar[string](t, pool, `SELECT actor_name FROM audit_events WHERE entity_id = $1`, orderID); got != "Mercado Pago" {
		t.Fatal(got)
	}
}

func TestWebhookRejectedPaymentCancelsAndReleasesStock(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, variantID := seedMPOrder(t, pool, 3, 9002)
	_, err := paymentService(pool).ApplyPaymentInfo(ctx, mpInfo(orderID, 9002, payments.StatusFailed, "rejected", 12345), payments.MercadoPagoActor)
	mustNoErr(t, err)
	if s := statusOf(t, pool, orderID); s != "cancelled" {
		t.Fatal(s)
	}
	if q, r := stockOf(t, pool, variantID); q != 3 || r != 0 {
		t.Fatalf("(%d, %d)", q, r)
	}
}

func TestLateApprovalAfterExpiryReopensAndPays(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, variantID := seedMPOrder(t, pool, 3, 9003)
	exec(t, pool, `UPDATE orders SET reservation_expires_at = now() - interval '2 hours' WHERE id = $1`, orderID)
	mustNoErr(t, store.ExpireOne(ctx, pool, orderID))
	if q, r := stockOf(t, pool, variantID); q != 3 || r != 0 || statusOf(t, pool, orderID) != "cancelled" {
		t.Fatalf("(%d, %d) %s", q, r, statusOf(t, pool, orderID))
	}
	out, err := paymentService(pool).ApplyPaymentInfo(ctx, mpInfo(orderID, 9003, payments.StatusApproved, "approved", 12345), payments.MercadoPagoActor)
	mustNoErr(t, err)
	if out.Kind != payments.OutcomeApplied || statusOf(t, pool, orderID) != "paid" {
		t.Fatalf("%+v", out)
	}
	if q, r := stockOf(t, pool, variantID); q != 2 || r != 0 {
		t.Fatalf("(%d, %d)", q, r)
	}
}

func TestLateApprovalWithoutStockNeedsAttention(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, variantID := seedMPOrder(t, pool, 3, 9004)
	exec(t, pool, `UPDATE orders SET reservation_expires_at = now() - interval '2 hours' WHERE id = $1`, orderID)
	mustNoErr(t, store.ExpireOne(ctx, pool, orderID))
	exec(t, pool, `UPDATE inventory SET quantity = 0 WHERE variant_id = $1`, variantID)
	out, err := paymentService(pool).ApplyPaymentInfo(ctx, mpInfo(orderID, 9004, payments.StatusApproved, "approved", 12345), payments.MercadoPagoActor)
	mustNoErr(t, err)
	if out.Kind != payments.OutcomeNeedsAttention || statusOf(t, pool, orderID) != "cancelled" {
		t.Fatalf("%+v %s", out, statusOf(t, pool, orderID))
	}
}

func TestRefundedPaymentRestocksPaidOrder(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, variantID := seedMPOrder(t, pool, 3, 9005)
	svc := paymentService(pool)
	_, err := svc.ApplyPaymentInfo(ctx, mpInfo(orderID, 9005, payments.StatusApproved, "approved", 12345), payments.MercadoPagoActor)
	mustNoErr(t, err)
	_, err = svc.ApplyPaymentInfo(ctx, mpInfo(orderID, 9005, payments.StatusRefunded, "refunded", 12345), payments.MercadoPagoActor)
	mustNoErr(t, err)
	if s := statusOf(t, pool, orderID); s != "refunded" {
		t.Fatal(s)
	}
	// Paid refund restocks by default.
	if q, r := stockOf(t, pool, variantID); q != 3 || r != 0 {
		t.Fatalf("(%d, %d)", q, r)
	}
}

func TestPaymentAwareSweepUsesOrderFSM(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, variantID := seedOrderWithStock(t, pool, 2)
	exec(t, pool, `UPDATE orders SET reservation_expires_at = now() - interval '1 hour' WHERE id = $1`, orderID)
	mustNoErr(t, paymentService(pool).ExpirePendingReservations(ctx))
	if s := statusOf(t, pool, orderID); s != "cancelled" {
		t.Fatal(s)
	}
	if _, r := stockOf(t, pool, variantID); r != 0 {
		t.Fatal(r)
	}
}
