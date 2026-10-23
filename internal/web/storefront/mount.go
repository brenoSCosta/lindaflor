// Package storefront: home, catalog (/produtos + busca shard + filters),
// product and set pages, cart sheet + cart procedures, favoritos, reviews,
// avise-me, newsletter, static pages (termos, politica-privacidade,
// trocas-devolucoes, guia-de-medidas) and the shell data loader.
//
// Owner: wave2-D (UI) on top of wave1-C (store domain). Only this
// package's files change when the feature lands; router.go already calls
// Mount and installs ShellLoader.
package storefront

import (
	"net/http"

	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/web/components"
	"github.com/wellington-vell/lindaflor/internal/web/layout"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// Mount registers the storefront routes.
func Mount(r chi.Router, d *app.Deps) {
	// TODO(wave2-D): real home page (hero, sections, sobre.js) and the rest.
	r.Get("/", func(w http.ResponseWriter, r *http.Request) {
		render.Page(w, r, d, placeholderHome())
	})
}

// ShellLoader fills layout.ShellData for every full page render.
// TODO(wave2-D/wave1-C): store_settings (storefront_settings), first six
// active sets, cart item count (cart cookie), avatar presign, sidebar_state
// cookie, cart sheet component.
func ShellLoader(d *app.Deps) app.ShellLoader {
	_ = d
	return func(r *http.Request) (layout.ShellData, error) {
		s := layout.ShellData{Settings: layout.DefaultStoreSettings(), SidebarExpanded: true}
		if u, ok := auth.CurrentUser(r.Context()); ok {
			s.User = u
		}
		if c, err := r.Cookie(components.SidebarCookie); err == nil && c.Value == "collapsed" {
			s.SidebarExpanded = false
		}
		return s, nil
	}
}
