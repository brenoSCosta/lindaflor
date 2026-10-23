-- Owner: store (admin products, internal/store/admin_products.go).

-- name: StoreAdminProductCount :one
-- query is LIKE-escaped by the caller; active is 'true' | 'false' | 'all'.
SELECT COUNT(*) FROM products p
WHERE (sqlc.arg(query)::text = '' OR p.name ILIKE '%' || sqlc.arg(query)::text || '%' ESCAPE '\')
  AND (sqlc.arg(active)::text = 'all' OR p.active = (sqlc.arg(active)::text = 'true'));

-- name: StoreAdminProductList :many
SELECT p.id, p.name, p.slug, p.category::text AS category, p.price_in_cents, p.active,
       COUNT(DISTINCT pv.id) AS variant_count,
       COALESCE(SUM(GREATEST(i.quantity - i.reserved, 0)) FILTER (WHERE w.active = true), 0)::bigint AS available_total
FROM products p
LEFT JOIN product_variants pv ON pv.product_id = p.id
LEFT JOIN inventory i ON i.variant_id = pv.id
LEFT JOIN warehouses w ON w.id = i.warehouse_id
WHERE (sqlc.arg(query)::text = '' OR p.name ILIKE '%' || sqlc.arg(query)::text || '%' ESCAPE '\')
  AND (sqlc.arg(active)::text = 'all' OR p.active = (sqlc.arg(active)::text = 'true'))
GROUP BY p.id
ORDER BY p.name
LIMIT sqlc.arg(page_size)::bigint OFFSET sqlc.arg(page_offset)::bigint;

-- name: StoreDefaultWarehouse :one
SELECT id FROM warehouses WHERE is_default = true ORDER BY created_at LIMIT 1;

-- name: StoreAnyWarehouse :one
SELECT id FROM warehouses ORDER BY created_at LIMIT 1;

-- name: StoreCreateDefaultWarehouse :exec
INSERT INTO warehouses (id, code, name, is_default, active, created_at)
VALUES ($1, 'PRINCIPAL', 'Depósito Principal', true, true, now());

-- name: StoreProductIDBySlug :one
SELECT id FROM products WHERE slug = $1;

-- name: StoreProductSlugTaken :one
SELECT id FROM products WHERE slug = sqlc.arg(slug) AND id != sqlc.arg(product_id)::uuid;

-- name: StoreInsertProduct :exec
INSERT INTO products (id, name, slug, description, price_in_cents, compare_at_price_in_cents, category, active, featured, created_at, updated_at)
VALUES (sqlc.arg(id), sqlc.arg(name), sqlc.arg(slug), sqlc.narg(description), sqlc.arg(price_in_cents), sqlc.narg(compare_at_price_in_cents),
        sqlc.arg(category)::text::product_category, true, sqlc.arg(featured), now(), now());

-- name: StoreInsertVariant :exec
INSERT INTO product_variants (id, product_id, sku, size, color, color_hex, price_in_cents, low_stock_threshold, created_at, updated_at)
VALUES (sqlc.arg(id), sqlc.arg(product_id), sqlc.arg(sku), sqlc.arg(size)::text::product_size, sqlc.arg(color), sqlc.narg(color_hex),
        NULL, sqlc.arg(low_stock_threshold), now(), now());

-- name: StoreInsertInventory :exec
INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved, updated_at)
VALUES ($1, $2, $3, $4, 0, now());

-- name: StoreAdminProduct :one
SELECT id, name, slug, description, price_in_cents, compare_at_price_in_cents, category::text AS category, featured, active
FROM products WHERE id = $1;

-- name: StoreAdminVariants :many
SELECT id, sku, size::text AS size, color, color_hex, price_in_cents, low_stock_threshold
FROM product_variants WHERE product_id = $1 ORDER BY created_at;

-- name: StoreAdminProductImages :many
SELECT id, url FROM product_images WHERE product_id = $1 ORDER BY sort_order ASC;

-- name: StoreLookOptions :many
SELECT id, name, category::text AS category FROM products WHERE active = true AND id != $1 ORDER BY name;

-- name: StoreExistingLooks :many
SELECT related_product_id FROM product_looks WHERE product_id = $1;

-- name: StoreProductExists :one
SELECT id FROM products WHERE id = $1;

-- name: StoreProductImageCount :one
SELECT COUNT(*) FROM product_images WHERE product_id = $1;

-- name: StoreUpdateProduct :exec
UPDATE products
SET name = sqlc.arg(name), slug = sqlc.arg(slug), description = sqlc.narg(description), price_in_cents = sqlc.arg(price_in_cents),
    compare_at_price_in_cents = sqlc.narg(compare_at_price_in_cents), category = sqlc.arg(category)::text::product_category,
    featured = sqlc.arg(featured), active = sqlc.arg(active), updated_at = now()
WHERE id = sqlc.arg(id);

-- name: StoreExistingVariantSKUs :many
SELECT id, sku FROM product_variants WHERE product_id = $1;

-- name: StoreUpdateVariant :exec
UPDATE product_variants
SET size = sqlc.arg(size)::text::product_size, color = sqlc.arg(color), color_hex = sqlc.narg(color_hex),
    low_stock_threshold = sqlc.arg(low_stock_threshold), updated_at = now()
WHERE id = sqlc.arg(id);

-- name: StoreInsertEmptyInventory :exec
INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved, updated_at)
VALUES ($1, $2, $3, 0, 0, now())
ON CONFLICT (variant_id, warehouse_id) DO NOTHING;

-- name: StoreDeleteVariant :exec
DELETE FROM product_variants WHERE id = $1;

-- name: StoreDeleteLooks :exec
DELETE FROM product_looks WHERE product_id = $1;

-- name: StoreInsertLooks :exec
-- Keeps the submitted order; unknown ids and the product itself are skipped.
INSERT INTO product_looks (product_id, related_product_id, sort_order)
SELECT sqlc.arg(product_id)::uuid, p.id, (ids.ord - 1)::int
FROM unnest(sqlc.arg(related_ids)::uuid[]) WITH ORDINALITY AS ids(id, ord)
INNER JOIN products p ON p.id = ids.id
WHERE p.id <> sqlc.arg(product_id)::uuid
ON CONFLICT DO NOTHING;

-- name: StoreImageOwner :one
SELECT product_id FROM product_images WHERE id = $1;

-- name: StoreNextImageOrder :one
SELECT (COALESCE(MAX(sort_order), -1) + 1)::int FROM product_images WHERE product_id = $1;
