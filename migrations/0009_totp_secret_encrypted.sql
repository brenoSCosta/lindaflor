-- TOTP secrets encrypted at rest (XChaCha20-Poly1305, KMS-wrapped key).
--
-- Direct cutover (no legacy support, nothing deployed yet): `two_factor`
-- holds only `secret_nonce` (24B XChaCha nonce) + `secret_ciphertext`
-- (Poly1305 AEAD of the base32 secret) + `secret_key_version`. The old
-- plaintext `secret` column and its index are gone; every row must carry
-- nonce/ciphertext (NOT NULL). Missing or tampered values fail hard at
-- decrypt time — there is no plaintext fallback and no re-enroll path.

DELETE FROM two_factor;

DROP INDEX IF EXISTS two_factor_secret_idx;

ALTER TABLE two_factor
  DROP COLUMN IF EXISTS secret;

ALTER TABLE two_factor
  ADD COLUMN IF NOT EXISTS secret_nonce BYTEA NOT NULL,
  ADD COLUMN IF NOT EXISTS secret_ciphertext BYTEA NOT NULL,
  ADD COLUMN IF NOT EXISTS secret_key_version SMALLINT NOT NULL DEFAULT 1;
