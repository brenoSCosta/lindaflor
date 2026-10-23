package store_test

import (
	"context"
	"fmt"
	"strings"
	"testing"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

var ctx = context.Background()

// TestStatementsPrepare checks every static statement against the schema
// (what sqlx's compile-time checks did for the Rust code).
func TestStatementsPrepare(t *testing.T) {
	pool := testutil.NewPool(t)
	conn, err := pool.Acquire(ctx)
	if err != nil {
		t.Fatal(err)
	}
	defer conn.Release()
	stmts := store.Statements()
	if len(stmts) < 150 {
		t.Fatalf("only %d statements registered", len(stmts))
	}
	for i, sql := range stmts {
		if _, err := conn.Conn().Prepare(ctx, fmt.Sprintf("s%d", i), sql); err != nil {
			t.Errorf("statement %d does not prepare: %v\n%s", i, err, sql)
		}
	}
}

// --- fixtures ---

func newID() uuid.UUID { return uuid.Must(uuid.NewV7()) }

func exec(t testing.TB, q store.DBTX, sql string, args ...any) {
	t.Helper()
	if _, err := q.Exec(ctx, sql, args...); err != nil {
		t.Fatalf("exec %q: %v", sql, err)
	}
}

func scalar[T any](t testing.TB, q store.DBTX, sql string, args ...any) T {
	t.Helper()
	var v T
	if err := q.QueryRow(ctx, sql, args...).Scan(&v); err != nil {
		t.Fatalf("query %q: %v", sql, err)
	}
	return v
}

func begin(t testing.TB, pool *pgxpool.Pool) pgx.Tx {
	t.Helper()
	tx, err := pool.Begin(ctx)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { _ = tx.Rollback(ctx) })
	return tx
}

func commit(t testing.TB, tx pgx.Tx) {
	t.Helper()
	if err := tx.Commit(ctx); err != nil {
		t.Fatal(err)
	}
}

func insertProduct(t testing.TB, q store.DBTX, name string, price int64, active bool) uuid.UUID {
	t.Helper()
	id := newID()
	exec(t, q, `INSERT INTO products (id, name, slug, price_in_cents, active) VALUES ($1, $2, $3, $4, $5)`,
		id, name, "p-"+id.String(), price, active)
	return id
}

func insertVariant(t testing.TB, q store.DBTX, productID uuid.UUID, size, color string) uuid.UUID {
	t.Helper()
	id := newID()
	exec(t, q, `INSERT INTO product_variants (id, product_id, sku, size, color) VALUES ($1, $2, $3, $4::text::product_size, $5)`,
		id, productID, "sku-"+id.String(), size, color)
	return id
}

func insertWarehouse(t testing.TB, q store.DBTX, isDefault, active bool) uuid.UUID {
	t.Helper()
	id := newID()
	exec(t, q, `INSERT INTO warehouses (id, code, name, is_default, active) VALUES ($1, $2, 'W', $3, $4)`,
		id, "wh-"+strings.ReplaceAll(id.String(), "-", ""), isDefault, active)
	return id
}

func insertInventory(t testing.TB, q store.DBTX, variantID, warehouseID uuid.UUID, qty, reserved int32) uuid.UUID {
	t.Helper()
	id := newID()
	exec(t, q, `INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved) VALUES ($1, $2, $3, $4, $5)`,
		id, variantID, warehouseID, qty, reserved)
	return id
}

// seedVariant creates an active product with one variant (m/azul) and
// stock units in a default warehouse.
func seedVariant(t testing.TB, q store.DBTX, price int64, stock int32) (productID, variantID, inventoryID uuid.UUID) {
	t.Helper()
	productID = insertProduct(t, q, "Peça teste", price, true)
	variantID = insertVariant(t, q, productID, "m", "azul")
	inventoryID = insertInventory(t, q, variantID, insertWarehouse(t, q, true, true), stock, 0)
	return productID, variantID, inventoryID
}

func insertUser(t testing.TB, q store.DBTX, name string) uuid.UUID {
	t.Helper()
	id := newID()
	exec(t, q, `INSERT INTO users (id, name, email) VALUES ($1, $2, $3)`, id, name, "u-"+id.String()+"@example.com")
	return id
}

func newCart(t testing.TB, q store.DBTX) uuid.UUID {
	t.Helper()
	id := newID()
	exec(t, q, `INSERT INTO carts (id, token) VALUES ($1, $2)`, id, newID())
	return id
}

func addCartLine(t testing.TB, q store.DBTX, cartID, variantID uuid.UUID, qty int32) {
	t.Helper()
	exec(t, q, `INSERT INTO cart_items (id, cart_id, variant_id, quantity) VALUES ($1, $2, $3, $4)`, newID(), cartID, variantID, qty)
}

// insertPendingOrder creates a pending order with one line per (variant, qty).
func insertPendingOrder(t testing.TB, q store.DBTX, lines ...store.ReserveLine) uuid.UUID {
	t.Helper()
	id := newID()
	exec(t, q, `INSERT INTO orders (id, guest_email, status, access_token, total_cents) VALUES ($1, 'a@b.co', 'pending_payment', $2, 1000)`, id, newID())
	for _, l := range lines {
		exec(t, q, `INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
			VALUES ($1, $2, $3, 'T', 'M / azul', $4, 1000)`, newID(), id, l.VariantID, l.Quantity)
	}
	return id
}

func stockOf(t testing.TB, q store.DBTX, variantID uuid.UUID) (quantity, reserved int32) {
	t.Helper()
	if err := q.QueryRow(ctx, `SELECT SUM(quantity)::int, SUM(reserved)::int FROM inventory WHERE variant_id = $1`, variantID).Scan(&quantity, &reserved); err != nil {
		t.Fatal(err)
	}
	return quantity, reserved
}

func statusOf(t testing.TB, q store.DBTX, orderID uuid.UUID) string {
	t.Helper()
	return scalar[string](t, q, `SELECT status::text FROM orders WHERE id = $1`, orderID)
}

func ensurePixSettings(t testing.TB, q store.DBTX) {
	t.Helper()
	exec(t, q, `INSERT INTO store_settings (id, pix_key, pix_key_type, pix_merchant_name, pix_merchant_city)
		VALUES (true, '79999816511', 'phone', 'Linda Flor', 'Aracaju')
		ON CONFLICT (id) DO UPDATE SET pix_key = EXCLUDED.pix_key, pix_key_type = EXCLUDED.pix_key_type,
		  pix_merchant_name = EXCLUDED.pix_merchant_name, pix_merchant_city = EXCLUDED.pix_merchant_city`)
}

func mustNoErr(t testing.TB, err error) {
	t.Helper()
	if err != nil {
		t.Fatal(err)
	}
}

func userMsg(t testing.TB, err error) string {
	t.Helper()
	msg, ok := store.UserMessage(err)
	if !ok {
		t.Fatalf("expected a *UserError, got %v", err)
	}
	return msg
}
