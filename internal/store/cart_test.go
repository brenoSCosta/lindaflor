package store_test

import (
	"errors"
	"slices"
	"strings"
	"testing"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// Port of the src/app/store/cart.rs tests.

func TestCartTokenIsRandomV4(t *testing.T) {
	pool := testutil.NewPool(t)
	ref, write, err := store.LoadOrCreateCart(ctx, pool, nil, nil)
	mustNoErr(t, err)
	if !write || ref.Token.Version() != 4 || ref.Token == ref.ID {
		t.Fatalf("%+v write=%v", ref, write)
	}
}

func cartQty(t *testing.T, q store.DBTX, cartID, variantID uuid.UUID) int32 {
	t.Helper()
	return scalar[int32](t, q, `SELECT quantity FROM cart_items WHERE cart_id = $1 AND variant_id = $2`, cartID, variantID)
}

func TestAddItemMergesSameVariantAndCapsStock(t *testing.T) {
	pool := testutil.NewPool(t)
	_, variantID, _ := seedVariant(t, pool, 10000, 3)
	cartID := newCart(t, pool)
	for _, q := range []int32{1, 1, 5} {
		mustNoErr(t, store.AddItem(ctx, pool, cartID, variantID, q))
	}
	if q := cartQty(t, pool, cartID, variantID); q != 3 {
		t.Fatal(q)
	}
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM cart_items WHERE cart_id = $1`, cartID); n != 1 {
		t.Fatal(n)
	}
}

func TestHydrateDropsInactiveProduct(t *testing.T) {
	pool := testutil.NewPool(t)
	productID, variantID, _ := seedVariant(t, pool, 10000, 4)
	cartID := newCart(t, pool)
	mustNoErr(t, store.AddItem(ctx, pool, cartID, variantID, 2))
	exec(t, pool, `UPDATE products SET active = false WHERE id = $1`, productID)
	h, err := store.HydrateCart(ctx, pool, cartID)
	mustNoErr(t, err)
	if len(h.Items) != 0 || len(h.Notices) == 0 || !h.Notices[0].Removed {
		t.Fatalf("%+v", h)
	}
	if !strings.Contains(h.Notices[0].Message(), "foi removido do carrinho (indisponível).") {
		t.Fatal(h.Notices[0].Message())
	}
}

func TestCartsAreIsolatedByToken(t *testing.T) {
	pool := testutil.NewPool(t)
	_, variantID, _ := seedVariant(t, pool, 10000, 4)
	a, b := newCart(t, pool), newCart(t, pool)
	mustNoErr(t, store.AddItem(ctx, pool, a, variantID, 1))
	hb, err := store.HydrateCart(ctx, pool, b)
	mustNoErr(t, err)
	ha, err := store.HydrateCart(ctx, pool, a)
	mustNoErr(t, err)
	if len(hb.Items) != 0 || len(ha.Items) != 1 {
		t.Fatalf("a=%d b=%d", len(ha.Items), len(hb.Items))
	}
}

func TestLoginMergeSumsGuestIntoUserCartAndCapsStock(t *testing.T) {
	pool := testutil.NewPool(t)
	_, variantID, _ := seedVariant(t, pool, 10000, 3)
	userID := insertUser(t, pool, "Cliente")
	guest := newCart(t, pool)
	userCart := newCart(t, pool)
	exec(t, pool, `UPDATE carts SET user_id = $2 WHERE id = $1`, userCart, userID)
	mustNoErr(t, store.AddItem(ctx, pool, guest, variantID, 2))
	mustNoErr(t, store.AddItem(ctx, pool, userCart, variantID, 2))
	guestToken := scalar[uuid.UUID](t, pool, `SELECT token FROM carts WHERE id = $1`, guest)
	token, err := store.MergeGuestCart(ctx, pool, &guestToken, userID)
	mustNoErr(t, err)
	if surviving := scalar[uuid.UUID](t, pool, `SELECT token FROM carts WHERE user_id = $1`, userID); token != surviving {
		t.Fatal("wrong surviving token")
	}
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM carts WHERE id = $1`, guest); n != 0 {
		t.Fatal("guest cart not deleted")
	}
	if q := cartQty(t, pool, userCart, variantID); q != 3 {
		t.Fatal(q)
	}
}

