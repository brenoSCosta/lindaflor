-- Shipment tracking, guest-link expiry and saved customer addresses.

-- Tracking is written by the `Ship` transition (order.rs) in the same
-- transaction as the status change; `delivered_at` by `Deliver`.
ALTER TABLE orders
    ADD COLUMN IF NOT EXISTS carrier text NOT NULL DEFAULT 'Correios',
    ADD COLUMN IF NOT EXISTS tracking_code text NULL,
    ADD COLUMN IF NOT EXISTS shipped_at timestamptz NULL,
    ADD COLUMN IF NOT EXISTS delivered_at timestamptz NULL,
    ADD COLUMN IF NOT EXISTS access_token_expires_at timestamptz NULL;

-- Guest order links (`/pedido/{id}?t=...`) stop working 90 days after the
-- order was placed. Logged-in owners are not affected.
UPDATE orders
SET access_token_expires_at = created_at + interval '90 days'
WHERE access_token_expires_at IS NULL;

ALTER TABLE orders
    ALTER COLUMN access_token_expires_at SET DEFAULT (now() + interval '90 days'),
    ALTER COLUMN access_token_expires_at SET NOT NULL;

CREATE TABLE IF NOT EXISTS user_addresses (
    id uuid PRIMARY KEY,
    user_id uuid NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    label text NOT NULL DEFAULT '',
    recipient_name text NOT NULL,
    zip_code text NOT NULL CHECK (zip_code ~ '^[0-9]{8}$'),
    street text NOT NULL,
    number text NOT NULL,
    complement text NOT NULL DEFAULT '',
    neighborhood text NOT NULL,
    city text NOT NULL,
    state text NOT NULL CHECK (state ~ '^[A-Z]{2}$'),
    phone text NOT NULL,
    is_default boolean NOT NULL DEFAULT false,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS user_addresses_user_id_idx
    ON user_addresses (user_id);

-- At most one default per user.
CREATE UNIQUE INDEX IF NOT EXISTS user_addresses_one_default_uidx
    ON user_addresses (user_id) WHERE is_default;
