-- Hash one-time tokens at rest.
--
-- `verifications.value` held raw reset / verify / 2FA-pending / change-email /
-- delete tokens (and the OAuth state JSON payload). New code stores
-- `HMAC-SHA256(TOKEN_PEPPER, token)` (plain SHA-256 when the pepper is empty)
-- in `value_hash` and leaves `value` empty for token rows, so a DB dump or
-- log line no longer yields a usable secret.
--
-- Strategy: one-time invalidation. Outstanding tokens cannot be re-hashed
-- server-side without the raw secret (and the pepper is deliberately not
-- stored in the DB), so all pending rows are dropped; users re-request.
-- `value_hash` stays nullable so pre-migration writers (OAuth state JSON,
-- pending-2FA during rollout) keep working with NULL; the UNIQUE index only
-- constrains non-NULL hashes, and NULLs never collide in Postgres.

DELETE FROM verifications;

ALTER TABLE verifications
  ADD COLUMN IF NOT EXISTS value_hash text;

CREATE UNIQUE INDEX IF NOT EXISTS verifications_identifier_value_hash_uidx
  ON verifications (identifier, value_hash);
