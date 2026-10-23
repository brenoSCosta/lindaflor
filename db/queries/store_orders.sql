-- Owner: store (order state machine, internal/store/order.go).

-- name: StoreApplyLock :one
-- expired is read before side effects: releasing the hold clears
-- reservation_expires_at.
SELECT status::text AS status, (paid_at IS NOT NULL)::bool AS paid,
       COALESCE(reservation_expires_at < now(), false)::bool AS expired
FROM orders WHERE id = $1 FOR UPDATE;

-- name: StoreSetStatusPaid :exec
UPDATE orders SET status = sqlc.arg(status)::order_status, paid_at = now(), updated_at = now() WHERE id = sqlc.arg(id);

-- name: StoreSetStatus :exec
UPDATE orders SET status = sqlc.arg(status)::order_status, updated_at = now() WHERE id = sqlc.arg(id);

-- name: StoreSetStatusShip :exec
-- The guest link keeps working for 90 days after shipping.
UPDATE orders SET status = sqlc.arg(status)::order_status, carrier = sqlc.arg(carrier), tracking_code = sqlc.arg(tracking_code)::text,
        shipped_at = now(),
        access_token_expires_at = GREATEST(access_token_expires_at, now() + interval '90 days'),
        updated_at = now()
WHERE id = sqlc.arg(id);

-- name: StoreSetStatusDeliver :exec
UPDATE orders SET status = sqlc.arg(status)::order_status, delivered_at = now(),
        access_token_expires_at = GREATEST(access_token_expires_at, now() + interval '90 days'),
        updated_at = now()
WHERE id = sqlc.arg(id);

-- name: StoreExpiredPending :many
SELECT id FROM orders
WHERE status = 'pending_payment'
  AND reservation_expires_at IS NOT NULL
  AND reservation_expires_at < now()
ORDER BY id;

-- name: StoreClaimExpiredOne :one
SELECT id FROM orders
WHERE id = $1
  AND status = 'pending_payment'
  AND reservation_expires_at < now()
FOR UPDATE SKIP LOCKED;
