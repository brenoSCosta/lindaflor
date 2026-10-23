// Package authpages: the HTML auth pages — /login (signin + ?mode=signup),
// POST /login, /signup, /login/google, /two-factor, /forgot-password
// (+/sent), /reset-password, /verify-email (+/resend), /check-email
// (+/resend), /confirmar-email, /excluir-conta, POST /logout and POST
// /stop-impersonating. Port of src/app/{login,two_factor,forgot_password,
// reset_password,verify_email,check_email,confirmar_email,excluir_conta}.rs.
//
// Every form works without JS (POST → 303 + toast cookie); the login,
// signup, two-factor and reset forms are htmx-enhanced (hx-post, toast on
// X-Toast, HX-Location redirect) with Alpine client-side hints.
package authpages

import (
	"net/http"

	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
)

type pages struct {
	d   *app.Deps
	svc *auth.Service
}

// Mount registers the auth pages.
func Mount(r chi.Router, d *app.Deps) {
	p := &pages{d: d, svc: auth.NewService(d.Cfg, d.Pool, d.Valkey, d.Storage)}
	mut := ratelimit.MutationsOnly

	r.Get("/login", p.loginPage)
	r.Post("/login", p.signinPost)
	r.Post("/signup", p.signupPost)
	r.With(mut(ratelimit.ScopeSignIn, ratelimit.DefaultSignInPerMin)).Post("/login/google", p.loginGoogle)

	twoFactor := mut(ratelimit.ScopeTOTP, ratelimit.DefaultTOTPPerMin)
	r.Get("/two-factor", p.twoFactorPage)
	r.With(twoFactor).Post("/two-factor", p.twoFactorPost)

	reset := mut(ratelimit.ScopePasswordReset, ratelimit.DefaultPasswordResetPerMin)
	r.Get("/forgot-password", p.forgotPage)
	r.With(reset).Post("/forgot-password", p.forgotPost)
	r.Get("/forgot-password/sent", p.forgotSent)
	r.Get("/reset-password", p.resetPage)
	r.With(reset).Post("/reset-password", p.resetPost)

	verify := mut(ratelimit.ScopeVerifyEmail, ratelimit.DefaultVerifyEmailPerMin)
	r.Get("/verify-email", p.verifyEmailPage)
	r.With(verify).Post("/verify-email/resend", p.verifyEmailResend)
	r.Get("/check-email", p.checkEmailPage)
	r.With(verify).Post("/check-email/resend", p.checkEmailResend)

	r.Get("/confirmar-email", p.confirmEmailPage)
	r.Post("/confirmar-email", p.confirmEmailPost)
	r.Get("/excluir-conta", p.deleteAccountPage)
	r.Post("/excluir-conta", p.deleteAccountPost)

	r.Post("/logout", p.logout)
	r.Post("/stop-impersonating", p.stopImpersonating)
}

// formValue parses the form (413 on oversize) and returns a getter.
func formValue(w http.ResponseWriter, r *http.Request) (func(string) string, bool) {
	if err := r.ParseForm(); err != nil {
		if isTooLarge(err) {
			tooLarge(w)
			return nil, false
		}
		http.Error(w, "Requisição inválida.", http.StatusBadRequest)
		return nil, false
	}
	return r.PostFormValue, true
}
