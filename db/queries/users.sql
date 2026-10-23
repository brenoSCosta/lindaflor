-- Owner: wave1-A (auth). Basic user lookups used across the app.

-- name: GetUserByID :one
SELECT * FROM users WHERE id = $1;

-- name: GetUserByEmail :one
SELECT * FROM users WHERE lower(email) = lower(sqlc.arg(email)::text);

-- name: CountUsers :one
SELECT count(*) FROM users;
