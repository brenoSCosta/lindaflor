package store

import (
	"context"

	"github.com/google/uuid"
)

// Wishlist (src/app/store/wishlist.rs).

var (
	sqlWishlistIDs    = sqlText(`SELECT product_id FROM wishlist_items WHERE user_id = $1`)
	sqlWishlistRemove = sqlText(`DELETE FROM wishlist_items WHERE user_id = $1 AND product_id = $2`)
	sqlWishlistAdd    = sqlText(`INSERT INTO wishlist_items (user_id, product_id)
 SELECT $1, id FROM products WHERE id = $2 AND active = true
 ON CONFLICT DO NOTHING`)
	sqlWishlistHas = sqlText(`SELECT EXISTS(SELECT 1 FROM wishlist_items WHERE user_id = $1 AND product_id = $2)`)
)

// WishlistIDs is the set of products the user liked (product cards).
func WishlistIDs(ctx context.Context, q DBTX, userID uuid.UUID) (map[uuid.UUID]bool, error) {
	ids, err := scalars[uuid.UUID](q.Query(ctx, sqlWishlistIDs, userID))
	if err != nil {
		return nil, err
	}
	out := make(map[uuid.UUID]bool, len(ids))
	for _, id := range ids {
		out[id] = true
	}
	return out, nil
}

// ToggleWishlist removes the product when liked, else adds it (active
// products only). Returns the new liked state. Toast: "Adicionado aos
// favoritos." / "Removido dos favoritos."; procedure answer "liked" /
// "unliked".
func ToggleWishlist(ctx context.Context, q DBTX, userID, productID uuid.UUID) (bool, error) {
	tag, err := q.Exec(ctx, sqlWishlistRemove, userID, productID)
	if err != nil {
		return false, err
	}
	if tag.RowsAffected() > 0 {
		return false, nil
	}
	if _, err := q.Exec(ctx, sqlWishlistAdd, userID, productID); err != nil {
		return false, err
	}
	var liked bool
	err = q.QueryRow(ctx, sqlWishlistHas, userID, productID).Scan(&liked)
	return liked, err
}

// WishlistToggleMessage is the toast of a toggle.
func WishlistToggleMessage(liked bool) string {
	if liked {
		return "Adicionado aos favoritos."
	}
	return "Removido dos favoritos."
}

// ListWishlistProducts lists the user's active liked products, most
// recently added first (resolve images before rendering).
func ListWishlistProducts(ctx context.Context, q DBTX, userID uuid.UUID) ([]ProductSummary, error) {
	return listSummaries(ctx, q, sqlWishlistProducts, userID)
}
