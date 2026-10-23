package admin

import (
	"net/http"

	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// GET /admin (src/app/admin.rs).
func (h *handler) dashboard(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	d, err := store.LoadAdminDashboard(r.Context(), h.d.Pool)
	if err != nil {
		serverError(w, r, err)
		return
	}
	head(r, "Painel")
	render.Page(w, r, h.d, dashboardPage(d))
}
