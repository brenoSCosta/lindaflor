-- Owner: seed (cmd/seed: admin user, warehouse, settings, catalog, sets).
-- Idempotent inserts only.

-- name: SeedUserIDByEmail :one
SELECT id FROM users WHERE email = $1;

-- name: SeedInsertAdminUser :exec
INSERT INTO users (id, name, email, email_verified, role, created_at, updated_at)
VALUES ($1, $2, $3, true, 'admin', now(), now());

-- name: SeedInsertCredentialAccount :exec
INSERT INTO accounts (id, account_id, provider_id, user_id, password, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, now(), now());

-- name: SeedWarehouseID :one
SELECT id FROM warehouses WHERE code = 'PRINCIPAL';

-- name: SeedInsertWarehouse :exec
INSERT INTO warehouses (id, code, name, is_default, active, created_at)
VALUES ($1, 'PRINCIPAL', 'Depósito Principal', true, true, now());

-- name: SeedInsertSettings :execrows
INSERT INTO store_settings (id, pix_key, pix_key_type, pix_merchant_name, pix_merchant_city, pix_discount_percent,
                            announcement_text, first_purchase_coupon_code, whatsapp_number, whatsapp_message_template,
                            created_at, updated_at)
VALUES (true, sqlc.arg(pix_key)::text, 'phone', 'Linda Flor Moda Praia', 'Aracaju', 5, NULL,
        sqlc.arg(first_purchase_coupon_code)::text, sqlc.arg(whatsapp_number)::text,
        sqlc.arg(whatsapp_message_template)::text, now(), now())
ON CONFLICT (id) DO NOTHING;

-- name: SeedInsertFirstPurchaseCoupon :exec
INSERT INTO coupons (id, code, discount_type, discount_value, min_subtotal_cents, active, usage_type, per_user_limit, created_at, updated_at)
VALUES ($1, $2, 'percent', 10, 0, true, 'unlimited', 1, now(), now())
ON CONFLICT ((upper(code))) DO NOTHING;

-- name: SeedCountBanners :one
SELECT COUNT(*) FROM home_banners;

-- name: SeedInsertBanners :exec
INSERT INTO home_banners (id, title, subtitle, cta_label, link_url, image_url, sort_order, active, created_at)
VALUES
    (sqlc.arg(first_id)::uuid, 'Verão 2027', 'Coleção Nova', 'Explorar', '/produtos?sort=novos',
     'https://images.unsplash.com/photo-1519046904884-53103b34b206?w=1200&h=600&fit=crop', 0, true, now()),
    (sqlc.arg(second_id)::uuid, 'Liquida', '20% OFF', 'Ver Liquida', '/produtos?sale=true',
     'https://images.unsplash.com/photo-1473496169904-658ba7c44d8a?w=1200&h=600&fit=crop', 1, true, now());

-- name: SeedLookCandidates :many
SELECT p.id, p.category::text AS category, pv.color
FROM products p
INNER JOIN product_variants pv ON pv.product_id = p.id
WHERE p.active = true
  AND p.category IN ('biquini_top'::product_category, 'biquini_bottom'::product_category);

-- name: SeedInsertLook :exec
INSERT INTO product_looks (product_id, related_product_id, sort_order)
VALUES ($1, $2, $3) ON CONFLICT DO NOTHING;

-- name: SeedSetMemberBySlug :one
-- The product and its first variant.
SELECT p.id AS product_id, fv.id AS variant_id, p.price_in_cents
FROM products p
INNER JOIN LATERAL (
    SELECT pv.id FROM product_variants pv
    WHERE pv.product_id = p.id
    ORDER BY pv.created_at ASC LIMIT 1
) fv ON true
WHERE p.slug = $1 AND p.active = true;

-- name: SeedSetIDBySlug :one
SELECT id FROM sets WHERE slug = $1;

-- name: SeedInsertSet :exec
INSERT INTO sets (id, name, slug, description, active, price_in_cents, compare_at_price_in_cents, created_at, updated_at)
VALUES ($1, $2, $3, $4, true, $5, $6, now(), now());

-- name: SeedCountSetImages :one
SELECT COUNT(*) FROM set_images WHERE set_id = $1;

-- name: SeedMemberImages :many
SELECT url, alt FROM product_images
WHERE product_id = ANY(sqlc.arg(product_ids)::uuid[]) ORDER BY sort_order ASC LIMIT 3;

-- name: SeedInsertSetImage :exec
INSERT INTO set_images (id, set_id, url, alt, sort_order, created_at)
VALUES ($1, $2, $3, $4, $5, now());

-- name: SeedInsertSetMember :exec
INSERT INTO set_members (set_id, product_id, sort_order)
VALUES ($1, $2, $3) ON CONFLICT DO NOTHING;

-- name: SeedCountSetSlots :one
SELECT COUNT(*) FROM set_slots WHERE set_id = $1;

-- name: SeedInsertSetSlot :exec
INSERT INTO set_slots (id, set_id, label, sort_order) VALUES ($1, $2, $3, $4);

-- name: SeedInsertSlotOption :exec
INSERT INTO set_slot_options (slot_id, variant_id, price_override_cents)
VALUES ($1, $2, NULL) ON CONFLICT DO NOTHING;

-- name: SeedProductIDBySlug :one
SELECT id FROM products WHERE slug = $1;

-- name: SeedInsertProduct :exec
INSERT INTO products (id, name, slug, description, price_in_cents, compare_at_price_in_cents, category, active, featured, created_at, updated_at)
VALUES (sqlc.arg(id), sqlc.arg(name), sqlc.arg(slug), sqlc.narg(description), sqlc.arg(price_in_cents),
        sqlc.narg(compare_at_price_in_cents), sqlc.arg(category)::text::product_category, true, sqlc.arg(featured), now(), now());

-- name: SeedInsertVariant :exec
INSERT INTO product_variants (id, product_id, sku, size, color, color_hex, price_in_cents, low_stock_threshold, created_at, updated_at)
VALUES (sqlc.arg(id), sqlc.arg(product_id), sqlc.arg(sku), sqlc.arg(size)::text::product_size, sqlc.arg(color),
        sqlc.narg(color_hex), NULL, 5, now(), now());

-- name: SeedInsertInventory :exec
INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved, updated_at)
VALUES ($1, $2, $3, $4, 0, now());

-- name: SeedInsertProductImage :exec
INSERT INTO product_images (id, product_id, url, alt, sort_order, created_at)
VALUES ($1, $2, $3, $4, $5, now());
