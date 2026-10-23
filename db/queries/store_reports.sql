-- Owner: store (admin reports and CSV export, internal/store/reports.go).
--
-- Ranges are inclusive Brasília calendar days [@from_day, @to_day]:
--   start = (@from_day::date)::timestamp AT TIME ZONE 'America/Sao_Paulo'
--   end   = (@to_day::date + 1)::timestamp AT TIME ZONE 'America/Sao_Paulo'
-- Sales are orders in ('paid', 'processing', 'shipped', 'delivered')
-- bucketed by COALESCE(paid_at, created_at) (orders_sale_at_idx).

-- name: StoreReportSummary :one
WITH sales AS (
    SELECT COALESCE(SUM(o.total_cents), 0)::bigint AS revenue_cents,
           COUNT(*) AS orders,
           COALESCE(SUM(o.discount_cents), 0)::bigint AS discount_cents,
           COALESCE(SUM(o.pix_discount_cents), 0)::bigint AS pix_discount_cents,
           COALESCE(SUM(o.shipping_cents), 0)::bigint AS shipping_cents
    FROM orders o
    WHERE o.status IN ('paid', 'processing', 'shipped', 'delivered')
      AND COALESCE(o.paid_at, o.created_at) >= ((sqlc.arg(from_day)::date)::timestamp AT TIME ZONE 'America/Sao_Paulo')
      AND COALESCE(o.paid_at, o.created_at) < (((sqlc.arg(to_day)::date) + 1)::timestamp AT TIME ZONE 'America/Sao_Paulo')
),
created AS (
    SELECT COUNT(*) FILTER (WHERE o.status = 'pending_payment') AS pending_orders,
           COUNT(*) FILTER (WHERE o.status = 'cancelled' AND o.paid_at IS NULL) AS abandoned_orders,
           COUNT(*) FILTER (WHERE o.status = 'refunded') AS refunded_orders
    FROM orders o
    WHERE o.created_at >= ((sqlc.arg(from_day)::date)::timestamp AT TIME ZONE 'America/Sao_Paulo')
      AND o.created_at < (((sqlc.arg(to_day)::date) + 1)::timestamp AT TIME ZONE 'America/Sao_Paulo')
),
partial AS (
    SELECT COALESCE(SUM(r.refund_cents), 0)::bigint AS partial_refund_cents
    FROM return_requests r
    JOIN orders o ON o.id = r.order_id
    WHERE r.status = 'resolved' AND r.resolution = 'refunded'
      AND o.status <> 'refunded'
      AND r.resolved_at >= ((sqlc.arg(from_day)::date)::timestamp AT TIME ZONE 'America/Sao_Paulo')
      AND r.resolved_at < (((sqlc.arg(to_day)::date) + 1)::timestamp AT TIME ZONE 'America/Sao_Paulo')
)
SELECT sales.revenue_cents, sales.orders, sales.discount_cents, sales.pix_discount_cents, sales.shipping_cents,
       created.pending_orders, created.abandoned_orders, created.refunded_orders,
       partial.partial_refund_cents
FROM sales, created, partial;

-- name: StoreRevenueByDay :many
WITH days AS (
    SELECT generate_series(sqlc.arg(from_day)::date, sqlc.arg(to_day)::date, interval '1 day')::date AS day
),
sales AS (
    SELECT (COALESCE(o.paid_at, o.created_at) AT TIME ZONE 'America/Sao_Paulo')::date AS day,
           COUNT(*) AS orders,
           SUM(o.total_cents)::bigint AS revenue_cents
    FROM orders o
    WHERE o.status IN ('paid', 'processing', 'shipped', 'delivered')
      AND COALESCE(o.paid_at, o.created_at) >= ((sqlc.arg(from_day)::date)::timestamp AT TIME ZONE 'America/Sao_Paulo')
      AND COALESCE(o.paid_at, o.created_at) < (((sqlc.arg(to_day)::date) + 1)::timestamp AT TIME ZONE 'America/Sao_Paulo')
    GROUP BY 1
)
SELECT d.day, COALESCE(s.orders, 0)::bigint AS orders,
       COALESCE(s.revenue_cents, 0)::bigint AS revenue_cents
FROM days d LEFT JOIN sales s ON s.day = d.day
ORDER BY d.day;

-- name: StoreTopVariants :many
SELECT MAX(oi.product_name)::text AS name, MAX(oi.variant_label)::text AS detail,
       SUM(oi.quantity)::bigint AS units,
       SUM(oi.quantity * oi.unit_price_cents)::bigint AS revenue_cents
FROM orders o
JOIN order_items oi ON oi.order_id = o.id
JOIN product_variants v ON v.id = oi.variant_id
WHERE o.status IN ('paid', 'processing', 'shipped', 'delivered')
  AND COALESCE(o.paid_at, o.created_at) >= ((sqlc.arg(from_day)::date)::timestamp AT TIME ZONE 'America/Sao_Paulo')
  AND COALESCE(o.paid_at, o.created_at) < (((sqlc.arg(to_day)::date) + 1)::timestamp AT TIME ZONE 'America/Sao_Paulo')
