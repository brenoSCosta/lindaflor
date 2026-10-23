// Package checkout: checkout (/checkout, PIX), order pages (/pedido,
// /pedido/{id} with the guest token ?t=, status poll /pedido/status-proc,
// cancel, troca). Port of src/app/checkout.rs, src/app/pedido.rs,
// src/app/pedido/id.rs, src/app/pedido/id/troca.rs and
// src/app/store/order_timeline.rs (shared pieces in ./ui).
//
// Every form works without JS (POST → 303 + toast cookie). The order
// page polls its status with htmx (hx-trigger "every 10s") while the
// order waits for payment and reloads once it changes.
package checkout

import (
	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/payments"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/store"
)

type handlers struct {
	d *app.Deps
}

func (h *handlers) secure() bool { return h.d.Cfg != nil && h.d.Cfg.SecureCookies }

func (h *handlers) checkoutDeps() store.CheckoutDeps {
	return store.NewCheckoutDeps(h.d.Pool, h.d.Payments, h.d.Orders)
}

func (h *handlers) paymentService() *payments.Service {
	return &payments.Service{Pool: h.d.Pool, Provider: h.d.Payments, Orders: h.d.Orders}
}

// Mount registers the package's routes.
func Mount(r chi.Router, d *app.Deps) {
	h := &handlers{d: d}

	r.Get("/checkout", h.checkoutPage)
	r.Post("/checkout", h.checkoutPage)

	// /pedido and below own their budgets (ratelimit.GlobalOverridePrefixes):
	// the page budget skips the status poll, which has a tighter one.
	r.Route("/pedido", func(r chi.Router) {
		r.Use(ratelimit.LimitExcept(ratelimit.ScopePedidoPage, ratelimit.DefaultPedidoPagePerMin, "/pedido/status-proc"))
		r.Get("/", h.trackPage)
		poll := r.With(ratelimit.Limit(ratelimit.ScopePedidoPoll, ratelimit.DefaultPedidoPollPerMin))
		poll.Get("/status-proc", h.statusProc)
		poll.Post("/status-proc", h.statusProc)
		r.Get("/{id}", h.orderPage)
		r.Post("/{id}/cancelar", h.cancelOrder)
		r.Get("/{id}/troca", h.returnPage)
		r.Post("/{id}/troca", h.submitReturn)
	})
}
