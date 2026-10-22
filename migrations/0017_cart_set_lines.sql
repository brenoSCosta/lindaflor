-- Separate loose cart lines from set-member lines.
--
-- The original `UNIQUE (cart_id, variant_id)` (constraint
-- `cart_items_cart_id_variant_id_key` from 0005) merged a loose unit into a
-- set-member row and let `add_set_to_cart` overwrite a loose line's `set_id`,
-- which priced extra units at the cheaper set share (§1.2). Two partial
-- unique indexes replace it: one loose line per variant, plus one line per
-- (cart, set, variant) for set members, so the same variant can be both a
-- loose line (normal price) and a member of one or more sets.
ALTER TABLE cart_items
    DROP CONSTRAINT IF EXISTS cart_items_cart_id_variant_id_key;

CREATE UNIQUE INDEX IF NOT EXISTS cart_items_loose_variant_uidx
    ON cart_items USING btree (cart_id, variant_id)
    WHERE set_id IS NULL;

CREATE UNIQUE INDEX IF NOT EXISTS cart_items_set_member_uidx
    ON cart_items USING btree (cart_id, set_id, variant_id)
    WHERE set_id IS NOT NULL;
