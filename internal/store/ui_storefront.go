package store

import (
	"context"
	"errors"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// Thin lookups for the storefront UI (internal/web/storefront).

// ActiveProductSlug is the slug of an active product ("" when unknown or
// inactive): the review form's product check (src/app/store/reviews.rs
// submit).
func ActiveProductSlug(ctx context.Context, q DBTX, productID uuid.UUID) (string, error) {
	slug, err := sqlc.New(q).StoreActiveProductSlug(ctx, productID)
	if errors.Is(err, pgx.ErrNoRows) {
		return "", nil
	}
	return slug, err
}
