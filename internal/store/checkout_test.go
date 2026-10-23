package store_test

import (
	"context"
	"errors"
	"fmt"
	"strings"
	"sync"
	"testing"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/payments"
	"github.com/wellington-vell/lindaflor/internal/payments/pix"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// Port of the src/app/store/checkout_service.rs tests.

const (
	unitPrice     int64 = 10000
	discountCents int64 = 1500
)

func validInput() store.CheckoutInput {
	return store.CheckoutInput{
		GuestEmail: "cliente@example.com", Name: "Maria Silva", Phone: "79999816511",
		Street: "Rua das Flores", Number: "10", Neighborhood: "Centro", City: "Aracaju",
		State: "SE", ZipCode: "49000000", Intent: "pay",
	}
}

func TestCheckoutAcceptsValuesAtTheLimit(t *testing.T) {
	in := validInput()
	in.Name = strings.Repeat("a", 100)
	in.Street = strings.Repeat("r", 120)
	in.Complement = strings.Repeat("ç", 80)
	in.Notes = strings.Repeat("n", 500)
	if _, err := store.ValidatePayFields(in); err != nil {
		t.Fatal(err)
	}
}

func TestCheckoutRejectsEachOversizedFieldWithPtBRMessage(t *testing.T) {
	cases := []struct {
		set    func(*store.CheckoutInput, string)
		max    int
		prefix string
	}{
		{func(i *store.CheckoutInput, v string) { i.GuestEmail = v }, 254, "O e-mail deve"},
		{func(i *store.CheckoutInput, v string) { i.Name = v }, 100, "O nome deve"},
		{func(i *store.CheckoutInput, v string) { i.Phone = v }, 20, "O WhatsApp deve"},
		{func(i *store.CheckoutInput, v string) { i.Street = v }, 120, "A rua deve"},
		{func(i *store.CheckoutInput, v string) { i.Number = v }, 20, "O número deve"},
		{func(i *store.CheckoutInput, v string) { i.Complement = v }, 80, "O complemento deve"},
		{func(i *store.CheckoutInput, v string) { i.Neighborhood = v }, 80, "O bairro deve"},
		{func(i *store.CheckoutInput, v string) { i.City = v }, 80, "A cidade deve"},
		{func(i *store.CheckoutInput, v string) { i.Notes = v }, 500, "As observações devem"},
	}
	for _, c := range cases {
		in := validInput()
		c.set(&in, strings.Repeat("x", c.max+1))
		_, err := store.ValidatePayFields(in)
		if got, want := userMsg(t, err), fmt.Sprintf("%s ter no máximo %d caracteres.", c.prefix, c.max); got != want {
			t.Errorf("got %q, want %q", got, want)
		}
	}
}

func checkoutInput(email, code string) store.CheckoutInput {
	return store.CheckoutInput{
		GuestEmail: email, Name: "Maria Silva", Phone: "79999816511", Street: "Rua da Praia",
		Number: "10", Neighborhood: "Centro", City: "Aracaju", State: "SE", ZipCode: "49000-000",
		CouponCode: code, Intent: "pay",
	}
}

func uniqueEmail(prefix string) string {
	return prefix + "-" + strings.ReplaceAll(newID().String(), "-", "") + "@example.com"
}

func createOrder(pool *pgxpool.Pool, cartID uuid.UUID, in store.CheckoutInput) (store.CreatedOrder, error) {
	return store.CreateOrder(ctx, pool, store.StaticPixCheckout, cartID, in, nil)
}

func seedCartWith(t testing.TB, pool *pgxpool.Pool, variantID uuid.UUID, qty int32) uuid.UUID {
	t.Helper()
	cartID := newCart(t, pool)
	addCartLine(t, pool, cartID, variantID, qty)
	return cartID
}

func TestCreateOrderRedeemsUniqueCouponOnce(t *testing.T) {
	pool := testutil.NewPool(t)
	ensurePixSettings(t, pool)
	_, variantID, _ := seedVariant(t, pool, unitPrice, 5)
	couponID := newID()
	code := "Pay" + strings.ReplaceAll(newID().String(), "-", "")
	email := uniqueEmail("checkout")
	exec(t, pool, `INSERT INTO coupons (id, code, discount_type, discount_value, usage_type) VALUES ($1, $2, 'fixed', $3, 'unique')`, couponID, code, discountCents)
	cartID := seedCartWith(t, pool, variantID, 1)
	shipping, _ := store.CalculateShipping(unitPrice, "SE", store.DefaultFreeShippingThresholdCents)
	expectedPix := store.PixDiscountCents(unitPrice-discountCents, 5)
	created, err := createOrder(pool, cartID, checkoutInput(email, strings.ToLower(code)))
	mustNoErr(t, err)
	var savedCoupon *uuid.UUID
	var disc, total, sub, ship, pixDisc int64
	mustNoErr(t, pool.QueryRow(ctx, `SELECT coupon_id, discount_cents, total_cents, subtotal_cents, shipping_cents, pix_discount_cents FROM orders WHERE id = $1`, created.ID).
		Scan(&savedCoupon, &disc, &total, &sub, &ship, &pixDisc))
	if savedCoupon == nil || *savedCoupon != couponID || disc != discountCents || sub != unitPrice || ship != shipping ||
		pixDisc != expectedPix || total != sub-disc-pixDisc+ship || total != unitPrice-discountCents-expectedPix+shipping {
		t.Fatalf("coupon=%v disc=%d total=%d sub=%d ship=%d pix=%d", savedCoupon, disc, total, sub, ship, pixDisc)
	}
	if got := scalar[string](t, pool, `SELECT payment_meta->>'coupon_code' FROM orders WHERE id = $1`, created.ID); got != code {
		t.Fatal(got)
	}
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM coupon_redemptions WHERE coupon_id = $1 AND refunded_at IS NULL`, couponID); n != 1 {
		t.Fatal(n)
	}
	_, err = createOrder(pool, seedCartWith(t, pool, variantID, 1), checkoutInput(email, strings.ToLower(code)))
	if msg := userMsg(t, err); !strings.Contains(msg, "limite de utilizações") {
		t.Fatal(msg)
	}
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM orders WHERE coupon_id = $1`, couponID); n != 1 {
		t.Fatal(n)
	}
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM coupon_redemptions WHERE coupon_id = $1 AND refunded_at IS NULL`, couponID); n != 1 {
		t.Fatal(n)
	}
}

func TestCreateOrderUsesLiveDBPrice(t *testing.T) {
	pool := testutil.NewPool(t)
	ensurePixSettings(t, pool)
	productID, variantID, _ := seedVariant(t, pool, unitPrice, 2)
	cartID := seedCartWith(t, pool, variantID, 1)
	exec(t, pool, `UPDATE products SET price_in_cents = 24000 WHERE id = $1`, productID)
	created, err := createOrder(pool, cartID, checkoutInput(uniqueEmail("price"), ""))
	mustNoErr(t, err)
	if sub := scalar[int64](t, pool, `SELECT subtotal_cents FROM orders WHERE id = $1`, created.ID); sub != 24000 {
		t.Fatal(sub)
	}
	if unit := scalar[int64](t, pool, `SELECT unit_price_cents FROM order_items WHERE order_id = $1`, created.ID); unit != 24000 {
		t.Fatal(unit)
	}
}

func TestCreateOrderQueuesPixMail(t *testing.T) {
	pool := testutil.NewPool(t)
	ensurePixSettings(t, pool)
	_, variantID, _ := seedVariant(t, pool, unitPrice, 2)
	email := uniqueEmail("mail")
	created, err := createOrder(pool, seedCartWith(t, pool, variantID, 1), checkoutInput(email, ""))
	mustNoErr(t, err)
	var kind, text string
	mustNoErr(t, pool.QueryRow(ctx, `SELECT kind, text_body FROM email_outbox WHERE to_address = $1`, email).Scan(&kind, &text))
	if kind != "order_created" {
		t.Fatal(kind)
	}
	if link := fmt.Sprintf("/pedido/%s?t=%s", created.ID, created.AccessToken); !strings.Contains(text, link) {
		t.Fatalf("missing order link %s:\n%s", link, text)
	}
	if !strings.Contains(text, "PIX copia-e-cola") {
		t.Fatal(text)
	}
	if code := scalar[string](t, pool, `SELECT payment_meta->>'pix_copy_paste' FROM orders WHERE id = $1`, created.ID); !strings.HasPrefix(code, "000201") {
		t.Fatal(code)
	}
}

func TestCreateOrderReservesInventory(t *testing.T) {
	pool := testutil.NewPool(t)
	ensurePixSettings(t, pool)
	_, variantID, _ := seedVariant(t, pool, unitPrice, 3)
	created, err := createOrder(pool, seedCartWith(t, pool, variantID, 2), checkoutInput(uniqueEmail("reserve"), ""))
	mustNoErr(t, err)
	if _, r := stockOf(t, pool, variantID); r != 2 {
		t.Fatal(r)
	}
	if !scalar[bool](t, pool, `SELECT reservation_expires_at IS NOT NULL FROM orders WHERE id = $1`, created.ID) {
		t.Fatal("no hold deadline")
	}
	tx := begin(t, pool)
	mustNoErr(t, store.ConvertReservation(ctx, tx, created.ID))
	commit(t, tx)
	if q, r := stockOf(t, pool, variantID); q != 1 || r != 0 {
		t.Fatalf("(%d, %d)", q, r)
	}
}

func TestLastUnitRaceOnlyOneOrderSucceeds(t *testing.T) {
	pool := testutil.NewPool(t)
	ensurePixSettings(t, pool)
	_, variantID, _ := seedVariant(t, pool, unitPrice, 1)
	carts := []uuid.UUID{seedCartWith(t, pool, variantID, 1), seedCartWith(t, pool, variantID, 1)}
	errs := make([]error, 2)
	var wg sync.WaitGroup
	for i := range carts {
		wg.Go(func() {
			_, errs[i] = createOrder(pool, carts[i], checkoutInput(uniqueEmail(fmt.Sprintf("race-%d", i)), ""))
		})
	}
	wg.Wait()
	ok := 0
	loser := uuid.Nil
	for i, err := range errs {
		if err == nil {
			ok++
			continue
		}
		if msg := userMsg(t, err); msg != "Estoque insuficiente para concluir o pedido." {
			t.Fatal(msg)
		}
		loser = carts[i]
	}
	if ok != 1 || loser == uuid.Nil {
		t.Fatalf("ok=%d errs=%v", ok, errs)
	}
	if q := cartQty(t, pool, loser, variantID); q != 1 {
		t.Fatal("loser cart should keep its line")
	}
}

func TestExpiredReservationIsReleased(t *testing.T) {
	pool := testutil.NewPool(t)
	ensurePixSettings(t, pool)
	_, variantID, _ := seedVariant(t, pool, unitPrice, 2)
	created, err := createOrder(pool, seedCartWith(t, pool, variantID, 1), checkoutInput(uniqueEmail("expire"), ""))
	mustNoErr(t, err)
	exec(t, pool, `UPDATE orders SET reservation_expires_at = now() - interval '1 hour' WHERE id = $1`, created.ID)
	mustNoErr(t, store.ExpirePendingReservations(ctx, pool))
	if s := statusOf(t, pool, created.ID); s != "cancelled" {
		t.Fatal(s)
	}
	if _, r := stockOf(t, pool, variantID); r != 0 {
		t.Fatal(r)
	}
}

func TestPayRequiresWhatsapp(t *testing.T) {
	pool := testutil.NewPool(t)
	ensurePixSettings(t, pool)
	_, variantID, _ := seedVariant(t, pool, unitPrice, 2)
	in := checkoutInput(uniqueEmail("phone"), "")
	in.Phone = ""
	_, err := createOrder(pool, seedCartWith(t, pool, variantID, 1), in)
	if msg := userMsg(t, err); !strings.Contains(msg, "WhatsApp") {
		t.Fatal(msg)
	}
}

func TestShippingUsesCEPUFNotPostedState(t *testing.T) {
	pool := testutil.NewPool(t)
	ensurePixSettings(t, pool)
	_, variantID, _ := seedVariant(t, pool, unitPrice, 2)
	cartID := seedCartWith(t, pool, variantID, 1)
	in := checkoutInput(uniqueEmail("uf"), "")
	in.ZipCode = "66010-000"
	_, err := createOrder(pool, cartID, in)
	if msg := userMsg(t, err); !strings.Contains(msg, "PA") {
		t.Fatal(msg)
	}
	in.State = "PA"
	created, err := createOrder(pool, cartID, in)
	mustNoErr(t, err)
	var ship int64
	var state string
	mustNoErr(t, pool.QueryRow(ctx, `SELECT shipping_cents, shipping_address->>'state' FROM orders WHERE id = $1`, created.ID).Scan(&ship, &state))
	if ship != 5990 || state != "PA" {
		t.Fatalf("%d %s", ship, state)
	}
}

func TestCreateOrderFailsWhenPixNotConfigured(t *testing.T) {
	pool := testutil.NewPool(t)
	_, variantID, _ := seedVariant(t, pool, unitPrice, 2)
	_, err := createOrder(pool, seedCartWith(t, pool, variantID, 1), checkoutInput(uniqueEmail("nopix"), ""))
	if msg := userMsg(t, err); msg != pix.UnavailableMessage {
		t.Fatal(msg)
	}
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM orders`); n != 0 {
		t.Fatal(n)
	}
	if _, r := stockOf(t, pool, variantID); r != 0 {
		t.Fatal(r)
	}
	exec(t, pool, `INSERT INTO store_settings (pix_key, pix_key_type) VALUES ('79999816511', NULL)`)
	_, err = createOrder(pool, seedCartWith(t, pool, variantID, 1), checkoutInput(uniqueEmail("nopix"), ""))
	if msg := userMsg(t, err); msg != pix.UnavailableMessage {
		t.Fatal(msg)
	}
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM orders`); n != 0 {
		t.Fatal(n)
	}
}

func TestStoredCartCouponAppliesWhenCheckoutOmitsCode(t *testing.T) {
	pool := testutil.NewPool(t)
	ensurePixSettings(t, pool)
	_, variantID, _ := seedVariant(t, pool, unitPrice, 2)
	couponID := newID()
	code := "CART" + strings.ReplaceAll(newID().String(), "-", "")
	exec(t, pool, `INSERT INTO coupons (id, code, discount_type, discount_value, usage_type) VALUES ($1, $2, 'fixed', $3, 'unlimited')`, couponID, code, discountCents)
	cartID := seedCartWith(t, pool, variantID, 1)
	mustNoErr(t, store.SetCartCoupon(ctx, pool, cartID, code))
	created, err := createOrder(pool, cartID, checkoutInput(uniqueEmail("stored"), ""))
	mustNoErr(t, err)
	if got := scalar[uuid.UUID](t, pool, `SELECT coupon_id FROM orders WHERE id = $1`, created.ID); got != couponID {
		t.Fatal(got)
	}
}

func TestStaleCouponClearsCartInsteadOfHardFail(t *testing.T) {
	pool := testutil.NewPool(t)
	ensurePixSettings(t, pool)
	_, variantID, _ := seedVariant(t, pool, unitPrice, 2)
	cartID := seedCartWith(t, pool, variantID, 1)
	mustNoErr(t, store.SetCartCoupon(ctx, pool, cartID, "GONE-NOW"))
	_, err := createOrder(pool, cartID, checkoutInput(uniqueEmail("stale"), ""))
	if !errors.Is(err, store.ErrCouponRemoved) || !strings.Contains(store.CheckoutErrorMessage(err), "Cupom removido") {
		t.Fatal(err)
	}
	if code, _ := store.CartCouponCode(ctx, pool, cartID); code != "" {
		t.Fatal(code)
	}
}

func TestExpiryRestoresUniqueCoupon(t *testing.T) {
	pool := testutil.NewPool(t)
	ensurePixSettings(t, pool)
	_, variantID, _ := seedVariant(t, pool, unitPrice, 2)
	couponID := newID()
	code := "EXP" + strings.ReplaceAll(newID().String(), "-", "")
	exec(t, pool, `INSERT INTO coupons (id, code, discount_type, discount_value, usage_type) VALUES ($1, $2, 'fixed', $3, 'unique')`, couponID, code, discountCents)
	email := uniqueEmail("expc")
	created, err := createOrder(pool, seedCartWith(t, pool, variantID, 1), checkoutInput(email, code))
	mustNoErr(t, err)
	exec(t, pool, `UPDATE orders SET reservation_expires_at = now() - interval '1 hour' WHERE id = $1`, created.ID)
	mustNoErr(t, store.ExpirePendingReservations(ctx, pool))
	if !scalar[bool](t, pool, `SELECT refunded_at IS NOT NULL FROM coupon_redemptions WHERE order_id = $1`, created.ID) {
		t.Fatal("redemption not restored")
	}
	if _, err := createOrder(pool, seedCartWith(t, pool, variantID, 1), checkoutInput(email, code)); err != nil {
		t.Fatalf("coupon reusable after expiry: %v", err)
	}
}

func seedSplitWarehouses(t *testing.T, pool *pgxpool.Pool, def, other int32) (uuid.UUID, uuid.UUID, uuid.UUID) {
	t.Helper()
	variantID := insertVariant(t, pool, insertProduct(t, pool, "Peça teste", unitPrice, true), "m", "azul")
	a := insertInventory(t, pool, variantID, insertWarehouse(t, pool, true, true), def, 0)
	b := insertInventory(t, pool, variantID, insertWarehouse(t, pool, false, true), other, 0)
	return variantID, a, b
}

func TestReservationsConvertAndReleaseByInventoryID(t *testing.T) {
	pool := testutil.NewPool(t)
	ensurePixSettings(t, pool)
	variantID, a, b := seedSplitWarehouses(t, pool, 1, 1)
	created, err := createOrder(pool, seedCartWith(t, pool, variantID, 2), checkoutInput(uniqueEmail("rows"), ""))
	mustNoErr(t, err)
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM inventory_reservations WHERE order_id = $1 AND inventory_id = ANY($2)`, created.ID, []uuid.UUID{a, b}); n != 2 {
		t.Fatal(n)
	}
	tx := begin(t, pool)
	mustNoErr(t, store.ConvertReservation(ctx, tx, created.ID))
	commit(t, tx)
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM inventory_reservations WHERE order_id = $1`, created.ID); n != 0 {
		t.Fatal(n)
	}
	if qa, _ := invRow(t, pool, a); qa != 0 {
		t.Fatal(qa)
	}
	if qb, _ := invRow(t, pool, b); qb != 0 {
		t.Fatal(qb)
	}

	variant2, ra, rb := seedSplitWarehouses(t, pool, 1, 1)
	order2, err := createOrder(pool, seedCartWith(t, pool, variant2, 2), checkoutInput(uniqueEmail("release"), ""))
	mustNoErr(t, err)
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM inventory_reservations WHERE order_id = $1`, order2.ID); n != 2 {
		t.Fatal(n)
	}
	tx = begin(t, pool)
	mustNoErr(t, store.ReleaseReservation(ctx, tx, order2.ID))
	commit(t, tx)
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM inventory_reservations WHERE order_id = $1`, order2.ID); n != 0 {
		t.Fatal(n)
	}
	if _, r := invRow(t, pool, ra); r != 0 {
		t.Fatal(r)
	}
	if _, r := invRow(t, pool, rb); r != 0 {
		t.Fatal(r)
	}
}

// fakeMP is a Mercado Pago provider double recording CreateCharge calls.
type fakeMP struct {
	mu        sync.Mutex
	accepting bool
	charges   []payments.NewCharge
}

func (f *fakeMP) Kind() config.ProviderKind { return config.ProviderMercadoPago }
func (f *fakeMP) AcceptingCharges() bool    { return f.accepting }
func (f *fakeMP) CreateCharge(_ context.Context, c payments.NewCharge) (payments.Charge, error) {
	f.mu.Lock()
	defer f.mu.Unlock()
	f.charges = append(f.charges, c)
	qr := "00020126MPQR"
	return payments.Charge{PaymentID: 4242 + int64(len(f.charges)), RawStatus: "pending", QRCode: &qr}, nil
}

func (f *fakeMP) FetchStatus(context.Context, int64) (payments.PaymentInfo, error) {
	return payments.PaymentInfo{}, payments.ErrNotSupported
}
func (f *fakeMP) Refund(context.Context, int64) error { return payments.ErrNotSupported }

func TestMercadoPagoCheckoutChargesServerTotal(t *testing.T) {
	pool := testutil.NewPool(t)
	_, variantID, _ := seedVariant(t, pool, unitPrice, 3)
	mp := &fakeMP{accepting: true}
	deps := store.NewCheckoutDeps(pool, mp, store.OrderFSM{Pool: pool})
	created, err := store.CreateOrder(ctx, pool, deps, seedCartWith(t, pool, variantID, 2), checkoutInput(uniqueEmail("mp"), ""), nil)
	mustNoErr(t, err)
	var total int64
	var provider string
	var paymentID *int64
	var static *string
	mustNoErr(t, pool.QueryRow(ctx, `SELECT total_cents, payment_provider, mp_payment_id, payment_meta->>'pix_copy_paste' FROM orders WHERE id = $1`, created.ID).
		Scan(&total, &provider, &paymentID, &static))
	if provider != "mercadopago" || paymentID == nil || static != nil {
		t.Fatalf("%s %v %v", provider, paymentID, static)
	}
	if len(mp.charges) != 1 || mp.charges[0].AmountCents != total || mp.charges[0].OrderID != created.ID {
		t.Fatalf("%+v", mp.charges)
	}
	// The MP order mail never carries a static BR Code.
	body := scalar[string](t, pool, `SELECT text_body FROM email_outbox WHERE kind = 'order_created'`)
	if !strings.Contains(body, "Mercado Pago") || strings.Contains(body, "PIX copia-e-cola:") {
		t.Fatal(body)
	}
}

func TestMercadoPagoCheckoutFailsFastWhenCircuitOpen(t *testing.T) {
	pool := testutil.NewPool(t)
	_, variantID, _ := seedVariant(t, pool, unitPrice, 3)
	mp := &fakeMP{accepting: false}
	deps := store.NewCheckoutDeps(pool, mp, store.OrderFSM{Pool: pool})
	_, err := store.CreateOrder(ctx, pool, deps, seedCartWith(t, pool, variantID, 1), checkoutInput(uniqueEmail("mp-open"), ""), nil)
	if msg := userMsg(t, err); msg != payments.UnavailableMessage {
		t.Fatal(msg)
	}
	if _, r := stockOf(t, pool, variantID); r != 0 {
		t.Fatal("no order, no hold")
	}
	if len(mp.charges) != 0 {
		t.Fatal("no MP call")
	}
	if store.PaymentAvailable(deps, store.DefaultStoreSettings()) {
		t.Fatal("open circuit is not available")
	}
}

func TestHoneypotTripsOnlyWhenFilled(t *testing.T) {
	in := checkoutInput("hp@example.com", "")
	if store.HoneypotTripped(in) {
		t.Fatal("empty")
	}
	in.Website = "   "
	if store.HoneypotTripped(in) {
		t.Fatal("blank")
	}
	in.Website = "http://spam.example"
	if !store.HoneypotTripped(in) || store.HoneypotField != "website" {
		t.Fatal("filled")
	}
}

func refillCart(t *testing.T, pool *pgxpool.Pool, cartID, variantID uuid.UUID) {
	t.Helper()
	addCartLine(t, pool, cartID, variantID, 1)
}

func TestPendingOrdersAreCappedPerEmailAndCart(t *testing.T) {
	pool := testutil.NewPool(t)
	ensurePixSettings(t, pool)
	_, variantID, _ := seedVariant(t, pool, unitPrice, 20)
	local := "cap." + strings.ReplaceAll(newID().String(), "-", "")
	email := local + "@gmail.com"
	cartID := seedCartWith(t, pool, variantID, 1)
	var created []store.CreatedOrder
	for range store.MaxPendingOrders {
		c, err := createOrder(pool, cartID, checkoutInput(email, ""))
		mustNoErr(t, err)
		created = append(created, c)
		refillCart(t, pool, cartID, variantID)
	}
	_, err := createOrder(pool, cartID, checkoutInput(uniqueEmail("other"), ""))
	msg := userMsg(t, err)
	if !strings.Contains(msg, "#"+store.OrderCode(created[2].ID)) || !strings.Contains(msg, "aguardando pagamento") {
		t.Fatal(msg)
	}
	alias := strings.ReplaceAll(local, ".", "") + "+promo@googlemail.com"
	second := seedCartWith(t, pool, variantID, 1)
	if _, err := createOrder(pool, second, checkoutInput(alias, "")); !store.IsUserError(err) {
		t.Fatal(err)
	}
	if _, r := stockOf(t, pool, variantID); r != 3 {
		t.Fatalf("rejected attempts hold no stock: %d", r)
	}
	exec(t, pool, `UPDATE orders SET status = 'cancelled' WHERE id = $1`, created[0].ID)
	if _, err := createOrder(pool, second, checkoutInput(alias, "")); err != nil {
		t.Fatalf("back under the cap: %v", err)
	}
}

func TestCheckoutSummaryAndAddressPrefill(t *testing.T) {
	pool := testutil.NewPool(t)
	ensurePixSettings(t, pool)
	_, variantID, _ := seedVariant(t, pool, unitPrice, 5)
	user := insertUser(t, pool, "Ana")
	addr, err := store.CreateAddress(ctx, pool, user, sampleAddress("Casa"))
	mustNoErr(t, err)
	ref, _, err := store.LoadOrCreateCart(ctx, pool, nil, &user)
	mustNoErr(t, err)
	mustNoErr(t, store.AddItem(ctx, pool, ref.ID, variantID, 1))
	s, err := store.LoadCheckoutSummary(ctx, pool, store.CheckoutPageRequest{Cart: &ref, SessionUserID: &user, Deps: store.StaticPixCheckout})
	mustNoErr(t, err)
	if s == nil || s.Prefill == nil || *s.SelectedAddressID != addr || s.PreviewUF != "SE" || s.ShippingCents != 1990 {
		t.Fatalf("%+v", s)
	}
	if s.TotalCents != unitPrice-store.PixDiscountCents(unitPrice, 5)+1990 || !s.PixAvailable {
		t.Fatalf("%+v", s)
	}
	posted := checkoutInput(uniqueEmail("sum"), "NOPE")
	s, err = store.LoadCheckoutSummary(ctx, pool, store.CheckoutPageRequest{Cart: &ref, Posted: &posted, Deps: store.StaticPixCheckout})
	mustNoErr(t, err)
	if s.CouponError != store.CouponGoneMessage || s.Prefill != nil {
		t.Fatalf("%+v", s)
	}
	if got := scalar[string](t, pool, `SELECT contact_email FROM carts WHERE id = $1`, ref.ID); got != strings.ToLower(posted.GuestEmail) {
		t.Fatal(got)
	}
	empty := store.CartRef{ID: newCart(t, pool)}
	if s, err := store.LoadCheckoutSummary(ctx, pool, store.CheckoutPageRequest{Cart: &empty}); err != nil || s != nil {
		t.Fatal("empty cart must render the empty page")
	}
}