GROUP BY oi.variant_id
ORDER BY units DESC, revenue_cents DESC, name
LIMIT sqlc.arg(max_rows)::bigint;

-- name: StoreTopProducts :many
SELECT MAX(oi.product_name)::text AS name, ''::text AS detail,
       SUM(oi.quantity)::bigint AS units,
       SUM(oi.quantity * oi.unit_price_cents)::bigint AS revenue_cents
FROM orders o
JOIN order_items oi ON oi.order_id = o.id
JOIN product_variants v ON v.id = oi.variant_id
WHERE o.status IN ('paid', 'processing', 'shipped', 'delivered')
  AND COALESCE(o.paid_at, o.created_at) >= ((sqlc.arg(from_day)::date)::timestamp AT TIME ZONE 'America/Sao_Paulo')
  AND COALESCE(o.paid_at, o.created_at) < (((sqlc.arg(to_day)::date) + 1)::timestamp AT TIME ZONE 'America/Sao_Paulo')
GROUP BY v.product_id
ORDER BY revenue_cents DESC, units DESC, name
LIMIT sqlc.arg(max_rows)::bigint;

-- name: StoreCouponUsage :many
SELECT upper(COALESCE(c.code, o.payment_meta->>'coupon_code'))::text AS code,
       COUNT(*) AS orders,
       SUM(o.discount_cents)::bigint AS discount_cents,
       SUM(o.total_cents)::bigint AS revenue_cents
FROM orders o
LEFT JOIN coupons c ON c.id = o.coupon_id
WHERE o.status IN ('paid', 'processing', 'shipped', 'delivered')
  AND COALESCE(o.paid_at, o.created_at) >= ((sqlc.arg(from_day)::date)::timestamp AT TIME ZONE 'America/Sao_Paulo')
  AND COALESCE(o.paid_at, o.created_at) < (((sqlc.arg(to_day)::date) + 1)::timestamp AT TIME ZONE 'America/Sao_Paulo')
  AND COALESCE(c.code, NULLIF(o.payment_meta->>'coupon_code', '')) IS NOT NULL
GROUP BY 1
ORDER BY orders DESC, discount_cents DESC, code;

-- name: StoreProviderSplit :many
SELECT o.payment_provider AS provider, COUNT(*) AS orders,
       SUM(o.total_cents)::bigint AS revenue_cents
FROM orders o
WHERE o.status IN ('paid', 'processing', 'shipped', 'delivered')
  AND COALESCE(o.paid_at, o.created_at) >= ((sqlc.arg(from_day)::date)::timestamp AT TIME ZONE 'America/Sao_Paulo')
  AND COALESCE(o.paid_at, o.created_at) < (((sqlc.arg(to_day)::date) + 1)::timestamp AT TIME ZONE 'America/Sao_Paulo')
GROUP BY 1
ORDER BY revenue_cents DESC;

-- name: StoreCSVOrders :many
-- Orders created in range (any status), oldest first.
SELECT o.id,
       to_char(o.created_at AT TIME ZONE 'America/Sao_Paulo', 'YYYY-MM-DD HH24:MI') AS created_at,
       to_char(o.paid_at AT TIME ZONE 'America/Sao_Paulo', 'YYYY-MM-DD HH24:MI')::null_text AS paid_at,
       o.status::text AS status,
       COALESCE(NULLIF(btrim(o.guest_email), ''), u.email)::null_text AS email,
       (o.shipping_address->>'state')::null_text AS uf,
       (o.shipping_address->>'city')::null_text AS city,
       items.summary::null_text AS items,
       COALESCE(items.units, 0)::bigint AS units,
       o.subtotal_cents, o.shipping_cents, o.discount_cents,
       o.pix_discount_cents, o.total_cents,
       o.payment_provider AS provider,
       COALESCE(c.code, NULLIF(o.payment_meta->>'coupon_code', ''))::null_text AS coupon,
       o.carrier, o.tracking_code,
       to_char(o.shipped_at AT TIME ZONE 'America/Sao_Paulo', 'YYYY-MM-DD HH24:MI')::null_text AS shipped_at,
       to_char(o.delivered_at AT TIME ZONE 'America/Sao_Paulo', 'YYYY-MM-DD HH24:MI')::null_text AS delivered_at
FROM orders o
LEFT JOIN users u ON u.id = o.user_id
LEFT JOIN coupons c ON c.id = o.coupon_id
LEFT JOIN LATERAL (
    SELECT string_agg(
               oi.quantity || 'x ' || oi.product_name || ' (' || oi.variant_label || ')',
               '; ' ORDER BY oi.created_at, oi.id) AS summary,
           SUM(oi.quantity) AS units
    FROM order_items oi WHERE oi.order_id = o.id
) items ON true
WHERE o.created_at >= ((sqlc.arg(from_day)::date)::timestamp AT TIME ZONE 'America/Sao_Paulo')
  AND o.created_at < (((sqlc.arg(to_day)::date) + 1)::timestamp AT TIME ZONE 'America/Sao_Paulo')
ORDER BY o.created_at, o.id
LIMIT sqlc.arg(max_rows)::bigint;
