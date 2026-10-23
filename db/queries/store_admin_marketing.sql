-- Owner: store (home banners and admin coupons, internal/store/admin_marketing.go).

-- name: StoreHeroBanners :many
SELECT id, title, subtitle, cta_label, link_url, image_url
FROM home_banners WHERE active = true
ORDER BY sort_order ASC, created_at DESC LIMIT sqlc.arg(max_rows)::int;

-- name: StoreAdminBanners :many
SELECT id, title, subtitle, link_url, image_url, sort_order, active
FROM home_banners ORDER BY sort_order ASC, created_at DESC;

-- name: StoreInsertBanner :exec
INSERT INTO home_banners (id, title, subtitle, cta_label, link_url, image_url, sort_order, active, created_at)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now());

-- name: StoreToggleBanner :exec
UPDATE home_banners SET active = NOT active WHERE id = $1;

-- name: StoreDeleteBanner :one
DELETE FROM home_banners WHERE id = $1 RETURNING image_url;

-- name: StoreAdminCoupons :many
-- query is LIKE-escaped by the caller ('' = every coupon).
SELECT c.id, c.code, c.discount_type, c.discount_value,
       c.min_subtotal_cents, c.max_discount_cents, c.active,
       c.starts_at::text::null_text AS starts_at, c.expires_at::text::null_text AS expires_at,
       c.usage_type, c.max_uses, c.per_user_limit,
       (SELECT COUNT(*) FROM coupon_redemptions r WHERE r.coupon_id = c.id) AS uses_count,
       (SELECT COUNT(*) FROM coupon_assignments a WHERE a.coupon_id = c.id) AS assignee_count
FROM coupons c
WHERE (sqlc.arg(query)::text = '' OR c.code ILIKE '%' || sqlc.arg(query)::text || '%' ESCAPE '\')
ORDER BY c.created_at DESC;

-- name: StoreCouponAssignees :many
SELECT u.id, u.name, u.email FROM coupon_assignments a
INNER JOIN users u ON u.id = a.user_id
WHERE a.coupon_id = $1 ORDER BY u.name ASC;

-- name: StoreCouponAssigneeIDs :many
SELECT user_id FROM coupon_assignments WHERE coupon_id = $1;

-- name: StoreInsertCoupon :execrows
-- starts_at / expires_at are '' (none) or a timestamp literal.
INSERT INTO coupons (id, code, discount_type, discount_value,
                     min_subtotal_cents, max_discount_cents, active, starts_at, expires_at,
                     usage_type, max_uses, per_user_limit)
VALUES (sqlc.arg(id), sqlc.arg(code), sqlc.arg(discount_type), sqlc.arg(discount_value)::int,
        sqlc.arg(min_subtotal_cents)::int, sqlc.narg(max_discount_cents)::int, true,
        NULLIF(sqlc.arg(starts_at)::text, '')::timestamptz, NULLIF(sqlc.arg(expires_at)::text, '')::timestamptz,
        sqlc.arg(usage_type), sqlc.narg(max_uses)::int, sqlc.arg(per_user_limit)::int)
ON CONFLICT ((upper(code))) DO NOTHING;

-- name: StoreToggleCoupon :execrows
UPDATE coupons SET active = NOT active WHERE id = $1;

-- name: StoreDeleteCoupon :execrows
DELETE FROM coupons WHERE id = $1;

-- name: StoreAssignCoupon :exec
INSERT INTO coupon_assignments (coupon_id, user_id)
SELECT sqlc.arg(coupon_id)::uuid, unnest(sqlc.arg(user_ids)::uuid[])
ON CONFLICT DO NOTHING;

-- name: StoreUnassignCoupon :exec
DELETE FROM coupon_assignments WHERE coupon_id = $1 AND user_id = $2;
