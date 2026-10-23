-- Owner: store (images, settings, newsletter, stock alerts:
-- internal/store/{images,settings,newsletter,stockalerts}.go).

-- name: StoreInsertProductImage :exec
INSERT INTO product_images (id, product_id, url, alt, sort_order, created_at)
VALUES ($1, $2, $3, NULL, $4, now());

-- name: StoreProductImageURL :one
SELECT url FROM product_images WHERE id = $1;

-- name: StoreDeleteProductImage :exec
DELETE FROM product_images WHERE id = $1;

-- name: StoreInsertSetImage :exec
INSERT INTO set_images (id, set_id, url, alt, sort_order, created_at)
VALUES ($1, $2, $3, NULL, $4, now());

-- name: StoreSetImageURL :one
SELECT url FROM set_images WHERE id = $1;

-- name: StoreDeleteSetImage :exec
DELETE FROM set_images WHERE id = $1;

-- name: StoreGetSettings :one
SELECT whatsapp_number, whatsapp_message_template, pix_key, pix_key_type::text::null_text AS pix_key_type,
       pix_merchant_name, pix_merchant_city, free_shipping_threshold_cents, pix_discount_percent,
       announcement_text, first_purchase_coupon_code
FROM store_settings WHERE id;

-- name: StoreUpsertSettings :exec
-- Race-free singleton write.
INSERT INTO store_settings (
    id, pix_key, pix_key_type, pix_merchant_name, pix_merchant_city,
    pix_discount_percent, announcement_text, first_purchase_coupon_code,
    whatsapp_number, whatsapp_message_template, free_shipping_threshold_cents,
    created_at, updated_at
) VALUES (true, sqlc.narg(pix_key), sqlc.narg(pix_key_type)::text::pix_key_type, sqlc.narg(pix_merchant_name),
          sqlc.narg(pix_merchant_city), sqlc.arg(pix_discount_percent), sqlc.narg(announcement_text),
          sqlc.narg(first_purchase_coupon_code), sqlc.narg(whatsapp_number), sqlc.narg(whatsapp_message_template),
          sqlc.arg(free_shipping_threshold_cents), now(), now())
ON CONFLICT (id) DO UPDATE SET
    pix_key = EXCLUDED.pix_key,
    pix_key_type = EXCLUDED.pix_key_type,
    pix_merchant_name = EXCLUDED.pix_merchant_name,
    pix_merchant_city = EXCLUDED.pix_merchant_city,
    pix_discount_percent = EXCLUDED.pix_discount_percent,
    announcement_text = EXCLUDED.announcement_text,
    first_purchase_coupon_code = EXCLUDED.first_purchase_coupon_code,
    whatsapp_number = EXCLUDED.whatsapp_number,
    whatsapp_message_template = EXCLUDED.whatsapp_message_template,
    free_shipping_threshold_cents = EXCLUDED.free_shipping_threshold_cents,
    updated_at = now();

-- name: StoreNewsletterSubscribe :one
-- inserted is false when an unsubscribed address was reactivated; no row
-- when the address is already active.
INSERT INTO newsletter_subscribers (id, email, name, created_at)
VALUES (gen_random_uuid(), sqlc.arg(email), sqlc.narg(name), now())
ON CONFLICT (lower(email)) DO UPDATE
    SET unsubscribed_at = NULL,
        name = COALESCE(EXCLUDED.name, newsletter_subscribers.name)
    WHERE newsletter_subscribers.unsubscribed_at IS NOT NULL
RETURNING (xmax::text = '0')::bool AS inserted;

-- name: StoreNewsletterUnsubscribe :one
-- cancelled is true when an active subscription was cancelled; unknown
-- addresses get a suppression row; no row when already unsubscribed.
INSERT INTO newsletter_subscribers (id, email, created_at, unsubscribed_at)
VALUES (gen_random_uuid(), $1, now(), now())
ON CONFLICT (lower(email)) DO UPDATE
    SET unsubscribed_at = now()
    WHERE newsletter_subscribers.unsubscribed_at IS NULL
RETURNING (xmax::text <> '0')::bool AS cancelled;

-- name: StoreAllSubscribers :many
SELECT id, email, name, created_at, unsubscribed_at
FROM newsletter_subscribers ORDER BY created_at DESC;

-- name: StoreActiveSubscribers :many
SELECT id, email, name, created_at, unsubscribed_at
FROM newsletter_subscribers WHERE unsubscribed_at IS NULL
ORDER BY created_at DESC;

-- name: StoreVariantStock :one
SELECT p.active,
       COALESCE((
           SELECT SUM(inventory_live_available(i.id, i.quantity, i.reserved))
           FROM inventory i
           INNER JOIN warehouses w ON w.id = i.warehouse_id
           WHERE i.variant_id = pv.id AND w.active = true
       ), 0)::bigint AS available
FROM product_variants pv
INNER JOIN products p ON p.id = pv.product_id
WHERE pv.id = $1;

-- name: StoreVariantSlug :one
SELECT p.slug FROM product_variants pv
INNER JOIN products p ON p.id = pv.product_id
WHERE pv.id = $1;

-- name: StorePendingAlerts :one
SELECT count(*) FROM stock_alerts WHERE email_norm = $1 AND notified_at IS NULL;

-- name: StoreInsertAlert :execrows
INSERT INTO stock_alerts (id, variant_id, email_norm, user_id)
VALUES ($1, $2, $3, $4)
ON CONFLICT (variant_id, email_norm) WHERE notified_at IS NULL
DO NOTHING;

-- name: StoreCancelAlerts :execrows
DELETE FROM stock_alerts WHERE email_norm = $1 AND notified_at IS NULL;

-- name: StoreDueAlerts :many
-- Pending alerts whose variant is sellable again; SKIP LOCKED so
-- concurrent sweepers never mail the same alert twice.
SELECT a.id, a.email_norm, pv.id AS variant_id,
       p.name AS product_name, p.slug AS product_slug,
       pv.size::text AS size, pv.color
FROM stock_alerts a
INNER JOIN product_variants pv ON pv.id = a.variant_id
INNER JOIN products p ON p.id = pv.product_id
WHERE a.notified_at IS NULL
  AND p.active = true
  AND COALESCE((
      SELECT SUM(inventory_live_available(i.id, i.quantity, i.reserved))
      FROM inventory i
      INNER JOIN warehouses w ON w.id = i.warehouse_id
      WHERE i.variant_id = pv.id AND w.active = true
  ), 0) > 0
ORDER BY a.created_at
LIMIT sqlc.arg(max_rows)::int
FOR UPDATE OF a SKIP LOCKED;

-- name: StoreCloseAlerts :exec
UPDATE stock_alerts SET notified_at = now() WHERE id = ANY(sqlc.arg(ids)::uuid[]);
