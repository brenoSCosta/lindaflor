// Package authapi: the better-auth style JSON API under /api/auth/* (38
// endpoints) and the Google OAuth callback. Port of src/auth/routes/**:
// same paths, methods, JSON shapes and status codes. Errors are
// {"code","message"} with the HTTP status of the failure.
package authapi

import (
	"net/http"

	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
)

type api struct {
	d   *app.Deps
	svc *auth.Service
}

// Mount registers the /api/auth/* routes.
func Mount(r chi.Router, d *app.Deps) {
	a := &api{d: d, svc: auth.NewService(d.Cfg, d.Pool, d.Valkey, d.Storage)}
	lim := ratelimit.Limit

	r.Route("/api/auth", func(r chi.Router) {
		signIn := lim(ratelimit.ScopeSignIn, ratelimit.DefaultSignInPerMin)
		r.With(lim(ratelimit.ScopeSignUp, ratelimit.DefaultSignUpPerMin)).Post("/sign-up/email", a.signUpEmail)
		r.With(signIn).Post("/sign-in/email", a.signInEmail)
		r.With(signIn).Post("/sign-in/social", a.signInSocial)
		r.Get("/get-session", a.getSession)
		r.Post("/sign-out", a.signOut)

		reset := lim(ratelimit.ScopePasswordReset, ratelimit.DefaultPasswordResetPerMin)
		r.With(reset).Post("/request-password-reset", a.requestPasswordReset)
		r.With(reset).Post("/reset-password", a.resetPassword)

		verify := lim(ratelimit.ScopeVerifyEmail, ratelimit.DefaultVerifyEmailPerMin)
		r.With(verify).Post("/send-verification-email", a.sendVerificationEmail)
		r.With(verify).Get("/verify-email", a.verifyEmailGet)
		r.With(verify).Post("/verify-email", a.verifyEmailPost)

		r.Post("/change-email", a.changeEmail)
		r.Post("/change-email/confirm", a.confirmChangeEmail)
		r.Post("/change-password", a.changePassword)
		r.Post("/delete-user", a.deleteUser)
		r.Post("/update-user", a.updateUser)

		r.Get("/list-sessions", a.listSessions)
		r.Post("/revoke-session", a.revokeSession)
		r.Post("/revoke-other-sessions", a.revokeOtherSessions)

		r.Get("/list-accounts", a.listAccounts)
		r.Post("/link-social", a.linkSocial)
		r.Post("/unlink-account", a.unlinkAccount)

		r.Route("/admin", func(r chi.Router) {
			r.Post("/impersonate-user", a.impersonateUser)
			r.Post("/stop-impersonating", a.stopImpersonating)
			r.Get("/list-users", a.listUsers)
			r.Post("/set-role", a.setRole)
			r.Post("/update-user", a.adminUpdateUser)
			r.Post("/ban-user", a.banUser)
			r.Post("/unban-user", a.unbanUser)
			r.Post("/remove-user", a.removeUser)
			r.Post("/list-user-sessions", a.listUserSessions)
			r.Post("/revoke-user-session", a.revokeUserSession)
			r.Post("/revoke-user-sessions", a.revokeUserSessions)
		})

		r.Route("/two-factor", func(r chi.Router) {
			r.Post("/enable", a.twoFactorEnable)
			r.With(lim(ratelimit.ScopeTOTP, ratelimit.DefaultTOTPPerMin)).Post("/verify-totp", a.twoFactorVerifyTOTP)
			r.Post("/disable", a.twoFactorDisable)
			r.With(lim(ratelimit.ScopeBackupCode, ratelimit.DefaultBackupCodePerMin)).Post("/verify-backup-code", a.twoFactorVerifyBackupCode)
			r.Post("/generate-backup-codes", a.twoFactorGenerateBackupCodes)
		})

		r.Get("/callback/google", a.googleCallback)
	})
}

// requireSession returns the signed-in user/session or answers 401.
func requireSession(w http.ResponseWriter, r *http.Request) (*auth.User, *auth.Session, bool) {
	u, ok := auth.CurrentUser(r.Context())
	s, sok := auth.CurrentSession(r.Context())
	if !ok || !sok {
		writeError(w, r, auth.ErrUnauthorized)
		return nil, nil, false
	}
	return u, s, true
}
