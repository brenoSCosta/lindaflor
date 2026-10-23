package store

import (
	"context"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// Wishlist (src/app/store/wishlist.rs).

// WishlistIDs is the set of products the user liked (product cards).
func WishlistIDs(ctx context.Context, q DBTX, userID uuid.UUID) (map[uuid.UUID]bool, error) {
	ids, err := queries(q).StoreWishlistIDs(ctx, userID)
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
	qs := queries(q)
	removed, err := qs.StoreWishlistRemove(ctx, sqlc.StoreWishlistRemoveParams{UserID: userID, ProductID: productID})
	if err != nil {
		return false, err
	}
	if removed > 0 {
		return false, nil
	}
	if err := qs.StoreWishlistAdd(ctx, sqlc.StoreWishlistAddParams{UserID: userID, ProductID: productID}); err != nil {
		return false, err
	}
	return qs.StoreWishlistHas(ctx, sqlc.StoreWishlistHasParams{UserID: userID, ProductID: productID})
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
	rows, err := queries(q).StoreWishlistProducts(ctx, userID)
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, func(r sqlc.StoreWishlistProductsRow) ProductSummary { return toSummary(productSummaryRow(r)) }), nil
}
