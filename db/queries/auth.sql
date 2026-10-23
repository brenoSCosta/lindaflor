-- Owner: wave1-A (auth). Users, credentials, sessions, one-time tokens,
-- TOTP, linked accounts and admin user management (port of src/auth/**).

-- ---------------------------------------------------------------------------
-- Users
-- ---------------------------------------------------------------------------

-- name: AuthUserLiteByEmail :one
SELECT id, name, email_verified FROM users WHERE email = $1;

-- name: AuthUserExistsByEmail :one
SELECT EXISTS (SELECT 1 FROM users WHERE email = $1)::boolean;

-- name: AuthEmailTakenByOther :one
SELECT EXISTS (SELECT 1 FROM users WHERE email = $1 AND id <> $2)::boolean;

-- name: AuthInsertUser :exec
INSERT INTO users (id, name, email, email_verified, image, role, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, now(), now());

-- name: AuthUpdateUserName :exec
UPDATE users SET name = $2, updated_at = now() WHERE id = $1;

-- name: AuthVerifyEmailByAddress :execrows
UPDATE users SET email_verified = true, updated_at = now() WHERE email = $1;

-- name: AuthChangeUserEmail :execrows
UPDATE users SET email = $1, email_verified = true, updated_at = now() WHERE id = $2;

-- name: AuthDeleteUser :execrows
DELETE FROM users WHERE id = $1;

-- name: AuthSetTwoFactorEnabled :exec
UPDATE users SET two_factor_enabled = $2, updated_at = now() WHERE id = $1;

-- name: AuthUserImage :one
SELECT image FROM users WHERE id = $1;

-- name: AuthSetUserImage :execrows
UPDATE users SET image = $2, updated_at = now() WHERE id = $1;

-- name: AuthUserBanState :one
SELECT id, banned, ban_expires, two_factor_enabled FROM users WHERE id = $1;

-- ---------------------------------------------------------------------------
-- Credentials / accounts
-- ---------------------------------------------------------------------------

-- name: AuthInsertAccount :exec
INSERT INTO accounts (id, account_id, provider_id, user_id, password, scope, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, $6, now(), now());

-- name: AuthCredentialLogin :one
SELECT u.id, u.name, u.email, u.email_verified, u.image, u.two_factor_enabled, u.role,
       u.banned, u.ban_reason, u.ban_expires, a.password
FROM users u
INNER JOIN accounts a ON a.user_id = u.id AND a.provider_id = 'credential'
WHERE u.email = $1;

-- name: AuthCredentialPassword :one
SELECT password FROM accounts WHERE user_id = $1 AND provider_id = 'credential';

-- name: AuthUpdateCredentialPassword :execrows
UPDATE accounts SET password = $2, updated_at = now()
WHERE user_id = $1 AND provider_id = 'credential';

-- name: AuthListAccounts :many
SELECT id, provider_id, account_id, created_at
FROM accounts WHERE user_id = $1
ORDER BY created_at ASC;

-- name: AuthFindAccount :one
SELECT id, provider_id, password FROM accounts
WHERE user_id = $1 AND provider_id = $2 AND account_id = $3;

-- name: AuthOtherAccounts :many
SELECT provider_id, password FROM accounts WHERE user_id = $1 AND id <> $2;

-- name: AuthDeleteAccount :exec
DELETE FROM accounts WHERE id = $1;

-- name: AuthDeleteAccountsForUser :exec
DELETE FROM accounts WHERE user_id = $1;

-- name: AuthAccountOwner :one
SELECT user_id FROM accounts WHERE provider_id = $1 AND account_id = $2;

-- name: AuthRefreshOAuthAccount :exec
UPDATE accounts
SET access_token = NULL, refresh_token = NULL, id_token = NULL,
    access_token_expires_at = NULL, scope = COALESCE(sqlc.narg(scope), scope), updated_at = now()
WHERE provider_id = $1 AND account_id = $2;

-- name: AuthUserExists :one
SELECT EXISTS (SELECT 1 FROM users WHERE id = $1)::boolean;

-- name: AuthLinkGoogleProfile :exec
UPDATE users
SET email_verified = CASE WHEN sqlc.arg(verified)::boolean AND email = sqlc.arg(google_email)::text THEN true ELSE email_verified END,
    image = COALESCE(image, sqlc.narg(image)),
    updated_at = now()
WHERE id = sqlc.arg(id);

-- name: AuthFillUserImage :exec
UPDATE users SET image = COALESCE(image, $2), updated_at = now() WHERE id = $1;

-- name: AuthMarkGoogleVerified :exec
UPDATE users SET email_verified = true, image = COALESCE(image, sqlc.narg(image)), updated_at = now()
WHERE id = sqlc.arg(id);

-- name: AuthTakeOverUser :exec
UPDATE users
SET name = $2, email_verified = true, two_factor_enabled = false,
    image = COALESCE(sqlc.narg(image), image), updated_at = now()
WHERE id = $1;

-- ---------------------------------------------------------------------------
-- Sessions (token = hex sha256 of the raw cookie token)
-- ---------------------------------------------------------------------------

-- name: AuthInsertSession :exec
INSERT INTO sessions (id, expires_at, token, created_at, updated_at, ip_address, user_agent, user_id, impersonated_by)
VALUES ($1, $2, $3, now(), now(), $4, $5, $6, $7);

-- name: AuthSessionWithUser :one
SELECT s.id AS session_id, s.expires_at, s.updated_at, s.impersonated_by,
       u.id AS user_id, u.name, u.email, u.email_verified, u.image, u.two_factor_enabled,
       u.role, u.banned, u.ban_reason, u.ban_expires
FROM sessions s
INNER JOIN users u ON u.id = s.user_id
WHERE s.token = $1 AND s.expires_at > now();

-- name: AuthDeleteSessionByToken :exec
DELETE FROM sessions WHERE token = $1;

-- name: AuthDeleteOwnedSession :execrows
DELETE FROM sessions WHERE id = $1 AND user_id = $2;

-- name: AuthDeleteSessionsExceptToken :exec
DELETE FROM sessions WHERE user_id = $1 AND token <> $2;

-- name: AuthDeleteSessionsForUser :exec
DELETE FROM sessions WHERE user_id = $1;

-- name: AuthDeleteSessionsExceptID :exec
DELETE FROM sessions WHERE user_id = $1 AND id <> $2;

-- name: AuthSlideSessionExpiry :exec
UPDATE sessions
SET expires_at = CASE WHEN impersonated_by IS NULL THEN sqlc.arg(expires_at)::timestamptz
                      ELSE LEAST(expires_at, sqlc.arg(expires_at)::timestamptz) END,
    updated_at = now()
WHERE token = sqlc.arg(token);

-- name: AuthListActiveSessions :many
SELECT id, token, user_agent, ip_address, created_at, expires_at
FROM sessions
WHERE user_id = $1 AND expires_at > now()
ORDER BY created_at DESC;

-- name: AuthSessionOwner :one
SELECT user_id FROM sessions WHERE id = $1;

-- ---------------------------------------------------------------------------
-- One-time verification tokens
-- ---------------------------------------------------------------------------

-- name: AuthInsertVerification :exec
INSERT INTO verifications (id, identifier, value, value_hash, expires_at, created_at, updated_at)
VALUES ($1, $2, $3, $4, $5, now(), now());

-- name: AuthDeleteVerificationsByIdentifier :exec
DELETE FROM verifications WHERE identifier = $1;

-- name: AuthDeleteVerificationsLike :exec
DELETE FROM verifications WHERE identifier LIKE $1;

-- name: AuthFindVerification :one
SELECT id, identifier FROM verifications
WHERE value_hash = $1 AND identifier LIKE sqlc.arg(identifier_like)::text AND expires_at > now();

-- name: AuthDeleteVerification :exec
DELETE FROM verifications WHERE id = $1;

-- name: AuthVerificationByIdentifier :one
SELECT id, value FROM verifications WHERE identifier = $1 AND expires_at > now();

-- name: AuthDeleteUserScopedVerifications :exec
DELETE FROM verifications
WHERE identifier = sqlc.arg(pending)::text OR identifier = sqlc.arg(delete_user)::text
   OR identifier LIKE sqlc.arg(change_email_like)::text;

-- ---------------------------------------------------------------------------
-- Two-factor
-- ---------------------------------------------------------------------------

-- name: AuthTwoFactorByUser :one
SELECT id, backup_codes, verified, secret_nonce, secret_ciphertext, secret_key_version
FROM two_factor WHERE user_id = $1;

-- name: AuthVerifiedTwoFactor :one
SELECT id, secret_nonce, secret_ciphertext, secret_key_version
FROM two_factor WHERE user_id = $1 AND verified = true;

-- name: AuthHasUnverifiedTwoFactor :one
SELECT EXISTS (SELECT 1 FROM two_factor WHERE user_id = $1 AND verified = false)::boolean;

-- name: AuthDeleteTwoFactor :exec
DELETE FROM two_factor WHERE user_id = $1;

-- name: AuthInsertTwoFactor :exec
INSERT INTO two_factor (id, backup_codes, user_id, verified, secret_nonce, secret_ciphertext, secret_key_version)
VALUES ($1, $2, $3, false, $4, $5, $6);

-- name: AuthMarkTwoFactorVerified :exec
UPDATE two_factor SET verified = true WHERE id = $1;

-- name: AuthSetBackupCodes :exec
UPDATE two_factor SET backup_codes = $2 WHERE id = $1;

-- ---------------------------------------------------------------------------
-- Admin user management
-- ---------------------------------------------------------------------------

-- name: AuthAdminCountUsers :one
SELECT count(*) FROM users
WHERE (sqlc.arg(pattern)::text = '' OR name ILIKE sqlc.arg(pattern)::text ESCAPE '\'
       OR email ILIKE sqlc.arg(pattern)::text ESCAPE '\');

-- name: AuthAdminListUsers :many
SELECT id, name, email, email_verified, image, two_factor_enabled, role, banned, ban_reason, ban_expires
FROM users
WHERE (sqlc.arg(pattern)::text = '' OR name ILIKE sqlc.arg(pattern)::text ESCAPE '\'
       OR email ILIKE sqlc.arg(pattern)::text ESCAPE '\')
ORDER BY created_at DESC, id DESC
LIMIT sqlc.arg(lim) OFFSET sqlc.arg(off);

-- name: AuthSetUserRole :exec
UPDATE users SET role = $2, updated_at = now() WHERE id = $1;

-- name: AuthBanUser :exec
UPDATE users SET banned = true, ban_reason = $2, ban_expires = NULL, updated_at = now() WHERE id = $1;

-- name: AuthUnbanUser :exec
UPDATE users SET banned = false, ban_reason = NULL, ban_expires = NULL, updated_at = now() WHERE id = $1;

-- ---------------------------------------------------------------------------
-- Bootstrap (cmd/create-admin)
-- ---------------------------------------------------------------------------

-- name: AuthUserIDByEmailForUpdate :one
SELECT id FROM users WHERE email = $1 FOR UPDATE;

-- name: AuthPromoteAdmin :exec
UPDATE users SET role = 'admin', email_verified = true, updated_at = now() WHERE id = $1;
