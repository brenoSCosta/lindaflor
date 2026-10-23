-- Owner: store (wishlist, internal/store/wishlist.go). The liked product
-- cards are StoreWishlistProducts in store_catalog.sql.

-- name: StoreWishlistIDs :many
SELECT product_id FROM wishlist_items WHERE user_id = $1;

-- name: StoreWishlistRemove :execrows
DELETE FROM wishlist_items WHERE user_id = $1 AND product_id = $2;

-- name: StoreWishlistAdd :exec
-- Active products only.
INSERT INTO wishlist_items (user_id, product_id)
SELECT sqlc.arg(user_id)::uuid, id FROM products WHERE id = sqlc.arg(product_id)::uuid AND active = true
ON CONFLICT DO NOTHING;

-- name: StoreWishlistHas :one
SELECT EXISTS(SELECT 1 FROM wishlist_items WHERE user_id = $1 AND product_id = $2);
