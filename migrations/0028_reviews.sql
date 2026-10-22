-- Product reviews from verified buyers (see `src/app/store/reviews.rs`).
-- One review per user per product; shown on the storefront only once an
-- admin approves it. Reviews are personal content: they go with the
-- account (LGPD), while the order link is kept only as provenance.
CREATE TYPE review_status AS ENUM ('pending', 'approved', 'rejected');
CREATE TYPE review_fit AS ENUM ('pequeno', 'ideal', 'grande');

CREATE TABLE product_reviews (
    id uuid PRIMARY KEY NOT NULL,
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    order_id uuid REFERENCES orders(id) ON DELETE SET NULL,
    rating smallint NOT NULL CHECK (rating BETWEEN 1 AND 5),
    title text NOT NULL CHECK (char_length(title) BETWEEN 1 AND 80),
    body text NOT NULL CHECK (char_length(body) BETWEEN 1 AND 2000),
    fit review_fit,
    status review_status NOT NULL DEFAULT 'pending',
    created_at timestamptz NOT NULL DEFAULT now(),
    moderated_at timestamptz,
    moderated_by uuid REFERENCES users(id) ON DELETE SET NULL,
    CONSTRAINT product_reviews_user_product_uidx UNIQUE (user_id, product_id)
);

-- Storefront list + aggregates read only approved rows per product.
CREATE INDEX product_reviews_approved_idx
    ON product_reviews (product_id, created_at DESC) WHERE status = 'approved';
-- Admin queue: pending first.
CREATE INDEX product_reviews_status_idx
    ON product_reviews (status, created_at);
-- Eligibility looks up order items by variant.
CREATE INDEX IF NOT EXISTS order_items_variant_id_idx
    ON order_items (variant_id);
