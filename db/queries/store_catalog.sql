-- Owner: store (storefront catalog, internal/store/catalog.go).
--
-- Product cards come from the product_summaries view (00001_init.sql).

-- name: StoreListProducts :many
-- Faceted listing. Every filter is optional (NULL / false / empty array =
-- off); @sort picks one of the CASE sort keys, then featured/newest.
SELECT ps.id, ps.name, ps.slug, ps.price_in_cents, ps.compare_at_price_in_cents, ps.category::text AS category,
       ps.image_url, ps.secondary_image_url, ps.available_total, ps.colors_json
FROM product_summaries ps
WHERE ps.active = true
  AND (sqlc.narg(category)::text IS NULL OR ps.category = sqlc.narg(category)::text::product_category)
  AND (sqlc.narg(search)::text IS NULL
       OR ps.name ILIKE sqlc.narg(search)::text ESCAPE '\'
       OR ps.description ILIKE sqlc.narg(search)::text ESCAPE '\'
       OR EXISTS (SELECT 1 FROM product_variants pv
                  WHERE pv.product_id = ps.id AND pv.sku ILIKE sqlc.narg(search)::text ESCAPE '\'))
  AND (NOT sqlc.arg(featured_only)::bool OR ps.featured = true)
  AND (sqlc.arg(include_out_of_stock)::bool OR ps.available_total > 0)
  AND (sqlc.narg(price_min)::bigint IS NULL OR ps.price_in_cents >= sqlc.narg(price_min)::bigint)
  AND (sqlc.narg(price_max)::bigint IS NULL OR ps.price_in_cents <= sqlc.narg(price_max)::bigint)
  AND (COALESCE(cardinality(sqlc.arg(colors)::text[]), 0) = 0
       OR EXISTS (SELECT 1 FROM product_variants pv
                  WHERE pv.product_id = ps.id AND pv.color = ANY(sqlc.arg(colors)::text[])))
  AND (COALESCE(cardinality(sqlc.arg(sizes)::text[]), 0) = 0
       OR EXISTS (SELECT 1 FROM product_variants pv
                  WHERE pv.product_id = ps.id AND pv.size::text = ANY(sqlc.arg(sizes)::text[])))
  AND (NOT sqlc.arg(sale_only)::bool OR ps.compare_at_price_in_cents IS NOT NULL)
ORDER BY
  CASE WHEN sqlc.arg(sort)::text = 'mais_vendidos' THEN
    COALESCE((SELECT SUM(oi.quantity)::int FROM order_items oi
              INNER JOIN orders o ON o.id = oi.order_id
              WHERE oi.variant_id IN (SELECT pv.id FROM product_variants pv WHERE pv.product_id = ps.id)
                AND o.status IN ('paid', 'processing', 'shipped', 'delivered')), 0)
  END DESC,
  CASE WHEN sqlc.arg(sort)::text = 'maior_desconto' THEN
    CASE WHEN ps.compare_at_price_in_cents IS NOT NULL
         THEN CAST(ps.compare_at_price_in_cents - ps.price_in_cents AS FLOAT) / ps.compare_at_price_in_cents
         ELSE 0 END
  END DESC,
  CASE WHEN sqlc.arg(sort)::text = 'menor_preco' THEN ps.price_in_cents END ASC,
  CASE WHEN sqlc.arg(sort)::text = 'maior_preco' THEN ps.price_in_cents END DESC,
  CASE WHEN sqlc.arg(sort)::text = 'novos' THEN ps.created_at END DESC,
  CASE WHEN sqlc.arg(sort)::text = 'antigos' THEN ps.created_at END ASC,
  ps.featured DESC, ps.created_at DESC
LIMIT sqlc.arg(page_size)::bigint OFFSET sqlc.arg(page_offset)::bigint;

-- name: StoreCountProducts :one
-- Same filters as StoreListProducts.
SELECT COUNT(*) FROM product_summaries ps
WHERE ps.active = true
  AND (sqlc.narg(category)::text IS NULL OR ps.category = sqlc.narg(category)::text::product_category)
  AND (sqlc.narg(search)::text IS NULL
       OR ps.name ILIKE sqlc.narg(search)::text ESCAPE '\'
       OR ps.description ILIKE sqlc.narg(search)::text ESCAPE '\'
       OR EXISTS (SELECT 1 FROM product_variants pv
                  WHERE pv.product_id = ps.id AND pv.sku ILIKE sqlc.narg(search)::text ESCAPE '\'))
  AND (NOT sqlc.arg(featured_only)::bool OR ps.featured = true)
  AND (sqlc.arg(include_out_of_stock)::bool OR ps.available_total > 0)
  AND (sqlc.narg(price_min)::bigint IS NULL OR ps.price_in_cents >= sqlc.narg(price_min)::bigint)
  AND (sqlc.narg(price_max)::bigint IS NULL OR ps.price_in_cents <= sqlc.narg(price_max)::bigint)
  AND (COALESCE(cardinality(sqlc.arg(colors)::text[]), 0) = 0
       OR EXISTS (SELECT 1 FROM product_variants pv
                  WHERE pv.product_id = ps.id AND pv.color = ANY(sqlc.arg(colors)::text[])))
  AND (COALESCE(cardinality(sqlc.arg(sizes)::text[]), 0) = 0
       OR EXISTS (SELECT 1 FROM product_variants pv
                  WHERE pv.product_id = ps.id AND pv.size::text = ANY(sqlc.arg(sizes)::text[])))
  AND (NOT sqlc.arg(sale_only)::bool OR ps.compare_at_price_in_cents IS NOT NULL);

-- name: StoreWishlistProducts :many
SELECT ps.id, ps.name, ps.slug, ps.price_in_cents, ps.compare_at_price_in_cents, ps.category::text AS category,
       ps.image_url, ps.secondary_image_url, ps.available_total, ps.colors_json
FROM wishlist_items wl
INNER JOIN product_summaries ps ON ps.id = wl.product_id
WHERE wl.user_id = $1 AND ps.active = true
ORDER BY wl.created_at DESC;

-- name: StoreRelatedProducts :many
SELECT ps.id, ps.name, ps.slug, ps.price_in_cents, ps.compare_at_price_in_cents, ps.category::text AS category,
       ps.image_url, ps.secondary_image_url, ps.available_total, ps.colors_json
FROM product_summaries ps
WHERE ps.active = true
  AND ps.category = sqlc.arg(category)::text::product_category
  AND ps.id != sqlc.arg(product_id)::uuid
ORDER BY ps.featured DESC, ps.created_at DESC
LIMIT 4;

-- name: StoreProductLooks :many
SELECT ps.id, ps.name, ps.slug, ps.price_in_cents, ps.compare_at_price_in_cents, ps.category::text AS category,
       ps.image_url, ps.secondary_image_url, ps.available_total, ps.colors_json
FROM product_looks pl
INNER JOIN product_summaries ps ON ps.id = pl.related_product_id
WHERE pl.product_id = $1 AND ps.active = true
ORDER BY pl.sort_order ASC;

-- name: StoreSetMemberProducts :many
SELECT ps.id, ps.name, ps.slug, ps.price_in_cents, ps.compare_at_price_in_cents, ps.category::text AS category,
       ps.image_url, ps.secondary_image_url, ps.available_total, ps.colors_json
FROM set_members sm
INNER JOIN product_summaries ps ON ps.id = sm.product_id
WHERE sm.set_id = $1 AND ps.active = true
ORDER BY sm.sort_order ASC;

-- name: StoreProductBySlug :one
SELECT id, name, slug, description, price_in_cents, compare_at_price_in_cents, category::text AS category
FROM products WHERE slug = $1 AND active = true;

-- name: StoreProductImages :many
SELECT id, url, alt FROM product_images WHERE product_id = $1 ORDER BY sort_order ASC;

-- name: StoreProductVariants :many
SELECT id, sku, size::text AS size, color, price_in_cents,
       COALESCE((SELECT SUM(inventory_live_available(i.id, i.quantity, i.reserved))::int
                 FROM inventory i
                 INNER JOIN warehouses w ON w.id = i.warehouse_id
                 WHERE i.variant_id = product_variants.id AND w.active = true), 0)::int AS available
FROM product_variants WHERE product_id = $1 ORDER BY size ASC, color ASC;

-- name: StoreProductColors :one
SELECT COALESCE(
    (SELECT json_agg(json_build_array(pv.color, pv.color_hex) ORDER BY pv.color ASC)
     FROM (SELECT DISTINCT ON (pv.color) pv.color, pv.color_hex
           FROM product_variants pv
           WHERE pv.product_id = $1
           ORDER BY pv.color ASC, pv.color_hex ASC) pv),
    '[]'::json
)::text AS colors_json;

-- name: StoreListSets :many
SELECT s.id, s.name, s.slug, s.description, s.active,
       (SELECT si.url FROM set_images si WHERE si.set_id = s.id ORDER BY si.sort_order ASC LIMIT 1)::null_text AS cover_url,
       (SELECT COUNT(*) FROM set_members sm INNER JOIN products p ON p.id = sm.product_id
        WHERE sm.set_id = s.id AND p.active = true) AS product_count,
       s.price_in_cents, s.compare_at_price_in_cents
FROM sets s WHERE s.active = true ORDER BY s.name ASC;

-- name: StoreSetBySlug :one
SELECT s.id, s.name, s.slug, s.description, s.active,
       (SELECT si.url FROM set_images si WHERE si.set_id = s.id ORDER BY si.sort_order ASC LIMIT 1)::null_text AS cover_url,
       (SELECT COUNT(*) FROM set_members sm INNER JOIN products p ON p.id = sm.product_id
        WHERE sm.set_id = s.id AND p.active = true) AS product_count,
       s.price_in_cents, s.compare_at_price_in_cents
FROM sets s WHERE s.slug = $1 AND s.active = true;

-- name: StoreSetImages :many
SELECT id, url, alt FROM set_images WHERE set_id = $1 ORDER BY sort_order ASC;

-- name: StoreSetMemberCount :one
SELECT COUNT(*) FROM set_members sm
INNER JOIN products p ON p.id = sm.product_id
WHERE sm.set_id = $1 AND p.active = true;

-- name: StoreSetAvailable :one
-- MIN over slots of the summed live availability of each slot's options.
SELECT COALESCE(MIN(slot_qty), 0)::bigint FROM (
    SELECT COALESCE(SUM(COALESCE((
        SELECT SUM(inventory_live_available(i.id, i.quantity, i.reserved))
        FROM inventory i
        INNER JOIN warehouses w ON w.id = i.warehouse_id
        WHERE i.variant_id = o.variant_id AND w.active = true
    ), 0)), 0) AS slot_qty
    FROM set_slots s
    LEFT JOIN set_slot_options o ON o.slot_id = s.id
    WHERE s.set_id = $1
    GROUP BY s.id
) t;

-- name: StoreSetPrice :one
SELECT price_in_cents, compare_at_price_in_cents FROM sets WHERE id = $1;

-- name: StoreFilterColors :many
SELECT DISTINCT ON (pv.color) pv.color, pv.color_hex
FROM product_variants pv
INNER JOIN products p ON p.id = pv.product_id
WHERE p.active = true
  AND (sqlc.narg(category)::text IS NULL OR p.category::text = sqlc.narg(category)::text)
ORDER BY pv.color ASC, pv.color_hex ASC NULLS LAST
LIMIT 30;

-- name: StoreHasBanners :one
SELECT EXISTS(SELECT 1 FROM home_banners WHERE active = true);

-- name: StoreActiveProductSlug :one
SELECT slug FROM products WHERE id = $1 AND active;
