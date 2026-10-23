package admin

import (
	"net/http"
	"strings"

	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/payments"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// /admin/pedidos (src/app/admin/pedidos.rs, pedidos/id.rs).

const (
	ordersPath     = "/admin/pedidos"
	ordersListPath = "/admin/pedidos/lista"
)

func ordersURL(path, q, status string, page int64) string {
	if status == "all" {
		status = ""
	}
	return withQuery(path, "q", q, "status", status, "page", pageParam(page))
}

type orderDirectory struct {
	store.AdminOrderDirectory
	Filters []dirLink
	Pager   pagerData
}

func (h *handler) loadOrders(r *http.Request, q, status string, page int64) (orderDirectory, error) {
	d, err := store.LoadAdminOrders(r.Context(), h.d.Pool, q, status, store.ClampPage(page))
	if err != nil {
		return orderDirectory{}, err
	}
	out := orderDirectory{AdminOrderDirectory: d}
	for _, f := range store.OrderStatusFilters {
		out.Filters = append(out.Filters, dirLink{
			Label: f[1], Selected: d.Status == f[0],
			Page: ordersURL(ordersPath, d.Query, f[0], 1), Frag: ordersURL(ordersListPath, d.Query, f[0], 1),
		})
	}
	out.Pager = pagerFor(d.Pager.Page, d.Pager.TotalPages, func(p int64) dirLink {
		return dirLink{Page: ordersURL(ordersPath, d.Query, d.Status, p), Frag: ordersURL(ordersListPath, d.Query, d.Status, p)}
	})
	return out, nil
}

// GET /admin/pedidos.
func (h *handler) ordersPage(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	q := r.URL.Query()
	d, err := h.loadOrders(r, q.Get("q"), q.Get("status"), store.ParsePage(q.Get("page")))
	if err != nil {
		serverError(w, r, err)
		return
	}
	head(r, "Pedidos")
	render.Page(w, r, h.d, ordersPage(d))
}

// GET /admin/pedidos/lista (htmx fragment).
func (h *handler) ordersList(w http.ResponseWriter, r *http.Request) {
	if !httpx.IsHTMX(r) {
		h.ordersPage(w, r)
		return
	}
	if _, ok := h.actor(w, r); !ok {
		return
	}
	q := r.URL.Query()
	d, err := h.loadOrders(r, q.Get("q"), q.Get("status"), store.ParsePage(q.Get("page")))
	if err != nil {
		serverError(w, r, err)
		return
	}
	fragment(w, r, ordersURL(ordersPath, d.Query, d.Status, d.AdminOrderDirectory.Pager.Page), orderResults(d))
}

// orderView is everything the order page renders.
type orderView struct {
	O *store.AdminOrderDetail
	// Payment is the Mercado Pago panel (nil for static PIX orders).
	Payment      *payments.AdminPaymentInfo
	MPRefundable bool
	// ManualMoney: static-PIX orders keep the local paid/refund buttons.
	ManualMoney bool
	Override    *overrideAction
}

type overrideAction struct {
	Status, Label string
	Restock       bool
}

func (v orderView) action() string { return "/admin/pedidos/" + v.O.ID.String() }

// GET /admin/pedidos/{id}.
func (h *handler) orderPage(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	id, ok := parseUUID(chi.URLParam(r, "id"))
	if !ok {
		render.NotFound(w, r, h.d)
		return
	}
	o, err := store.LoadAdminOrder(r.Context(), h.d.Pool, id)
	if err != nil {
		serverError(w, r, err)
		return
	}
	if o == nil {
		render.NotFound(w, r, h.d)
		return
	}
	v := orderView{O: o, ManualMoney: true}
	info, found, err := h.pay.AdminPaymentInfo(r.Context(), id)
	if err != nil {
		serverError(w, r, err)
		return
	}
	if found && info.Provider == config.ProviderMercadoPago {
		v.Payment = &info
		v.ManualMoney = false
		v.MPRefundable = info.Refundable(sqlc.OrderStatus(o.Status))
		if status, label, restock, ok := o.ManualOverride(); ok {
			v.Override = &overrideAction{Status: status, Label: label, Restock: restock}
		}
	}
	head(r, "Pedido "+o.Code())
	render.Page(w, r, h.d, orderPage(v))
}

// POST /admin/pedidos/{id}: status change, MP sync/refund (routed to
// internal/payments first) and the MP manual override.
func (h *handler) orderUpdate(w http.ResponseWriter, r *http.Request) {
	actor, ok := h.actor(w, r)
	if !ok || !parseForm(w, r) {
		return
	}
	id, ok := parseUUID(chi.URLParam(r, "id"))
	if !ok {
		failTo(w, r, ordersPath, "Pedido inválido.")
		return
	}
	back := ordersPath + "/" + id.String()
	status := strings.TrimSpace(r.PostFormValue("status"))
	if action, ok := payments.ParseAdminAction(status); ok {
		msg, err := h.pay.RunAdminAction(r.Context(), id, action, paymentsActor(actor))
		if err != nil {
			failTo(w, r, back, err.Error())
			return
		}
		okTo(w, r, back, msg)
		return
	}
	in := store.AdminStatusInput{
		Status: status, Restock: r.PostForm.Has("restock"),
		Carrier: r.PostFormValue("carrier"), TrackingCode: r.PostFormValue("tracking_code"),
	}
	if r.PostForm.Has("override_confirm") {
		v := r.PostFormValue("override_confirm")
		in.OverrideConfirm = &v
	}
	if r.PostForm.Has("override_reason") {
		v := r.PostFormValue("override_reason")
		in.OverrideReason = &v
	}
	msg, err := store.AdminUpdateOrder(r.Context(), h.d.Pool, id, in, storeActor(actor))
	if err != nil {
		if _, user := store.UserMessage(err); !user && !store.IsApplyDomainError(err) {
			serverLog(r, err)
		}
		failTo(w, r, back, store.AdminOrderErrorMessage(err))
		return
	}
	okTo(w, r, back, msg)
}
