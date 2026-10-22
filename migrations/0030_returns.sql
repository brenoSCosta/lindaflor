-- Returns / exchanges ("Troca fácil — até 7 dias", see
-- src/app/trocas_devolucoes.rs and src/app/store/returns.rs).
--
-- A customer opens a request from the order page; the admin moves it
-- requested -> approved -> received -> resolved, or rejects it. Every
-- admin step also lands in `audit_events` (aggregate_type 'return').
CREATE TABLE IF NOT EXISTS return_requests (
    id uuid PRIMARY KEY,
    order_id uuid NOT NULL REFERENCES orders(id) ON DELETE CASCADE,
    -- Signed-in requester (NULL for a guest link); kept on user deletion.
    user_id uuid REFERENCES users(id) ON DELETE SET NULL,
    kind text NOT NULL CHECK (kind IN ('exchange', 'return', 'defect')),
    status text NOT NULL DEFAULT 'requested'
        CHECK (status IN ('requested', 'approved', 'received', 'resolved', 'rejected')),
    resolution text CHECK (resolution IN ('refunded', 'exchanged')),
    reason text NOT NULL CHECK (char_length(reason) BETWEEN 1 AND 1000),
    desired_size text CHECK (char_length(desired_size) <= 20),
    -- Reverse-logistics note sent with the approval mail.
    instructions text,
    rejection_reason text,
    -- Amount actually returned to the customer (full-order refund or a
    -- manual PIX for a partial return).
    refund_cents bigint CHECK (refund_cents >= 0),
    created_at timestamptz NOT NULL DEFAULT now(),
    updated_at timestamptz NOT NULL DEFAULT now(),
    approved_at timestamptz,
    received_at timestamptz,
    resolved_at timestamptz,
    CONSTRAINT return_requests_resolution_matches_status
        CHECK ((status = 'resolved') = (resolution IS NOT NULL))
);

-- One open request per order at a time (also stops resubmits/spam).
CREATE UNIQUE INDEX IF NOT EXISTS return_requests_open_order_uidx
    ON return_requests (order_id)
    WHERE status IN ('requested', 'approved', 'received');
CREATE INDEX IF NOT EXISTS return_requests_order_idx
    ON return_requests (order_id);
-- Admin list: filter by status, newest first.
CREATE INDEX IF NOT EXISTS return_requests_status_created_idx
    ON return_requests (status, created_at DESC);

CREATE TABLE IF NOT EXISTS return_request_items (
    return_request_id uuid NOT NULL REFERENCES return_requests(id) ON DELETE CASCADE,
    order_item_id uuid NOT NULL REFERENCES order_items(id) ON DELETE CASCADE,
    quantity integer NOT NULL CHECK (quantity > 0),
    PRIMARY KEY (return_request_id, order_item_id)
);

CREATE INDEX IF NOT EXISTS return_request_items_order_item_idx
    ON return_request_items (order_item_id);

-- Admin reports (/admin/relatorios) and CSV export: range scans by the
-- day a sale counts (paid_at, falling back to created_at) and by creation.
CREATE INDEX IF NOT EXISTS orders_sale_at_idx
    ON orders ((COALESCE(paid_at, created_at)))
    WHERE status IN ('paid', 'processing', 'shipped', 'delivered');
CREATE INDEX IF NOT EXISTS orders_created_at_idx
    ON orders (created_at);