func TestMergeWithoutGuestEnsuresUserCartAndAttachesGuest(t *testing.T) {
	pool := testutil.NewPool(t)
	_, variantID, _ := seedVariant(t, pool, 10000, 3)
	userID := insertUser(t, pool, "Cliente")
	token, err := store.MergeGuestCart(ctx, pool, nil, userID)
	mustNoErr(t, err)
	if got := scalar[uuid.UUID](t, pool, `SELECT token FROM carts WHERE user_id = $1`, userID); got != token {
		t.Fatal("user cart not created")
	}
	other := insertUser(t, pool, "Outra")
	guest := newCart(t, pool)
	mustNoErr(t, store.AddItem(ctx, pool, guest, variantID, 1))
	guestToken := scalar[uuid.UUID](t, pool, `SELECT token FROM carts WHERE id = $1`, guest)
	tok, err := store.MergeGuestCart(ctx, pool, &guestToken, other)
	mustNoErr(t, err)
	if tok != guestToken || scalar[uuid.UUID](t, pool, `SELECT user_id FROM carts WHERE id = $1`, guest) != other {
		t.Fatal("guest cart must be attached to the user")
	}
	// A cart owned by someone else is never merged.
	tok, err = store.MergeGuestCart(ctx, pool, &guestToken, userID)
	mustNoErr(t, err)
	if tok != token {
		t.Fatal("foreign cart merged")
	}
}

func TestFindAndLoadCartRespectOwnership(t *testing.T) {
	pool := testutil.NewPool(t)
	owner := insertUser(t, pool, "Dona")
	intruder := insertUser(t, pool, "Outra")
	ref, write, err := store.LoadOrCreateCart(ctx, pool, nil, &owner)
	mustNoErr(t, err)
	if !write {
		t.Fatal("new cart must set the cookie")
	}
	found, write, err := store.FindCart(ctx, pool, &ref.Token, &owner)
	mustNoErr(t, err)
	if found == nil || found.ID != ref.ID || write {
		t.Fatalf("%+v %v", found, write)
	}
	// Someone else's cookie: not theirs.
	found, _, err = store.FindCart(ctx, pool, &ref.Token, &intruder)
	mustNoErr(t, err)
	if found != nil {
		t.Fatal("foreign cart returned")
	}
	// No cookie: the user's cart is found and the cookie rewritten.
	found, write, err = store.FindCart(ctx, pool, nil, &owner)
	mustNoErr(t, err)
	if found == nil || !write {
		t.Fatal("user cart not found")
	}
	// Anonymous without cookie: nothing is created.
	found, _, err = store.FindCart(ctx, pool, nil, nil)
	mustNoErr(t, err)
	if found != nil || scalar[int64](t, pool, `SELECT COUNT(*) FROM carts`) != 1 {
		t.Fatal("FindCart must not create carts")
	}
}

func TestCartCouponPersistsUntilCleared(t *testing.T) {
	pool := testutil.NewPool(t)
	cartID := newCart(t, pool)
	mustNoErr(t, store.SetCartCoupon(ctx, pool, cartID, "CUPOM10"))
	if code, _ := store.CartCouponCode(ctx, pool, cartID); code != "CUPOM10" {
		t.Fatal(code)
	}
	mustNoErr(t, store.SetCartCoupon(ctx, pool, cartID, ""))
	if code, _ := store.CartCouponCode(ctx, pool, cartID); code != "" {
		t.Fatal(code)
	}
}

