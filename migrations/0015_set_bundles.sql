-- V2 sellable sets: bundle pricing, fixed slots with variant options,
-- and cart/order linkage.

ALTER TABLE sets
    ADD COLUMN IF NOT EXISTS price_in_cents bigint;

ALTER TABLE sets
    ADD COLUMN IF NOT EXISTS compare_at_price_in_cents bigint;

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM pg_constraint WHERE conname = 'sets_compare_at_price_check'
    ) THEN
        ALTER TABLE sets
            ADD CONSTRAINT sets_compare_at_price_check
            CHECK (compare_at_price_in_cents IS NULL OR compare_at_price_in_cents > price_in_cents);
    END IF;
END
$$;

CREATE TABLE IF NOT EXISTS set_slots (
    id uuid PRIMARY KEY,
    set_id uuid NOT NULL REFERENCES sets(id) ON DELETE CASCADE,
    label text,
    sort_order integer NOT NULL DEFAULT 0
);

CREATE INDEX IF NOT EXISTS set_slots_set_id_idx
    ON set_slots USING btree (set_id);

CREATE TABLE IF NOT EXISTS set_slot_options (
    slot_id uuid NOT NULL REFERENCES set_slots(id) ON DELETE CASCADE,
    variant_id uuid NOT NULL REFERENCES product_variants(id) ON DELETE RESTRICT,
    price_override_cents bigint,
    PRIMARY KEY (slot_id, variant_id)
);

CREATE INDEX IF NOT EXISTS set_slot_options_variant_id_idx
    ON set_slot_options USING btree (variant_id);

-- A cart line can point at a set (bundle) instead of / in addition to a variant.
ALTER TABLE cart_items
    ADD COLUMN IF NOT EXISTS set_id uuid REFERENCES sets(id) ON DELETE CASCADE;

-- Snapshot the set on the order line for display/history.
ALTER TABLE order_items
    ADD COLUMN IF NOT EXISTS set_id uuid;

ALTER TABLE order_items
    ADD COLUMN IF NOT EXISTS set_name text;
