package storefront

import (
	"context"
	"encoding/json"
	"errors"
	"net/http"
	"strconv"
	"strings"

	"github.com/a-h/templ"
	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// Cart sheet + procedures (src/app/store/cart_sheet.rs, cart.rs).
//
// Topcoat procedures become htmx POST endpoints that answer with the
// refreshed sheet body (swapped into #cart-sheet-body) plus an
// <hx-partial> for the header badge (#cart-count). Without htmx each form
// posts normally and gets a 303 back to the page it came from.

// cartShowEvent opens the sheet (HX-Trigger, handled by Alpine on the
// sheet: x-on:cart-show.window).
const cartShowEvent = "cart-show"

// setGroup is one set bundle in the sheet: members share one quantity.
type setGroup struct {
	ID          uuid.UUID
	Name        string
	Members     []store.CartItem
	Quantity    int32
	MaxQuantity int32
	TotalCents  int64
}

func groupSetLines(items []store.CartItem) ([]store.CartItem, []setGroup) {
	var loose []store.CartItem
	var groups []setGroup
	index := map[uuid.UUID]int{}
	for _, item := range items {
		if item.SetID == nil {
			loose = append(loose, item)
			continue
		}
		i, ok := index[*item.SetID]
		if !ok {
			index[*item.SetID] = len(groups)
			groups = append(groups, setGroup{ID: *item.SetID, Name: deref(item.SetName)})
			i = len(groups) - 1
		}
		groups[i].Members = append(groups[i].Members, item)
	}
	for i := range groups {
		g := &groups[i]
		g.Quantity = g.Members[0].Quantity
		g.MaxQuantity = g.Members[0].MaxQuantity
		for _, m := range g.Members {
			g.MaxQuantity = min(g.MaxQuantity, m.MaxQuantity)
			g.TotalCents += m.LineTotalCents()
		}
	}
	return loose, groups
}

// sheetView is the cart sheet body.
type sheetView struct {
	Loose       []store.CartItem
	Groups      []setGroup
	Notices     []store.CartNotice
	ItemCount   int32
	Subtotal    int64
	Discount    int64
	Coupon      *store.ResolvedCoupon
	CouponInput string
	CouponError string
	Settings    store.StoreSettings
	// Page renders the no-JS cart page (GET /carrinho/sheet without htmx):
	// the close buttons become links.
	Page bool
}

func (v sheetView) empty() bool { return len(v.Loose) == 0 && len(v.Groups) == 0 }

func (v sheetView) missingForFreeShipping() int64 {
	return max(v.Settings.FreeShippingThresholdCents-v.Subtotal, 0)
}

func (v sheetView) freeShippingPercent() int {
	t := v.Settings.FreeShippingThresholdCents
	if t <= 0 {
		return 100
	}
	pct := float64(max(v.Subtotal, 0)) / float64(t) * 100
	return int(min(pct, 100))
}

func (v sheetView) pixTotal() int64 {
	return store.PixPrice(max(v.Subtotal-v.Discount, 0), v.Settings.PixDiscountPercent)
}

func (v sheetView) freeShippingHint() string {
	if v.Settings.FreeShippingThresholdCents <= 0 {
		return "Frete grátis em todas as compras. Calculado no checkout."
	}
	return "Frete grátis para compras acima de " + price(v.Settings.FreeShippingThresholdCents) + ". Calculado no checkout."
}

// sheetOptions override the coupon field after a failed apply.
type sheetOptions struct {
	couponInput *string
	couponError string
	page        bool
}

// buildSheet hydrates the cart (nil ref = no cart yet) and previews the
// stored coupon (validation only, rolled back).
func (h *handlers) buildSheet(ctx context.Context, ref *store.CartRef, uid *uuid.UUID, o sheetOptions) (sheetView, error) {
	v := sheetView{Page: o.page}
	var items []store.CartItem
	stored := ""
	if ref != nil {
		hc, err := store.HydrateCart(ctx, h.d.Pool, ref.ID)
		if err != nil {
			return v, err
		}
		items, v.Notices = hc.Items, hc.Notices
		if stored, err = store.CartCouponCode(ctx, h.d.Pool, ref.ID); err != nil {
			return v, err
		}
	}
	store.ResolveCartImages(ctx, h.d.Storage, items)
	v.Subtotal, _ = store.CartSubtotalCents(items)
	for _, it := range items {
		v.ItemCount += it.Quantity
	}
	v.CouponInput = stored
	if stored != "" {
		tx, err := h.d.Pool.Begin(ctx)
		if err != nil {
			return v, err
		}
		coupon, cerr := store.ResolveCoupon(ctx, tx, stored, v.Subtotal, uid, nil, false)
		_ = tx.Rollback(ctx)
		switch {
		case cerr == nil:
			v.Coupon = coupon
		case errors.Is(cerr, store.ErrCouponGone):
			v.CouponError = store.CouponGoneMessage
		default:
			msg, ok := store.UserMessage(cerr)
			if !ok {
				return v, cerr
			}
			v.CouponError = msg
		}
	}
	if v.Coupon != nil {
		v.Discount = v.Coupon.DiscountCents
	}
	if o.couponInput != nil {
		v.CouponInput = *o.couponInput
	}
	if o.couponError != "" {
		v.CouponError = o.couponError
	}
	v.Loose, v.Groups = groupSetLines(items)
	settings, err := store.GetStoreSettings(ctx, h.d.Pool)
	if err != nil {
		return v, err
	}
	v.Settings = settings
	return v, nil
}

// cartSheet is GET /carrinho/sheet: the sheet body for htmx (the header
// cart button loads it when opening), the no-JS cart page otherwise.
// Read-only: rendering never creates a cart.
func (h *handlers) cartSheet(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	uid := userID(r)
	ref, err := store.FindCartHTTP(w, r, h.d.Pool, h.secure(), uid)
	if err != nil {
		serverError(w, r, "find cart", err)
		return
	}
	partial := httpx.IsPartial(r)
	v, err := h.buildSheet(ctx, ref, uid, sheetOptions{page: !partial})
	if err != nil {
		serverError(w, r, "build cart sheet", err)
		return
	}
	if partial {
		render.Fragment(w, r, http.StatusOK, cartSheetBody(v)) //nolint:contextcheck // templ components get the request ctx through Render
		return
	}
	seo.SetHead(ctx, seo.New("Carrinho").WithNoIndex())
	render.Page(w, r, h.d, cartPage(v)) //nolint:contextcheck // templ components get the request ctx through Render
}

// mutation runs one cart procedure on the request's cart (created when
// missing, like Rust load_or_create_cart) and answers: htmx → refreshed
// sheet + badge partial (+ extra partials); plain → 303 back.
type mutationResult struct {
	// errMsg: shopper-facing error toast.
	errMsg string
	opts   sheetOptions
	// added: also update the page's #add-to-cart-feedback and open the sheet.
	added bool
	// success toast for the no-JS path.
	okMsg string
}

func (h *handlers) respondCart(w http.ResponseWriter, r *http.Request, ref store.CartRef, res mutationResult) {
	ctx := r.Context()
	if !httpx.IsHTMX(r) {
		switch {
		case res.errMsg != "":
			toast.Set(w, r, toast.Error(res.errMsg))
		case res.opts.couponError != "":
			toast.Set(w, r, toast.Error(res.opts.couponError))
		case res.okMsg != "":
			toast.Set(w, r, toast.Success(res.okMsg))
		}
		toast.Redirect(w, r, backOr(r, "/carrinho/sheet"))
		return
	}
	if res.errMsg != "" {
		toast.Set(w, r, toast.Error(res.errMsg))
	}
	count, err := store.CartItemCount(ctx, h.d.Pool, &ref.Token)
	if err != nil {
		serverError(w, r, "cart count", err)
		return
	}
	v, err := h.buildSheet(ctx, &ref, userID(r), res.opts)
	if err != nil {
		serverError(w, r, "build cart sheet", err)
		return
	}
	if res.added {
		b, _ := json.Marshal(map[string]bool{cartShowEvent: true})
		w.Header().Set(httpx.HXTrigger, string(b))
	}
	render.Fragment(w, r, http.StatusOK, cartMutationResponse(v, count, res.added)) //nolint:contextcheck // templ components get the request ctx through Render
}

func (h *handlers) cartRef(w http.ResponseWriter, r *http.Request) (store.CartRef, bool) {
	ref, err := store.LoadOrCreateCartHTTP(w, r, h.d.Pool, h.secure(), userID(r))
	if err != nil {
		serverError(w, r, "load cart", err)
		return ref, false
	}
	return ref, true
}

func formUUID(r *http.Request, key string) (uuid.UUID, bool) {
	id, err := uuid.Parse(strings.TrimSpace(r.PostForm.Get(key)))
	return id, err == nil
}

func formQuantity(r *http.Request) (int32, bool) {
	n, err := strconv.ParseInt(strings.TrimSpace(r.PostForm.Get("quantity")), 10, 32)
	return int32(n), err == nil
}

// addFailed answers a failed add-to-cart: htmx gets the inline message
// (422, retargeted to the page's #add-to-cart-feedback); plain requests a
// toast and a 303 back.
func addFailed(w http.ResponseWriter, r *http.Request, msg string) {
	if !httpx.IsHTMX(r) {
		toast.Set(w, r, toast.Error(msg))
		toast.Redirect(w, r, backOr(r, "/produtos"))
		return
	}
	w.Header().Set(httpx.HXRetarget, "#add-to-cart-feedback")
	w.Header().Set(httpx.HXReswap, "outerHTML")
	render.Fragment(w, r, http.StatusUnprocessableEntity, addToCartFeedback(false, msg))
}

// cartAdd is POST /carrinho/adicionar (variant_id).
func (h *handlers) cartAdd(w http.ResponseWriter, r *http.Request) {
	if !parseForm(w, r) {
		return
	}
	variantID, ok := formUUID(r, "variant_id")
	if !ok {
		addFailed(w, r, "Variação inválida.") //nolint:contextcheck // templ components get the request ctx through Render
		return
	}
	ref, ok := h.cartRef(w, r)
	if !ok {
		return
	}
	switch err := store.AddItem(r.Context(), h.d.Pool, ref.ID, variantID, 1); {
	case errors.Is(err, store.ErrInvalidVariant):
		addFailed(w, r, "Esta variação não está disponível.") //nolint:contextcheck // templ components get the request ctx through Render
		return
	case errors.Is(err, store.ErrOutOfStock):
		addFailed(w, r, "Esta peça está esgotada.") //nolint:contextcheck // templ components get the request ctx through Render
		return
	case err != nil:
		serverError(w, r, "add to cart", err)
		return
	}
	h.respondCart(w, r, ref, mutationResult{added: true, okMsg: "Adicionado ao carrinho!"})
}

// setAdd is POST /conjuntos/adicionar (set_id, quantity): the whole
// fixed-composition bundle.
func (h *handlers) setAdd(w http.ResponseWriter, r *http.Request) {
	if !parseForm(w, r) {
		return
	}
	setID, ok := formUUID(r, "set_id")
	if !ok {
		addFailed(w, r, "Conjunto inválido.") //nolint:contextcheck // templ components get the request ctx through Render
		return
	}
	qty, ok := formQuantity(r)
	if !ok {
		addFailed(w, r, "Quantidade inválida.") //nolint:contextcheck // templ components get the request ctx through Render
		return
	}
	ref, ok := h.cartRef(w, r)
	if !ok {
		return
	}
	switch err := store.AddSetToCart(r.Context(), h.d.Pool, ref.ID, setID, qty); {
	case errors.Is(err, store.ErrInvalidVariant):
		addFailed(w, r, "Este conjunto não está disponível.") //nolint:contextcheck // templ components get the request ctx through Render
		return
	case errors.Is(err, store.ErrOutOfStock):
		addFailed(w, r, "Este conjunto está esgotado.") //nolint:contextcheck // templ components get the request ctx through Render
		return
	case err != nil:
		serverError(w, r, "add set to cart", err)
		return
	}
	h.respondCart(w, r, ref, mutationResult{added: true, okMsg: "Adicionado ao carrinho!"})
}

// cartLineMutation handles remove / quantity for a variant line or a set
// group. Stock/variant refusals are ignored (the sheet shows the state).
func (h *handlers) cartLineMutation(key, invalidMsg string, withQty bool, apply func(ctx context.Context, ref store.CartRef, id uuid.UUID, qty int32) error) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		if !parseForm(w, r) {
			return
		}
		id, okID := formUUID(r, key)
		var qty int32
		okQty := true
		if withQty {
			qty, okQty = formQuantity(r)
		}
		ref, ok := h.cartRef(w, r)
		if !ok {
			return
		}
		res := mutationResult{}
		switch {
		case !okID:
			res.errMsg = invalidMsg
		case !okQty:
			res.errMsg = "Quantidade inválida."
		default:
			err := apply(r.Context(), ref, id, qty)
			if err != nil && !errors.Is(err, store.ErrInvalidVariant) && !errors.Is(err, store.ErrOutOfStock) {
				serverError(w, r, "cart mutation", err)
				return
			}
		}
		h.respondCart(w, r, ref, res)
	}
}

