package store

import (
	"context"

	"github.com/google/uuid"
)

// Thin lookups for the storefront UI (internal/web/storefront, wave2-D).

var sqlActiveProductSlug = sqlText(`SELECT slug FROM products WHERE id = $1 AND active`)

// ActiveProductSlug is the slug of an active product ("" when unknown or
// inactive): the review form's product check (src/app/store/reviews.rs
// submit).
func ActiveProductSlug(ctx context.Context, q DBTX, productID uuid.UUID) (string, error) {
	var slug string
	err := q.QueryRow(ctx, sqlActiveProductSlug, productID).Scan(&slug)
	if found, err := optional(err); err != nil || !found {
		return "", err
	}
	return slug, nil
}
