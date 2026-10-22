-- Email-driven notifications (see `store::stock_alerts` and
-- `store::abandoned_cart`), both delivered by the background sweeper.

-- 1. Back-in-stock alerts ("Avise-me"). One pending alert per variant and
--    address; `notified_at` closes it, so a variant that sells out again
--    never re-sends (the shopper subscribes again if they want).
CREATE TABLE IF NOT EXISTS stock_alerts (
    id uuid PRIMARY KEY NOT NULL,
    variant_id uuid NOT NULL REFERENCES product_variants(id) ON DELETE CASCADE,
    email_norm text NOT NULL,
    user_id uuid REFERENCES users(id) ON DELETE CASCADE,
    created_at timestamptz DEFAULT now() NOT NULL,
    notified_at timestamptz
);

CREATE UNIQUE INDEX IF NOT EXISTS stock_alerts_active_uidx
    ON stock_alerts (variant_id, email_norm)
    WHERE notified_at IS NULL;

CREATE INDEX IF NOT EXISTS stock_alerts_pending_email_idx
    ON stock_alerts (email_norm)
    WHERE notified_at IS NULL;

-- 2. Abandoned-cart reminders. `contact_email` is the address typed on the
--    checkout page (guests have no account email); `abandoned_reminded_at`
--    caps reminders per cart.
ALTER TABLE carts
    ADD COLUMN IF NOT EXISTS contact_email text,
    ADD COLUMN IF NOT EXISTS abandoned_reminded_at timestamptz;

CREATE INDEX IF NOT EXISTS carts_updated_at_idx ON carts (updated_at);

-- Per-recipient cap lookups for marketing kinds.
CREATE INDEX IF NOT EXISTS email_outbox_kind_recipient_idx
    ON email_outbox (kind, lower(to_address), created_at);
