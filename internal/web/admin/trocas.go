package admin

import (
	"net/http"

	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/payments"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// /admin/trocas (src/app/admin/trocas.rs, trocas/id.rs).

const returnsPath = "/admin/trocas"

type returnsData struct {
	Filter  string
	Filters []dirLink
	Rows    []store.AdminReturnRow
}

// GET /admin/trocas?status=.
func (h *handler) returnsPage(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	filter := store.ClampReturnFilter(r.URL.Query().Get("status"))
	rows, err := store.ListAdminReturns(r.Context(), h.d.Pool, filter)
	if err != nil {
		serverError(w, r, err)
		return
	}
	d := returnsData{Filter: filter, Rows: rows}
	for _, f := range store.ReturnFilters {
		u := withQuery(returnsPath, "status", f[0])
		d.Filters = append(d.Filters, dirLink{Label: f[1], Page: u, Frag: u, Selected: f[0] == filter})
	}
	head(r, "Trocas e devoluções")
	render.Page(w, r, h.d, returnsPage(d))
}

// GET /admin/trocas/{id}.
func (h *handler) returnPage(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	id, ok := parseUUID(chi.URLParam(r, "id"))
	if !ok {
		render.NotFound(w, r, h.d)
		return
	}
	d, err := store.LoadAdminReturn(r.Context(), h.d.Pool, id)
	if err != nil {
		serverError(w, r, err)
		return
	}
	if d == nil {
		render.NotFound(w, r, h.d)
		return
	}
	head(r, d.Summary.KindLabel())
	render.Page(w, r, h.d, returnPage(d))
}

// POST /admin/trocas/{id}: one action of the request.
func (h *handler) returnUpdate(w http.ResponseWriter, r *http.Request) {
	actor, ok := h.actor(w, r)
	if !ok || !parseForm(w, r) {
		return
	}
	id, ok := parseUUID(chi.URLParam(r, "id"))
	if !ok {
		failTo(w, r, returnsPath, "Solicitação inválida.")
		return
	}
	back := returnsPath + "/" + id.String()
	action, err := store.ParseReturnForm(r.PostFormValue("action"), r.PostFormValue("text"),
		r.PostFormValue("amount"), r.PostForm.Has("restock"))
	if err != nil {
		failTo(w, r, back, userMessage(err, "Ação inválida."))
		return
	}
	ctx := r.Context()
	sa := storeActor(actor)
	if action.Local != nil {
		if _, err := store.AdminApplyReturn(ctx, h.d.Pool, id, *action.Local, sa); err != nil {
			if store.ReturnErrorMessage(err) == "" {
				serverLog(r, err)
			}
			failTo(w, r, back, store.ReturnAdminErrorMessage(err))
			return
		}
		okTo(w, r, back, "Solicitação atualizada.")
		return
	}
	// Mercado Pago refund: same PSP path as the order page, then resolve.
	orderID, err := store.CheckMPRefund(ctx, h.d.Pool, id)
	if err != nil {
		failTo(w, r, back, store.ReturnAdminErrorMessage(err))
		return
	}
	if _, err := h.pay.RunAdminAction(ctx, orderID, payments.AdminRefund, paymentsActor(actor)); err != nil {
		failTo(w, r, back, err.Error())
		return
	}
	if err := store.FinishMPRefund(ctx, h.d.Pool, id, action.MPRestock, sa); err != nil {
		serverLog(r, err)
		failTo(w, r, back, "Estorno feito no Mercado Pago, mas a solicitação não foi concluída: "+store.ReturnAdminErrorMessage(err))
		return
	}
	okTo(w, r, back, "Estornado no Mercado Pago e solicitação concluída.")
}
