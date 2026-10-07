CREATE TABLE IF NOT EXISTS carts (
    id uuid PRIMARY KEY NOT NULL,
    token uuid UNIQUE NOT NULL,
    user_id uuid REFERENCES users(id) ON DELETE SET NULL,
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL
);

CREATE UNIQUE INDEX IF NOT EXISTS carts_user_id_uidx
    ON carts (user_id)
    WHERE user_id IS NOT NULL;

CREATE INDEX IF NOT EXISTS carts_token_idx ON carts USING btree (token);

CREATE TABLE IF NOT EXISTS cart_items (
    id uuid PRIMARY KEY NOT NULL,
    cart_id uuid NOT NULL REFERENCES carts(id) ON DELETE CASCADE,
    variant_id uuid NOT NULL REFERENCES product_variants(id) ON DELETE CASCADE,
    quantity integer NOT NULL CHECK (quantity > 0),
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL,
    UNIQUE (cart_id, variant_id)
);

CREATE INDEX IF NOT EXISTS cart_items_cart_id_idx ON cart_items USING btree (cart_id);
CREATE INDEX IF NOT EXISTS cart_items_variant_id_idx ON cart_items USING btree (variant_id);

ALTER TABLE orders
    ADD COLUMN IF NOT EXISTS reservation_expires_at timestamptz;

CREATE INDEX IF NOT EXISTS orders_reservation_expires_at_idx
    ON orders (reservation_expires_at)
    WHERE status = 'pending_payment' AND reservation_expires_at IS NOT NULL;
