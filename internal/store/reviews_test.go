package store_test

import (
	"errors"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// reviewOrder inserts an order of the user with one line of the variant;
// shippedDaysAgo nil leaves shipped_at NULL.
func reviewOrder(t testing.TB, q store.DBTX, userID, variantID uuid.UUID, status string, shippedDaysAgo *int32) uuid.UUID {
	t.Helper()
	id := newID()
	exec(t, q, `INSERT INTO orders (id, user_id, status, total_cents, shipped_at)
		VALUES ($1, $2, $3::order_status, 15990, now() - make_interval(days => $4))`, id, userID, status, shippedDaysAgo)
	exec(t, q, `INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
		VALUES ($1, $2, $3, 'Biquíni Avaliado', 'M · Coral', 1, 15990)`, newID(), id, variantID)
	return id
}

func shippedDays(n int32) *int32 { return &n }

// Port of tests/reviews.rs only_received_orders_make_a_verified_buyer.
func TestOnlyReceivedOrdersMakeAVerifiedBuyer(t *testing.T) {
	pool := testutil.NewPool(t)
	productID, variantID, _ := seedVariant(t, pool, 15990, 5)
	review := func(title string) store.NewReview {
		r, err := store.ParseNewReview("5", title, "Muito bom", nil)
		mustNoErr(t, err)
		return r
	}

	// No order at all.
	stranger := insertUser(t, pool, "Stranger")
	if _, err := store.SubmitReview(ctx, pool, stranger, productID, review("Oi")); !errors.Is(err, store.ErrReviewNotEligible) {
		t.Fatalf("stranger: %v", err)
	}

	// Paid (not shipped), recently shipped and cancelled orders are not received.
	early := insertUser(t, pool, "Early")
	reviewOrder(t, pool, early, variantID, "paid", nil)
	reviewOrder(t, pool, early, variantID, "shipped", shippedDays(2))
	reviewOrder(t, pool, early, variantID, "cancelled", nil)
	if _, err := store.SubmitReview(ctx, pool, early, productID, review("Cedo")); !errors.Is(err, store.ErrReviewNotEligible) {
		t.Fatalf("early: %v", err)
	}

	// Delivered: accepted once, as pending, linked to the order.
	buyer := insertUser(t, pool, "Buyer")
	orderID := reviewOrder(t, pool, buyer, variantID, "delivered", shippedDays(5))
	id, err := store.SubmitReview(ctx, pool, buyer, productID, review("Amei"))
	mustNoErr(t, err)
	if got := scalar[string](t, pool, `SELECT status::text FROM product_reviews WHERE id = $1`, id); got != "pending" {
		t.Fatalf("status = %q", got)
	}
	if got := scalar[uuid.UUID](t, pool, `SELECT order_id FROM product_reviews WHERE id = $1`, id); got != orderID {
		t.Fatalf("order_id = %s, want %s", got, orderID)
	}
	if _, err := store.SubmitReview(ctx, pool, buyer, productID, review("De novo")); !errors.Is(err, store.ErrAlreadyReviewed) {
		t.Fatalf("second review: %v", err)
	}

	// Shipped long enough ago counts as received (delivery rarely marked).
	late := insertUser(t, pool, "Late")
	reviewOrder(t, pool, late, variantID, "shipped", shippedDays(30))
	if _, err := store.SubmitReview(ctx, pool, late, productID, review("Chegou")); err != nil {
		t.Fatalf("shipped 30 days ago may review: %v", err)
	}

	if n := scalar[int64](t, pool, `SELECT count(*) FROM product_reviews WHERE product_id = $1`, productID); n != 2 {
		t.Fatalf("review count = %d", n)
	}
}
