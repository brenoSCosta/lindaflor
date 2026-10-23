package storefront

import (
	"net/http"
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// activeProductSlug is the slug of an active product ("" when unknown).
func (h *handlers) activeProductSlug(r *http.Request, raw string) (uuid.UUID, string, error) {
	id, err := uuid.Parse(strings.TrimSpace(raw))
	if err != nil {
		return uuid.Nil, "", nil
	}
	slug, err := store.ActiveProductSlug(r.Context(), h.d.Pool, id)
	return id, slug, err
}

// reviewSubmit is POST /avaliacoes/enviar (verified buyers; per-user
// window budget), back to the product's #avaliacoes with a toast.
func (h *handlers) reviewSubmit(w http.ResponseWriter, r *http.Request) {
	if !parseForm(w, r) {
		return
	}
	productID, slug, err := h.activeProductSlug(r, r.PostForm.Get("product_id"))
	if err != nil {
		serverError(w, r, "review product", err)
		return
	}
	if slug == "" {
		toast.Set(w, r, toast.Error("Produto inválido."))
		toast.Redirect(w, r, "/produtos")
		return
	}
	productURL := productPath(slug)
	back := productURL + "#avaliacoes"
	uid := userID(r)
	if uid == nil {
		toast.Redirect(w, r, loginWithNext(productURL))
		return
	}
	if !ratelimit.CheckWindow(store.ScopeReviewSubmit, uid.String(), store.DefaultReviewsPerWindow, store.ReviewSubmitWindow) {
		toast.Set(w, r, toast.Error(store.ReviewRateLimitedMessage))
		toast.Redirect(w, r, back)
		return
	}
	var fit *string
	if vs, ok := r.PostForm["fit"]; ok && len(vs) > 0 {
		fit = &vs[0]
	}
	review, err := store.ParseNewReview(r.PostForm.Get("rating"), r.PostForm.Get("title"), r.PostForm.Get("body"), fit)
	if err == nil {
		_, err = store.SubmitReview(r.Context(), h.d.Pool, *uid, productID, review)
	}
	if err != nil {
		msg := store.ReviewErrorMessage(err)
		if msg == "" {
			serverError(w, r, "submit review", err)
			return
		}
		toast.Set(w, r, toast.Error(msg))
	} else {
		toast.Set(w, r, toast.Success(store.ReviewSubmittedTitle).WithDescription(store.ReviewSubmittedDescription))
	}
	toast.Redirect(w, r, back)
}

// reviewListFragment is GET /avaliacoes/lista?product_id=&page= (the
// review pagination shard). Plain requests go to the product page.
func (h *handlers) reviewListFragment(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	productID, slug, err := h.activeProductSlug(r, q.Get("product_id"))
	if err != nil {
		serverError(w, r, "review product", err)
		return
	}
	if slug == "" {
		render.NotFound(w, r, h.d)
		return
	}
	page := store.ParsePage(q.Get("page"))
	if !httpx.IsPartial(r) {
		httpx.Redirect(w, r, reviewsPageHref(slug, page))
		return
	}
	sec, err := store.LoadReviewSection(r.Context(), h.d.Pool, productID, userID(r), page)
	if err != nil {
		serverError(w, r, "load reviews", err)
		return
	}
	render.Fragment(w, r, http.StatusOK, reviewList(productID, slug, sec)) //nolint:contextcheck // templ components get the request ctx through Render
}
