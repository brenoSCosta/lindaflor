// Package admin: every /admin/** page, fragment and CSV export (port of
// src/app/admin.rs and src/app/admin/**).
//
// The whole group runs auth.RequireAdmin (anonymous → /login, signed-in
// non-admins → /dashboard) and every handler re-checks the admin role.
// Topcoat shards are htmx GET fragment endpoints at the same paths
// (…/lista, /admin/cupons/usuarios); toast-promise forms post with htmx
// (X-Toast + HX-Location) and fall back to POST → 303 + toast cookie.
//
// Owner: wave2-F.
package admin

import (
	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/payments"
)

// Mount registers the package's routes.
func Mount(r chi.Router, d *app.Deps) {
	h := &handler{
		d:    d,
		auth: auth.NewService(d.Cfg, d.Pool, d.Valkey, d.Storage),
		pay:  &payments.Service{Pool: d.Pool, Provider: d.Payments, Orders: d.Orders},
	}
	r.Route("/admin", func(r chi.Router) {
		r.Use(auth.RequireAdmin)

		r.Get("/", h.dashboard)

		r.Get("/usuarios", h.usersPage)
		r.Get("/usuarios/lista", h.usersList)
		r.Post("/usuarios/papel", h.userSetRole)
		r.Post("/usuarios/nome", h.userRename)
		r.Post("/usuarios/banir", h.userBan)
		r.Post("/usuarios/desbanir", h.userUnban)
		r.Post("/usuarios/atuar", h.userImpersonate)
		r.Post("/usuarios/sessoes/revogar", h.userRevokeSession)
		r.Post("/usuarios/sessoes/revogar-todas", h.userRevokeSessions)
		r.Post("/usuarios/remover", h.userRemove)

		r.Get("/produtos", h.productsPage)
		r.Get("/produtos/lista", h.productsList)
		r.Get("/produtos/novo", h.productNewPage)
		r.With(httpx.BodyLimit(productBodyLimit)).Post("/produtos/create", h.productCreate)
		r.With(httpx.BodyLimit(productBodyLimit)).Post("/produtos/update", h.productUpdate)
		r.Get("/produtos/{id}", h.productEditPage)

		r.Get("/sets", h.setsPage)
		r.With(httpx.BodyLimit(setBodyLimit)).Post("/sets/create", h.setCreate)
		r.Post("/sets/toggle", h.setToggle)
		r.Post("/sets/delete", h.setDelete)
		r.Get("/sets/{id}", h.setEditPage)
		r.With(httpx.BodyLimit(setBodyLimit)).Post("/sets/{id}/update", h.setUpdate)

		r.Get("/pedidos", h.ordersPage)
		r.Get("/pedidos/lista", h.ordersList)
		r.Get("/pedidos/{id}", h.orderPage)
		r.Post("/pedidos/{id}", h.orderUpdate)

		r.Get("/trocas", h.returnsPage)
		r.Get("/trocas/{id}", h.returnPage)
		r.Post("/trocas/{id}", h.returnUpdate)

		r.Get("/estoque", h.stockPage)
		r.Post("/estoque", h.stockAdjust)
		r.Get("/estoque/lista", h.stockList)

		r.Get("/configuracoes", h.settingsPage)
		r.Post("/configuracoes", h.settingsSave)

		r.Get("/cupons", h.couponsPage)
		r.Get("/cupons/lista", h.couponsList)
		r.Get("/cupons/usuarios", h.couponUsersList)
		r.Post("/cupons/create", h.couponCreate)
		r.Post("/cupons/toggle", h.couponToggle)
		r.Post("/cupons/toggle-proc", h.couponToggleProc)
		r.Post("/cupons/delete", h.couponDelete)
		r.Post("/cupons/assign", h.couponAssign)
		r.Post("/cupons/unassign", h.couponUnassign)

		r.Get("/avaliacoes", h.reviewsPage)
		r.Post("/avaliacoes", h.reviewsModerate)

		r.Get("/banners", h.bannersPage)
		r.With(httpx.BodyLimit(bannerBodyLimit)).Post("/banners/create", h.bannerCreate)
		r.Post("/banners/toggle", h.bannerToggle)
		r.Post("/banners/delete", h.bannerDelete)

		r.Get("/newsletter", h.newsletterPage)
		r.Get("/newsletter/export-csv", h.newsletterCSV)

		r.Get("/relatorios", h.reportsPage)
		r.Get("/relatorios/pedidos.csv", h.reportsCSV)
	})
}
