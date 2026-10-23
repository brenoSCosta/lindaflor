// Package authpages: HTML auth pages: /login, /signup (login?mode=signup), /two-factor, /forgot-password(/sent), /reset-password, /verify-email(/resend), /check-email(/resend), /confirmar-email, /excluir-conta, POST /logout, POST /stop-impersonating.
//
// Owner: wave1-A. Only this package's files change when the feature lands;
// router.go already calls Mount.
package authpages

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
