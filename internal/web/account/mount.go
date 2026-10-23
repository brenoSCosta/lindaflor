// Package account: /dashboard, /conta/pedidos (+ /conta/pedidos/lista
// fragment), /conta/enderecos (+ salvar/remover/padrao), /settings
// (profile, account, sessions, security/2FA, linked accounts, danger
// zone; single POST /settings action dispatcher), /settings/avatar
// (multipart, 2 MB, cropper dialog), /settings/two-factor (one-time QR +
// backup codes) and the htmx session-revoke endpoints. Port of
// src/app/{dashboard,settings}.rs and src/app/conta/*.rs.
//
// Every route requires a session (auth.RequireUser → /login). Forms work
// without JS (POST → 303 + toast cookie); the settings forms are
// htmx-enhanced (hx-post + loading toast, HX-Location back to the tab).
package account

import (
	"log/slog"
	"net/http"

	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/httpx"
)

// AvatarBodyLimit caps POST /settings/avatar (src/app.rs: 2 MiB body;
// the image itself is capped at auth.MaxImageBytes).
const AvatarBodyLimit = 2 << 20

type handlers struct {
	d   *app.Deps
	svc *auth.Service
}

// Mount registers the package's routes.
func Mount(r chi.Router, d *app.Deps) {
	h := &handlers{d: d, svc: auth.NewService(d.Cfg, d.Pool, d.Valkey, d.Storage)}
	r.Group(func(r chi.Router) {
		r.Use(auth.RequireUser)

		r.Get("/dashboard", h.dashboard)

		r.Get("/conta/pedidos", h.myOrders)
		r.Get("/conta/pedidos/lista", h.myOrdersList)

		r.Get("/conta/enderecos", h.addresses)
		r.Post("/conta/enderecos/salvar", h.saveAddress)
		r.Post("/conta/enderecos/remover", h.removeAddress)
		r.Post("/conta/enderecos/padrao", h.defaultAddress)

		r.Get("/settings", h.settingsPage)
		r.Post("/settings", h.settingsPost)
		r.Get("/settings/two-factor", h.twoFactorSetup)
		r.Post("/settings/two-factor", h.twoFactorSetupEnable)
		r.With(httpx.BodyLimit(AvatarBodyLimit)).Post("/settings/avatar", h.uploadAvatar)
		r.Post("/settings/revoke-session", h.revokeSessionProc)
		r.Post("/settings/revoke-other-sessions", h.revokeOtherSessionsProc)
	})
}

// current is the signed-in user and session (RequireUser guarantees both).
func current(r *http.Request) (*auth.User, *auth.Session) {
	u, _ := auth.CurrentUser(r.Context())
	s, _ := auth.CurrentSession(r.Context())
	return u, s
}

func (h *handlers) fail(w http.ResponseWriter, r *http.Request, what string, err error) {
	slog.ErrorContext(r.Context(), what, "error", err)
	http.Error(w, "Erro interno.", http.StatusInternalServerError)
}

// parseForm parses an urlencoded body (413 on oversize, 400 otherwise).
func parseForm(w http.ResponseWriter, r *http.Request) bool {
	if err := r.ParseForm(); err != nil {
		if httpx.IsBodyTooLarge(err) {
			httpx.TooLarge(w)
			return false
		}
		http.Error(w, "Formulário inválido.", http.StatusBadRequest)
		return false
	}
	return true
}
