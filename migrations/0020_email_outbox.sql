-- Durable outbox for transactional email (see src/mail.rs).
--
-- Rows are enqueued inside the business transaction (order transitions,
-- checkout, auth token issuance) and drained by a background worker that
-- retries with exponential backoff. Bodies may embed one-time links, so
-- the worker blanks them once a row reaches a terminal state.
CREATE TABLE IF NOT EXISTS email_outbox (
    id uuid PRIMARY KEY,
    kind text NOT NULL,
    to_address text NOT NULL,
    subject text NOT NULL,
    html_body text NOT NULL,
    text_body text NOT NULL,
    status text NOT NULL DEFAULT 'pending'
        CHECK (status IN ('pending', 'sent', 'failed')),
    attempts integer NOT NULL DEFAULT 0,
    next_attempt_at timestamptz NOT NULL DEFAULT now(),
    last_error text,
    -- Optional idempotency key (e.g. `order:{id}:shipped`): a second
    -- enqueue with the same key is a no-op.
    dedupe_key text,
    created_at timestamptz NOT NULL DEFAULT now(),
    sent_at timestamptz
);

CREATE UNIQUE INDEX IF NOT EXISTS email_outbox_dedupe_key_uidx
    ON email_outbox (dedupe_key)
    WHERE dedupe_key IS NOT NULL;

CREATE INDEX IF NOT EXISTS email_outbox_due_idx
    ON email_outbox (next_attempt_at)
    WHERE status = 'pending';
