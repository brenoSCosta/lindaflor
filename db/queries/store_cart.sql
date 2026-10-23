-- Owner: store (cart, internal/store/cart.go).

-- name: StoreCartByToken :one
SELECT id, token, user_id FROM carts WHERE token = $1;

-- name: StoreCartByUser :one
SELECT id, token, user_id FROM carts WHERE user_id = sqlc.arg(user_id)::uuid;

-- name: StoreInsertCart :exec
INSERT INTO carts (id, token, user_id) VALUES ($1, $2, $3);

-- name: StoreVariantAvailability :one
SELECT p.active,
    COALESCE((
        SELECT SUM(inventory_live_available(i.id, i.quantity, i.reserved))::int
        FROM inventory i
        INNER JOIN warehouses w ON w.id = i.warehouse_id
        WHERE i.variant_id = pv.id AND w.active = true
    ), 0)::int AS available
FROM product_variants pv
INNER JOIN products p ON p.id = pv.product_id
WHERE pv.id = $1;

-- name: StoreUpsertLooseLine :exec
INSERT INTO cart_items (id, cart_id, variant_id, quantity)
VALUES (sqlc.arg(id), sqlc.arg(cart_id), sqlc.arg(variant_id), sqlc.arg(quantity))
ON CONFLICT (cart_id, variant_id) WHERE set_id IS NULL
DO UPDATE SET
    quantity = LEAST(cart_items.quantity + EXCLUDED.quantity, sqlc.arg(max_quantity)::int),
    updated_at = now();

-- name: StoreUpsertSetLine :exec
INSERT INTO cart_items (id, cart_id, variant_id, quantity, set_id)
VALUES (sqlc.arg(id), sqlc.arg(cart_id), sqlc.arg(variant_id), sqlc.arg(quantity), sqlc.arg(set_id))
ON CONFLICT (cart_id, set_id, variant_id) WHERE set_id IS NOT NULL
DO UPDATE SET
    quantity = LEAST(cart_items.quantity + EXCLUDED.quantity, sqlc.arg(max_quantity)::int),
    updated_at = now();

-- name: StoreTouchCart :exec
UPDATE carts SET updated_at = now() WHERE id = $1;

-- name: StoreSetForCart :one
SELECT active, price_in_cents FROM sets WHERE id = $1;

-- name: StoreSetVariants :many
SELECT o.variant_id
FROM set_slot_options o
INNER JOIN set_slots s ON s.id = o.slot_id
WHERE s.set_id = $1
ORDER BY s.sort_order ASC, o.variant_id ASC;

-- name: StoreSetLooseQuantity :execrows
UPDATE cart_items SET quantity = $3, updated_at = now()
WHERE cart_id = $1 AND variant_id = $2 AND set_id IS NULL;

-- name: StoreSetMemberVariants :many
SELECT variant_id FROM cart_items WHERE cart_id = sqlc.arg(cart_id) AND set_id = sqlc.arg(set_id)::uuid;

-- name: StoreSetGroupQuantity :exec
UPDATE cart_items SET quantity = sqlc.arg(quantity), updated_at = now() WHERE cart_id = sqlc.arg(cart_id) AND set_id = sqlc.arg(set_id)::uuid;

-- name: StoreRemoveLoose :exec
DELETE FROM cart_items WHERE cart_id = $1 AND variant_id = $2 AND set_id IS NULL;

-- name: StoreRemoveSet :exec
DELETE FROM cart_items WHERE cart_id = sqlc.arg(cart_id) AND set_id = sqlc.arg(set_id)::uuid;

-- name: StoreClearItems :exec
DELETE FROM cart_items WHERE cart_id = $1;

-- name: StoreCartCountForToken :one
SELECT COALESCE(SUM(ci.quantity), 0)::int
FROM cart_items ci
INNER JOIN carts c ON c.id = ci.cart_id
WHERE c.token = $1;

