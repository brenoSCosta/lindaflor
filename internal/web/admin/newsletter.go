package admin

import (
	"io"
	"net/http"

	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// /admin/newsletter (src/app/admin/newsletter.rs).

// GET /admin/newsletter.
func (h *handler) newsletterPage(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	d, err := store.LoadNewsletterAdmin(r.Context(), h.d.Pool)
	if err != nil {
		serverError(w, r, err)
		return
	}
	head(r, "Newsletter")
	render.Page(w, r, h.d, newsletterPage(d))
}

// GET /admin/newsletter/export-csv: active subscribers only (LGPD).
func (h *handler) newsletterCSV(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	body, err := store.NewsletterCSV(r.Context(), h.d.Pool)
	if err != nil {
		serverError(w, r, err)
		return
	}
	writeCSV(w, "newsletter.csv", body)
}

// writeCSV sends a CSV attachment (never cached).
func writeCSV(w http.ResponseWriter, filename, body string) {
	h := w.Header()
	h.Set("Content-Type", "text/csv; charset=utf-8")
	h.Set("Content-Disposition", `attachment; filename="`+filename+`"`)
	h.Set("Cache-Control", "no-store")
	w.WriteHeader(http.StatusOK)
	_, _ = io.WriteString(w, body)
}
