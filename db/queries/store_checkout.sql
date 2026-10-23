-- Owner: store (checkout order creation, internal/store/checkout.go).

-- name: StoreInsertOrder :exec
INSERT INTO orders (id, user_id, guest_email, status, subtotal_cents, shipping_cents, discount_cents, pix_discount_cents, total_cents, shipping_address, notes, coupon_id, access_token, email_norm, cart_id)
VALUES (sqlc.arg(id), sqlc.narg(user_id), sqlc.narg(guest_email), 'pending_payment', sqlc.arg(subtotal_cents), sqlc.arg(shipping_cents),
        sqlc.arg(discount_cents), sqlc.arg(pix_discount_cents), sqlc.arg(total_cents), sqlc.narg(shipping_address), sqlc.narg(notes),
        sqlc.narg(coupon_id), sqlc.arg(access_token), sqlc.narg(email_norm), sqlc.narg(cart_id));

-- name: StoreInsertOrderItem :exec
INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents, set_id, set_name)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9);

-- name: StoreSetPaymentMeta :exec
UPDATE orders SET payment_meta = sqlc.arg(payment_meta), payment_provider = sqlc.arg(payment_provider) WHERE id = sqlc.arg(id);

-- name: StoreAdvisoryXactLock :exec
-- Transaction-scoped advisory lock on a text key.
SELECT pg_advisory_xact_lock(hashtextextended(sqlc.arg(key)::text, 0));

-- name: StorePendingForBuyer :many
SELECT id FROM orders
WHERE status = 'pending_payment'
  AND (reservation_expires_at IS NULL OR reservation_expires_at > now())
  AND (email_norm = sqlc.narg(email_norm)::text OR cart_id = sqlc.arg(cart_id)::uuid
       OR (sqlc.narg(user_id)::uuid IS NOT NULL AND user_id = sqlc.narg(user_id)::uuid))
ORDER BY created_at DESC, id DESC;
