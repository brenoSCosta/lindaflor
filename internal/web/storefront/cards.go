package storefront

import (
	"net/http"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/store"
)

// CardCtx is the per-request data every product card reads (one query each
// per page, like the Rust #[memoize] helpers): the viewer's liked products,
// approved ratings and the Pix discount.
type CardCtx struct {
	Liked      map[uuid.UUID]bool
	Ratings    map[uuid.UUID]store.CardRating
	PixPercent int
}

func (c CardCtx) liked(id uuid.UUID) bool { return c.Liked[id] }

// cardCtx loads the card context; failures degrade to empty maps (the Rust
// helpers swallowed errors too).
func (h *handlers) cardCtx(r *http.Request, settings store.StoreSettings) CardCtx {
	ctx := r.Context()
	c := CardCtx{Liked: map[uuid.UUID]bool{}, Ratings: map[uuid.UUID]store.CardRating{}, PixPercent: settings.PixDiscountPercent}
	if uid := userID(r); uid != nil {
		if liked, err := store.WishlistIDs(ctx, h.d.Pool, *uid); err == nil {
			c.Liked = liked
		}
	}
	if ratings, err := store.AllRatings(ctx, h.d.Pool); err == nil {
		c.Ratings = ratings
	}
	return c
}

// colorDots are the first five swatches of a card plus the "+N" overflow.
func colorDots(colors []store.ColorSwatch) ([]store.ColorSwatch, int) {
	if len(colors) > 5 {
		return colors[:5], len(colors) - 5
	}
	return colors, 0
}

func swatchStyle(hex *string) string {
	if hex == nil {
		return ""
	}
	return "background-color: " + *hex
}

func wishlistLabel(liked bool) string {
	if liked {
		return "Remover dos favoritos"
	}
	return "Adicionar aos favoritos"
}

func wishlistClass(liked bool) string {
	if liked {
		return "rounded-full bg-background/90 p-2 text-rose-600 shadow-sm transition hover:scale-105 [&_svg]:fill-current"
	}
	return "rounded-full bg-background/90 p-2 text-foreground shadow-sm transition hover:scale-105 hover:text-rose-600"
}

func starClass(filled bool, small bool) string {
	switch {
	case filled && small:
		return "size-3.5 text-amber-500 fill-current"
	case filled:
		return "size-4 text-amber-500 fill-current"
	case small:
		return "size-3.5 text-muted-foreground/40"
	default:
		return "size-4 text-muted-foreground/40"
	}
}

func boolStr(b bool) string {
	if b {
		return "true"
	}
	return "false"
}
