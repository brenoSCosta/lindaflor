package payments_test

import (
	"bytes"
	"context"
	"errors"
	"fmt"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/audit"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/payments"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

// fakeOrders is a minimal stand-in for agent C's order FSM: status,
// paid_at and one audit row per transition (stock side effects are the
// FSM's and are covered by its own tests).
type fakeOrders struct {
	noStock bool
}

func (f *fakeOrders) Apply(ctx context.Context, tx pgx.Tx, orderID uuid.UUID, ev payments.OrderEvent, actor payments.Actor) (sqlc.OrderStatus, error) {
	var to sqlc.OrderStatus
	switch ev {
	case payments.EventPay:
		to = sqlc.OrderStatusPaid
	case payments.EventReopenPay:
		if f.noStock {
			return "", fmt.Errorf("reserve: %w", payments.ErrInsufficientStock)
		}
		to = sqlc.OrderStatusPaid
	case payments.EventCancel:
		to = sqlc.OrderStatusCancelled
	case payments.EventRefund:
		to = sqlc.OrderStatusRefunded
	}
	if _, err := tx.Exec(ctx, `UPDATE orders SET status = $2,
		paid_at = CASE WHEN $2 = 'paid'::order_status THEN COALESCE(paid_at, now()) ELSE paid_at END WHERE id = $1`, orderID, to); err != nil {
		return "", err
	}
	name := actor.Name
	return to, audit.Record(ctx, tx, audit.Event{
		AggregateType: "order", AggregateID: orderID.String(), EntityType: "order", EntityID: orderID,
		ActorID: actor.ID, ActorName: &name, Metadata: map[string]any{"action": ev.String()},
	})
}

func (f *fakeOrders) ExpireOne(ctx context.Context, orderID uuid.UUID) error {
	pool := ctx.Value(poolKey{}).(*pgxpool.Pool)
	_, err := pool.Exec(ctx, `UPDATE orders SET status = 'cancelled'
		WHERE id = $1 AND status = 'pending_payment' AND reservation_expires_at < now()`, orderID)
	return err
}

func (f *fakeOrders) ExpirePendingReservations(context.Context) error { return nil }

type poolKey struct{}

type env struct {
	t    *testing.T
	ctx  context.Context
	pool *pgxpool.Pool
	mock *mockMP
	svc  *payments.Service
	ord  *fakeOrders
}

func newEnv(t *testing.T) *env {
	pool := testutil.NewPool(t)
	mock := newMockMP(t)
	ord := &fakeOrders{}
	return &env{
		t: t, ctx: context.WithValue(context.Background(), poolKey{}, pool), pool: pool, mock: mock, ord: ord,
		svc: &payments.Service{Pool: pool, Provider: mock.provider(), Orders: ord},
	}
}

// seedOrder inserts a pending Mercado Pago order (hold 30 min ahead).
func (e *env) seedOrder(totalCents int64) uuid.UUID {
	e.t.Helper()
	id := uuid.Must(uuid.NewV7())
	if _, err := e.pool.Exec(e.ctx, `INSERT INTO orders (id, guest_email, status, subtotal_cents, total_cents, payment_provider, reservation_expires_at)
		VALUES ($1, 'cliente@example.com', 'pending_payment', $2, $2, 'mercadopago', now() + interval '30 minutes')`, id, totalCents); err != nil {
		e.t.Fatal(err)
	}
	return id
}

func (e *env) seedCharged() (uuid.UUID, int64) {
	e.t.Helper()
	id := e.seedOrder(12_345)
	v, err := e.svc.EnsureCharge(e.ctx, id)
	if err != nil || v == nil {
		e.t.Fatal(v, err)
	}
	return id, v.PaymentID
}

func (e *env) status(id uuid.UUID) string {
	e.t.Helper()
	var s string
	if err := e.pool.QueryRow(e.ctx, `SELECT status::text FROM orders WHERE id = $1`, id).Scan(&s); err != nil {
		e.t.Fatal(err)
	}
	return s
}

func (e *env) expireHold(id uuid.UUID, ago string) {
	e.t.Helper()
	if _, err := e.pool.Exec(e.ctx, `UPDATE orders SET reservation_expires_at = now() - $2::interval WHERE id = $1`, id, ago); err != nil {
		e.t.Fatal(err)
	}
}

func (e *env) auditCount(id uuid.UUID, actor string) int {
	e.t.Helper()
	var n int
	if err := e.pool.QueryRow(e.ctx, `SELECT count(*) FROM audit_events WHERE entity_id = $1 AND actor_name = $2`, id, actor).Scan(&n); err != nil {
		e.t.Fatal(err)
	}
	return n
}

// --- client: create / retry / breaker ---

func TestCreateChargeUsesOrderAmountAndIdempotencyKey(t *testing.T) {
	e := newEnv(t)
	orderID, paymentID := e.seedCharged()
	var posts []recorded
	for _, r := range e.mock.recorded() {
		if r.Method == http.MethodPost {
			posts = append(posts, r)
		}
	}
	if len(posts) != 1 {
		t.Fatal(posts)
	}
	c := posts[0]
	if c.IdempotencyKey != orderID.String() || c.Body["transaction_amount"] != 123.45 || c.Body["payment_method_id"] != "pix" ||
		c.Body["payer"].(map[string]any)["email"] != "cliente@example.com" || c.Body["external_reference"] != orderID.String() ||
		!strings.HasSuffix(c.Body["date_of_expiration"].(string), "-03:00") {
		t.Fatalf("%+v", c)
	}
	var stored *int64
	var qr, mpStatus *string
	if err := e.pool.QueryRow(e.ctx, `SELECT mp_payment_id, mp_qr_code, mp_status FROM orders WHERE id = $1`, orderID).Scan(&stored, &qr, &mpStatus); err != nil {
		t.Fatal(err)
	}
	if *stored != paymentID || *qr != fmt.Sprintf("00020126MOCKPIX%d", paymentID) || *mpStatus != "pending" {
		t.Fatal(*stored, *qr, *mpStatus)
	}
	// The order page reuses the stored charge: no second POST.
	op, err := e.svc.OrderPayment(e.ctx, orderID)
	if err != nil || !op.MercadoPago || op.Charge == nil || op.Charge.PaymentID != paymentID || op.Charge.QRCodeBase64 == nil || op.Charge.ExpiresLabel == "" {
		t.Fatalf("%+v %v", op, err)
	}
	if e.mock.count(http.MethodPost, "/v1/payments") != 1 {
		t.Fatal("second POST")
	}
}

func TestOrderPaymentRetriesChargeLazilyAndToleratesOutage(t *testing.T) {
	e := newEnv(t)
	id := e.seedOrder(5_000)
	e.mock.force(500, 500, 500)
	op, err := e.svc.OrderPayment(e.ctx, id)
	if err != nil || !op.MercadoPago || op.Charge != nil {
		t.Fatalf("%+v %v", op, err)
	}
	op, err = e.svc.OrderPayment(e.ctx, id)
	if err != nil || op.Charge == nil {
		t.Fatalf("next load creates the charge: %+v %v", op, err)
	}
	// Static PIX orders.
	staticID := uuid.New()
	if _, err := e.pool.Exec(e.ctx, `INSERT INTO orders (id, status, total_cents) VALUES ($1, 'pending_payment', 100)`, staticID); err != nil {
		t.Fatal(err)
	}
	if op, err := e.svc.OrderPayment(e.ctx, staticID); err != nil || op.MercadoPago {
		t.Fatal(op, err)
	}
}

func TestCreateChargeRetries5xxWithSameIdempotencyKey(t *testing.T) {
	e := newEnv(t)
	id := e.seedOrder(5_000)
	e.mock.force(500, 503)
	if v, err := e.svc.EnsureCharge(e.ctx, id); err != nil || v == nil {
		t.Fatal(v, err)
	}
	reqs := e.mock.recorded()
	if len(reqs) != 3 {
		t.Fatal(len(reqs))
	}
	for _, r := range reqs {
		if r.IdempotencyKey != id.String() {
			t.Fatal(r)
		}
	}
}

func TestCreateChargeDoesNotRetry4xx(t *testing.T) {
	e := newEnv(t)
	id := e.seedOrder(5_000)
	e.mock.force(400)
	_, err := e.svc.EnsureCharge(e.ctx, id)
	var re *payments.RejectedError
	if !errors.As(err, &re) || re.Status != 400 {
		t.Fatal(err)
	}
	if len(e.mock.recorded()) != 1 {
		t.Fatal("4xx is final")
	}
	var stored *int64
	if err := e.pool.QueryRow(e.ctx, `SELECT mp_payment_id FROM orders WHERE id = $1`, id).Scan(&stored); err != nil || stored != nil {
		t.Fatal(stored, err)
	}
	if !e.svc.Provider.AcceptingCharges() {
		t.Fatal("a 4xx says nothing about MP's health")
	}
}

func TestExhaustedRetriesOpenTheCircuitAndFailFast(t *testing.T) {
	mock := newMockMP(t)
	p := mock.providerWith(payments.NewCircuitBreaker(1, time.Minute))
	mock.force(500, 502, 504)
	_, err := p.FetchStatus(context.Background(), 1)
	var te *payments.TransientError
	if !errors.As(err, &te) || len(mock.recorded()) != 3 {
		t.Fatal(err, len(mock.recorded()))
	}
	if p.AcceptingCharges() {
		t.Fatal("open circuit")
	}
	_, err = p.FetchStatus(context.Background(), 1)
	if !errors.Is(err, payments.ErrUnavailable) || err.Error() != "Pagamento temporariamente indisponível. Tente novamente em instantes." {
		t.Fatal(err)
	}
	if len(mock.recorded()) != 3 {
		t.Fatal("no HTTP call while open")
	}
}

// --- webhook ---

func (e *env) webhook(dataID, secret string, ts int64, sigHeader string) int {
	e.t.Helper()
	srv := httptest.NewServer(e.svc.WebhookHandler(http.NotFound))
	defer srv.Close()
	return postWebhook(e.t, srv.URL, dataID, secret, ts, sigHeader)
}

func postWebhook(t *testing.T, base, dataID, secret string, ts int64, sigHeader string) int {
	t.Helper()
	requestID := uuid.NewString()
	tss := strconv.FormatInt(ts, 10)
	if sigHeader == "" {
		sigHeader = "ts=" + tss + ",v1=" + payments.Sign(secret, payments.Manifest(dataID, requestID, tss))
	}
	body := fmt.Sprintf(`{"action":"payment.updated","type":"payment","data":{"id":%q}}`, dataID)
	req, err := http.NewRequest(http.MethodPost, base+"/api/webhooks/mercadopago?data.id="+dataID+"&type=payment", bytes.NewBufferString(body)) //nolint:noctx // test
	if err != nil {
		t.Fatal(err)
	}
	req.Header.Set("Origin", "https://www.mercadopago.com.br")
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Request-Id", requestID)
	req.Header.Set("X-Signature", sigHeader)
	res, err := http.DefaultClient.Do(req)
	if err != nil {
		t.Fatal(err)
	}
	_ = res.Body.Close()
	return res.StatusCode
}

func now() int64 { return time.Now().Unix() }

func TestWebhookApprovedMarksPaid(t *testing.T) {
	e := newEnv(t)
	id, pid := e.seedCharged()
	e.mock.setStatus(pid, "approved")
	if code := e.webhook(strconv.FormatInt(pid, 10), mockSecret, now(), ""); code != http.StatusOK {
		t.Fatal(code)
	}
	if e.status(id) != "paid" {
		t.Fatal(e.status(id))
	}
	var mpStatus string
	var paid bool
	if err := e.pool.QueryRow(e.ctx, `SELECT mp_status, paid_at IS NOT NULL FROM orders WHERE id = $1`, id).Scan(&mpStatus, &paid); err != nil {
		t.Fatal(err)
	}
	if mpStatus != "approved" || !paid || e.auditCount(id, "Mercado Pago") != 1 {
		t.Fatal(mpStatus, paid)
	}
}

func TestWebhookRejectsBadAndStaleSignatures(t *testing.T) {
	e := newEnv(t)
	id, pid := e.seedCharged()
	e.mock.setStatus(pid, "approved")
	sid := strconv.FormatInt(pid, 10)
	tss := strconv.FormatInt(now(), 10)
	other := "ts=" + tss + ",v1=" + payments.Sign(mockSecret, payments.Manifest("1", "", tss))
	for name, code := range map[string]int{
		"wrong secret":     e.webhook(sid, "not-the-secret", now(), ""),
		"outside replay":   e.webhook(sid, mockSecret, now()-3600, ""),
		"garbage":          e.webhook(sid, mockSecret, now(), "ts=1,v1=00"),
		"other payment id": e.webhook(sid, mockSecret, now(), other),
	} {
		if code != http.StatusUnauthorized {
			t.Errorf("%s: %d", name, code)
		}
	}
	if e.status(id) != "pending_payment" || e.mock.count(http.MethodGet, "/v1/payments") != 0 {
		t.Fatal("never fetched")
	}
}

func TestWebhookRedeliveryAppliesOnce(t *testing.T) {
	e := newEnv(t)
	id, pid := e.seedCharged()
	e.mock.setStatus(pid, "approved")
	srv := httptest.NewServer(e.svc.WebhookHandler(http.NotFound))
	defer srv.Close()
	sid := strconv.FormatInt(pid, 10)
	var wg sync.WaitGroup
	codes := make([]int, 2)
	for i := range 2 {
		wg.Go(func() { codes[i] = postWebhook(t, srv.URL, sid, mockSecret, now(), "") })
	}
	wg.Wait()
	if codes[0] != http.StatusOK || codes[1] != http.StatusOK || postWebhook(t, srv.URL, sid, mockSecret, now(), "") != http.StatusOK {
		t.Fatal(codes)
	}
	if e.status(id) != "paid" || e.auditCount(id, "Mercado Pago") != 1 {
		t.Fatal("one transition")
	}
}

func TestWebhookRejectedPaymentCancels(t *testing.T) {
	e := newEnv(t)
	id, pid := e.seedCharged()
	e.mock.setStatus(pid, "cancelled")
	if code := e.webhook(strconv.FormatInt(pid, 10), mockSecret, now(), ""); code != http.StatusOK || e.status(id) != "cancelled" {
		t.Fatal(code, e.status(id))
	}
}

func TestWebhookAsksForRedeliveryWhenMPIsDown(t *testing.T) {
	e := newEnv(t)
	_, pid := e.seedCharged()
	e.mock.force(500, 500, 500)
	if code := e.webhook(strconv.FormatInt(pid, 10), mockSecret, now(), ""); code != http.StatusServiceUnavailable {
		t.Fatal(code)
	}
	// Unknown payment at MP (4xx): redelivery cannot help.
	if code := e.webhook("424242", mockSecret, now(), ""); code != http.StatusOK {
		t.Fatal(code)
	}
}

func TestApprovalWithWrongAmountIsNotApplied(t *testing.T) {
	e := newEnv(t)
	id, pid := e.seedCharged()
	e.mock.setStatus(pid, "approved")
	e.mock.setField(pid, "transaction_amount", 1.0)
	out, err := e.svc.SyncPayment(e.ctx, pid, payments.MercadoPagoActor)
	if err != nil || out.Kind != payments.OutcomeMismatch || out.OrderID != id || e.status(id) != "pending_payment" {
		t.Fatal(out, err)
	}
}

func TestPaymentFoundByExternalReferenceWhenIDWasNotStored(t *testing.T) {
	e := newEnv(t)
	id, pid := e.seedCharged()
	if _, err := e.pool.Exec(e.ctx, `UPDATE orders SET mp_payment_id = NULL WHERE id = $1`, id); err != nil {
		t.Fatal(err)
	}
	e.mock.setStatus(pid, "approved")
	out, err := e.svc.SyncPayment(e.ctx, pid, payments.MercadoPagoActor)
	if err != nil || out.Kind != payments.OutcomeApplied || out.To != sqlc.OrderStatusPaid {
		t.Fatal(out, err)
	}
	// A reference to another order is a mismatch.
	e.mock.setField(pid, "external_reference", uuid.NewString())
	if out, _ := e.svc.SyncPayment(e.ctx, pid, payments.MercadoPagoActor); out.Kind != payments.OutcomeMismatch {
		t.Fatal(out)
	}
}

// --- expiry sweep + late approval ---

func TestSweepMarksPaidWhenMPApprovedBeforeExpiryCancel(t *testing.T) {
	e := newEnv(t)
	id, pid := e.seedCharged()
	e.expireHold(id, "1 minute")
	e.mock.setStatus(pid, "approved")
	if err := e.svc.ExpirePendingReservations(e.ctx); err != nil {
		t.Fatal(err)
	}
	if e.status(id) != "paid" {
		t.Fatal(e.status(id))
	}
}

func TestLateApprovalAfterExpiryReopensAndPays(t *testing.T) {
	e := newEnv(t)
	id, pid := e.seedCharged()
	e.expireHold(id, "1 minute")
	if err := e.svc.ExpirePendingReservations(e.ctx); err != nil {
		t.Fatal(err)
	}
	if e.status(id) != "cancelled" {
		t.Fatal(e.status(id))
	}
	e.mock.setStatus(pid, "approved")
	if code := e.webhook(strconv.FormatInt(pid, 10), mockSecret, now(), ""); code != http.StatusOK || e.status(id) != "paid" {
		t.Fatal(code, e.status(id))
	}
}

func TestLateApprovalWithoutStockNeedsAttention(t *testing.T) {
	e := newEnv(t)
	id, pid := e.seedCharged()
	e.expireHold(id, "1 minute")
	if err := e.svc.ExpirePendingReservations(e.ctx); err != nil {
		t.Fatal(err)
	}
	e.ord.noStock = true
	e.mock.setStatus(pid, "approved")
	out, err := e.svc.SyncPayment(e.ctx, pid, payments.MercadoPagoActor)
	if err != nil || out.Kind != payments.OutcomeNeedsAttention || out.OrderID != id || e.status(id) != "cancelled" {
		t.Fatal(out, err)
	}
	info, found, err := e.svc.AdminPaymentInfo(e.ctx, id)
	if err != nil || !found || info.MPStatus == nil || *info.MPStatus != "approved" || !info.Refundable(sqlc.OrderStatusCancelled) {
		t.Fatal("admin can refund it", info, err)
	}
}

func TestSweepDefersCancelWhileMPIsUnreachable(t *testing.T) {
	e := newEnv(t)
	id, _ := e.seedCharged()
	e.expireHold(id, "1 minute")
	e.mock.force(500, 500, 500)
	if err := e.svc.ExpirePendingReservations(e.ctx); err != nil {
		t.Fatal(err)
	}
	if e.status(id) != "pending_payment" {
		t.Fatal(e.status(id))
	}
	e.expireHold(id, "2 hours")
	e.mock.force(500, 500, 500)
	if err := e.svc.ExpirePendingReservations(e.ctx); err != nil {
		t.Fatal(err)
	}
	if e.status(id) != "cancelled" {
		t.Fatal("past the grace window the hold is released")
	}
}

// --- admin ---

func TestAdminRefundCallsMPThenRefundsOrder(t *testing.T) {
	e := newEnv(t)
	id, pid := e.seedCharged()
	e.mock.setStatus(pid, "approved")
	actor := payments.Actor{Name: "Admin Teste"}
	msg, err := e.svc.RunAdminAction(e.ctx, id, payments.AdminSync, actor)
	if err != nil || !strings.Contains(msg, "sincronizado") || e.status(id) != "paid" {
		t.Fatal(msg, err)
	}
	if msg, err := e.svc.RunAdminAction(e.ctx, id, payments.AdminSync, actor); err != nil || !strings.Contains(msg, "em dia") {
		t.Fatal(msg, err)
	}
	if _, err := e.svc.RunAdminAction(e.ctx, id, payments.AdminRefund, actor); err != nil {
		t.Fatal(err)
	}
	if e.mock.count(http.MethodPost, fmt.Sprintf("/v1/payments/%d/refunds", pid)) != 1 || e.status(id) != "refunded" {
		t.Fatal("refund")
	}
	var audited int
	if err := e.pool.QueryRow(e.ctx, `SELECT count(*) FROM audit_events WHERE entity_id = $1 AND metadata->>'action' = 'mp_refund' AND actor_name = 'Admin Teste'`, id).Scan(&audited); err != nil || audited != 1 {
		t.Fatal(audited, err)
	}
	out, err := e.svc.SyncPayment(e.ctx, pid, payments.MercadoPagoActor)
	if err != nil || out.Kind != payments.OutcomeUnchanged {
		t.Fatal("a refund webhook afterwards is a no-op", out, err)
	}
	if _, err := e.svc.RunAdminAction(e.ctx, id, payments.AdminRefund, actor); err == nil || err.Error() != "Este pedido não pode ser estornado." {
		t.Fatal(err)
	}
}

func TestAdminActionErrors(t *testing.T) {
	e := newEnv(t)
	if _, err := e.svc.RunAdminAction(e.ctx, uuid.New(), payments.AdminSync, payments.MercadoPagoActor); err == nil || err.Error() != "Pedido não encontrado." {
		t.Fatal(err)
	}
	id := e.seedOrder(100)
	if _, err := e.svc.RunAdminAction(e.ctx, id, payments.AdminSync, payments.MercadoPagoActor); err == nil || err.Error() != "Pedido sem pagamento no Mercado Pago." {
		t.Fatal(err)
	}
	if a, ok := payments.ParseAdminAction("mp_refund"); !ok || a != payments.AdminRefund {
		t.Fatal("parse")
	}
	if _, ok := payments.ParseAdminAction("paid"); ok {
		t.Fatal("parse")
	}
}

func TestStaticPixProviderHasNoWebhook(t *testing.T) {
	e := newEnv(t)
	e.svc.Provider = payments.StaticPix{}
	if code := e.webhook("1", mockSecret, now(), ""); code != http.StatusNotFound {
		t.Fatal(code)
	}
}
