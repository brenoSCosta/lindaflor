ALTER TABLE orders
    ALTER COLUMN access_token SET DEFAULT gen_random_uuid();
