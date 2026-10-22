-- `store_settings` holds exactly one row. A SELECT-then-INSERT race in the
-- admin save could create duplicates, so keep the most recently updated row
-- and replace the uuid key with a boolean that can only be `true`; writers
-- upsert with `ON CONFLICT (id) DO UPDATE`.

DELETE FROM store_settings
WHERE id IS DISTINCT FROM (
    SELECT id FROM store_settings
    ORDER BY updated_at DESC, created_at DESC
    LIMIT 1
);

ALTER TABLE store_settings DROP COLUMN id;

ALTER TABLE store_settings
    ADD COLUMN id boolean NOT NULL DEFAULT true,
    ADD CONSTRAINT store_settings_pkey PRIMARY KEY (id),
    ADD CONSTRAINT store_settings_singleton CHECK (id);
