-- Owner: maintenance (periodic data hygiene, internal/maintenance).

-- name: MaintenanceSweepCarts :execrows
-- Guest carts untouched for 30 days (the cart cookie lifetime), or empty
-- and untouched for a day; cart_items cascade. At most @batch rows.
DELETE FROM carts WHERE id IN (
    SELECT c.id FROM carts c
    WHERE c.user_id IS NULL
      AND (
        c.updated_at < now() - interval '30 days'
        OR (
          c.updated_at < now() - interval '1 day'
          AND NOT EXISTS (SELECT 1 FROM cart_items i WHERE i.cart_id = c.id)
        )
      )
    LIMIT sqlc.arg(batch)::int
);

-- name: MaintenanceSweepVerifications :execrows
-- Expired verification rows, at most @batch.
DELETE FROM verifications WHERE id IN (
    SELECT v.id FROM verifications v WHERE v.expires_at < now() LIMIT sqlc.arg(batch)::int
);