func seedSetVariant(t testing.TB, pool *pgxpool.Pool, price int64, stock int32) uuid.UUID {
	t.Helper()
	productID := insertProduct(t, pool, "Peça conjunto", price, true)
	variantID := insertVariant(t, pool, productID, "m", "branco")
	insertInventory(t, pool, variantID, insertWarehouse(t, pool, true, true), stock, 0)
	return variantID
}

func seedSetWithVariants(t testing.TB, pool *pgxpool.Pool, name string, price *int64, members []uuid.UUID) uuid.UUID {
	t.Helper()
	setID := newID()
	exec(t, pool, `INSERT INTO sets (id, name, slug, active, price_in_cents) VALUES ($1, $2, $3, true, $4)`, setID, name, "conj-"+setID.String(), price)
	for i, v := range members {
		slotID := newID()
		exec(t, pool, `INSERT INTO set_slots (id, set_id, sort_order) VALUES ($1, $2, $3)`, slotID, setID, int32(i)) //nolint:gosec // tiny
		exec(t, pool, `INSERT INTO set_slot_options (slot_id, variant_id) VALUES ($1, $2)`, slotID, v)
	}
	return setID
}

func seedSet(t testing.TB, pool *pgxpool.Pool, price *int64, memberPrices []int64, stocks []int32) (uuid.UUID, []uuid.UUID) {
	t.Helper()
	var variants []uuid.UUID
	for i := range memberPrices {
		variants = append(variants, seedSetVariant(t, pool, memberPrices[i], stocks[i]))
	}
	return seedSetWithVariants(t, pool, "Conjunto teste", price, variants), variants
}

func price(c int64) *int64 { return &c }

func quoteTotal(items []store.CartItem) int64 {
	var total int64
	for _, it := range items {
		total += it.LineTotalCents()
	}
	return total
}

func quote(t *testing.T, pool *pgxpool.Pool, cartID uuid.UUID) ([]store.CartItem, error) {
	t.Helper()
	tx := begin(t, pool)
	defer func() { _ = tx.Rollback(ctx) }()
	return store.QuoteCartForCheckout(ctx, tx, cartID)
}

func TestAddSetInsertsOneRowPerSlotClampedToMinStock(t *testing.T) {
	pool := testutil.NewPool(t)
	setID, variants := seedSet(t, pool, price(10000), []int64{6000, 7000}, []int32{2, 5})
	cartID := newCart(t, pool)
	mustNoErr(t, store.AddSetToCart(ctx, pool, cartID, setID, 9))
	rows, err := pool.Query(ctx, `SELECT variant_id, quantity, set_id FROM cart_items WHERE cart_id = $1`, cartID)
	mustNoErr(t, err)
	defer rows.Close()
	n := 0
	for rows.Next() {
		var v, s uuid.UUID
		var q int32
		mustNoErr(t, rows.Scan(&v, &q, &s))
		if !slices.Contains(variants, v) || q != 2 || s != setID {
			t.Fatalf("%v %d %v", v, q, s)
		}
		n++
	}
	if n != 2 {
		t.Fatal(n)
	}
}

func TestAddSetRejectsMissingPriceAndInactive(t *testing.T) {
	pool := testutil.NewPool(t)
	priceless, _ := seedSet(t, pool, nil, []int64{6000}, []int32{3})
	cartID := newCart(t, pool)
	if err := store.AddSetToCart(ctx, pool, cartID, priceless, 1); !errors.Is(err, store.ErrInvalidVariant) {
		t.Fatal(err)
	}
	setID, _ := seedSet(t, pool, price(9000), []int64{6000}, []int32{3})
	exec(t, pool, `UPDATE sets SET active = false WHERE id = $1`, setID)
	if err := store.AddSetToCart(ctx, pool, cartID, setID, 1); !errors.Is(err, store.ErrInvalidVariant) {
		t.Fatal(err)
	}
	empty, _ := seedSet(t, pool, price(9000), []int64{6000}, []int32{0})
	if err := store.AddSetToCart(ctx, pool, cartID, empty, 1); !errors.Is(err, store.ErrOutOfStock) {
		t.Fatal(err)
	}
}