-- name: StoreHydrateCart :many
-- Live availability (expired holds count as free); locks the cart lines.
SELECT
    ci.id AS cart_item_id,
    ci.variant_id,
    ci.quantity,
    p.id AS product_id,
    p.slug AS product_slug,
    p.name AS product_name,
    p.active AS product_active,
    p.price_in_cents AS product_price,
    pv.size::text::null_text AS size,
    pv.color,
    pv.price_in_cents AS variant_price,
    COALESCE((
        SELECT SUM(inventory_live_available(i.id, i.quantity, i.reserved))::int
        FROM inventory i
        INNER JOIN warehouses w ON w.id = i.warehouse_id
        WHERE i.variant_id = pv.id AND w.active = true
    ), 0)::int AS available,
    (
        SELECT pi.url FROM product_images pi
        WHERE pi.product_id = p.id
        ORDER BY pi.sort_order ASC
        LIMIT 1
    )::null_text AS image_url,
    ci.set_id AS set_id,
    s.name AS set_name
FROM cart_items ci
LEFT JOIN product_variants pv ON pv.id = ci.variant_id
LEFT JOIN products p ON p.id = pv.product_id
LEFT JOIN sets s ON s.id = ci.set_id
WHERE ci.cart_id = $1
FOR UPDATE OF ci;

-- name: StoreQuoteCart :many
-- Same as StoreHydrateCart but with raw availability (quantity - reserved):
-- the checkout quote runs after LockCartInventory reclaimed expired holds.
SELECT
    ci.id AS cart_item_id,
    ci.variant_id,
    ci.quantity,
    p.id AS product_id,
    p.slug AS product_slug,
    p.name AS product_name,
    p.active AS product_active,
    p.price_in_cents AS product_price,
    pv.size::text::null_text AS size,
    pv.color,
    pv.price_in_cents AS variant_price,
    COALESCE((
        SELECT SUM(GREATEST(i.quantity - i.reserved, 0))::int
        FROM inventory i
        INNER JOIN warehouses w ON w.id = i.warehouse_id
        WHERE i.variant_id = pv.id AND w.active = true
    ), 0)::int AS available,
    (
        SELECT pi.url FROM product_images pi
        WHERE pi.product_id = p.id
        ORDER BY pi.sort_order ASC
        LIMIT 1
    )::null_text AS image_url,
    ci.set_id AS set_id,
    s.name AS set_name
FROM cart_items ci
LEFT JOIN product_variants pv ON pv.id = ci.variant_id
LEFT JOIN products p ON p.id = pv.product_id
LEFT JOIN sets s ON s.id = ci.set_id
WHERE ci.cart_id = $1
FOR UPDATE OF ci;

-- name: StoreCartToken :one
SELECT token FROM carts WHERE id = $1;

-- name: StoreDeleteCartItem :exec
DELETE FROM cart_items WHERE id = $1;

-- name: StoreUpdateCartItemQty :exec
UPDATE cart_items SET quantity = $2, updated_at = now() WHERE id = $1;

-- name: StoreQuoteSet :one
SELECT name, active, price_in_cents FROM sets WHERE id = $1;

-- name: StoreQuoteSetVariants :many
SELECT o.variant_id
FROM set_slot_options o
INNER JOIN set_slots s ON s.id = o.slot_id
WHERE s.set_id = $1;

-- name: StoreCartCouponCode :one
SELECT coupon_code FROM carts WHERE id = $1;

-- name: StoreSetCartCoupon :exec
UPDATE carts SET coupon_code = $2, updated_at = now() WHERE id = $1;

-- name: StoreAttachCart :exec
UPDATE carts SET user_id = sqlc.arg(user_id)::uuid, updated_at = now() WHERE id = sqlc.arg(id);

-- name: StoreCartOwner :one
SELECT user_id FROM carts WHERE id = $1;

-- name: StoreGuestLines :many
SELECT variant_id, quantity, set_id FROM cart_items WHERE cart_id = $1;

-- name: StoreDeleteCart :exec
DELETE FROM carts WHERE id = $1;
