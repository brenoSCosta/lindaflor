-- Replace `collections` (product grouping) with sellable `sets` (conjunto bundles).
-- V1: set membership + images. The sellable/bundle columns land in 0015.

CREATE TABLE IF NOT EXISTS sets (
    id uuid PRIMARY KEY,
    name text NOT NULL,
    slug text NOT NULL UNIQUE,
    description text,
    active boolean NOT NULL DEFAULT true,
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now()
);

CREATE UNIQUE INDEX IF NOT EXISTS sets_slug_uidx ON sets USING btree (slug);
CREATE INDEX IF NOT EXISTS sets_active_idx ON sets USING btree (active);

CREATE TABLE IF NOT EXISTS set_members (
    set_id uuid NOT NULL REFERENCES sets(id) ON DELETE CASCADE,
    product_id uuid NOT NULL REFERENCES products(id) ON DELETE CASCADE,
    sort_order integer NOT NULL DEFAULT 0,
    PRIMARY KEY (set_id, product_id)
);

CREATE INDEX IF NOT EXISTS set_members_product_id_idx
    ON set_members USING btree (product_id);

CREATE TABLE IF NOT EXISTS set_images (
    id uuid PRIMARY KEY,
    set_id uuid NOT NULL REFERENCES sets(id) ON DELETE CASCADE,
    url text NOT NULL,
    alt text,
    sort_order integer NOT NULL DEFAULT 0,
    created_at timestamptz NOT NULL DEFAULT now()
);

CREATE INDEX IF NOT EXISTS set_images_set_id_idx
    ON set_images USING btree (set_id);

-- Drop the old model. Each statement is guarded so the migration is
-- idempotent on fresh DBs (no collections table) and migrated DBs.
ALTER TABLE products DROP CONSTRAINT IF EXISTS products_collection_id_collections_id_fk;
ALTER TABLE products DROP COLUMN IF EXISTS collection_id;
DROP TABLE IF EXISTS collections;
