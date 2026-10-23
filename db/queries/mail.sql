-- Owner: wave1-B (mail outbox, worker, mail hooks).

-- name: MailEnqueueThrottled :execrows
-- Insert unless the recipient already has max_per_window throttled mails
-- (any of the throttled kinds, case-insensitive address) in the window.
INSERT INTO email_outbox (id, kind, to_address, subject, html_body, text_body)
SELECT sqlc.arg(id)::uuid, sqlc.arg(kind)::text, sqlc.arg(to_address)::text,
       sqlc.arg(subject)::text, sqlc.arg(html_body)::text, sqlc.arg(text_body)::text
WHERE (
    SELECT count(*) FROM email_outbox
    WHERE lower(to_address) = lower(sqlc.arg(to_address)::text)
      AND kind = ANY(sqlc.arg(throttled_kinds)::text[])
      AND created_at > now() - make_interval(secs => sqlc.arg(window_secs)::float8)
) < sqlc.arg(max_per_window)::bigint;

-- name: MailClaimDue :many
-- Claim a batch of due rows: bump attempts and push next_attempt_at out by
-- the lease so other workers skip them while this one sends.
UPDATE email_outbox
SET attempts = attempts + 1,
    next_attempt_at = now() + make_interval(secs => sqlc.arg(lease_secs)::float8)
WHERE id IN (
    SELECT o.id FROM email_outbox o
    WHERE o.status = 'pending' AND o.next_attempt_at <= now()
    ORDER BY o.next_attempt_at
    LIMIT sqlc.arg(batch)::bigint
    FOR UPDATE SKIP LOCKED
)
RETURNING id, kind, to_address, subject, html_body, text_body, attempts;

-- name: MailMarkSent :exec
UPDATE email_outbox
SET status = 'sent', sent_at = now(), last_error = NULL, html_body = '', text_body = ''
WHERE id = $1;

-- name: MailMarkFailed :exec
UPDATE email_outbox
SET status = 'failed', last_error = sqlc.arg(last_error)::text, html_body = '', text_body = ''
WHERE id = sqlc.arg(id)::uuid;

-- name: MailScheduleRetry :exec
UPDATE email_outbox
SET last_error = sqlc.arg(last_error)::text,
    next_attempt_at = now() + make_interval(secs => sqlc.arg(delay_secs)::float8)
WHERE id = sqlc.arg(id)::uuid;

-- name: MailFirstPurchaseCode :one
-- The storefront's welcome coupon (NULL when unset).
SELECT first_purchase_coupon_code FROM store_settings WHERE id;

-- name: MailOrderRecipient :one
-- Empty strings stand for NULL (no recipient / name / PIX code / deadline).
SELECT o.access_token,
       o.total_cents::bigint AS total_cents,
       COALESCE(NULLIF(btrim(o.guest_email), ''), u.email, '')::text AS email,
       COALESCE(NULLIF(btrim(o.shipping_address->>'name'), ''), u.name, '')::text AS name,
       COALESCE(o.payment_meta->>'pix_copy_paste', '')::text AS pix,
       COALESCE(to_char(o.reservation_expires_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI'), '')::text AS pay_until,
       o.payment_provider, o.carrier, o.tracking_code
FROM orders o
LEFT JOIN users u ON u.id = o.user_id
WHERE o.id = $1;

-- name: MailReturnRecipient :one
SELECT o.id AS order_id, o.access_token, r.kind,
       COALESCE(NULLIF(btrim(o.guest_email), ''), u.email, '')::text AS email,
       COALESCE(NULLIF(btrim(o.shipping_address->>'name'), ''), u.name, '')::text AS name
FROM return_requests r
JOIN orders o ON o.id = r.order_id
LEFT JOIN users u ON u.id = o.user_id
WHERE r.id = $1;

-- name: MailReturnItems :many
SELECT ri.quantity, oi.product_name, oi.variant_label
FROM return_request_items ri
JOIN order_items oi ON oi.id = ri.order_item_id
WHERE ri.return_request_id = $1
ORDER BY oi.created_at, oi.id;
