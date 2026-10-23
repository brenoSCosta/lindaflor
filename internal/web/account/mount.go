// Package account: /dashboard, /conta/pedidos, /conta/enderecos, /settings (profile, account, sessions, security/2FA, linked accounts, avatar upload).
//
// Owner: wave2-E. Only this package's files change when the feature lands;
// router.go already calls Mount.
package account

import (
	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/app"
)

// Mount registers the package's routes.
func Mount(r chi.Router, d *app.Deps) {
	_ = r
	_ = d
	// TODO(wave2-E): routes.
}
