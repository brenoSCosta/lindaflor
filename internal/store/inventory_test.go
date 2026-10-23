package store_test

import (
	"errors"
	"fmt"
	"strings"
	"sync"
	"testing"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/db"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// Port of the src/app/store/inventory.rs tests.

// seedOrderItemWithoutReservation: order line over a variant with
// (quantity, reserved) and no inventory_reservations rows (legacy path).
func seedOrderItemWithoutReservation(t *testing.T, pool *pgxpool.Pool, qty, reserved, itemQty int32) (uuid.UUID, uuid.UUID) {
	t.Helper()
	productID := insertProduct(t, pool, "T", 1000, true)
	variantID := insertVariant(t, pool, productID, "m", "azul")
	inv := insertInventory(t, pool, variantID, insertWarehouse(t, pool, true, true), qty, reserved)
	orderID := insertPendingOrder(t, pool, store.ReserveLine{VariantID: variantID, Quantity: itemQty})
	return orderID, inv
}

func invRow(t *testing.T, q store.DBTX, inventoryID uuid.UUID) (int32, int32) {
	t.Helper()
	var qty, res int32
	mustNoErr(t, q.QueryRow(ctx, `SELECT quantity, reserved FROM inventory WHERE id = $1`, inventoryID).Scan(&qty, &res))
	return qty, res
}

func TestFallbackReleaseTwiceDoesNotGoNegative(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, inv := seedOrderItemWithoutReservation(t, pool, 5, 1, 1)
	tx := begin(t, pool)
	mustNoErr(t, store.ReleaseReservation(ctx, tx, orderID))
	mustNoErr(t, store.ReleaseReservation(ctx, tx, orderID))
	if q, r := invRow(t, tx, inv); q != 5 || r != 0 {
		t.Fatalf("(%d, %d)", q, r)
	}
}

func TestFallbackConvertTwiceDoesNotGoNegative(t *testing.T) {
	pool := testutil.NewPool(t)
	orderID, inv := seedOrderItemWithoutReservation(t, pool, 5, 1, 1)
	tx := begin(t, pool)
	mustNoErr(t, store.ConvertReservation(ctx, tx, orderID))
	mustNoErr(t, store.ConvertReservation(ctx, tx, orderID))
	if q, r := invRow(t, tx, inv); q != 4 || r != 0 {
		t.Fatalf("(%d, %d)", q, r)
	}
}

// seedVariantTwoRows: one variant with a default and a secondary active
// warehouse row of perRow units each.
func seedVariantTwoRows(t testing.TB, pool *pgxpool.Pool, perRow int32) uuid.UUID {
	t.Helper()
	variantID := insertVariant(t, pool, insertProduct(t, pool, "T", 1000, true), "m", "azul")
	for _, def := range []bool{true, false} {
		insertInventory(t, pool, variantID, insertWarehouse(t, pool, def, true), perRow, 0)
	}
	return variantID
}

func seedReservedOrder(t testing.TB, pool *pgxpool.Pool, lines []store.ReserveLine) uuid.UUID {
	t.Helper()
	orderID := insertPendingOrder(t, pool, lines...)
	err := db.WithTx(ctx, pool, func(tx pgx.Tx) error { return store.ReserveForOrder(ctx, tx, orderID, lines) })
	if err != nil {
		t.Fatal(err)
	}
	return orderID
}

func expireHold(t testing.TB, q store.DBTX, orderID uuid.UUID) {
	t.Helper()
	exec(t, q, `UPDATE orders SET reservation_expires_at = now() - interval '1 minute' WHERE id = $1`, orderID)
}

func seedCartLines(t testing.TB, pool *pgxpool.Pool, lines []store.ReserveLine) uuid.UUID {
	t.Helper()
	cartID := newCart(t, pool)
	for _, l := range lines {
		addCartLine(t, pool, cartID, l.VariantID, l.Quantity)
	}
	return cartID
}

// checkoutStyle mirrors checkout's stock steps: lock the cart's rows
// (reclaiming expired holds), then reserve for a new order.
func checkoutStyle(pool *pgxpool.Pool, orderID, cartID uuid.UUID, lines []store.ReserveLine) error {
	return db.WithTx(ctx, pool, func(tx pgx.Tx) error {
		if err := store.LockCartInventory(ctx, tx, cartID); err != nil {
			return err
		}
		return store.ReserveForOrder(ctx, tx, orderID, lines)
	})
}

func liveAvailable(t testing.TB, q store.DBTX, variantID uuid.UUID) int64 {
	t.Helper()
	return scalar[int64](t, q, `SELECT COALESCE(SUM(live_available), 0)::bigint FROM inventory_live WHERE variant_id = $1`, variantID)
}

func TestExpiredUnsweptHoldDoesNotBlockAvailability(t *testing.T) {
	pool := testutil.NewPool(t)
	variantID := seedVariantTwoRows(t, pool, 1)
	lines := []store.ReserveLine{{VariantID: variantID, Quantity: 2}}
	stale := seedReservedOrder(t, pool, lines)
	if got := liveAvailable(t, pool, variantID); got != 0 {
		t.Fatal(got)
	}
	expireHold(t, pool, stale)
	if raw := scalar[int64](t, pool, `SELECT SUM(quantity - reserved)::bigint FROM inventory WHERE variant_id = $1`, variantID); raw != 0 {
		t.Fatalf("raw counter %d", raw)
	}
	if got := liveAvailable(t, pool, variantID); got != 2 {
		t.Fatalf("live %d", got)
	}
	cartID := newCart(t, pool)
	if err := store.AddItem(ctx, pool, cartID, variantID, 2); err != nil {
		t.Fatalf("expired hold must not block add to cart: %v", err)
	}
	fresh := insertPendingOrder(t, pool, lines...)
	if err := checkoutStyle(pool, fresh, cartID, lines); err != nil {
		t.Fatalf("checkout reclaims expired hold: %v", err)
	}
	if s := statusOf(t, pool, stale); s != "cancelled" {
		t.Fatal(s)
	}
	if s := statusOf(t, pool, fresh); s != "pending_payment" {
		t.Fatal(s)
	}
	if held := scalar[int64](t, pool, `SELECT COALESCE(SUM(quantity), 0)::bigint FROM inventory_reservations WHERE order_id = $1`, fresh); held != 2 {
		t.Fatal(held)
	}
	if got := liveAvailable(t, pool, variantID); got != 0 {
		t.Fatal(got)
	}
}

type stockOp int

const (
	opPay stockOp = iota
	opCancel
	opPayRefund
	opSweep
	opCheckout
)

func runEvent(pool *pgxpool.Pool, orderID uuid.UUID, ev store.OrderEvent) error {
	return db.WithTx(ctx, pool, func(tx pgx.Tx) error {
		_, err := store.Apply(ctx, tx, orderID, ev, store.ApplyOptions{})
		return err
	})
}

func TestConcurrentPayCancelCheckoutDoNotDeadlock(t *testing.T) {
	pool := testutil.NewPool(t)
	const perRow = 500
	var variants []uuid.UUID
	for range 4 {
		variants = append(variants, seedVariantTwoRows(t, pool, perRow))
	}
	linesFor := func(k int) []store.ReserveLine {
		var lines []store.ReserveLine
		for j := range 3 {
			lines = append(lines, store.ReserveLine{VariantID: variants[(k+j)%len(variants)], Quantity: int32(1 + j)}) //nolint:gosec // tiny
		}
		if k%2 == 1 {
			for i, j := 0, len(lines)-1; i < j; i, j = i+1, j-1 {
				lines[i], lines[j] = lines[j], lines[i]
			}
		}
		return lines
	}
	type job struct {
		op     stockOp
		target uuid.UUID
		order  uuid.UUID // checkout: the order to reserve for
		lines  []store.ReserveLine
	}
	var jobs []job
	for k := range 40 {
		lines := linesFor(k)
		op := stockOp(k % 5)
		j := job{op: op, lines: lines}
		if op == opCheckout {
			j.target = seedCartLines(t, pool, lines)
			j.order = insertPendingOrder(t, pool, lines...)
		} else {
			j.target = seedReservedOrder(t, pool, lines)
			if op == opSweep {
				// Expired: sweeps and checkout reclaims race to cancel it.
				expireHold(t, pool, j.target)
			}
		}
		jobs = append(jobs, j)
	}
	var wg sync.WaitGroup
	errs := make(chan error, len(jobs))
	for _, j := range jobs {
		wg.Go(func() {
			var err error
			switch j.op {
			case opPay:
				err = runEvent(pool, j.target, store.EventPay)
			case opCancel:
				err = runEvent(pool, j.target, store.EventCancel)
			case opPayRefund:
				if err = runEvent(pool, j.target, store.EventPay); err == nil {
					err = runEvent(pool, j.target, store.EventRefund)
				}
			case opSweep:
				err = store.ExpirePendingReservations(ctx, pool)
			case opCheckout:
				err = checkoutStyle(pool, j.order, j.target, j.lines)
			}
			if err != nil {
				errs <- fmt.Errorf("op %d: %w", j.op, err)
			}
		})
	}
	wg.Wait()
	close(errs)
	for err := range errs {
		if strings.Contains(err.Error(), "deadlock") {
			t.Fatalf("deadlock (40P01) under concurrent stock ops: %v", err)
		}
		t.Fatalf("stock op failed: %v", err)
	}
	// Settle any expired order a racing sweep skipped while it was locked.
	mustNoErr(t, store.ExpirePendingReservations(ctx, pool))

	sold := map[uuid.UUID]int32{}
	held := map[uuid.UUID]int32{}
	for _, j := range jobs {
		if j.op != opCheckout {
			want := map[stockOp]string{opPay: "paid", opPayRefund: "refunded"}[j.op]
			if want == "" {
				want = "cancelled"
			}
			if s := statusOf(t, pool, j.target); s != want {
				t.Errorf("op %d: status %s, want %s", j.op, s, want)
			}
		}
		for _, l := range j.lines {
			switch j.op {
			case opPay:
				sold[l.VariantID] += l.Quantity
			case opCheckout:
				held[l.VariantID] += l.Quantity
			}
		}
	}
	for _, v := range variants {
		q, r := stockOf(t, pool, v)
		if q != 2*perRow-sold[v] {
			t.Errorf("on hand %d, want %d", q, 2*perRow-sold[v])
		}
		if r != held[v] {
			t.Errorf("reserved %d, want %d", r, held[v])
		}
	}
	drift := scalar[int64](t, pool, `SELECT COUNT(*) FROM inventory i
		WHERE i.reserved <> COALESCE((SELECT SUM(r.quantity) FROM inventory_reservations r WHERE r.inventory_id = i.id), 0)`)
	if drift != 0 {
		t.Fatalf("%d inventory rows drifted from their reservations", drift)
	}
}

func TestReserveTakesDefaultWarehouseFirstAndRefusesOversell(t *testing.T) {
	pool := testutil.NewPool(t)
	variantID := seedVariantTwoRows(t, pool, 1)
	orderID := insertPendingOrder(t, pool)
	tx := begin(t, pool)
	err := store.ReserveForOrder(ctx, tx, orderID, []store.ReserveLine{{VariantID: variantID, Quantity: 3}})
	if !errors.Is(err, store.ErrInsufficientStock) {
		t.Fatalf("want insufficient, got %v", err)
	}
	mustNoErr(t, tx.Rollback(ctx))
	tx = begin(t, pool)
	mustNoErr(t, store.ReserveForOrder(ctx, tx, orderID, []store.ReserveLine{{VariantID: variantID, Quantity: 1}}))
	defReserved := scalar[int32](t, tx, `SELECT i.reserved FROM inventory i JOIN warehouses w ON w.id = i.warehouse_id WHERE i.variant_id = $1 AND w.is_default`, variantID)
	if defReserved != 1 {
		t.Fatalf("default warehouse reserved %d", defReserved)
	}
	if !scalar[bool](t, tx, `SELECT reservation_expires_at > now() + interval '47 hours' FROM orders WHERE id = $1`, orderID) {
		t.Fatal("48h hold not set")
	}
}

func TestAdjustStockRejectsNegativeAndBelowReserved(t *testing.T) {
	pool := testutil.NewPool(t)
	_, variantID, inv := seedVariant(t, pool, 1000, 5)
	exec(t, pool, `UPDATE inventory SET reserved = 2 WHERE variant_id = $1`, variantID)
	tx := begin(t, pool)
	if _, err := store.AdjustStock(ctx, tx, inv, -4); !errors.Is(err, store.ErrInsufficientStock) {
		t.Fatalf("below reserved: %v", err)
	}
	if _, err := store.AdjustStock(ctx, tx, inv, -3); err != nil {
		t.Fatal(err)
	}
	if q, _ := invRow(t, tx, inv); q != 2 {
		t.Fatal(q)
	}
	if _, err := store.AdjustStock(ctx, tx, newID(), 1); !errors.Is(err, store.ErrInsufficientStock) {
		t.Fatalf("unknown row: %v", err)
	}
}

func TestAdjustStockWithAuditValidatesAndAudits(t *testing.T) {
	pool := testutil.NewPool(t)
	_, _, inv := seedVariant(t, pool, 1000, 5)
	admin := insertUser(t, pool, "Admin")
	actor := store.UserActor(admin, "Admin")
	for _, c := range []struct{ id, delta, reason string }{
		{"nope", "1", "x"}, {inv.String(), "x", "x"}, {inv.String(), "1", "  "}, {inv.String(), "1", strings.Repeat("a", 201)},
	} {
		if err := store.AdjustStockWithAudit(ctx, pool, c.id, c.delta, c.reason, actor); !store.IsUserError(err) {
			t.Errorf("%+v: %v", c, err)
		}
	}
	mustNoErr(t, store.AdjustStockWithAudit(ctx, pool, inv.String(), "3", "Recontagem", actor))
	if q, _ := invRow(t, pool, inv); q != 8 {
		t.Fatal(q)
	}
	if got := scalar[string](t, pool, `SELECT metadata->>'reason' FROM audit_events WHERE entity_id = $1`, inv); got != "Recontagem" {
		t.Fatal(got)
	}
	d, err := store.LoadStockDirectory(ctx, pool, "teste")
	mustNoErr(t, err)
	if len(d.Rows) != 1 || d.Rows[0].Available() != 8 {
		t.Fatalf("%+v", d.Rows)
	}
}

func TestLowStockUsesPerVariantThresholdAndAvailableStock(t *testing.T) {
	pool := testutil.NewPool(t)
	productID := insertProduct(t, pool, "Teste", 1000, true)
	active := insertWarehouse(t, pool, false, true)
	second := insertWarehouse(t, pool, false, true)
	inactive := insertWarehouse(t, pool, false, false)
	variant := func(threshold int32) uuid.UUID {
		id := insertVariant(t, pool, productID, "m", "Preto")
		exec(t, pool, `UPDATE product_variants SET low_stock_threshold = $2 WHERE id = $1`, id, threshold)
		return id
	}
	// 10 on hand, 6 reserved: 4 available <= threshold 5 -> low.
	insertInventory(t, pool, variant(5), active, 10, 6)
	// 8 available, threshold 10 -> low.
	insertInventory(t, pool, variant(10), active, 8, 0)
	// 3 available, threshold 2 -> ok.
	insertInventory(t, pool, variant(2), active, 3, 0)
	// Split across warehouses sums; inactive warehouses don't count.
	split := variant(5)
	insertInventory(t, pool, split, active, 4, 0)
	insertInventory(t, pool, split, second, 4, 0)
	insertInventory(t, pool, split, inactive, 100, 0)
	// Only inactive-warehouse stock -> 0 available -> low.
	insertInventory(t, pool, variant(5), inactive, 100, 0)
	n, err := store.CountLowStockVariants(ctx, pool)
	mustNoErr(t, err)
	if n != 3 {
		t.Fatalf("low stock = %d, want 3", n)
	}
}
