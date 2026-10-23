package checkout

import (
	"errors"
	"net/http"
	"strings"

	"github.com/go-chi/chi/v5"
	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/payments"
	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/checkout/qrsvg"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// statusPollInterval is how often a pending order page re-checks its
// status (pedido:poll allows 10/min per IP).
const statusPollInterval = "every 10s"

// GET /pedido: where the order links live.
func (h *handlers) trackPage(w http.ResponseWriter, r *http.Request) {
	seo.SetHead(r.Context(), seo.New("Acompanhe seu pedido").WithNoIndex())
	render.Page(w, r, h.d, trackPage())
}

// orderID parses {id}; false renders the 404.
func (h *handlers) orderID(w http.ResponseWriter, r *http.Request) (uuid.UUID, bool) {
	id, err := uuid.Parse(chi.URLParam(r, "id"))
	if err != nil {
		render.NotFound(w, r, h.d)
		return uuid.Nil, false
	}
	return id, true
}

// queryToken is the trimmed ?t= (nil when absent).
func queryToken(raw string) *string {
	if t := strings.TrimSpace(raw); t != "" {
		return &t
	}
	return nil
}

func deref(s *string) string {
	if s == nil {
		return ""
	}
	return *s
}

// orderView is the /pedido/{id} template input.
type orderView struct {
	Order       *store.OrderDetail
	Token       string
	Pending     bool
	MercadoPago bool
	PixCode     string
	QRImage     string
	TicketURL   string
	Expires     string
	WhatsappURL string
	Returns     store.OrderReturns
	Reviewable  []store.ReviewableProduct
}

// GET /pedido/{id}?t=…: guest token or owner session.
func (h *handlers) orderPage(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	id, ok := h.orderID(w, r)
	if !ok {
		return
	}
	token := queryToken(r.URL.Query().Get("t"))
	userID := sessionUserID(r)
	seo.SetHead(ctx, seo.New("Pedido").WithNoIndex())
	order, err := store.GetOrderForViewer(ctx, h.d.Pool, id, token, userID)
	if err != nil {
		h.fail(w, r, "load order", err)
		return
	}
	if order == nil {
		render.Status(w, r, h.d, http.StatusNotFound, orderNotFound(true))
		return
	}
	v := orderView{Order: order, Token: deref(token), Pending: order.Status == string(store.StatusPendingPayment)}
	// Mercado Pago orders show the PSP's QR / copy-paste code (created
	// lazily here if MP was down at checkout); static orders keep the
	// local BR Code from payment_meta.
	payment := payments.OrderPayment{}
	if v.Pending {
		if payment, err = h.paymentService().OrderPayment(ctx, order.ID); err != nil {
			h.fail(w, r, "order payment", err)
			return
		}
	}
	v.MercadoPago = payment.MercadoPago
	if c := payment.Charge; c != nil {
		v.PixCode = deref(c.QRCode)
		if b64 := deref(c.QRCodeBase64); b64 != "" {
			v.QRImage = "data:image/png;base64," + b64
		}
		v.TicketURL = deref(c.TicketURL)
		v.Expires = c.ExpiresLabel
	} else if !v.MercadoPago {
		v.PixCode = order.Meta.Pix()
	}
	if v.Pending && v.QRImage == "" && v.PixCode != "" {
		v.QRImage = qrsvg.DataURI(v.PixCode, 224)
	}
	settings, err := store.GetStoreSettings(ctx, h.d.Pool)
	if err != nil {
		h.fail(w, r, "store settings", err)
		return
	}
	v.WhatsappURL = store.OrderWhatsappURL(settings, order.ID, order.TotalCents)
	if v.Returns, err = store.LoadOrderReturns(ctx, h.d.Pool, order.ID); err != nil {
		h.fail(w, r, "order returns", err)
		return
	}
	if userID != nil {
		if v.Reviewable, err = store.ReviewableProductsForOrder(ctx, h.d.Pool, *userID, order.ID); err != nil {
			h.fail(w, r, "reviewable products", err)
			return
		}
	}
	render.Page(w, r, h.d, orderPage(v))
}

// GET|POST /pedido/status-proc?order_id=…&access_token=…: the current
// status of an order the caller may see ("unknown" otherwise). Plain
// requests get the status as text; htmx requests (the order page poller)
// get the poller fragment back while the order is still pending, and an
// HX-Refresh once it changed.
func (h *handlers) statusProc(w http.ResponseWriter, r *http.Request) {
	if err := r.ParseForm(); err != nil {
		if httpx.IsBodyTooLarge(err) {
			httpx.TooLarge(w)
			return
		}
		http.Error(w, "Formulário inválido.", http.StatusBadRequest)
		return
	}
	rawID, token := r.Form.Get("order_id"), r.Form.Get("access_token")
	status, err := store.RefreshOrderStatus(r.Context(), h.d.Pool, rawID, token, sessionUserID(r))
	if err != nil {
		h.fail(w, r, "refresh order status", err)
		return
	}
	w.Header().Set("Cache-Control", "no-store")
	if !httpx.IsHTMX(r) {
		w.Header().Set("Content-Type", "text/plain; charset=utf-8")
		_, _ = w.Write([]byte(status)) //nolint:gosec // G705: text/plain order status enum (or "unknown"), nosniff is global
		return
	}
	switch status {
	case string(store.StatusPendingPayment):
		id, _ := uuid.Parse(strings.TrimSpace(rawID))
		render.Fragment(w, r, http.StatusOK, statusPoller(id, strings.TrimSpace(token), true))
	case "unknown":
		// Stop polling (no access / unknown order).
		render.Fragment(w, r, http.StatusOK, statusPoller(uuid.Nil, "", false))
	default:
		w.Header().Set("HX-Refresh", "true")
		w.WriteHeader(http.StatusNoContent)
	}
}

// POST /pedido/{id}/cancelar (t=…): customer cancel of an unpaid order.
func (h *handlers) cancelOrder(w http.ResponseWriter, r *http.Request) {
	id, ok := h.orderID(w, r)
	if !ok {
		return
	}
	if err := r.ParseForm(); err != nil {
		if httpx.IsBodyTooLarge(err) {
			httpx.TooLarge(w)
			return
		}
		http.Error(w, "Formulário inválido.", http.StatusBadRequest)
		return
	}
	token := queryToken(r.PostForm.Get("t"))
	back := orderURL(id, deref(token))
	out, err := store.CancelUnpaidOrder(r.Context(), h.d.Pool, id, token, sessionUserID(r))
	if err != nil {
		var illegal *store.IllegalTransitionError
		msg, isUser := store.UserMessage(err)
		switch {
		case isUser:
		case errors.As(err, &illegal):
			msg = illegal.Error()
		default:
			h.fail(w, r, "cancel order", err)
			return
		}
		toast.Set(w, r, toast.Error(msg))
		toast.Redirect(w, r, back)
		return
	}
	if msg, okMsg := out.Message(); okMsg {
		toast.Set(w, r, toast.Success(msg))
	} else {
		toast.Set(w, r, toast.Error(msg))
	}
	toast.Redirect(w, r, back)
}
