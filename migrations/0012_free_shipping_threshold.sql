ALTER TABLE store_settings
    ADD COLUMN "free_shipping_threshold_cents" bigint DEFAULT 29900 NOT NULL;