func TestQuoteSplitsSetPriceWithFirstSlotRemainder(t *testing.T) {
	pool := testutil.NewPool(t)
	setID, _ := seedSet(t, pool, price(10001), []int64{6000, 7000}, []int32{4, 4})
	cartID := newCart(t, pool)
	mustNoErr(t, store.AddSetToCart(ctx, pool, cartID, setID, 1))
	items, err := quote(t, pool, cartID)
	mustNoErr(t, err)
	if len(items) != 2 {
		t.Fatal(len(items))
	}
	units := []int64{items[0].UnitPriceCents, items[1].UnitPriceCents}
	slices.Sort(units)
	if units[0] != 5000 || units[1] != 5001 || quoteTotal(items) != 10001 {
		t.Fatalf("%v", units)
	}
	for _, it := range items {
		if it.SetID == nil || *it.SetID != setID || !strings.Contains(it.VariantLabel, "parte do conjunto Conjunto teste") {
			t.Fatalf("%+v", it)
		}
	}
}

func TestQuotePartialSetAfterMemberRemoveIsInsufficient(t *testing.T) {
	pool := testutil.NewPool(t)
	setID, variants := seedSet(t, pool, price(10000), []int64{6000, 7000}, []int32{4, 4})
	cartID := newCart(t, pool)
	mustNoErr(t, store.AddSetToCart(ctx, pool, cartID, setID, 1))
	exec(t, pool, `DELETE FROM cart_items WHERE cart_id = $1 AND set_id = $2 AND variant_id = $3`, cartID, setID, variants[0])
	if _, err := quote(t, pool, cartID); !errors.Is(err, store.ErrInsufficientStock) {
		t.Fatal(err)
	}
}

func TestSetGroupQuantityChangeAppliesToAllMembers(t *testing.T) {
	pool := testutil.NewPool(t)
	setID, _ := seedSet(t, pool, price(10000), []int64{6000, 7000}, []int32{5, 5})
	cartID := newCart(t, pool)
	mustNoErr(t, store.AddSetToCart(ctx, pool, cartID, setID, 1))
	mustNoErr(t, store.SetSetQuantity(ctx, pool, cartID, setID, 3))
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM cart_items WHERE cart_id = $1 AND set_id = $2 AND quantity = 3`, cartID, setID); n != 2 {
		t.Fatal(n)
	}
	items, err := quote(t, pool, cartID)
	mustNoErr(t, err)
	if quoteTotal(items) != 30000 {
		t.Fatal(quoteTotal(items))
	}
}

func TestHydrateDropsWholeSetWhenAMemberSellsOut(t *testing.T) {
	pool := testutil.NewPool(t)
	setID, variants := seedSet(t, pool, price(10000), []int64{6000, 7000}, []int32{5, 5})
	cartID := newCart(t, pool)
	mustNoErr(t, store.AddSetToCart(ctx, pool, cartID, setID, 2))
	exec(t, pool, `UPDATE inventory SET quantity = 0, reserved = 0 WHERE variant_id = $1`, variants[0])
	h, err := store.HydrateCart(ctx, pool, cartID)
	mustNoErr(t, err)
	if len(h.Items) != 0 {
		t.Fatal(len(h.Items))
	}
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM cart_items WHERE cart_id = $1 AND set_id = $2`, cartID, setID); n != 0 {
		t.Fatal(n)
	}
}

