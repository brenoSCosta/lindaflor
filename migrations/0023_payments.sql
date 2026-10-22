-- Payment provider strategy (PRODUCTION_READINESS §2.0/§2.1).
--
-- `payment_provider` records which strategy created the order's charge, so an
-- order keeps its payment path even if `PAYMENT_PROVIDER` changes later.
-- `mp_*` columns hold the Mercado Pago PIX charge (QR code, copy-paste code,
-- ticket URL, expiry) and the last status fetched from the MP API. MP's raw
-- status is informational only: order state moves exclusively via
-- `order.apply` (see `src/payments/service.rs`).

ALTER TABLE orders
    ADD COLUMN IF NOT EXISTS payment_provider text NOT NULL DEFAULT 'static_pix',
    ADD COLUMN IF NOT EXISTS mp_payment_id bigint,
    ADD COLUMN IF NOT EXISTS mp_status text,
    ADD COLUMN IF NOT EXISTS mp_status_detail text,
    ADD COLUMN IF NOT EXISTS mp_qr_code text,
    ADD COLUMN IF NOT EXISTS mp_qr_code_base64 text,
    ADD COLUMN IF NOT EXISTS mp_ticket_url text,
    ADD COLUMN IF NOT EXISTS mp_expires_at timestamptz,
    ADD COLUMN IF NOT EXISTS mp_synced_at timestamptz;

ALTER TABLE orders
    DROP CONSTRAINT IF EXISTS orders_payment_provider_check;
ALTER TABLE orders
    ADD CONSTRAINT orders_payment_provider_check
    CHECK (payment_provider IN ('static_pix', 'mercadopago'));

-- Webhook lookups + idempotency key: one order per MP payment.
CREATE UNIQUE INDEX IF NOT EXISTS orders_mp_payment_id_uidx
    ON orders (mp_payment_id)
    WHERE mp_payment_id IS NOT NULL;
