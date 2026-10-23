-- +goose Up
-- Product card columns shared by every storefront listing (catalog,
-- wishlist, related products, looks, set members): the Rust
-- PRODUCT_SUMMARY_COLUMNS fragment as a view, so the sqlc queries select
-- from it instead of repeating the subqueries. A simple view is flattened
-- by the planner; its subqueries only run for the rows a query returns.
--
-- available_total is raw stock (quantity - reserved over active
-- warehouses), not inventory_live_available: cards and the in-stock filter
-- ignore not-yet-swept expired holds, as in Rust.
CREATE VIEW product_summaries AS
SELECT
    p.id,
    p.name,
    p.slug,
    p.description,
    p.price_in_cents,
    p.compare_at_price_in_cents,
    p.category,
    p.active,
    p.featured,
    p.created_at,
    (SELECT pi.url FROM product_images pi
     WHERE pi.product_id = p.id
     ORDER BY pi.sort_order ASC LIMIT 1)::null_text AS image_url,
    (SELECT pi.url FROM product_images pi
     WHERE pi.product_id = p.id
     ORDER BY pi.sort_order ASC LIMIT 1 OFFSET 1)::null_text AS secondary_image_url,
    COALESCE((
        SELECT SUM(GREATEST(i.quantity - i.reserved, 0))::int
        FROM inventory i
        INNER JOIN warehouses w ON w.id = i.warehouse_id
        WHERE i.variant_id IN (SELECT pv.id FROM product_variants pv WHERE pv.product_id = p.id)
          AND w.active = true), 0)::int AS available_total,
    COALESCE(
        (SELECT json_agg(json_build_array(pv.color, pv.color_hex) ORDER BY pv.color ASC)
         FROM (SELECT DISTINCT ON (pv.color) pv.color, pv.color_hex
               FROM product_variants pv
               WHERE pv.product_id = p.id
               ORDER BY pv.color ASC, pv.color_hex ASC) pv),
        '[]'::json
    )::text AS colors_json
FROM products p;
