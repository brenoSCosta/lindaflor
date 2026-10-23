package storefront

import (
	"errors"
	"net/http"
	"strconv"
	"strings"

	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// setCard is a /sets card.
type setCard struct {
	Set   store.SetSummary
	Cover string
}

// setsIndex is GET /sets (src/app/sets.rs).
func (h *handlers) setsIndex(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	sets, err := store.ListSets(ctx, h.d.Pool)
	if err != nil {
		serverError(w, r, "list sets", err)
		return
	}
	seo.SetHead(ctx, seo.New("Conjuntos de Biquíni").
		WithDescription("Conjuntos Linda Flor: top e calcinha combinando, com preço especial. Moda praia feita em Aracaju.").
		WithCanonical("/sets"))
	cards := make([]setCard, 0, len(sets))
	for _, s := range sets {
		cards = append(cards, setCard{Set: s, Cover: resolveCover(ctx, h.d, s.CoverURL)})
	}
	render.Page(w, r, h.d, setsPage(cards)) //nolint:contextcheck // templ components get the request ctx through Render
}

// setView is /sets/{slug} (src/app/sets/slug.rs).
type setView struct {
	Set         *store.SetSummary
	Gallery     []store.ProductImage // URLs resolved
	InitialImg  string
	Members     []store.ProductSummary
	MemberCount int64
	Available   int64
	Settings    store.StoreSettings
	Cards       CardCtx
	Added       bool
	AddError    string
}

func (v setView) soldOut() bool { return v.Available <= 0 }

func (v setView) discountBadge() string {
	if v.Set.PriceInCents == nil {
		return ""
	}
	if pct, ok := store.DiscountPercent(v.Set.CompareAtPriceInCents, *v.Set.PriceInCents); ok {
		return "-" + strconv.Itoa(pct) + "%"
	}
	return ""
}

func (v setView) pixCents() int64 {
	p := *v.Set.PriceInCents
	if v.Settings.PixDiscountPercent > 0 {
		return store.PixPrice(p, v.Settings.PixDiscountPercent)
	}
	return p
}

// setPage serves GET and POST /sets/{slug}; POST is the no-JS "Adicionar
// conjunto" (htmx posts to /conjuntos/adicionar).
func (h *handlers) setPage(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	seo.SetHead(ctx, seo.New("Conjunto não encontrado").WithNoIndex())
	set, err := store.GetSetBySlug(ctx, h.d.Pool, chi.URLParam(r, "slug"))
	if err != nil {
		serverError(w, r, "load set", err)
		return
	}
	if set == nil {
		render.Page(w, r, h.d, setNotFound())
		return
	}
	v := setView{Set: set}
	images, err := store.ListSetImages(ctx, h.d.Pool, set.ID)
	if err != nil {
		serverError(w, r, "list set images", err)
		return
	}
	for _, img := range images {
		if u := store.ResolveStorageURL(ctx, h.d.Storage, &img.URL); u != "" {
			img.URL = u
			v.Gallery = append(v.Gallery, img)
		}
	}
	if v.Members, err = store.ListSetMembers(ctx, h.d.Pool, set.ID); err != nil {
		serverError(w, r, "list set members", err)
		return
	}
	v.Members = store.ResolveSummaryImages(ctx, h.d.Storage, v.Members)
	if v.MemberCount, err = store.CountSetMembers(ctx, h.d.Pool, set.ID); err != nil {
		serverError(w, r, "count set members", err)
		return
	}
	if v.Available, err = store.SetAvailableQuantity(ctx, h.d.Pool, set.ID); err != nil {
		serverError(w, r, "set availability", err)
		return
	}
	v.Settings = store.StorefrontSettings(ctx, h.d.Pool)
	cover := ""
	if len(v.Gallery) > 0 {
		cover = v.Gallery[0].URL
	}
	seo.SetHead(ctx, seo.SetPageHead(seo.SiteOrigin(h.d.Cfg.AppOrigin, r), seo.SetInfo{
		Name: set.Name, Slug: set.Slug, Description: deref(set.Description), PriceCents: set.PriceInCents,
	}, cover, v.Available > 0))
	want := r.URL.Query().Get("img")
	for _, img := range v.Gallery {
		if img.ID.String() == want {
			v.InitialImg = want
		}
	}
	if v.InitialImg == "" && len(v.Gallery) > 0 {
		v.InitialImg = v.Gallery[0].ID.String()
	}

	if r.Method == http.MethodPost {
		if !parseForm(w, r) {
			return
		}
		qty := int32(1)
		if n, err := strconv.ParseInt(strings.TrimSpace(r.PostForm.Get("quantity")), 10, 32); err == nil {
			qty = int32(n)
		}
		ref, ok := h.cartRef(w, r)
		if !ok {
			return
		}
		switch err := store.AddSetToCart(ctx, h.d.Pool, ref.ID, set.ID, qty); {
		case err == nil:
			v.Added = true
		case errors.Is(err, store.ErrInvalidVariant):
			v.AddError = "Este conjunto não está disponível."
		case errors.Is(err, store.ErrOutOfStock):
			v.AddError = "Este conjunto está esgotado."
		default:
			serverError(w, r, "add set to cart", err)
			return
		}
	}
	v.Cards = h.cardCtx(r, v.Settings)
	render.Page(w, r, h.d, setPageView(v)) //nolint:contextcheck // templ components get the request ctx through Render
}
