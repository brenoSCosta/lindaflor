-- Owner: store (admin conjuntos, internal/store/admin_sets.go).

-- name: StoreAdminSets :many
SELECT s.id, s.name, s.slug, s.price_in_cents, s.active,
       (SELECT si.url FROM set_images si WHERE si.set_id = s.id ORDER BY si.sort_order ASC LIMIT 1)::null_text AS cover_url,
       (SELECT COUNT(*) FROM set_members sm WHERE sm.set_id = s.id) AS member_count,
       (SELECT COUNT(*) FROM set_images si WHERE si.set_id = s.id) AS image_count
FROM sets s ORDER BY s.name ASC;

-- name: StoreSetIDBySlug :one
SELECT id FROM sets WHERE slug = $1;

-- name: StoreSetSlugTaken :one
SELECT id FROM sets WHERE slug = sqlc.arg(slug) AND id != sqlc.arg(set_id)::uuid;

-- name: StoreInsertSet :exec
INSERT INTO sets (id, name, slug, description, price_in_cents, compare_at_price_in_cents, active, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now());

-- name: StoreToggleSet :exec
UPDATE sets SET active = NOT active, updated_at = now() WHERE id = $1;

-- name: StoreSetImageKeys :many
SELECT url FROM set_images WHERE set_id = $1;

-- name: StoreDeleteSet :exec
DELETE FROM sets WHERE id = $1;

-- name: StoreAdminSet :one
SELECT id, name, slug, description, price_in_cents, compare_at_price_in_cents, active FROM sets WHERE id = $1;

-- name: StoreAdminSetImages :many
SELECT id, url FROM set_images WHERE set_id = $1 ORDER BY sort_order ASC;

-- name: StoreAdminSetMembers :many
SELECT sm.product_id, p.name FROM set_members sm
INNER JOIN products p ON p.id = sm.product_id
WHERE sm.set_id = $1 ORDER BY sm.sort_order ASC;

-- name: StoreSetProductOptions :many
SELECT id, name FROM products WHERE active = true ORDER BY name ASC LIMIT 100;

-- name: StoreAdminSetSlots :many
SELECT id, label FROM set_slots WHERE set_id = $1 ORDER BY sort_order ASC;

-- name: StoreAdminSlotOptions :many
SELECT slot_id, variant_id FROM set_slot_options WHERE slot_id = ANY(sqlc.arg(slot_ids)::uuid[]);

-- name: StoreSetVariantOptions :many
SELECT pv.id, pv.sku, p.name AS product_name, pv.size::text AS size, pv.color
FROM product_variants pv
INNER JOIN products p ON p.id = pv.product_id
WHERE p.active = true
ORDER BY p.name ASC, pv.sku ASC LIMIT 200;

-- name: StoreUpdateSet :exec
UPDATE sets
SET name = sqlc.arg(name), slug = sqlc.arg(slug), description = sqlc.narg(description), price_in_cents = sqlc.narg(price_in_cents),
    compare_at_price_in_cents = sqlc.narg(compare_at_price_in_cents), active = sqlc.arg(active), updated_at = now()
WHERE id = sqlc.arg(id);

-- name: StoreDeleteSetMembers :exec
DELETE FROM set_members WHERE set_id = $1;

-- name: StoreInsertSetMembers :exec
-- Keeps the submitted order; unknown product ids are skipped.
INSERT INTO set_members (set_id, product_id, sort_order)
SELECT sqlc.arg(set_id)::uuid, p.id, (ids.ord - 1)::int
FROM unnest(sqlc.arg(product_ids)::uuid[]) WITH ORDINALITY AS ids(id, ord)
INNER JOIN products p ON p.id = ids.id
ON CONFLICT DO NOTHING;

-- name: StoreDeleteSetSlots :exec
DELETE FROM set_slots WHERE set_id = $1;

-- name: StoreInsertSetSlot :exec
INSERT INTO set_slots (id, set_id, label, sort_order) VALUES ($1, $2, $3, $4);

-- name: StoreInsertSlotOption :exec
INSERT INTO set_slot_options (slot_id, variant_id) VALUES ($1, $2);

-- name: StoreCountVariants :one
SELECT COUNT(*) FROM product_variants WHERE id = ANY(sqlc.arg(ids)::uuid[]);

-- name: StoreSetExists :one
SELECT id FROM sets WHERE id = $1;

-- name: StoreSetImageCount :one
SELECT COUNT(*) FROM set_images WHERE set_id = $1;

-- name: StoreSetImageOwner :one
SELECT set_id FROM set_images WHERE id = $1;

-- name: StoreNextSetImageOrder :one
SELECT (COALESCE(MAX(sort_order), -1) + 1)::int FROM set_images WHERE set_id = $1;
