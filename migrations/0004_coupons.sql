CREATE TABLE IF NOT EXISTS coupons (
    id uuid PRIMARY KEY NOT NULL,
    code text UNIQUE NOT NULL,
    discount_type text NOT NULL CHECK (discount_type IN ('fixed', 'percent')),
    discount_value integer NOT NULL CHECK (discount_value > 0),
    min_subtotal_cents integer NOT NULL DEFAULT 0,
    max_discount_cents integer CHECK (max_discount_cents IS NULL OR max_discount_cents > 0),
    active boolean NOT NULL DEFAULT true,
    starts_at timestamptz NULL,
    expires_at timestamptz NULL,
    usage_type text NOT NULL DEFAULT 'unlimited' CHECK (usage_type IN ('unique', 'unlimited')),
    max_uses integer CHECK (max_uses IS NULL OR max_uses > 0),
    per_user_limit integer NOT NULL DEFAULT 1 CHECK (per_user_limit > 0),
    created_at timestamptz DEFAULT now() NOT NULL,
    updated_at timestamptz DEFAULT now() NOT NULL,
    CHECK (discount_type != 'percent' OR (discount_value BETWEEN 1 AND 50))
);

CREATE TABLE IF NOT EXISTS coupon_assignments (
    coupon_id uuid NOT NULL REFERENCES coupons(id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    assigned_at timestamptz DEFAULT now() NOT NULL,
    PRIMARY KEY (coupon_id, user_id)
);

CREATE TABLE IF NOT EXISTS coupon_redemptions (
    id uuid PRIMARY KEY NOT NULL,
    coupon_id uuid NOT NULL REFERENCES coupons(id) ON DELETE CASCADE,
    user_id uuid REFERENCES users(id) ON DELETE SET NULL,
    order_id uuid NOT NULL UNIQUE REFERENCES orders(id) ON DELETE CASCADE,
    redeemed_at timestamptz DEFAULT now() NOT NULL
);

CREATE INDEX IF NOT EXISTS coupon_redemptions_coupon_id_idx ON coupon_redemptions USING btree (coupon_id);
CREATE INDEX IF NOT EXISTS coupon_redemptions_user_id_idx ON coupon_redemptions USING btree (user_id);
CREATE INDEX IF NOT EXISTS coupon_redemptions_coupon_user_idx ON coupon_redemptions USING btree (coupon_id, user_id);
CREATE INDEX IF NOT EXISTS coupon_assignments_user_id_idx ON coupon_assignments USING btree (user_id);
CREATE INDEX IF NOT EXISTS coupons_code_upper_idx ON coupons (upper(code));

ALTER TABLE orders ADD COLUMN IF NOT EXISTS coupon_id uuid REFERENCES coupons(id) ON DELETE SET NULL;

-- Ensure orders.user_id FK exists (orders.user_id is nullable; preserve orders on user delete).
DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'orders_user_id_users_id_fk'
    ) THEN
        ALTER TABLE orders
            ADD CONSTRAINT orders_user_id_users_id_fk
            FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE SET NULL;
    END IF;
END
$$;

CREATE INDEX IF NOT EXISTS orders_user_id_idx ON orders USING btree (user_id);
CREATE INDEX IF NOT EXISTS orders_coupon_id_idx ON orders USING btree (coupon_id);