// cartCoupon is POST /carrinho/cupom (code): applies or (empty code)
// clears the coupon; refusals re-render the sheet with the inline message
// and the typed code.
func (h *handlers) cartCoupon(w http.ResponseWriter, r *http.Request) {
	if !parseForm(w, r) {
		return
	}
	ref, ok := h.cartRef(w, r)
	if !ok {
		return
	}
	code := r.PostForm.Get("code")
	msg, err := store.ApplyCartCoupon(r.Context(), h.d.Pool, ref.ID, userID(r), code)
	if err != nil {
		if m, ok := store.UserMessage(err); ok {
			msg = m
		} else {
			serverError(w, r, "apply coupon", err)
			return
		}
	}
	res := mutationResult{}
	if msg != "" {
		typed := code
		res.opts = sheetOptions{couponInput: &typed, couponError: msg}
	}
	h.respondCart(w, r, ref, res)
}

// sheetCloseAttrs are the close button attributes (hidden on the no-JS
// cart page).
func sheetCloseAttrs(page bool) templ.Attributes {
	if page {
		return templ.Attributes{"type": "button", "hidden": true}
	}
	return templ.Attributes{"type": "button", "aria-label": "Fechar carrinho", "x-on:click": "$store.cart.open = false"}
}

func (h *handlers) removeItem(ctx context.Context, ref store.CartRef, id uuid.UUID, _ int32) error {
	return store.RemoveItem(ctx, h.d.Pool, ref.ID, id)
}

func (h *handlers) setItemQuantity(ctx context.Context, ref store.CartRef, id uuid.UUID, qty int32) error {
	return store.SetQuantity(ctx, h.d.Pool, ref.ID, id, qty)
}

func (h *handlers) setGroupQuantity(ctx context.Context, ref store.CartRef, id uuid.UUID, qty int32) error {
	return store.SetSetQuantity(ctx, h.d.Pool, ref.ID, id, qty)
}

func (h *handlers) removeGroup(ctx context.Context, ref store.CartRef, id uuid.UUID, _ int32) error {
	return store.RemoveSet(ctx, h.d.Pool, ref.ID, id)
}
