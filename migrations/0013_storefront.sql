-- P1 storefront: new categories, compare price, color hex, product looks,
-- PIX discount, wishlist, newsletter, home banners.

ALTER TYPE product_category ADD VALUE IF NOT EXISTS 'top_biquini';
ALTER TYPE product_category ADD VALUE IF NOT EXISTS 'calcinha_biquini';

ALTER TABLE products
    ADD COLUMN IF NOT EXISTS compare_at_price_in_cents bigint NULL,
    ADD CONSTRAINT compare_at_price_check CHECK (compare_at_price_in_cents IS NULL OR compare_at_price_in_cents > price_in_cents);

ALTER TABLE product_variants
    ADD COLUMN IF NOT EXISTS color_hex text NULL,
    ADD CONSTRAINT color_hex_check CHECK (color_hex IS NULL OR color_hex ~ '^#[0-9A-Fa-f]{6}$');

CREATE TABLE IF NOT EXISTS product_looks (
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    related_product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    sort_order integer NOT NULL DEFAULT 0,
    PRIMARY KEY (product_id, related_product_id),
    CHECK (product_id <> related_product_id)
);

ALTER TABLE store_settings
    ADD COLUMN IF NOT EXISTS pix_discount_percent smallint NOT NULL DEFAULT 5,
    ADD COLUMN IF NOT EXISTS announcement_text text NULL,
    ADD COLUMN IF NOT EXISTS first_purchase_coupon_code text NULL,
    ADD CONSTRAINT pix_discount_percent_check CHECK (pix_discount_percent BETWEEN 0 AND 30);

ALTER TABLE orders
    ADD COLUMN IF NOT EXISTS pix_discount_cents bigint NOT NULL DEFAULT 0;

CREATE TABLE IF NOT EXISTS wishlist_items (
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    created_at timestamptz NOT NULL DEFAULT now(),
    PRIMARY KEY (user_id, product_id)
);

CREATE TABLE IF NOT EXISTS newsletter_subscribers (
    id uuid PRIMARY KEY,
    email text NOT NULL,
    name text NULL,
    created_at timestamptz NOT NULL DEFAULT now()
);
CREATE UNIQUE INDEX IF NOT EXISTS newsletter_subscribers_email_lower_idx
    ON newsletter_subscribers (lower(email));

CREATE TABLE IF NOT EXISTS home_banners (
    id uuid PRIMARY KEY,
    title text NOT NULL,
    subtitle text NULL,
    cta_label text NULL,
    link_url text NULL,
    image_url text NOT NULL,
    sort_order integer NOT NULL DEFAULT 0,
    active boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now()
);
