package checkout

import (
	"errors"
	"io"
	"net/http"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// returnFormURL is /pedido/{id}/troca, keeping the guest token.
func returnFormURL(id uuid.UUID, token string) string {
	return withToken("/pedido/"+id.String()+"/troca", token)
}

// returnView is the /pedido/{id}/troca template input.
type returnView struct {
	OrderID uuid.UUID
	Token   string
	Info    store.OrderReturns
	Items   []store.ReturnableItem
}

func (v returnView) canOpen() bool { return v.Info.CanOpen() && len(v.Items) > 0 }

func (v returnView) blocked() string {
	if v.Info.Blocked != nil {
		return *v.Info.Blocked
	}
	return ""
}

// GET /pedido/{id}/troca: the return/exchange request form.
func (h *handlers) returnPage(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	id, ok := h.orderID(w, r)
	if !ok {
		return
	}
	token := queryToken(r.URL.Query().Get("t"))
	seo.SetHead(ctx, seo.New("Troca ou devolução").WithNoIndex())
	allowed, err := store.OrderAccess(ctx, h.d.Pool, id, token, sessionUserID(r))
	if err != nil {
		h.fail(w, r, "order access", err)
		return
	}
	if !allowed {
		render.Status(w, r, h.d, http.StatusNotFound, orderNotFound(false))
		return
	}
	v := returnView{OrderID: id, Token: deref(token)}
	if v.Info, err = store.LoadOrderReturns(ctx, h.d.Pool, id); err != nil {
		h.fail(w, r, "order returns", err)
		return
	}
	items, err := store.ReturnableItems(ctx, h.d.Pool, id)
	if err != nil {
		h.fail(w, r, "returnable items", err)
		return
	}
	for _, it := range items {
		if it.Returnable > 0 {
			v.Items = append(v.Items, it)
		}
	}
	render.Page(w, r, h.d, returnPage(v))
}

// POST /pedido/{id}/troca: t, kind, reason, desired_size, qty_<item>.
func (h *handlers) submitReturn(w http.ResponseWriter, r *http.Request) {
	id, ok := h.orderID(w, r)
	if !ok {
		return
	}
	body, err := io.ReadAll(r.Body)
	if err != nil {
		if httpx.IsBodyTooLarge(err) {
			httpx.TooLarge(w)
			return
		}
		http.Error(w, "Formulário inválido.", http.StatusBadRequest)
		return
	}
	token, req := store.ParseCustomerReturnForm(store.QueryPairs(string(body)))
	back := returnFormURL(id, deref(token))
	if req == nil {
		toast.Set(w, r, toast.Error("Escolha o tipo de solicitação."))
		toast.Redirect(w, r, back)
		return
	}
	_, err = store.CreateReturnRequest(r.Context(), h.d.Pool, id, token, sessionUserID(r), *req)
	switch {
	case err == nil:
		toast.Set(w, r, toast.Success("Solicitação enviada. Responderemos por e-mail."))
		toast.Redirect(w, r, orderURL(id, deref(token)))
	case errors.Is(err, store.ErrReturnNotFound):
		toast.Set(w, r, toast.Error("Pedido não encontrado."))
		toast.Redirect(w, r, back)
	default:
		msg := store.ReturnErrorMessage(err)
		if msg == "" {
			h.fail(w, r, "create return request", err)
			return
		}
		toast.Set(w, r, toast.Error(msg))
		toast.Redirect(w, r, back)
	}
}
