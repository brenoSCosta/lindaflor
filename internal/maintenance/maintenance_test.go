package maintenance_test

import (
	"context"
	"testing"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/maintenance"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

func newID() uuid.UUID { return uuid.Must(uuid.NewV7()) }

func exec(t *testing.T, pool *pgxpool.Pool, sql string, args ...any) {
	t.Helper()
	if _, err := pool.Exec(context.Background(), sql, args...); err != nil {
		t.Fatal(err)
	}
}

func exists(t *testing.T, pool *pgxpool.Pool, table string, id uuid.UUID) bool {
	t.Helper()
	var ok bool
	if err := pool.QueryRow(context.Background(), `SELECT EXISTS (SELECT 1 FROM `+table+` WHERE id = $1)`, id).Scan(&ok); err != nil {
		t.Fatal(err)
	}
	return ok
}

// cart inserts a cart touched `age` ago, optionally owned and with an item.
func cart(t *testing.T, pool *pgxpool.Pool, age string, user, variant *uuid.UUID) uuid.UUID {
	t.Helper()
	id := newID()
	exec(t, pool, `INSERT INTO carts (id, token, user_id, updated_at) VALUES ($1, $2, $3, now() - $4::interval)`, id, newID(), user, age)
	if variant != nil {
		exec(t, pool, `INSERT INTO cart_items (id, cart_id, variant_id, quantity) VALUES ($1, $2, $3, 1)`, newID(), id, *variant)
	}
	return id
}

func TestSweepStaleRows(t *testing.T) {
	pool := testutil.NewPool(t)
	ctx := context.Background()
	user := newID()
	exec(t, pool, `INSERT INTO users (id, name, email) VALUES ($1, 'Ana', $2)`, user, user.String()+"@example.com")
	product, variant := newID(), newID()
	exec(t, pool, `INSERT INTO products (id, name, slug, price_in_cents, category) VALUES ($1, 'P', $2, 1000, 'biquini')`, product, "p-"+product.String())
	exec(t, pool, `INSERT INTO product_variants (id, product_id, sku, size, color) VALUES ($1, $2, $3, 'm', 'Azul')`, variant, product, "SKU-"+variant.String())

	oldGuest := cart(t, pool, "31 days", nil, &variant)
	emptyStale := cart(t, pool, "2 days", nil, nil)
	freshEmpty := cart(t, pool, "1 hour", nil, nil)
	recentWithItems := cart(t, pool, "10 days", nil, &variant)
	ownedOld := cart(t, pool, "90 days", &user, nil)

	expired, live := newID(), newID()
	exec(t, pool, `INSERT INTO verifications (id, identifier, value, expires_at) VALUES ($1, 'a', 'x', now() - interval '1 minute'), ($2, 'b', 'y', now() + interval '1 hour')`, expired, live)

	out, err := maintenance.SweepStaleRows(ctx, pool)
	if err != nil {
		t.Fatal(err)
	}
	if out.Carts != 2 || out.Verifications != 1 {
		t.Fatalf("%+v", out)
	}
	for id, want := range map[uuid.UUID]bool{oldGuest: false, emptyStale: false, freshEmpty: true, recentWithItems: true, ownedOld: true} {
		if exists(t, pool, "carts", id) != want {
			t.Errorf("cart %s: exists want %v", id, want)
		}
	}
	if exists(t, pool, "verifications", expired) || !exists(t, pool, "verifications", live) {
		t.Error("verifications")
	}
	var items int64
	if err := pool.QueryRow(ctx, `SELECT COUNT(*) FROM cart_items WHERE cart_id = $1`, oldGuest).Scan(&items); err != nil || items != 0 {
		t.Fatal("cart items cascade", items, err)
	}
	if out, _ := maintenance.SweepStaleRows(ctx, pool); out != (maintenance.SweepOutcome{}) {
		t.Fatalf("second pass %+v", out)
	}
}
