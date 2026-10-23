-- Owner: store (coupon resolution and redemptions, internal/store/coupons.go).

-- name: StoreCouponByCode :one
SELECT id, code, discount_type, discount_value, min_subtotal_cents, max_discount_cents, active, starts_at, expires_at, usage_type, max_uses, per_user_limit
FROM coupons WHERE UPPER(code) = UPPER(sqlc.arg(code)::text);

-- name: StoreCouponByCodeForUpdate :one
SELECT id, code, discount_type, discount_value, min_subtotal_cents, max_discount_cents, active, starts_at, expires_at, usage_type, max_uses, per_user_limit
FROM coupons WHERE UPPER(code) = UPPER(sqlc.arg(code)::text) FOR UPDATE;

-- name: StoreCouponAssignmentCount :one
SELECT COUNT(*) FROM coupon_assignments WHERE coupon_id = $1;

-- name: StoreCouponIsAssigned :one
SELECT EXISTS(SELECT 1 FROM coupon_assignments WHERE coupon_id = $1 AND user_id = $2);

-- name: StoreCouponGlobalUses :one
SELECT COUNT(*) FROM coupon_redemptions WHERE coupon_id = $1 AND refunded_at IS NULL;

-- name: StoreUserEmail :one
SELECT email FROM users WHERE id = $1;

-- name: StoreCouponPersonalUses :one
SELECT COUNT(*) FROM coupon_redemptions
WHERE coupon_id = sqlc.arg(coupon_id)
  AND refunded_at IS NULL
  AND (user_id = sqlc.narg(user_id)::uuid OR email_norm = sqlc.narg(email_norm)::text);

-- name: StoreFirstPurchaseCode :one
SELECT first_purchase_coupon_code FROM store_settings WHERE id;

-- name: StoreHasPaidOrder :one
SELECT EXISTS(
    SELECT 1 FROM orders o
    LEFT JOIN users u ON u.id = o.user_id
    WHERE o.paid_at IS NOT NULL
      AND (o.user_id = sqlc.narg(user_id)::uuid
           OR lower(btrim(o.guest_email)) = ANY(sqlc.arg(emails)::text[])
           OR lower(u.email) = ANY(sqlc.arg(emails)::text[]))
);

-- name: StoreRecordRedemption :exec
INSERT INTO coupon_redemptions (id, coupon_id, user_id, order_id, guest_email, email_norm)
VALUES ($1, $2, $3, $4, $5, $6);

-- name: StoreRestoreRedemption :exec
UPDATE coupon_redemptions SET refunded_at = now()
WHERE order_id = $1 AND refunded_at IS NULL;

-- name: StoreReclaimRedemption :exec
UPDATE coupon_redemptions SET refunded_at = NULL
WHERE order_id = $1 AND refunded_at IS NOT NULL;
