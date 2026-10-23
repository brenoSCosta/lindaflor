// Package admin: every /admin/** page, fragment and CSV export (wrap the group in auth.RequireAdmin).
//
// Owner: wave2-F. Only this package's files change when the feature lands;
// router.go already calls Mount.
package admin

import (
	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/app"
)

// Mount registers the package's routes.
func Mount(r chi.Router, d *app.Deps) {
	_ = r
	_ = d
	// TODO(wave2-F): routes.
}
