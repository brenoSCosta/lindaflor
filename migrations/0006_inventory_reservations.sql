CREATE TABLE IF NOT EXISTS inventory_reservations (
    id uuid PRIMARY KEY NOT NULL,
    order_id uuid NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    inventory_id uuid NOT NULL REFERENCES inventory(id),
    quantity integer NOT NULL CHECK (quantity > 0),
    created_at timestamptz DEFAULT now() NOT NULL
);

CREATE INDEX IF NOT EXISTS inventory_reservations_order_id_idx
    ON inventory_reservations USING btree (order_id);

CREATE INDEX IF NOT EXISTS inventory_reservations_inventory_id_idx
    ON inventory_reservations USING btree (inventory_id);

ALTER TABLE carts
    ADD COLUMN IF NOT EXISTS coupon_code text;

ALTER TABLE orders
    ADD COLUMN IF NOT EXISTS access_token uuid;

UPDATE orders
SET access_token = gen_random_uuid()
WHERE access_token IS NULL;

ALTER TABLE orders
    ALTER COLUMN access_token SET NOT NULL;

CREATE UNIQUE INDEX IF NOT EXISTS orders_access_token_uidx
    ON orders (access_token);
