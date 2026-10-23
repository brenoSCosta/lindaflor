package admin

import (
	"errors"
	"net/http"

	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// /admin/estoque (src/app/admin/estoque.rs): stock per warehouse with a
// search fragment and the manual adjustment form (audited).

const (
	stockPath     = "/admin/estoque"
	stockListPath = "/admin/estoque/lista"
)

// GET /admin/estoque.
func (h *handler) stockPage(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	d, err := store.LoadStockDirectory(r.Context(), h.d.Pool, r.URL.Query().Get("q"))
	if err != nil {
		serverError(w, r, err)
		return
	}
	head(r, "Estoque")
	render.Page(w, r, h.d, stockPage(d))
}

// GET /admin/estoque/lista (htmx fragment).
func (h *handler) stockList(w http.ResponseWriter, r *http.Request) {
	if !httpx.IsHTMX(r) {
		h.stockPage(w, r)
		return
	}
	if _, ok := h.actor(w, r); !ok {
		return
	}
	d, err := store.LoadStockDirectory(r.Context(), h.d.Pool, r.URL.Query().Get("q"))
	if err != nil {
		serverError(w, r, err)
		return
	}
	fragment(w, r, withQuery(stockPath, "q", d.Query), stockResults(d))
}

// POST /admin/estoque: adjust one inventory row (delta + reason).
func (h *handler) stockAdjust(w http.ResponseWriter, r *http.Request) {
	actor, ok := h.actor(w, r)
	if !ok || !parseForm(w, r) {
		return
	}
	back := withQuery(stockPath, "q", store.ClampAdminQuery(r.PostFormValue("q"), 80))
	err := store.AdjustStockWithAudit(r.Context(), h.d.Pool, r.PostFormValue("inventory_id"),
		r.PostFormValue("delta"), r.PostFormValue("reason"), storeActor(actor))
	if err != nil {
		if msg, ok := store.UserMessage(err); ok {
			failTo(w, r, back, msg)
			return
		}
		if errors.Is(err, store.ErrInsufficientStock) {
			failTo(w, r, back, err.Error())
			return
		}
		serverError(w, r, err)
		return
	}
	okTo(w, r, back, "Estoque ajustado.")
}
