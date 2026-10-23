-- Owner: wave1-B (payments service: Mercado Pago charges, sync, admin, sweep).

-- name: PaymentsChargeRow :one
SELECT payment_provider, status, total_cents, guest_email,
       reservation_expires_at, mp_payment_id, mp_qr_code, mp_qr_code_base64,
       mp_ticket_url, mp_expires_at
FROM orders WHERE id = $1;

-- name: PaymentsStoreCharge :exec
UPDATE orders
SET mp_payment_id = sqlc.arg(mp_payment_id)::bigint,
    mp_status = sqlc.arg(mp_status)::text,
    mp_status_detail = sqlc.narg(mp_status_detail)::text,
    mp_qr_code = sqlc.narg(mp_qr_code)::text,
    mp_qr_code_base64 = sqlc.narg(mp_qr_code_base64)::text,
    mp_ticket_url = sqlc.narg(mp_ticket_url)::text,
    mp_expires_at = sqlc.arg(mp_expires_at)::timestamptz,
    mp_synced_at = now(), updated_at = now()
WHERE id = sqlc.arg(id)::uuid
  AND (mp_payment_id IS NULL OR mp_payment_id = sqlc.arg(mp_payment_id)::bigint);

-- name: PaymentsOrderByMPPaymentID :one
SELECT id FROM orders WHERE mp_payment_id = $1;

-- name: PaymentsClaimMPPaymentID :one
-- Bind a payment to its order (by external_reference) when the id was never
-- stored (crash between the MP call and the UPDATE).
UPDATE orders SET mp_payment_id = sqlc.arg(mp_payment_id)::bigint
WHERE id = sqlc.arg(id)::uuid AND payment_provider = 'mercadopago' AND mp_payment_id IS NULL
RETURNING id;

-- name: PaymentsLockOrder :one
SELECT status, (paid_at IS NOT NULL)::bool AS ever_paid, total_cents
FROM orders WHERE id = $1 FOR UPDATE;

-- name: PaymentsSetMPStatus :exec
UPDATE orders
SET mp_status = sqlc.arg(mp_status)::text, mp_status_detail = sqlc.narg(mp_status_detail)::text, mp_synced_at = now()
WHERE id = sqlc.arg(id)::uuid;

-- name: PaymentsAdminInfo :one
SELECT payment_provider, mp_payment_id, mp_status, mp_status_detail, mp_synced_at
FROM orders WHERE id = $1;

-- name: PaymentsOrderStatus :one
SELECT status FROM orders WHERE id = $1;

-- name: PaymentsLockOrderStatus :one
SELECT status FROM orders WHERE id = $1 FOR UPDATE;

-- name: PaymentsMarkRefunded :exec
UPDATE orders SET mp_status = 'refunded', mp_synced_at = now() WHERE id = $1;

-- name: PaymentsExpiredPending :many
-- Expired pending orders for the payment-aware sweep: the MP payment id
-- (MP orders only; 0 = none) and whether the hold is past the grace window.
SELECT id,
       COALESCE(CASE WHEN payment_provider = 'mercadopago' THEN mp_payment_id END, 0)::bigint AS mp_payment_id,
       (reservation_expires_at < now() - make_interval(secs => sqlc.arg(grace_secs)::float8))::bool AS past_grace
FROM orders
WHERE status = 'pending_payment'
  AND reservation_expires_at IS NOT NULL
  AND reservation_expires_at < now()
ORDER BY id;