func TestHydrateLowersSetGroupToSmallestMember(t *testing.T) {
	pool := testutil.NewPool(t)
	setID, variants := seedSet(t, pool, price(10000), []int64{6000, 7000}, []int32{5, 5})
	cartID := newCart(t, pool)
	mustNoErr(t, store.AddSetToCart(ctx, pool, cartID, setID, 3))
	exec(t, pool, `UPDATE inventory SET quantity = 1, reserved = 0 WHERE variant_id = $1`, variants[1])
	h, err := store.HydrateCart(ctx, pool, cartID)
	mustNoErr(t, err)
	if len(h.Items) != 2 || h.Items[0].Quantity != 1 || h.Items[1].Quantity != 1 {
		t.Fatalf("%+v", h.Items)
	}
	items, err := quote(t, pool, cartID)
	mustNoErr(t, err)
	if quoteTotal(items) != 10000 {
		t.Fatal(quoteTotal(items))
	}
}

func TestLooseUnitOfSetVariantIsASeparateFullPriceLine(t *testing.T) {
	pool := testutil.NewPool(t)
	setID, variants := seedSet(t, pool, price(10000), []int64{8000, 7000}, []int32{5, 5})
	cartID := newCart(t, pool)
	mustNoErr(t, store.AddSetToCart(ctx, pool, cartID, setID, 1))
	mustNoErr(t, store.AddItem(ctx, pool, cartID, variants[0], 1))
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM cart_items WHERE cart_id = $1`, cartID); n != 3 {
		t.Fatal(n)
	}
	if v := scalar[uuid.UUID](t, pool, `SELECT variant_id FROM cart_items WHERE cart_id = $1 AND set_id IS NULL`, cartID); v != variants[0] {
		t.Fatal("loose line variant")
	}
	items, err := quote(t, pool, cartID)
	mustNoErr(t, err)
	if len(items) != 3 {
		t.Fatal(len(items))
	}
	for _, it := range items {
		if it.SetID == nil && it.UnitPriceCents != 8000 {
			t.Fatal(it.UnitPriceCents)
		}
	}
	if quoteTotal(items) != 18000 {
		t.Fatal(quoteTotal(items))
	}
}

func TestQuoteRejectsMismatchedMemberQuantities(t *testing.T) {
	pool := testutil.NewPool(t)
	setID, variants := seedSet(t, pool, price(10000), []int64{6000, 7000}, []int32{5, 5})
	cartID := newCart(t, pool)
	mustNoErr(t, store.AddSetToCart(ctx, pool, cartID, setID, 1))
	exec(t, pool, `UPDATE cart_items SET quantity = 4 WHERE cart_id = $1 AND set_id = $2 AND variant_id = $3`, cartID, setID, variants[0])
	if _, err := quote(t, pool, cartID); !errors.Is(err, store.ErrInsufficientStock) {
		t.Fatal(err)
	}
}

func TestQuoteSumsSharedVariantAcrossLooseAndSetLines(t *testing.T) {
	pool := testutil.NewPool(t)
	setID, variants := seedSet(t, pool, price(10000), []int64{8000, 7000}, []int32{3, 5})
	cartID := newCart(t, pool)
	mustNoErr(t, store.AddSetToCart(ctx, pool, cartID, setID, 2))
	mustNoErr(t, store.AddItem(ctx, pool, cartID, variants[0], 2))
	if _, err := quote(t, pool, cartID); !errors.Is(err, store.ErrInsufficientStock) {
		t.Fatal(err)
	}
}

func TestTwoSetsSharingAVariantCoexistInCart(t *testing.T) {
	pool := testutil.NewPool(t)
	shared := seedSetVariant(t, pool, 8000, 6)
	a := seedSetWithVariants(t, pool, "Conjunto A", price(10000), []uuid.UUID{shared, seedSetVariant(t, pool, 7000, 6)})
	b := seedSetWithVariants(t, pool, "Conjunto B", price(12000), []uuid.UUID{shared, seedSetVariant(t, pool, 6000, 6)})
	cartID := newCart(t, pool)
	mustNoErr(t, store.AddSetToCart(ctx, pool, cartID, a, 1))
	mustNoErr(t, store.AddSetToCart(ctx, pool, cartID, b, 1))
	if n := scalar[int64](t, pool, `SELECT COUNT(*) FROM cart_items WHERE cart_id = $1`, cartID); n != 4 {
		t.Fatal(n)
	}
	items, err := quote(t, pool, cartID)
	mustNoErr(t, err)
	if len(items) != 4 || quoteTotal(items) != 22000 {
		t.Fatalf("%d %d", len(items), quoteTotal(items))
	}
}

func TestCouponApplyBudgetIsPerCart(t *testing.T) {
	ratelimit.Reset()
	t.Cleanup(ratelimit.Reset)
	cart, other := newID(), newID()
	budget := config.RateLimitFor(ratelimit.ScopeCouponCart, ratelimit.DefaultCouponCartPerMin)
	for range budget {
		if !store.CheckCouponCartRate(cart) {
			t.Fatal("budget exhausted early")
		}
	}
	if store.CheckCouponCartRate(cart) {
		t.Fatal("budget not exhausted")
	}
	if !store.CheckCouponCartRate(other) {
		t.Fatal("other carts must be unaffected")
	}
}

func TestSetQuantityRemovesAndClamps(t *testing.T) {
	pool := testutil.NewPool(t)
	productID, variantID, _ := seedVariant(t, pool, 1000, 3)
	cartID := newCart(t, pool)
	mustNoErr(t, store.SetQuantity(ctx, pool, cartID, variantID, 9))
	if q := cartQty(t, pool, cartID, variantID); q != 3 {
		t.Fatal(q)
	}
	ref := store.CartRef{ID: cartID, Token: scalar[uuid.UUID](t, pool, `SELECT token FROM carts WHERE id = $1`, cartID)}
	if n, _ := store.CartItemCount(ctx, pool, &ref.Token); n != 3 {
		t.Fatal(n)
	}
	exec(t, pool, `UPDATE products SET active = false WHERE id = $1`, productID)
	if err := store.SetQuantity(ctx, pool, cartID, variantID, 1); !errors.Is(err, store.ErrOutOfStock) {
		t.Fatal(err)
	}
	if n, _ := store.CartItemCount(ctx, pool, &ref.Token); n != 0 {
		t.Fatal(n)
	}
}

func TestApplyCartCoupon(t *testing.T) {
	ratelimit.Reset()
	t.Cleanup(ratelimit.Reset)
	pool := testutil.NewPool(t)
	_, variantID, _ := seedVariant(t, pool, 10000, 3)
	cartID := newCart(t, pool)
	mustNoErr(t, store.AddItem(ctx, pool, cartID, variantID, 1))
	exec(t, pool, `INSERT INTO coupons (id, code, discount_type, discount_value, min_subtotal_cents) VALUES ($1, 'Verao', 'fixed', 500, 50000)`, newID())
	msg, err := store.ApplyCartCoupon(ctx, pool, cartID, nil, "nope")
	mustNoErr(t, err)
	if msg != store.CouponGoneMessage {
		t.Fatal(msg)
	}
	// Guests without an email can't use per-user coupons... but the minimum
	// is checked first.
	msg, err = store.ApplyCartCoupon(ctx, pool, cartID, nil, "verao")
	mustNoErr(t, err)
	if !strings.Contains(msg, "compra mínima") {
		t.Fatal(msg)
	}
	exec(t, pool, `UPDATE coupons SET min_subtotal_cents = 0`)
	user := insertUser(t, pool, "Ana")
	msg, err = store.ApplyCartCoupon(ctx, pool, cartID, &user, "verao")
	mustNoErr(t, err)
	if msg != "" {
		t.Fatal(msg)
	}
	if code, _ := store.CartCouponCode(ctx, pool, cartID); code != "Verao" {
		t.Fatal(code)
	}
	msg, err = store.ApplyCartCoupon(ctx, pool, cartID, &user, "  ")
	mustNoErr(t, err)
	if code, _ := store.CartCouponCode(ctx, pool, cartID); msg != "" || code != "" {
		t.Fatal("blank code must clear the coupon")
	}
}
