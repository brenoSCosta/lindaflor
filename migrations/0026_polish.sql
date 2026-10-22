-- Google sign-in only needs the identity (`accounts.account_id` = Google
-- `sub`); the app never calls Google APIs on the user's behalf. Drop the
-- OAuth tokens previously persisted in plaintext. New logins no longer
-- write them (see `GoogleAccountUpsert`).
UPDATE accounts
SET access_token = NULL,
    refresh_token = NULL,
    id_token = NULL,
    access_token_expires_at = NULL,
    updated_at = now()
WHERE provider_id = 'google'
  AND (access_token IS NOT NULL
       OR refresh_token IS NOT NULL
       OR id_token IS NOT NULL
       OR access_token_expires_at IS NOT NULL);

-- Per-recipient auth-mail cap (`mail::outbox::enqueue_throttled`) counts
-- recent rows by normalized recipient.
CREATE INDEX IF NOT EXISTS email_outbox_recipient_recent_idx
    ON email_outbox (lower(to_address), created_at);
