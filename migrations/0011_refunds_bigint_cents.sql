-- P1 commerce: order.refunded, bigint money, inventory CHECKs,
-- coupon restore/guest key, storefront index, paid_at.

ALTER TYPE order_status ADD VALUE IF NOT EXISTS 'refunded';

ALTER TABLE orders
    ADD COLUMN IF NOT EXISTS paid_at timestamptz NULL;

ALTER TABLE products
    ALTER COLUMN price_in_cents TYPE bigint;
ALTER TABLE product_variants
    ALTER COLUMN price_in_cents TYPE bigint;
ALTER TABLE orders
    ALTER COLUMN subtotal_cents TYPE bigint,
    ALTER COLUMN shipping_cents TYPE bigint,
    ALTER COLUMN discount_cents TYPE bigint,
    ALTER COLUMN total_cents TYPE bigint;
ALTER TABLE order_items
    ALTER COLUMN unit_price_cents TYPE bigint;
ALTER TABLE coupons
    ALTER COLUMN discount_value TYPE bigint,
    ALTER COLUMN min_subtotal_cents TYPE bigint,
    ALTER COLUMN max_discount_cents TYPE bigint;

ALTER TABLE coupon_redemptions
    ADD COLUMN IF NOT EXISTS guest_email text,
    ADD COLUMN IF NOT EXISTS refunded_at timestamptz;

CREATE INDEX IF NOT EXISTS coupon_redemptions_active_idx
    ON coupon_redemptions (coupon_id)
    WHERE refunded_at IS NULL;
CREATE INDEX IF NOT EXISTS coupon_redemptions_guest_email_idx
    ON coupon_redemptions (coupon_id, guest_email)
    WHERE guest_email IS NOT NULL AND refunded_at IS NULL;

UPDATE inventory
SET
    quantity = GREATEST(quantity, 0),
    reserved = GREATEST(LEAST(reserved, GREATEST(quantity, 0)), 0);

ALTER TABLE inventory
    DROP CONSTRAINT IF EXISTS inventory_non_negative,
    DROP CONSTRAINT IF EXISTS inventory_reserved_not_over_quantity;
ALTER TABLE inventory
    ADD CONSTRAINT inventory_non_negative CHECK (quantity >= 0 AND reserved >= 0),
    ADD CONSTRAINT inventory_reserved_not_over_quantity CHECK (reserved <= quantity);

CREATE INDEX IF NOT EXISTS products_storefront_idx
    ON products (active, featured, created_at DESC);
