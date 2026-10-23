package storefront

import (
	"net/http"
	"net/url"
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// favoritos is GET /favoritos (src/app/favoritos.rs; signed-in only).
func (h *handlers) favoritos(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	uid := userID(r)
	products, err := store.ListWishlistProducts(ctx, h.d.Pool, *uid)
	if err != nil {
		serverError(w, r, "list wishlist", err)
		return
	}
	products = store.ResolveSummaryImages(ctx, h.d.Storage, products)
	settings := store.StorefrontSettings(ctx, h.d.Pool)
	seo.SetHead(ctx, seo.New("Meus Favoritos").WithNoIndex())
	render.Page(w, r, h.d, favoritosPage(products, h.cardCtx(r, settings))) //nolint:contextcheck // templ components get the request ctx through Render
}

// returnPath: a same-site `next`, else "".
func returnPath(next string) string {
	if safe, ok := httpx.SafeLocalRedirect(next); ok {
		return safe
	}
	return ""
}

func loginWithNext(back string) string {
	return "/login?" + url.Values{"next": {back}}.Encode()
}

// wishlistToggle is POST /favoritos/alternar, the no-JS heart: toggles and
// goes back (next, Referer, else /favoritos) with a toast; guests go to
// /login?next=<back>.
func (h *handlers) wishlistToggle(w http.ResponseWriter, r *http.Request) {
	if !parseForm(w, r) {
		return
	}
	back := returnPath(r.PostForm.Get("next"))
	if back == "" {
		back = backOr(r, "/favoritos")
	}
	uid := userID(r)
	if uid == nil {
		toast.Redirect(w, r, loginWithNext(back))
		return
	}
	productID, err := uuid.Parse(strings.TrimSpace(r.PostForm.Get("product_id")))
	if err != nil {
		toast.Set(w, r, toast.Error("Produto inválido."))
		toast.Redirect(w, r, back)
		return
	}
	liked, err := store.ToggleWishlist(r.Context(), h.d.Pool, *uid, productID)
	if err != nil {
		serverError(w, r, "toggle wishlist", err)
		return
	}
	toast.Set(w, r, toast.Success(store.WishlistToggleMessage(liked)))
	toast.Redirect(w, r, back)
}

// wishlistToggleProc is POST /favoritos/alternar-proc, the htmx heart: it
// answers with the re-rendered button. Guests are sent to the login page
// (HX-Redirect with next = the current page); on /favoritos an unliked
// card is removed (HX-Retarget closest article + delete) and the page
// reloads when the list becomes empty.
func (h *handlers) wishlistToggleProc(w http.ResponseWriter, r *http.Request) {
	if !parseForm(w, r) {
		return
	}
	uid := userID(r)
	if uid == nil {
		w.Header().Set(httpx.HXRedirect, loginWithNext(currentPageURL(r)))
		w.WriteHeader(http.StatusNoContent)
		return
	}
	productID, err := uuid.Parse(strings.TrimSpace(r.PostForm.Get("product_id")))
	if err != nil {
		toast.Set(w, r, toast.Error("Produto inválido."))
		w.WriteHeader(http.StatusNoContent)
		return
	}
	ctx := r.Context()
	liked, err := store.ToggleWishlist(ctx, h.d.Pool, *uid, productID)
	if err != nil {
		serverError(w, r, "toggle wishlist", err)
		return
	}
	if !liked && strings.HasPrefix(httpx.CurrentPath(r), "/favoritos") {
		ids, err := store.WishlistIDs(ctx, h.d.Pool, *uid)
		if err != nil {
			serverError(w, r, "wishlist ids", err)
			return
		}
		if len(ids) == 0 {
			w.Header().Set(httpx.HXRefresh, "true")
			w.WriteHeader(http.StatusNoContent)
			return
		}
		w.Header().Set(httpx.HXRetarget, "closest article")
		w.Header().Set(httpx.HXReswap, "delete")
	}
	render.Fragment(w, r, http.StatusOK, WishlistButton(productID, liked, r.PostForm.Get("next"))) //nolint:contextcheck // templ components get the request ctx through Render
}

// currentPageURL is the path+query of the page an htmx request came from.
func currentPageURL(r *http.Request) string {
	if cur := r.Header.Get(httpx.HXCurrentURL); cur != "" {
		if u, err := url.Parse(cur); err == nil && u.Path != "" {
			p := u.EscapedPath()
			if u.RawQuery != "" {
				p += "?" + u.RawQuery
			}
			if safe := returnPath(p); safe != "" {
				return safe
			}
		}
	}
	return "/"
}
