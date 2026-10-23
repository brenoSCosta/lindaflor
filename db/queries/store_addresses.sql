-- Owner: store (saved addresses, internal/store/addresses.go). Every
-- mutation first locks the users row (StoreLockUser) to serialize the
-- default flag and the per-user limit.

-- name: StoreListAddresses :many
SELECT id, label, recipient_name, zip_code, street, number, complement,
       neighborhood, city, state, phone, is_default
FROM user_addresses WHERE user_id = $1
ORDER BY is_default DESC, created_at DESC;

-- name: StoreGetAddress :one
SELECT id, label, recipient_name, zip_code, street, number, complement,
       neighborhood, city, state, phone, is_default
FROM user_addresses WHERE id = $1 AND user_id = $2;

-- name: StoreLockUser :one
SELECT id FROM users WHERE id = $1 FOR UPDATE;

-- name: StoreClearDefaultAddress :exec
UPDATE user_addresses SET is_default = false, updated_at = now() WHERE user_id = $1 AND is_default;

-- name: StoreCountAddresses :one
SELECT COUNT(*) FROM user_addresses WHERE user_id = $1;

-- name: StoreInsertAddress :exec
INSERT INTO user_addresses (id, user_id, label, recipient_name, zip_code, street,
                            number, complement, neighborhood, city, state, phone, is_default)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13);

-- name: StoreAddressIsDefault :one
SELECT is_default FROM user_addresses WHERE id = $1 AND user_id = $2;

-- name: StoreUpdateAddress :exec
UPDATE user_addresses
SET label = sqlc.arg(label), recipient_name = sqlc.arg(recipient_name), zip_code = sqlc.arg(zip_code),
    street = sqlc.arg(street), number = sqlc.arg(number), complement = sqlc.arg(complement),
    neighborhood = sqlc.arg(neighborhood), city = sqlc.arg(city), state = sqlc.arg(state),
    phone = sqlc.arg(phone), is_default = sqlc.arg(is_default), updated_at = now()
WHERE id = sqlc.arg(id) AND user_id = sqlc.arg(user_id);

-- name: StoreDeleteAddress :one
DELETE FROM user_addresses WHERE id = $1 AND user_id = $2 RETURNING is_default;

-- name: StorePromoteLatestAddress :exec
UPDATE user_addresses SET is_default = true, updated_at = now()
WHERE id = (SELECT ua.id FROM user_addresses ua WHERE ua.user_id = sqlc.arg(user_id)::uuid ORDER BY ua.created_at DESC LIMIT 1);

-- name: StoreAddressOwned :one
SELECT id FROM user_addresses WHERE id = $1 AND user_id = $2;

-- name: StoreSetDefaultAddress :exec
UPDATE user_addresses SET is_default = true, updated_at = now() WHERE id = $1 AND user_id = $2;

-- name: StoreCheckoutAddressDupe :one
SELECT COUNT(*) AS count,
       COALESCE(bool_or(zip_code = sqlc.arg(zip_code)::text
                        AND lower(street) = lower(sqlc.arg(street)::text)
                        AND lower(number) = lower(sqlc.arg(number)::text)
                        AND lower(complement) = lower(sqlc.arg(complement)::text)), false)::bool AS dup
FROM user_addresses WHERE user_id = sqlc.arg(user_id);
