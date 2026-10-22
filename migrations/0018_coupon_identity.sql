-- Coupon identity: per-user limits key on a normalized email shared by
-- logged-in and guest checkouts, and coupon codes are unique
-- case-insensitively (lookups already use upper(code)).

-- 1. Case-insensitive unique code. Refuse to run when existing rows collide
--    instead of picking a winner; rename the duplicates by hand first.
DO $$
DECLARE
    dupes text;
BEGIN
    SELECT string_agg(code, ', ' ORDER BY code) INTO dupes
    FROM coupons
    WHERE upper(code) IN (
        SELECT upper(code) FROM coupons GROUP BY upper(code) HAVING count(*) > 1
    );
    IF dupes IS NOT NULL THEN
        RAISE EXCEPTION 'coupons have case-insensitive duplicate codes (%); rename them before applying 0018_coupon_identity', dupes;
    END IF;
END
$$;

ALTER TABLE coupons DROP CONSTRAINT IF EXISTS coupons_code_key;
DROP INDEX IF EXISTS coupons_code_upper_idx;
CREATE UNIQUE INDEX IF NOT EXISTS coupons_code_upper_key ON coupons (upper(code));

-- 2. Normalized redemption email. Mirrors `normalize_email` in
--    src/app/store/coupons.rs (lowercase, drop +tag, gmail dots); only used
--    for the backfill below, the app writes the column from then on.
ALTER TABLE coupon_redemptions ADD COLUMN IF NOT EXISTS email_norm text;

CREATE FUNCTION pg_temp.coupon_email_norm(raw text) RETURNS text
LANGUAGE sql IMMUTABLE AS $$
    WITH e AS (SELECT lower(btrim(raw)) AS v),
    p AS (
        SELECT
            v,
            split_part(substring(v FROM '^(.*)@[^@]*$'), '+', 1) AS local_part,
            substring(v FROM '@([^@]*)$') AS domain
        FROM e
    )
    SELECT CASE
        WHEN v IS NULL OR v = '' THEN NULL
        WHEN local_part IS NULL OR local_part = '' OR domain IS NULL THEN v
        WHEN domain IN ('gmail.com', 'googlemail.com')
            THEN replace(local_part, '.', '') || '@gmail.com'
        ELSE local_part || '@' || domain
    END
    FROM p
$$;

-- Account email for user redemptions, else the guest/checkout email.
UPDATE coupon_redemptions r
SET email_norm = pg_temp.coupon_email_norm(COALESCE(
    (SELECT u.email FROM users u WHERE u.id = r.user_id),
    r.guest_email,
    (SELECT o.guest_email FROM orders o WHERE o.id = r.order_id)
))
WHERE r.email_norm IS NULL;

CREATE INDEX IF NOT EXISTS coupon_redemptions_email_norm_idx
    ON coupon_redemptions (coupon_id, email_norm)
    WHERE email_norm IS NOT NULL AND refunded_at IS NULL;
