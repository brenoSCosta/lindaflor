-- Indexes for the background data-hygiene sweep (`src/maintenance.rs`):
-- stale guest carts by `updated_at`, expired verification rows by
-- `expires_at`. Without them every sweep tick scans both tables.
CREATE INDEX IF NOT EXISTS carts_guest_updated_at_idx
    ON carts (updated_at)
    WHERE user_id IS NULL;

CREATE INDEX IF NOT EXISTS verifications_expires_at_idx
    ON verifications (expires_at);
