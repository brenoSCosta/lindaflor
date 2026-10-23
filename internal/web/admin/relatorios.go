package admin

import (
	"net/http"

	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// /admin/relatorios (src/app/admin/relatorios.rs): sales report over a
// range of Brasília days and the orders CSV export.

const reportsPath = "/admin/relatorios"

func rangeParams(r *http.Request) (de, ate *string) {
	q := r.URL.Query()
	if q.Has("de") {
		v := q.Get("de")
		de = &v
	}
	if q.Has("ate") {
		v := q.Get("ate")
		ate = &v
	}
	return de, ate
}

// GET /admin/relatorios?de=&ate=.
func (h *handler) reportsPage(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	de, ate := rangeParams(r)
	p, err := store.LoadReportPage(r.Context(), h.d.Pool, de, ate)
	if err != nil {
		serverError(w, r, err)
		return
	}
	head(r, "Relatórios")
	render.Page(w, r, h.d, reportsPage(p))
}

// GET /admin/relatorios/pedidos.csv?de=&ate=: orders created in range
// (UTF-8 BOM, formula-safe); too many → toast + back to the report.
func (h *handler) reportsCSV(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	de, ate := rangeParams(r)
	e, err := store.ExportOrdersCSV(r.Context(), h.d.Pool, de, ate)
	if err != nil {
		serverError(w, r, err)
		return
	}
	if e.TooMany {
		failTo(w, r, withQuery(reportsPath, "de", store.FormatDateISO(e.Range.From), "ate", store.FormatDateISO(e.Range.To)), e.TooManyMessage())
		return
	}
	writeCSV(w, e.Filename, e.Body)
}
