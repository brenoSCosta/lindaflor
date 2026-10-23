-- Owner: store (inventory holds and movements, internal/store/inventory.go).
--
-- Lock order invariant: every transaction that writes inventory takes all
-- the row locks it needs in one statement, `ORDER BY i.id FOR UPDATE`.

-- name: StoreLockInventoryIDs :many
SELECT i.id, i.variant_id, i.quantity, i.reserved, w.is_default
FROM inventory i
INNER JOIN warehouses w ON w.id = i.warehouse_id
WHERE i.id = ANY(sqlc.arg(ids)::uuid[])
ORDER BY i.id
FOR UPDATE OF i;

-- name: StoreLockVariantInventory :many
SELECT i.id, i.variant_id, i.quantity, i.reserved, w.is_default
FROM inventory i
INNER JOIN warehouses w ON w.id = i.warehouse_id
WHERE i.variant_id = ANY(sqlc.arg(variant_ids)::uuid[]) AND w.active = true
ORDER BY i.id
FOR UPDATE OF i;

-- name: StoreCartInventoryRows :many
SELECT i.id
FROM inventory i
INNER JOIN warehouses w ON w.id = i.warehouse_id
INNER JOIN cart_items ci ON ci.variant_id = i.variant_id
WHERE ci.cart_id = $1 AND w.active = true;

-- name: StoreExpiredHolders :many
SELECT r.order_id, r.inventory_id
FROM inventory_reservations r
WHERE r.order_id IN (
    SELECT r2.order_id
    FROM inventory_reservations r2
    INNER JOIN orders o ON o.id = r2.order_id
    WHERE r2.inventory_id = ANY(sqlc.arg(inventory_ids)::uuid[])
      AND o.status = 'pending_payment'
      AND o.reservation_expires_at < now()
);

-- name: StoreClaimExpiredOrders :many
SELECT id FROM orders
WHERE id = ANY(sqlc.arg(ids)::uuid[])
  AND status = 'pending_payment'
  AND reservation_expires_at < now()
ORDER BY id
FOR UPDATE SKIP LOCKED;

-- name: StoreReserveInventory :exec
UPDATE inventory SET reserved = reserved + sqlc.arg(quantity)::int, updated_at = now() WHERE id = sqlc.arg(id);

-- name: StoreInsertReservation :exec
INSERT INTO inventory_reservations (id, order_id, inventory_id, quantity)
VALUES ($1, $2, $3, $4);

-- name: StoreSetReservationExpiry :exec
UPDATE orders SET reservation_expires_at = now() + interval '48 hours' WHERE id = $1;

-- name: StoreReservationRows :many
SELECT inventory_id, quantity FROM inventory_reservations WHERE order_id = $1;

-- name: StoreConvertInventory :exec
UPDATE inventory
SET reserved = GREATEST(reserved - sqlc.arg(quantity)::int, 0),
    quantity = GREATEST(quantity - sqlc.arg(quantity)::int, 0),
    updated_at = now()
WHERE id = sqlc.arg(id);

-- name: StoreReleaseInventory :exec
UPDATE inventory SET reserved = GREATEST(reserved - sqlc.arg(quantity)::int, 0), updated_at = now() WHERE id = sqlc.arg(id);

-- name: StoreClearReservations :exec
DELETE FROM inventory_reservations WHERE order_id = $1;

-- name: StoreClearReservationAt :exec
UPDATE orders SET reservation_expires_at = NULL WHERE id = $1;

-- name: StoreOrderItemQuantities :many
SELECT variant_id, quantity FROM order_items WHERE order_id = $1;

-- name: StoreRestockInventory :exec
UPDATE inventory SET quantity = quantity + sqlc.arg(quantity)::int, updated_at = now() WHERE id = sqlc.arg(id);

-- name: StoreAdjustLock :one
SELECT quantity, reserved, warehouse_id FROM inventory WHERE id = $1 FOR UPDATE;

-- name: StoreAdjustUpdate :exec
UPDATE inventory SET quantity = $2, updated_at = now() WHERE id = $1;
