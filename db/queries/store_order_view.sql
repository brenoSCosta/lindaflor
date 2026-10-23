-- Owner: store (customer order page / account orders, internal/store/orderview.go).

-- name: StoreOrderDetail :one
SELECT id, status::text AS status, user_id, access_token,
       (access_token_expires_at <= now())::bool AS token_expired,
       guest_email, subtotal_cents, shipping_cents,
       discount_cents, pix_discount_cents, total_cents, payment_provider, shipping_address, payment_meta,
       to_char(created_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS created_at
FROM orders WHERE id = $1;

-- name: StoreOrderItems :many
SELECT product_name, variant_label, quantity, unit_price_cents FROM order_items WHERE order_id = $1;

-- name: StoreOrderProgress :one
-- Columns match store.OrderProgress (store.ProgressColumns).
SELECT o.status::text AS status,
       to_char(o.created_at, 'DD/MM/YYYY HH24:MI')::null_text AS created_at,
       to_char(o.paid_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI')::null_text AS paid_at,
       to_char(o.shipped_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI')::null_text AS shipped_at,
       to_char(o.delivered_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI')::null_text AS delivered_at,
       o.carrier, o.tracking_code
FROM orders o WHERE o.id = $1;

-- name: StoreStatusAccess :one
SELECT status::text AS status, access_token,
       (access_token_expires_at <= now())::bool AS token_expired, user_id
FROM orders WHERE id = $1;

-- name: StoreStatusAccessLock :one
SELECT status::text AS status, access_token,
       (access_token_expires_at <= now())::bool AS token_expired, user_id
FROM orders WHERE id = $1 FOR UPDATE;

-- name: StoreRecentOrders :many
SELECT id, guest_email, status::text AS status, total_cents, created_at, carrier, tracking_code
FROM orders WHERE user_id = sqlc.arg(user_id)::uuid ORDER BY created_at DESC LIMIT 5;

-- name: StoreUserOrderCount :one
SELECT COUNT(*) FROM orders WHERE user_id = sqlc.arg(user_id)::uuid;

-- name: StoreUserOrdersPage :many
SELECT id, guest_email, status::text AS status, total_cents, created_at, carrier, tracking_code
FROM orders WHERE user_id = sqlc.arg(user_id)::uuid
ORDER BY created_at DESC LIMIT sqlc.arg(page_size)::bigint OFFSET sqlc.arg(page_offset)::bigint;
