-- Owner: store (admin dashboard, stock, orders, returns; internal/store/admin_ops.go).

-- name: StoreAdminCountProducts :one
SELECT COUNT(*) FROM products;

-- name: StoreAdminCountOrders :one
SELECT COUNT(*) FROM orders;

-- name: StoreAdminTotalRevenue :one
SELECT COALESCE(SUM(total_cents), 0)::bigint FROM orders WHERE status IN ('paid', 'shipped', 'delivered');

-- name: StoreAdminLowStockCount :one
SELECT COUNT(*) FROM (
    SELECT v.id
    FROM product_variants v
    INNER JOIN products p ON p.id = v.product_id
    LEFT JOIN inventory i
      ON i.variant_id = v.id
     AND i.warehouse_id IN (SELECT id FROM warehouses WHERE active)
    WHERE p.active
    GROUP BY v.id, v.low_stock_threshold
    HAVING COALESCE(SUM(i.quantity - i.reserved), 0) <= v.low_stock_threshold
) low;

-- name: StoreAdminRecentOrders :many
SELECT o.id, o.guest_email, o.status::text AS status, o.total_cents, o.created_at,
       o.carrier, o.tracking_code, 0::bigint AS item_count
FROM orders o ORDER BY o.created_at DESC LIMIT 10;

-- name: StoreAdminStockRows :many
-- query is LIKE-escaped by the caller ('' = everything).
SELECT i.id, i.variant_id, i.warehouse_id, i.quantity, i.reserved,
       p.name AS product_name, pv.sku, pv.size::text AS size, pv.color,
       w.name AS warehouse_name, w.code AS warehouse_code
FROM inventory i
JOIN product_variants pv ON pv.id = i.variant_id
JOIN products p ON p.id = pv.product_id
JOIN warehouses w ON w.id = i.warehouse_id
WHERE (sqlc.arg(query)::text = ''
       OR p.name ILIKE '%' || sqlc.arg(query)::text || '%' ESCAPE '\'
       OR pv.sku ILIKE '%' || sqlc.arg(query)::text || '%' ESCAPE '\')
ORDER BY p.name, pv.sku;

-- name: StoreAdminWarehouses :many
SELECT id, code, name, is_default FROM warehouses ORDER BY name;

-- name: StoreAdminOrderCount :one
SELECT COUNT(*) FROM orders o
WHERE (sqlc.arg(query)::text = ''
       OR o.guest_email ILIKE '%' || sqlc.arg(query)::text || '%' ESCAPE '\'
       OR o.tracking_code ILIKE '%' || sqlc.arg(query)::text || '%' ESCAPE '\')
  AND (sqlc.arg(status)::text = 'all' OR o.status::text = sqlc.arg(status)::text);

-- name: StoreAdminOrderList :many
SELECT o.id, o.guest_email, o.status::text AS status, o.total_cents, o.created_at,
       o.carrier, o.tracking_code,
       (SELECT COUNT(*) FROM order_items oi WHERE oi.order_id = o.id) AS item_count
FROM orders o
WHERE (sqlc.arg(query)::text = ''
       OR o.guest_email ILIKE '%' || sqlc.arg(query)::text || '%' ESCAPE '\'
       OR o.tracking_code ILIKE '%' || sqlc.arg(query)::text || '%' ESCAPE '\')
  AND (sqlc.arg(status)::text = 'all' OR o.status::text = sqlc.arg(status)::text)
ORDER BY o.created_at DESC
LIMIT sqlc.arg(page_size)::bigint OFFSET sqlc.arg(page_offset)::bigint;

-- name: StoreAdminOrder :one
SELECT id, status::text AS status, guest_email, subtotal_cents, shipping_cents,
       discount_cents, total_cents, payment_provider, shipping_address, payment_meta,
       (paid_at IS NOT NULL)::bool AS paid,
       to_char(reservation_expires_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI')::null_text AS reservation_expires_at,
       to_char(created_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS created_at,
       carrier, tracking_code,
       to_char(shipped_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI')::null_text AS shipped_at,
       to_char(delivered_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI')::null_text AS delivered_at
FROM orders WHERE id = $1;

-- name: StoreAdminReturns :many
SELECT r.id, r.order_id, r.kind, r.status, r.resolution, r.created_at,
       r.refund_cents, r.instructions, r.rejection_reason,
       COALESCE(NULLIF(btrim(o.guest_email), ''), u.email)::null_text AS email,
       COALESCE((SELECT SUM(ri.quantity) FROM return_request_items ri
                 WHERE ri.return_request_id = r.id), 0)::bigint AS units
FROM return_requests r
JOIN orders o ON o.id = r.order_id
LEFT JOIN users u ON u.id = o.user_id
WHERE CASE sqlc.arg(filter)::text
        WHEN 'todas' THEN true
        WHEN 'abertas' THEN r.status IN ('requested', 'approved', 'received')
        ELSE r.status = sqlc.arg(filter)::text
      END
ORDER BY r.created_at DESC
LIMIT sqlc.arg(max_rows)::bigint;

-- name: StoreAdminReturn :one
SELECT r.id, r.order_id, r.kind, r.status, r.resolution, r.created_at,
       r.refund_cents, r.instructions, r.rejection_reason, r.reason,
       r.desired_size,
       COALESCE(NULLIF(btrim(o.guest_email), ''), u.email)::null_text AS email,
       o.status::text AS order_status, o.total_cents AS order_total_cents
FROM return_requests r
JOIN orders o ON o.id = r.order_id
LEFT JOIN users u ON u.id = o.user_id
WHERE r.id = $1;

-- name: StoreAdminReturnItems :many
SELECT oi.product_name, oi.variant_label, ri.quantity,
       oi.quantity AS ordered, oi.unit_price_cents
FROM return_request_items ri
JOIN order_items oi ON oi.id = ri.order_item_id
WHERE ri.return_request_id = $1
ORDER BY oi.created_at, oi.id;

-- name: StoreAdminReturnAudit :many
SELECT occurred_at, actor_name, metadata FROM audit_events
WHERE aggregate_type = 'return' AND aggregate_id = $1
ORDER BY occurred_at, id;
