-- Convert every remaining `timestamp without time zone` column to
-- `timestamptz`. The old columns were written by `now()` under a UTC
-- session timezone or from UTC `PrimitiveDateTime`s in the app, so the
-- stored wall-clock values are UTC.

ALTER TABLE users
    ALTER COLUMN created_at TYPE timestamptz USING created_at AT TIME ZONE 'UTC',
    ALTER COLUMN updated_at TYPE timestamptz USING updated_at AT TIME ZONE 'UTC',
    ALTER COLUMN ban_expires TYPE timestamptz USING ban_expires AT TIME ZONE 'UTC';

ALTER TABLE sessions
    ALTER COLUMN created_at TYPE timestamptz USING created_at AT TIME ZONE 'UTC',
    ALTER COLUMN updated_at TYPE timestamptz USING updated_at AT TIME ZONE 'UTC',
    ALTER COLUMN expires_at TYPE timestamptz USING expires_at AT TIME ZONE 'UTC';

ALTER TABLE accounts
    ALTER COLUMN created_at TYPE timestamptz USING created_at AT TIME ZONE 'UTC',
    ALTER COLUMN updated_at TYPE timestamptz USING updated_at AT TIME ZONE 'UTC',
    ALTER COLUMN access_token_expires_at TYPE timestamptz USING access_token_expires_at AT TIME ZONE 'UTC',
    ALTER COLUMN refresh_token_expires_at TYPE timestamptz USING refresh_token_expires_at AT TIME ZONE 'UTC';

ALTER TABLE verifications
    ALTER COLUMN created_at TYPE timestamptz USING created_at AT TIME ZONE 'UTC',
    ALTER COLUMN updated_at TYPE timestamptz USING updated_at AT TIME ZONE 'UTC',
    ALTER COLUMN expires_at TYPE timestamptz USING expires_at AT TIME ZONE 'UTC';

ALTER TABLE products
    ALTER COLUMN created_at TYPE timestamptz USING created_at AT TIME ZONE 'UTC',
    ALTER COLUMN updated_at TYPE timestamptz USING updated_at AT TIME ZONE 'UTC';

ALTER TABLE product_images
    ALTER COLUMN created_at TYPE timestamptz USING created_at AT TIME ZONE 'UTC';

ALTER TABLE product_variants
    ALTER COLUMN created_at TYPE timestamptz USING created_at AT TIME ZONE 'UTC',
    ALTER COLUMN updated_at TYPE timestamptz USING updated_at AT TIME ZONE 'UTC';

ALTER TABLE warehouses
    ALTER COLUMN created_at TYPE timestamptz USING created_at AT TIME ZONE 'UTC';

ALTER TABLE inventory
    ALTER COLUMN updated_at TYPE timestamptz USING updated_at AT TIME ZONE 'UTC';

ALTER TABLE orders
    ALTER COLUMN created_at TYPE timestamptz USING created_at AT TIME ZONE 'UTC',
    ALTER COLUMN updated_at TYPE timestamptz USING updated_at AT TIME ZONE 'UTC';

ALTER TABLE order_items
    ALTER COLUMN created_at TYPE timestamptz USING created_at AT TIME ZONE 'UTC';

ALTER TABLE store_settings
    ALTER COLUMN created_at TYPE timestamptz USING created_at AT TIME ZONE 'UTC',
    ALTER COLUMN updated_at TYPE timestamptz USING updated_at AT TIME ZONE 'UTC';

-- Tables created after 0002 that may also carry naive timestamps (e.g.
-- `collections` if a later migration recreated it) are caught here so the
-- schema ends up uniformly `timestamptz`.
DO $$
DECLARE
    col record;
BEGIN
    FOR col IN
        SELECT table_name, column_name
        FROM information_schema.columns
        WHERE table_schema = 'public'
          AND data_type = 'timestamp without time zone'
    LOOP
        EXECUTE format(
            'ALTER TABLE %I ALTER COLUMN %I TYPE timestamptz USING %I AT TIME ZONE ''UTC''',
            col.table_name, col.column_name, col.column_name
        );
    END LOOP;
END
$$;
