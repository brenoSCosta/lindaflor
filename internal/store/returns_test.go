package store_test

import (
	"encoding/json"
	"errors"
	"strings"
	"testing"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// Port of src/app/store/returns/tests.rs.

func daysAgo(d int) *time.Time {
	t := time.Now().Add(-time.Duration(d) * 24 * time.Hour)
	return &t
}

func TestWindowCountsFromDelivery(t *testing.T) {
	now := time.Now()
	for _, k := range []store.ReturnKind{store.ReturnExchange, store.ReturnReturn} {
		if _, err := store.CheckReturnEligibility("delivered", k, daysAgo(6), daysAgo(10), now); err != nil {
			t.Fatal(err)
		}
		if _, err := store.CheckReturnEligibility("delivered", k, daysAgo(8), daysAgo(10), now); !errors.Is(err, store.ErrReturnExpired) {
			t.Fatal(err)
		}
	}
	delivered := now.Add(-store.ReturnWindowDays * 24 * time.Hour)
	if d, err := store.CheckReturnEligibility("delivered", store.ReturnReturn, &delivered, nil, now); err != nil || !d.Equal(now) {
		t.Fatalf("exactly on the deadline: %v %v", d, err)
	}
	if _, err := store.CheckReturnEligibility("delivered", store.ReturnDefect, daysAgo(60), nil, now); err != nil {
		t.Fatal(err)
	}
	if _, err := store.CheckReturnEligibility("delivered", store.ReturnDefect, daysAgo(91), nil, now); !errors.Is(err, store.ErrReturnExpired) {
		t.Fatal(err)
	}
}

func TestWindowWithoutDeliveryUsesShippingGrace(t *testing.T) {
	now := time.Now()
	limit := store.UnconfirmedDeliveryGraceDays + store.ReturnWindowDays
	if _, err := store.CheckReturnEligibility("shipped", store.ReturnExchange, nil, daysAgo(1), now); err != nil {
		t.Fatal(err)
	}
	if _, err := store.CheckReturnEligibility("shipped", store.ReturnExchange, nil, daysAgo(limit-1), now); err != nil {
		t.Fatal(err)
	}
	if _, err := store.CheckReturnEligibility("shipped", store.ReturnExchange, nil, daysAgo(limit+1), now); !errors.Is(err, store.ErrReturnExpired) {
		t.Fatal(err)
	}
	if _, ok := store.ReturnDeadline(store.ReturnReturn, nil, nil); ok {
		t.Fatal("no deadline before shipping")
	}
}

func TestWindowNeedsAShippedOrder(t *testing.T) {
	now := time.Now()
	for _, s := range []string{"pending_payment", "paid", "processing"} {
		if _, err := store.CheckReturnEligibility(s, store.ReturnReturn, nil, nil, now); !errors.Is(err, store.ErrReturnNotShipped) {
			t.Fatal(s, err)
		}
	}
	for _, s := range []string{"cancelled", "refunded"} {
		if _, err := store.CheckReturnEligibility(s, store.ReturnReturn, daysAgo(1), nil, now); !errors.Is(err, store.ErrReturnExpired) {
			t.Fatal(s, err)
		}
	}
}

func TestAdminTransitionTable(t *testing.T) {
	k := store.ReturnReturn
	type tc struct {
		from   store.ReturnStatus
		kind   store.ReturnKind
		action store.ReturnActionKind
		want   store.ReturnStatus // "" = illegal
	}
	cases := []tc{
		{store.ReturnRequested, k, store.ReturnActionApprove, store.ReturnApproved},
		{store.ReturnApproved, k, store.ReturnActionApprove, ""},
		{store.ReturnRequested, k, store.ReturnActionReceive, ""},
		{store.ReturnApproved, k, store.ReturnActionReceive, store.ReturnReceived},
		{store.ReturnApproved, k, store.ReturnActionRefundOrder, ""},
		{store.ReturnReceived, k, store.ReturnActionRefundOrder, store.ReturnResolved},
		{store.ReturnReceived, k, store.ReturnActionRecordRefund, store.ReturnResolved},
		{store.ReturnReceived, store.ReturnExchange, store.ReturnActionRefundOrder, ""},
		{store.ReturnReceived, k, store.ReturnActionResolveSwap, ""},
		{store.ReturnReceived, store.ReturnDefect, store.ReturnActionResolveSwap, store.ReturnResolved},
		{store.ReturnResolved, k, store.ReturnActionNote, store.ReturnResolved},
	}
	for _, from := range []store.ReturnStatus{store.ReturnRequested, store.ReturnApproved, store.ReturnReceived} {
		cases = append(cases, tc{from, k, store.ReturnActionReject, store.ReturnRejected})
	}
	for _, from := range []store.ReturnStatus{store.ReturnResolved, store.ReturnRejected} {
		cases = append(cases, tc{from, k, store.ReturnActionReject, ""}, tc{from, k, store.ReturnActionRefundOrder, ""})
	}
	for _, c := range cases {
		got, ok := store.AdminReturnTransition(c.from, c.kind, c.action)
		if (c.want == "") == ok || got != c.want {
			t.Errorf("%s/%s/%s = %q %v, want %q", c.from, c.kind, c.action, got, ok, c.want)
		}
	}
}

type returnFixture struct {
	orderID, token, owner uuid.UUID
	items                 [2][2]uuid.UUID // (order_item_id, variant_id): line 1 has 2 units, line 2 has 1
}

// seedReturnOrder: a delivered order (deliveredSQL) with two lines and 5
// units of stock per variant.
func seedReturnOrder(t *testing.T, pool *pgxpool.Pool, provider, deliveredSQL string) returnFixture {
	t.Helper()
	f := returnFixture{orderID: newID(), token: uuid.New(), owner: insertUser(t, pool, "Cliente")}
	wh := insertWarehouse(t, pool, true, true)
	exec(t, pool, `INSERT INTO orders (id, user_id, guest_email, status, access_token, subtotal_cents, total_cents, payment_provider, paid_at, shipped_at, delivered_at)
		VALUES ($1, $2, 'cliente@example.com', 'delivered', $3, 5000, 5000, $4, now() - interval '20 days', now() - interval '12 days', `+deliveredSQL+`)`,
		f.orderID, f.owner, f.token, provider)
	for i, line := range []struct {
		qty   int32
		price int64
	}{{2, 1500}, {1, 2000}} {
		variantID := insertVariant(t, pool, insertProduct(t, pool, "Biquíni", line.price, true), "m", "azul")
		insertInventory(t, pool, variantID, wh, 5, 0)
		itemID := newID()
		exec(t, pool, `INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
			VALUES ($1, $2, $3, 'Biquíni', 'M / azul', $4, $5)`, itemID, f.orderID, variantID, line.qty, line.price)
		f.items[i] = [2]uuid.UUID{itemID, variantID}
	}
	return f
}

func returnRequest(kind store.ReturnKind, lines ...store.ReturnLine) store.NewReturn {
	return store.NewReturn{Kind: kind, Reason: "Ficou pequeno", DesiredSize: str("G"), Lines: lines}
}

func line(id uuid.UUID, qty int32) store.ReturnLine { return store.ReturnLine{OrderItemID: id, Quantity: qty} }

func returnStatusOf(t *testing.T, pool *pgxpool.Pool, id uuid.UUID) (string, *string, *int64) {
	t.Helper()
	var s string
	var res *string
	var cents *int64
	mustNoErr(t, pool.QueryRow(ctx, `SELECT status, resolution, refund_cents FROM return_requests WHERE id = $1`, id).Scan(&s, &res, &cents))
	return s, res, cents
}

func returnMailKinds(t *testing.T, pool *pgxpool.Pool, id uuid.UUID) string {
	t.Helper()
	rows, err := pool.Query(ctx, `SELECT kind FROM email_outbox WHERE dedupe_key LIKE $1 ORDER BY created_at, id`, "return:"+id.String()+":%")
	mustNoErr(t, err)
	defer rows.Close()
	var out []string
	for rows.Next() {
		var k string
		mustNoErr(t, rows.Scan(&k))
		out = append(out, k)
	}
	return strings.Join(out, ",")
}

var adminActor = store.NamedActor("Admin")

func TestOnlyOwnerOrTokenHolderCanOpen(t *testing.T) {
	pool := testutil.NewPool(t)
	f := seedReturnOrder(t, pool, "static_pix", "now() - interval '2 days'")
	stranger := insertUser(t, pool, "Outra")
	wrong := uuid.New().String()
	for _, c := range []struct {
		token *string
		user  *uuid.UUID
	}{{nil, &stranger}, {&wrong, nil}, {&wrong, &stranger}, {nil, nil}} {
		if _, err := store.CreateReturnRequest(ctx, pool, f.orderID, c.token, c.user, returnRequest(store.ReturnExchange, line(f.items[0][0], 1))); !errors.Is(err, store.ErrReturnNotFound) {
			t.Fatal(err)
		}
	}
	if n := scalar[int64](t, pool, `SELECT count(*) FROM return_requests WHERE order_id = $1`, f.orderID); n != 0 {
		t.Fatal(n)
	}
	exec(t, pool, `UPDATE orders SET access_token_expires_at = now() - interval '1 minute' WHERE id = $1`, f.orderID)
	tok := f.token.String()
	if _, err := store.CreateReturnRequest(ctx, pool, f.orderID, &tok, nil, returnRequest(store.ReturnExchange, line(f.items[0][0], 1))); !errors.Is(err, store.ErrReturnNotFound) {
		t.Fatal(err)
	}
	id, err := store.CreateReturnRequest(ctx, pool, f.orderID, nil, &f.owner, returnRequest(store.ReturnExchange, line(f.items[0][0], 1)))
	mustNoErr(t, err)
	if s, _, _ := returnStatusOf(t, pool, id); s != "requested" {
		t.Fatal(s)
	}
	if got := returnMailKinds(t, pool, id); got != "return_requested" {
		t.Fatal(got)
	}
	if got := scalar[string](t, pool, `SELECT metadata->>'action' FROM audit_events WHERE aggregate_type = 'return' AND aggregate_id = $1`, id.String()); got != "create" {
		t.Fatal(got)
	}
}

func TestGuestTokenOpensAndCapsApply(t *testing.T) {
	pool := testutil.NewPool(t)
	f := seedReturnOrder(t, pool, "static_pix", "now() - interval '2 days'")
	tok := f.token.String()
	for _, lines := range [][]store.ReturnLine{
		{line(f.items[0][0], 3)}, {line(newID(), 1)}, {line(f.items[0][0], 0)}, {line(f.items[0][0], -1)},
	} {
		if _, err := store.CreateReturnRequest(ctx, pool, f.orderID, &tok, nil, returnRequest(store.ReturnReturn, lines...)); !store.IsUserError(err) {
			t.Fatal(lines, err)
		}
	}
	blank := returnRequest(store.ReturnReturn, line(f.items[0][0], 1))
	blank.Reason = "   "
	if _, err := store.CreateReturnRequest(ctx, pool, f.orderID, &tok, nil, blank); !store.IsUserError(err) {
		t.Fatal(err)
	}
	first, err := store.CreateReturnRequest(ctx, pool, f.orderID, &tok, nil, returnRequest(store.ReturnReturn, line(f.items[0][0], 1)))
	mustNoErr(t, err)
	if _, err := store.CreateReturnRequest(ctx, pool, f.orderID, &tok, nil, returnRequest(store.ReturnReturn, line(f.items[1][0], 1))); !errors.Is(err, store.ErrReturnAlreadyOpen) {
		t.Fatal(err)
	}
	page, err := store.LoadOrderReturns(ctx, pool, f.orderID)
	mustNoErr(t, err)
	if !page.Visible || page.CanOpen() {
		t.Fatalf("%+v", page)
	}
	_, err = store.AdminApplyReturn(ctx, pool, first, store.AdminReturnAction{Kind: store.ReturnActionReject, Text: "Peça usada"}, adminActor)
	mustNoErr(t, err)
	items, err := store.ReturnableItems(ctx, pool, f.orderID)
	mustNoErr(t, err)
	if items[0].Returnable != 2 {
		t.Fatal(items[0].Returnable)
	}
	if page, _ := store.LoadOrderReturns(ctx, pool, f.orderID); !page.CanOpen() {
		t.Fatal("can open again after rejection")
	}
}

func TestWindowIsEnforcedOnCreate(t *testing.T) {
	pool := testutil.NewPool(t)
	f := seedReturnOrder(t, pool, "static_pix", "now() - interval '8 days'")
	tok := f.token.String()
	if _, err := store.CreateReturnRequest(ctx, pool, f.orderID, &tok, nil, returnRequest(store.ReturnExchange, line(f.items[0][0], 1))); !errors.Is(err, store.ErrReturnExpired) {
		t.Fatal(err)
	}
	if _, err := store.CreateReturnRequest(ctx, pool, f.orderID, &tok, nil, returnRequest(store.ReturnDefect, line(f.items[0][0], 1))); err != nil {
		t.Fatalf("defect within 90 days: %v", err)
	}
	g := seedReturnOrder(t, pool, "static_pix", "NULL")
	exec(t, pool, `UPDATE orders SET status = 'paid', shipped_at = NULL WHERE id = $1`, g.orderID)
	gtok := g.token.String()
	if _, err := store.CreateReturnRequest(ctx, pool, g.orderID, &gtok, nil, returnRequest(store.ReturnReturn, line(g.items[0][0], 1))); !errors.Is(err, store.ErrReturnNotShipped) {
		t.Fatal(err)
	}
	if page, _ := store.LoadOrderReturns(ctx, pool, g.orderID); page.Visible {
		t.Fatal("not visible before shipping")
	}
}

func openReceived(t *testing.T, pool *pgxpool.Pool, f returnFixture, kind store.ReturnKind, lines ...store.ReturnLine) uuid.UUID {
	t.Helper()
	tok := f.token.String()
	id, err := store.CreateReturnRequest(ctx, pool, f.orderID, &tok, nil, returnRequest(kind, lines...))
	mustNoErr(t, err)
	_, err = store.AdminApplyReturn(ctx, pool, id, store.AdminReturnAction{Kind: store.ReturnActionApprove, Text: "Código de postagem 123"}, adminActor)
	mustNoErr(t, err)
	_, err = store.AdminApplyReturn(ctx, pool, id, store.AdminReturnAction{Kind: store.ReturnActionReceive}, adminActor)
	mustNoErr(t, err)
	return id
}

func TestAdminTransitionsAreAuditedAndMailed(t *testing.T) {
	pool := testutil.NewPool(t)
	f := seedReturnOrder(t, pool, "static_pix", "now() - interval '1 day'")
	id, err := store.CreateReturnRequest(ctx, pool, f.orderID, nil, &f.owner, returnRequest(store.ReturnExchange, line(f.items[0][0], 1)))
	mustNoErr(t, err)
	if _, err := store.AdminApplyReturn(ctx, pool, id, store.AdminReturnAction{Kind: store.ReturnActionReceive}, store.Actor{}); !errors.Is(err, store.ErrReturnIllegalTransition) {
		t.Fatal(err)
	}
	if _, err := store.AdminApplyReturn(ctx, pool, id, store.AdminReturnAction{Kind: store.ReturnActionApprove, Text: " "}, store.Actor{}); !store.IsUserError(err) {
		t.Fatal(err)
	}
	if s, _, _ := returnStatusOf(t, pool, id); s != "requested" {
		t.Fatal(s)
	}
	admin := insertUser(t, pool, "Admin")
	actor := store.UserActor(admin, "Admin")
	for _, a := range []store.AdminReturnAction{
		{Kind: store.ReturnActionNote, Text: "Cliente ligou"},
		{Kind: store.ReturnActionApprove, Text: "Código de postagem AB123"},
		{Kind: store.ReturnActionReceive},
		{Kind: store.ReturnActionResolveSwap},
	} {
		_, err := store.AdminApplyReturn(ctx, pool, id, a, actor)
		mustNoErr(t, err)
	}
	if s, res, cents := returnStatusOf(t, pool, id); s != "resolved" || res == nil || *res != "exchanged" || cents != nil {
		t.Fatal(s, res, cents)
	}
	if _, err := store.AdminApplyReturn(ctx, pool, id, store.AdminReturnAction{Kind: store.ReturnActionReject, Text: "x"}, store.Actor{}); !errors.Is(err, store.ErrReturnIllegalTransition) {
		t.Fatal(err)
	}
	rows, err := pool.Query(ctx, `SELECT metadata->>'action', actor_user_id, metadata FROM audit_events
		WHERE aggregate_type = 'return' AND aggregate_id = $1 ORDER BY occurred_at, id`, id.String())
	mustNoErr(t, err)
	var actions []string
	var actors []*uuid.UUID
	var metas []map[string]any
	for rows.Next() {
		var a string
		var who *uuid.UUID
		var raw []byte
		mustNoErr(t, rows.Scan(&a, &who, &raw))
		m := map[string]any{}
		mustNoErr(t, json.Unmarshal(raw, &m))
		actions, actors, metas = append(actions, a), append(actors, who), append(metas, m)
	}
	rows.Close()
	if strings.Join(actions, ",") != "create,note,approve,receive,resolve_exchanged" {
		t.Fatal(actions)
	}
	if actors[1] == nil || *actors[1] != admin || metas[1]["note"] != "Cliente ligou" || metas[2]["from"] != "requested" || metas[2]["to"] != "approved" {
		t.Fatalf("%v %v", actors, metas)
	}
	if got := returnMailKinds(t, pool, id); got != "return_requested,return_approved,return_resolved" {
		t.Fatal(got)
	}
	approved := scalar[string](t, pool, `SELECT text_body FROM email_outbox WHERE dedupe_key = $1`, "return:"+id.String()+":approved")
	if !strings.Contains(approved, "AB123") || !strings.Contains(approved, "/pedido/"+f.orderID.String()+"?t=") {
		t.Fatal(approved)
	}
	detail, err := store.LoadAdminReturn(ctx, pool, id)
	mustNoErr(t, err)
	if len(detail.Audit) != 5 || detail.Audit[1].Label != "Observação" || detail.Audit[1].Detail != "Cliente ligou" {
		t.Fatalf("%+v", detail.Audit)
	}
}

func TestRejectMailsTheReason(t *testing.T) {
	pool := testutil.NewPool(t)
	f := seedReturnOrder(t, pool, "static_pix", "now() - interval '1 day'")
	id := openReceived(t, pool, f, store.ReturnReturn, line(f.items[1][0], 1))
	_, err := store.AdminApplyReturn(ctx, pool, id, store.AdminReturnAction{Kind: store.ReturnActionReject, Text: "Peça com sinais de uso"}, adminActor)
	mustNoErr(t, err)
	if s, _, _ := returnStatusOf(t, pool, id); s != "rejected" {
		t.Fatal(s)
	}
	body := scalar[string](t, pool, `SELECT text_body FROM email_outbox WHERE dedupe_key = $1`, "return:"+id.String()+":rejected")
	if !strings.Contains(body, "Peça com sinais de uso") {
		t.Fatal(body)
	}
}

func TestFullReturnRefundsTheOrderWithRestock(t *testing.T) {
	pool := testutil.NewPool(t)
	f := seedReturnOrder(t, pool, "static_pix", "now() - interval '1 day'")
	id := openReceived(t, pool, f, store.ReturnReturn, line(f.items[0][0], 2), line(f.items[1][0], 1))
	opts, err := store.LoadRefundOptions(ctx, pool, id)
	mustNoErr(t, err)
	if !opts.FullOrder || opts.MercadoPago {
		t.Fatalf("%+v", opts)
	}
	if _, err := store.AdminApplyReturn(ctx, pool, id, store.AdminReturnAction{Kind: store.ReturnActionRecordRefund, RefundCents: 100}, store.Actor{}); !store.IsUserError(err) {
		t.Fatal(err)
	}
	_, err = store.AdminApplyReturn(ctx, pool, id, store.AdminReturnAction{Kind: store.ReturnActionRefundOrder, Restock: true}, adminActor)
	mustNoErr(t, err)
	if s, res, cents := returnStatusOf(t, pool, id); s != "resolved" || *res != "refunded" || *cents != 5000 {
		t.Fatal(s, *res, *cents)
	}
	if s := statusOf(t, pool, f.orderID); s != "refunded" {
		t.Fatal(s)
	}
	if q, _ := stockOf(t, pool, f.items[0][1]); q != 7 {
		t.Fatal(q)
	}
	if q, _ := stockOf(t, pool, f.items[1][1]); q != 6 {
		t.Fatal(q)
	}
	var raw []byte
	mustNoErr(t, pool.QueryRow(ctx, `SELECT metadata FROM audit_events WHERE aggregate_type = 'order' AND aggregate_id = $1`, f.orderID.String()).Scan(&raw))
	var meta map[string]any
	mustNoErr(t, json.Unmarshal(raw, &meta))
	if meta["action"] != "refund" || meta["restock"] != true {
		t.Fatal(meta)
	}
}

func TestPartialReturnRecordsRefundAndRestocksOnlyReturnedUnits(t *testing.T) {
	pool := testutil.NewPool(t)
	f := seedReturnOrder(t, pool, "static_pix", "now() - interval '1 day'")
	id := openReceived(t, pool, f, store.ReturnDefect, line(f.items[0][0], 1))
	opts, err := store.LoadRefundOptions(ctx, pool, id)
	mustNoErr(t, err)
	if opts.FullOrder || opts.MaxManualCents != 1500 {
		t.Fatalf("%+v", opts)
	}
	if _, err := store.AdminApplyReturn(ctx, pool, id, store.AdminReturnAction{Kind: store.ReturnActionRefundOrder, Restock: true}, store.Actor{}); !store.IsUserError(err) {
		t.Fatal(err)
	}
	for _, cents := range []int64{0, 1501} {
		if _, err := store.AdminApplyReturn(ctx, pool, id, store.AdminReturnAction{Kind: store.ReturnActionRecordRefund, RefundCents: cents, Restock: true}, store.Actor{}); !store.IsUserError(err) {
			t.Fatal(cents, err)
		}
	}
	_, err = store.AdminApplyReturn(ctx, pool, id, store.AdminReturnAction{Kind: store.ReturnActionRecordRefund, RefundCents: 1500, Restock: true}, adminActor)
	mustNoErr(t, err)
	if s, res, cents := returnStatusOf(t, pool, id); s != "resolved" || *res != "refunded" || *cents != 1500 {
		t.Fatal(s)
	}
	if q, _ := stockOf(t, pool, f.items[0][1]); q != 6 {
		t.Fatal(q)
	}
	if q, _ := stockOf(t, pool, f.items[1][1]); q != 5 {
		t.Fatal(q)
	}
	if s := statusOf(t, pool, f.orderID); s != "delivered" {
		t.Fatal(s)
	}
}

func TestMercadoPagoOrdersNeedThePSPRefund(t *testing.T) {
	pool := testutil.NewPool(t)
	f := seedReturnOrder(t, pool, "mercadopago", "now() - interval '1 day'")
	id := openReceived(t, pool, f, store.ReturnReturn, line(f.items[0][0], 2), line(f.items[1][0], 1))
	if _, err := store.AdminApplyReturn(ctx, pool, id, store.AdminReturnAction{Kind: store.ReturnActionRefundOrder}, store.Actor{}); !store.IsUserError(err) {
		t.Fatal(err)
	}
	orderID, err := store.CheckMPRefund(ctx, pool, id)
	mustNoErr(t, err)
	if orderID != f.orderID {
		t.Fatal(orderID)
	}
	if err := store.FinishMPRefund(ctx, pool, id, true, adminActor); !store.IsUserError(err) {
		t.Fatal(err)
	}
	tx := begin(t, pool)
	apply(t, tx, f.orderID, store.EventRefund, store.ApplyOptions{})
	commit(t, tx)
	mustNoErr(t, store.FinishMPRefund(ctx, pool, id, true, adminActor))
	if _, res, _ := returnStatusOf(t, pool, id); res == nil || *res != "refunded" {
		t.Fatal(res)
	}
	if q, _ := stockOf(t, pool, f.items[0][1]); q != 7 {
		t.Fatal(q)
	}
}

func TestParseReturnForms(t *testing.T) {
	a, err := store.ParseReturnForm("record_refund", "", "15,00", true)
	if err != nil || a.Local == nil || a.Local.RefundCents != 1500 || !a.Local.Restock {
		t.Fatal(a, err)
	}
	if a, _ := store.ParseReturnForm("mp_refund", "", "", true); !a.MPRefund || !a.MPRestock {
		t.Fatal(a)
	}
	if _, err := store.ParseReturnForm("bogus", "", "", false); !store.IsUserError(err) {
		t.Fatal(err)
	}
	item := newID()
	tok, req := store.ParseCustomerReturnForm(store.QueryPairs("t=abc&kind=return&reason=Ficou+grande&desired_size=G&qty_" + item.String() + "=2&qty_junk=1"))
	if tok == nil || *tok != "abc" || req == nil || req.Kind != store.ReturnReturn || req.Reason != "Ficou grande" || len(req.Lines) != 1 || req.Lines[0].Quantity != 2 {
		t.Fatalf("%v %+v", tok, req)
	}
	if _, req := store.ParseCustomerReturnForm(store.QueryPairs("kind=nope")); req != nil {
		t.Fatal("invalid kind must ask for the type")
	}
}

func TestAdminReturnListFilters(t *testing.T) {
	pool := testutil.NewPool(t)
	f := seedReturnOrder(t, pool, "static_pix", "now() - interval '1 day'")
	id := openReceived(t, pool, f, store.ReturnReturn, line(f.items[0][0], 1))
	open, err := store.ListAdminReturns(ctx, pool, "")
	mustNoErr(t, err)
	if len(open) != 1 || open[0].ID != id || open[0].Units != 1 || open[0].StatusLabel() != "Peça recebida" {
		t.Fatalf("%+v", open)
	}
	if rows, _ := store.ListAdminReturns(ctx, pool, "resolved"); len(rows) != 0 {
		t.Fatal(rows)
	}
	if store.ClampReturnFilter("junk") != "abertas" {
		t.Fatal("filter")
	}
}
