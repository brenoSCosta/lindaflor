package storefront

import (
	"errors"
	"net/http"
	"net/url"
	"strings"

	"github.com/go-chi/chi/v5"
	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// productView is the product page (src/app/produtos/slug.rs).
type productView struct {
	P            *store.ProductDetail
	Selected     *store.ProductVariant
	Price        int64
	InitialImg   string
	Settings     store.StoreSettings
	ShippingCopy string
	WhatsappURL  string
	Looks        []store.ProductSummary
	Related      []store.ProductSummary
	Cards        CardCtx
	Reviews      store.ReviewSection
	Added        bool
	AddError     string
	AlertEmail   string
	SiteKey      string // Turnstile on the avise-me form ("" = off)
}

func (v productView) selectedAvailable() int32 {
	if v.Selected == nil {
		return 0
	}
	return v.Selected.Available
}

func (v productView) selectedColor() string {
	if v.Selected == nil {
		return ""
	}
	return v.Selected.Color
}

func (v productView) pixPrice() int64 { return store.PixPrice(v.Price, v.Settings.PixDiscountPercent) }

// productPage serves GET and POST /produtos/{slug}. POST is the no-JS
// add-to-cart form (the htmx path posts to /carrinho/adicionar): the page
// re-renders with "Adicionado ao carrinho!" or the error, like Rust.
func (h *handlers) productPage(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	var added bool
	var addError string
	if r.Method == http.MethodPost {
		if !parseForm(w, r) {
			return
		}
		if raw := strings.TrimSpace(r.PostForm.Get("variant_id")); raw != "" {
			variantID, err := uuid.Parse(raw)
			if err != nil {
				addError = "Variação inválida."
			} else {
				msg, err := h.addVariant(w, r, variantID)
				if err != nil {
					serverError(w, r, "add to cart", err)
					return
				}
				added, addError = msg == "", msg
			}
		}
	}

	// Not found renders with a 200, kept out of the index.
	seo.SetHead(ctx, seo.New("Produto não encontrado").WithNoIndex())
	p, err := store.GetProductBySlug(ctx, h.d.Pool, chi.URLParam(r, "slug"))
	if err != nil {
		serverError(w, r, "load product", err)
		return
	}
	if p == nil {
		render.Page(w, r, h.d, productNotFound())
		return
	}
	store.ResolveDetailImages(ctx, h.d.Storage, p)
	rating, err := store.LoadAggregateRating(ctx, h.d.Pool, p.ID)
	if err != nil {
		serverError(w, r, "load rating", err)
		return
	}
	seo.SetHead(ctx, productHead(seo.SiteOrigin(h.d.Cfg.AppOrigin, r), p, rating))

	settings, err := store.GetStoreSettings(ctx, h.d.Pool)
	if err != nil {
		serverError(w, r, "load settings", err)
		return
	}
	v := productView{P: p, Settings: settings, Added: added, AddError: addError}
	if settings.FreeShippingThresholdCents <= 0 {
		v.ShippingCopy = "Frete grátis em todas as compras. Para Sergipe, prazo de 3 a 7 dias úteis. Demais estados, 7 a 15 dias úteis."
	} else {
		v.ShippingCopy = "Frete grátis para compras acima de " + price(settings.FreeShippingThresholdCents) + ". Para Sergipe, prazo de 3 a 7 dias úteis. Demais estados, 7 a 15 dias úteis."
	}
	if v.Related, err = store.ListRelatedProducts(ctx, h.d.Pool, p.ID, p.Category); err != nil {
		serverError(w, r, "load related", err)
		return
	}
	v.Related = store.ResolveSummaryImages(ctx, h.d.Storage, v.Related)
	if v.Looks, err = store.ListProductLooks(ctx, h.d.Pool, p.ID); err != nil {
		serverError(w, r, "load looks", err)
		return
	}
	v.Looks = store.ResolveSummaryImages(ctx, h.d.Storage, v.Looks)

	q := r.URL.Query()
	v.Selected = selectVariant(p.Variants, q.Get("variant"))
	v.Price = p.PriceInCents
	if v.Selected != nil && v.Selected.PriceInCents != nil {
		v.Price = *v.Selected.PriceInCents
	}
	for _, img := range p.Images {
		if img.ID.String() == q.Get("img") {
			v.InitialImg = img.ID.String()
		}
	}
	if v.InitialImg == "" && len(p.Images) > 0 {
		v.InitialImg = p.Images[0].ID.String()
	}
	if n := settings.WhatsappNumberTrimmed(); n != "" {
		msg := "Olá! Tenho interesse no " + p.Name + " no valor de " + price(v.Price) + "."
		if v.Selected != nil {
			msg = "Olá! Tenho interesse no " + p.Name + " (" + v.Selected.Label() + ") no valor de " + price(v.Price) + "."
		}
		v.WhatsappURL = store.WhatsappLink(n, msg)
	}
	v.Cards = h.cardCtx(r, settings)
	v.SiteKey = h.d.Captcha.SiteKey()
	if u, ok := auth.CurrentUser(ctx); ok {
		v.AlertEmail = u.Email
	}
	if v.Reviews, err = store.LoadReviewSection(ctx, h.d.Pool, p.ID, userID(r), store.ParsePage(q.Get("avaliacoes"))); err != nil {
		serverError(w, r, "load reviews", err)
		return
	}
	render.Page(w, r, h.d, productPageView(v)) //nolint:contextcheck // templ components get the request ctx through Render
}

// selectVariant: ?variant= when it matches, else the first available,
// else the first variant.
func selectVariant(variants []store.ProductVariant, want string) *store.ProductVariant {
	for i := range variants {
		if variants[i].ID.String() == want {
			return &variants[i]
		}
	}
	for i := range variants {
		if variants[i].Available > 0 {
			return &variants[i]
		}
	}
	if len(variants) > 0 {
		return &variants[0]
	}
	return nil
}

func productHead(origin string, p *store.ProductDetail, rating *store.AggregateRating) seo.Head {
	info := seo.ProductInfo{Name: p.Name, Slug: p.Slug, Description: deref(p.Description), Category: p.Category, PriceCents: p.PriceInCents}
	for _, img := range p.Images {
		info.Images = append(info.Images, seo.ProductImage{URL: img.URL, Alt: deref(img.Alt)})
	}
	for _, v := range p.Variants {
		info.Variants = append(info.Variants, seo.ProductVariant{SKU: v.SKU, Available: int64(v.Available)})
	}
	var agg *seo.AggregateRating
	if rating != nil {
		agg = &seo.AggregateRating{Average: rating.RatingAverage, Count: rating.ReviewCount}
	}
	return seo.ProductHead(origin, info, agg)
}

// addVariant adds one unit of a variant to the request's cart (creating
// it). Returns the shopper-facing error ("" on success).
func (h *handlers) addVariant(w http.ResponseWriter, r *http.Request, variantID uuid.UUID) (string, error) {
	cart, err := store.LoadOrCreateCartHTTP(w, r, h.d.Pool, h.secure(), userID(r))
	if err != nil {
		return "", err
	}
	switch err := store.AddItem(r.Context(), h.d.Pool, cart.ID, variantID, 1); {
	case err == nil:
		return "", nil
	case errors.Is(err, store.ErrInvalidVariant):
		return "Esta variação não está disponível.", nil
	case errors.Is(err, store.ErrOutOfStock):
		return "Esta peça está esgotada.", nil
	default:
		return "", err
	}
}

func imgAlt(alt *string, fallback string) string {
	if alt != nil && *alt != "" {
		return *alt
	}
	return fallback
}

// galleryHref is the no-JS link of a gallery thumbnail.
func galleryHref(base string, keep url.Values, imgID uuid.UUID) string {
	v := url.Values{}
	for k, vs := range keep {
		v[k] = vs
	}
	v.Set("img", imgID.String())
	return withQuery(base, v)
}

func variantQuery(sel *store.ProductVariant) url.Values {
	v := url.Values{}
	if sel != nil {
		v.Set("variant", sel.ID.String())
	}
	return v
}

// Gallery classes: the server renders the initial state; Alpine toggles
// the state classes with an object binding.
const (
	layerOn         = "z-10 opacity-100"
	layerOff        = "pointer-events-none z-0 opacity-0"
	layerBase       = "absolute inset-0 h-full w-full object-cover transition-opacity duration-700 ease-in-out"
	thumbOn         = "border-primary ring-1 ring-primary/30"
	thumbOff        = "border-border opacity-80 hover:border-primary/40 hover:opacity-100"
	thumbBase       = "block aspect-square overflow-hidden rounded-lg border transition-[border-color,box-shadow,opacity] duration-300"
	swatchOn        = "border-primary ring-2 ring-primary/30"
	swatchOff       = "border-border hover:border-primary/40"
	swatchBase      = "h-8 w-8 rounded-full border-2 transition-[border-color,box-shadow] duration-300"
	imgSelectedExpr = "img === $el.dataset.img"
	colorMatchExpr  = "color === $el.dataset.color"
)

func stateClass(base, on, off string, active bool) string {
	if active {
		return base + " " + on
	}
	return base + " " + off
}

// xClass is an Alpine object binding toggling on/off classes by cond (an
// expression without user data).
func xClass(on, off, cond string) string {
	return "{ '" + on + "': " + cond + ", '" + off + "': !(" + cond + ") }"
}
