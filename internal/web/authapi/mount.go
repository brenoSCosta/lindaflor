// Package authapi: the better-auth style JSON API under /api/auth/* (38 endpoints) and Google OAuth callbacks.
//
// Owner: wave1-A. Only this package's files change when the feature lands;
// router.go already calls Mount.
package authapi

import (
	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/app"
)

// Mount registers the package's routes.
func Mount(r chi.Router, d *app.Deps) {
	_ = r
	_ = d
	// TODO(wave1-A): routes.
}
