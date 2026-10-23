-- Owner: store (returns / exchanges, internal/store/returns.go).

-- name: StoreReturnableItems :many
SELECT oi.id AS order_item_id, oi.product_name, oi.variant_label,
       oi.quantity, oi.unit_price_cents,
       GREATEST(oi.quantity - COALESCE((
           SELECT SUM(ri.quantity)
           FROM return_request_items ri
           JOIN return_requests r ON r.id = ri.return_request_id
           WHERE ri.order_item_id = oi.id AND r.status <> 'rejected'
       ), 0), 0)::int AS returnable
FROM order_items oi
WHERE oi.order_id = $1
ORDER BY oi.created_at, oi.id;

-- name: StoreReturnOrderLock :one
SELECT status::text AS status, access_token,
       (access_token_expires_at <= now())::bool AS token_expired, user_id,
       delivered_at, shipped_at, now()::timestamptz AS now
FROM orders WHERE id = $1 FOR UPDATE;

-- name: StoreReturnOpenExists :one
SELECT EXISTS (SELECT 1 FROM return_requests
               WHERE order_id = $1
                 AND status IN ('requested', 'approved', 'received'));

-- name: StoreInsertReturn :exec
INSERT INTO return_requests (id, order_id, user_id, kind, reason, desired_size)
VALUES ($1, $2, $3, $4, $5, $6);

-- name: StoreInsertReturnItem :exec
INSERT INTO return_request_items (return_request_id, order_item_id, quantity)
VALUES ($1, $2, $3);

-- name: StoreRequestsForOrder :many
SELECT r.id, r.order_id, r.kind, r.status, r.resolution,
       r.created_at, r.refund_cents, r.instructions, r.rejection_reason
FROM return_requests r
WHERE r.order_id = $1 ORDER BY r.created_at DESC;

-- name: StoreOrderReturnState :one
SELECT status::text AS status, delivered_at, shipped_at, now()::timestamptz AS now
FROM orders WHERE id = $1;

-- name: StoreLockReturn :one
SELECT order_id, kind, status FROM return_requests WHERE id = $1 FOR UPDATE;

-- name: StoreReturnedLines :many
SELECT oi.variant_id, ri.quantity, oi.unit_price_cents
FROM return_request_items ri
JOIN order_items oi ON oi.id = ri.order_item_id
WHERE ri.return_request_id = $1
ORDER BY oi.variant_id;

-- name: StoreCoversWholeOrder :one
SELECT NOT EXISTS (
    SELECT 1 FROM order_items oi
    JOIN return_requests r ON r.order_id = oi.order_id
    LEFT JOIN return_request_items ri
      ON ri.order_item_id = oi.id AND ri.return_request_id = r.id
    WHERE r.id = $1 AND COALESCE(ri.quantity, 0) <> oi.quantity
) AS whole;

-- name: StoreRefundOptions :one
SELECT o.status::text AS status, o.payment_provider, o.total_cents,
       COALESCE((SELECT SUM(ri.quantity * oi.unit_price_cents)
                 FROM return_request_items ri
                 JOIN order_items oi ON oi.id = ri.order_item_id
                 WHERE ri.return_request_id = r.id), 0)::bigint AS items_cents
FROM return_requests r JOIN orders o ON o.id = r.order_id
WHERE r.id = $1;

-- name: StoreReturnTouch :exec
UPDATE return_requests SET updated_at = now() WHERE id = $1;

-- name: StoreReturnApprove :exec
UPDATE return_requests SET status = 'approved', instructions = sqlc.arg(instructions)::text, approved_at = now(), updated_at = now() WHERE id = sqlc.arg(id);

-- name: StoreReturnReject :exec
UPDATE return_requests SET status = 'rejected', rejection_reason = sqlc.arg(rejection_reason)::text, updated_at = now() WHERE id = sqlc.arg(id);

-- name: StoreReturnReceive :exec
UPDATE return_requests SET status = 'received', received_at = now(), updated_at = now() WHERE id = $1;

-- name: StoreReturnResolve :exec
UPDATE return_requests
SET status = 'resolved', resolution = sqlc.arg(resolution)::text, refund_cents = sqlc.narg(refund_cents),
    resolved_at = now(), updated_at = now()
WHERE id = sqlc.arg(id);

-- name: StoreOrderProvider :one
SELECT payment_provider FROM orders WHERE id = $1;

-- name: StoreOrderTotal :one
SELECT total_cents FROM orders WHERE id = $1;

-- name: StoreOrderStatusTotalLock :one
SELECT status::text AS status, total_cents FROM orders WHERE id = $1 FOR UPDATE;
