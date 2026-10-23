-- Owner: store (abandoned-cart reminders, internal/store/abandonedcart.go).

-- name: StoreRememberCheckoutEmail :exec
UPDATE carts SET contact_email = sqlc.arg(contact_email)::text
WHERE id = sqlc.arg(id) AND contact_email IS DISTINCT FROM sqlc.arg(contact_email)::text;

-- name: StoreAbandonedCandidates :many
-- Carts with an active product, a reachable email (verified account email,
-- else the checkout email), idle for @after_minutes but active within
-- @window_days, not reminded since their last activity (nor within the
-- window), without a later order, unsubscribed address or recent reminder.
WITH activity AS (
    SELECT c.id,
           c.user_id,
           c.abandoned_reminded_at,
           GREATEST(
               c.updated_at,
               (SELECT max(ci.updated_at) FROM cart_items ci WHERE ci.cart_id = c.id)
           ) AS last_activity,
           lower(btrim(COALESCE(
               CASE WHEN u.email_verified THEN u.email END,
               c.contact_email
           ))) AS email,
           u.name
    FROM carts c
    LEFT JOIN users u ON u.id = c.user_id
    WHERE EXISTS (
        SELECT 1 FROM cart_items ci
        INNER JOIN product_variants pv ON pv.id = ci.variant_id
        INNER JOIN products p ON p.id = pv.product_id
        WHERE ci.cart_id = c.id AND p.active = true
    )
)
SELECT a.id, a.email::text AS email, a.name::null_text AS name,
       a.abandoned_reminded_at::null_timestamptz AS reminded_at
FROM activity a
WHERE a.email IS NOT NULL AND a.email <> ''
  AND a.last_activity < now() - make_interval(mins => sqlc.arg(after_minutes)::int)
  AND a.last_activity > now() - make_interval(days => sqlc.arg(window_days)::int)
  AND (
      a.abandoned_reminded_at IS NULL
      OR (a.abandoned_reminded_at < a.last_activity
          AND a.abandoned_reminded_at < now() - make_interval(days => sqlc.arg(window_days)::int))
  )
  AND NOT EXISTS (
      SELECT 1 FROM orders o
      WHERE (o.cart_id = a.id OR (a.user_id IS NOT NULL AND o.user_id = a.user_id))
        AND o.created_at >= a.last_activity
  )
  AND NOT EXISTS (
      SELECT 1 FROM newsletter_subscribers ns
      WHERE lower(ns.email) = a.email AND ns.unsubscribed_at IS NOT NULL
  )
  AND NOT EXISTS (
      SELECT 1 FROM email_outbox eo
      WHERE eo.kind = sqlc.arg(kind)::text AND lower(eo.to_address) = a.email
        AND eo.created_at > now() - make_interval(days => sqlc.arg(window_days)::int)
  )
ORDER BY a.last_activity
LIMIT sqlc.arg(max_rows)::int;

-- name: StoreClaimReminder :execrows
-- Compare-and-set on abandoned_reminded_at: only one sweeper wins a cart.
UPDATE carts SET abandoned_reminded_at = now()
WHERE id = sqlc.arg(id) AND abandoned_reminded_at IS NOT DISTINCT FROM sqlc.narg(reminded_at)::timestamptz;

-- name: StoreRecentReminder :one
SELECT EXISTS (
    SELECT 1 FROM email_outbox
    WHERE kind = sqlc.arg(kind)::text AND lower(to_address) = sqlc.arg(email)::text
      AND created_at > now() - make_interval(days => sqlc.arg(days)::int)
);

-- name: StoreReminderLines :many
SELECT p.name AS product_name, pv.size::text AS size, pv.color,
       ci.quantity,
       COALESCE(pv.price_in_cents, p.price_in_cents)::bigint AS unit_price_cents
FROM cart_items ci
INNER JOIN product_variants pv ON pv.id = ci.variant_id
INNER JOIN products p ON p.id = pv.product_id
WHERE ci.cart_id = $1 AND p.active = true
ORDER BY ci.created_at, ci.id;
