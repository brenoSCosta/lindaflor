-- 1. Checkout abuse limits: at most N unpaid orders at a time per buyer
--    (normalized email, cart or account; see `store::checkout_service`).
--    No backfill: older pending orders expire within the 48h hold.
ALTER TABLE orders
    ADD COLUMN IF NOT EXISTS email_norm text,
    ADD COLUMN IF NOT EXISTS cart_id uuid REFERENCES carts(id) ON DELETE SET NULL;

CREATE INDEX IF NOT EXISTS orders_pending_email_norm_idx
    ON orders (email_norm) WHERE status = 'pending_payment';
CREATE INDEX IF NOT EXISTS orders_pending_cart_id_idx
    ON orders (cart_id) WHERE status = 'pending_payment';

-- 2. Newsletter unsubscribe (LGPD): keep the row, stop mailing it.
--    Re-subscribing clears the flag.
ALTER TABLE newsletter_subscribers
    ADD COLUMN IF NOT EXISTS unsubscribed_at timestamptz;

-- 3. Account deletion (LGPD). Personal data goes with the user:
--    sessions/accounts/two_factor/addresses/wishlist already cascade; the
--    cart did not (it became an anonymous cart), now it cascades too.
ALTER TABLE carts DROP CONSTRAINT IF EXISTS carts_user_id_fkey;
ALTER TABLE carts
    ADD CONSTRAINT carts_user_id_fkey
    FOREIGN KEY (user_id) REFERENCES users(id) ON DELETE CASCADE;

--    Orders are fiscal records and are retained: `orders.user_id` is SET
--    NULL by its FK, and this trigger first copies the account email onto
--    orders that lack a contact email so the record stays attributable
--    (shipping name/address already live in `shipping_address`).
CREATE OR REPLACE FUNCTION orders_keep_contact_on_user_delete()
RETURNS trigger
LANGUAGE plpgsql AS $$
BEGIN
    UPDATE orders
    SET guest_email = OLD.email
    WHERE user_id = OLD.id
      AND (guest_email IS NULL OR btrim(guest_email) = '');
    RETURN OLD;
END
$$;

DROP TRIGGER IF EXISTS users_keep_order_contact ON users;
CREATE TRIGGER users_keep_order_contact
    BEFORE DELETE ON users
    FOR EACH ROW EXECUTE FUNCTION orders_keep_contact_on_user_delete();
