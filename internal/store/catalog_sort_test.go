package store_test

import (
	"fmt"
	"slices"
	"testing"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"

	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// referenceCatalogOrder is the per-sort ORDER BY the catalog listing used
// when its SQL was built dynamically (src/app/store/queries.rs). The sqlc
// query expresses it with CASE keys; this test pins the two together.
var referenceCatalogOrder = map[string]string{
	"relevantes": "p.featured DESC, p.created_at DESC",
	"mais_vendidos": `COALESCE((SELECT SUM(oi.quantity)::int FROM order_items oi
              INNER JOIN orders o ON o.id = oi.order_id
              WHERE oi.variant_id IN (SELECT pv.id FROM product_variants pv WHERE pv.product_id = p.id)
              AND o.status IN ('paid', 'processing', 'shipped', 'delivered')), 0) DESC,
              p.featured DESC, p.created_at DESC`,
	"maior_desconto": `CASE WHEN p.compare_at_price_in_cents IS NOT NULL
              THEN CAST(p.compare_at_price_in_cents - p.price_in_cents AS FLOAT) / p.compare_at_price_in_cents
              ELSE 0 END DESC,
              p.featured DESC, p.created_at DESC`,
	"menor_preco":  "p.price_in_cents ASC, p.featured DESC, p.created_at DESC",
	"maior_preco":  "p.price_in_cents DESC, p.featured DESC, p.created_at DESC",
	"novos":        "p.created_at DESC, p.featured DESC",
	"antigos":      "p.created_at ASC, p.featured DESC",
	"":             "p.featured DESC, p.created_at DESC",
	"desconhecido": "p.featured DESC, p.created_at DESC",
}

func TestCatalogSortsMatchReferenceOrder(t *testing.T) {
	pool := testutil.NewPool(t)
	wh := insertWarehouse(t, pool, true, true)
	type fixture struct {
		price    int64
		compare  *int64
		featured bool
		sold     int32
	}
	fixtures := []fixture{
		{5000, nil, false, 0},
		{8000, price(16000), true, 3},
		{9000, price(10000), false, 7},
		{7000, nil, true, 0},
		{6000, price(9000), false, 3},
		{12000, nil, false, 1},
		{4000, price(5000), true, 0},
	}
	for i, f := range fixtures {
		id := newID()
		exec(t, pool, `INSERT INTO products (id, name, slug, price_in_cents, compare_at_price_in_cents, featured, created_at)
			VALUES ($1, $2, $3, $4, $5, $6, now() - make_interval(hours => $7))`,
			id, fmt.Sprintf("Peça %d", i), "sort-"+id.String(), f.price, f.compare, f.featured, int32(i))
		v := insertVariant(t, pool, id, "m", "Azul")
		insertInventory(t, pool, v, wh, int32(i%3), 0)
		if f.sold > 0 {
			order := newID()
			exec(t, pool, `INSERT INTO orders (id, status, access_token) VALUES ($1, 'paid', $2)`, order, newID())
			exec(t, pool, `INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
				VALUES ($1, $2, $3, 'x', 'M', $4, 100)`, newID(), order, v, f.sold)
		}
	}
	for sort, orderBy := range referenceCatalogOrder {
		rows, err := pool.Query(ctx, `SELECT p.id FROM products p WHERE p.active = true ORDER BY `+orderBy)
		mustNoErr(t, err)
		want, err := pgx.CollectRows(rows, pgx.RowTo[uuid.UUID])
		mustNoErr(t, err)
		got, err := store.ListProducts(ctx, pool, store.CatalogQuery{IncludeOutOfStock: true, Sort: sort, Limit: 100})
		mustNoErr(t, err)
		ids := make([]uuid.UUID, 0, len(got))
		for _, p := range got {
			ids = append(ids, p.ID)
		}
		if !slices.Equal(ids, want) {
			t.Errorf("sort %q: got %v, want %v", sort, ids, want)
		}
	}
}
