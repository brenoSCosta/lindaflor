package authpages

import (
	"log/slog"
	"net/http"
	"net/url"
	"strings"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/captcha"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

const (
	dashboardPath = "/dashboard"
	msgTooMany    = "Muitas tentativas. Aguarde um momento e tente novamente."
)

func isTooLarge(err error) bool      { return httpx.IsBodyTooLarge(err) }
func tooLarge(w http.ResponseWriter) { httpx.TooLarge(w) }

func head(r *http.Request, title string) {
	seo.SetHead(r.Context(), seo.New(title).WithNoIndex())
}

// safeNext: an invite goes to /accept-invitation, else a local `next`, else
// the dashboard.
func safeNext(inviteID, next string) string {
	if inviteID != "" {
		return "/accept-invitation?" + url.Values{"id": {inviteID}}.Encode()
	}
	return httpx.SafeLocalRedirectOr(next, dashboardPath)
}

// loginURL rebuilds /login with the form context so a failed attempt keeps
// what the user typed (never the password).
func loginURL(mode, name, email, inviteID, next string) string {
	v := url.Values{}
	v.Set("mode", mode)
	if name != "" {
		v.Set("name", name)
	}
	if email != "" {
		v.Set("email", email)
	}
	if inviteID != "" {
		v.Set("invite_id", inviteID)
	}
	if next != "" {
		v.Set("next", next)
	}
	return "/login?" + v.Encode()
}

func errorRedirect(w http.ResponseWriter, r *http.Request, msg, location string) {
	toast.Set(w, r, toast.Error(msg))
	toast.Redirect(w, r, location)
}

// logInternal logs unexpected failures behind a generic pt-BR toast.
func logInternal(r *http.Request, err error) {
	if err != nil && !auth.IsClientError(err) {
		slog.ErrorContext(r.Context(), "auth page error", "error", err, "path", r.URL.Path)
	}
}

// ---------------------------------------------------------------------------
// /login, /signup, /login/google
// ---------------------------------------------------------------------------

func (p *pages) loginPage(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	d := loginData{
		Signup:       q.Get("mode") == "signup",
		DefaultName:  q.Get("name"),
		DefaultEmail: q.Get("email"),
		InviteID:     q.Get("invite_id"),
		Next:         q.Get("next"),
	}
	d.CallbackURL = safeNext(d.InviteID, d.Next)
	d.SiteKey = p.d.Captcha.SiteKey()
	if d.Signup {
		head(r, "Criar conta")
	} else {
		head(r, "Entrar")
	}
	render.Page(w, r, p.d, loginPage(d))
}

func (p *pages) signinPost(w http.ResponseWriter, r *http.Request) {
	get, ok := formValue(w, r)
	if !ok {
		return
	}
	email, password := get("email"), get("password")
	inviteID, next := get("invite_id"), get("next")
	target := safeNext(inviteID, next)
	back := loginURL("signin", "", email, inviteID, next)
	if !auth.RateAllow(r, ratelimit.ScopeSignIn, ratelimit.DefaultSignInPerMin) {
		errorRedirect(w, r, msgTooMany, back)
		return
	}
	res, err := p.svc.SignInEmail(w, r, email, password, false, true)
	if err != nil {
		logInternal(r, err)
		errorRedirect(w, r, auth.PortugueseMessage(err), back)
		return
	}
	if res.PendingToken != "" {
		p.svc.SetPending2FACookie(w, res.PendingToken)
		toast.Redirect(w, r, "/two-factor")
		return
	}
	toast.Set(w, r, toast.Success("Bem-vinda de volta."))
	toast.Redirect(w, r, target)
}

func (p *pages) signupPost(w http.ResponseWriter, r *http.Request) {
	get, ok := formValue(w, r)
	if !ok {
		return
	}
	name, email, password := get("name"), get("email"), get("password")
	inviteID, next := get("invite_id"), get("next")
	target := safeNext(inviteID, next)
	back := loginURL("signup", name, email, inviteID, next)
	if !auth.RateAllow(r, ratelimit.ScopeSignUp, ratelimit.DefaultSignUpPerMin) {
		errorRedirect(w, r, msgTooMany, back)
		return
	}
	if !p.d.Captcha.CheckForm(r) {
		errorRedirect(w, r, captcha.Message, back)
		return
	}
	if _, _, err := p.svc.SignUpEmail(w, r, name, email, password, true); err != nil {
		logInternal(r, err)
		errorRedirect(w, r, auth.PortugueseMessage(err), back)
		return
	}
	toast.Set(w, r, toast.Success("Conta criada."))
	toast.Redirect(w, r, target)
}

func (p *pages) loginGoogle(w http.ResponseWriter, r *http.Request) {
	get, ok := formValue(w, r)
	if !ok {
		return
	}
	u, err := p.svc.StartGoogleOAuth(w, r, get("callback_url"), nil, "")
	if err != nil {
		logInternal(r, err)
		errorRedirect(w, r, auth.PortugueseMessage(err), loginURL("signin", "", "", "", ""))
		return
	}
	// External redirect: always a plain 303 (never HX-Location).
	http.Redirect(w, r, u, http.StatusSeeOther) //nolint:gosec // G710: server-built Google authorize URL
}

// ---------------------------------------------------------------------------
// /two-factor
// ---------------------------------------------------------------------------

func (p *pages) twoFactorPage(w http.ResponseWriter, r *http.Request) {
	head(r, "Verificação em duas etapas")
	if p.svc.ReadPending2FACookie(r) == "" {
		render.Page(w, r, p.d, twoFactorExpired())
		return
	}
	render.Page(w, r, p.d, twoFactorPage(r.URL.Query().Get("method") == "backup"))
}

func (p *pages) twoFactorPost(w http.ResponseWriter, r *http.Request) {
	get, ok := formValue(w, r)
	if !ok {
		return
	}
	token := p.svc.ReadPending2FACookie(r)
	if token == "" {
		toast.Redirect(w, r, auth.LoginPath)
		return
	}
	backup := r.URL.Query().Get("method") == "backup"
	code := get("code")
	if backup {
		code = get("backup_code")
	}
	if _, _, err := p.svc.Complete2FA(w, r, token, code, backup, true); err != nil {
		logInternal(r, err)
		back := "/two-factor"
		if backup {
			back += "?method=backup"
		}
		errorRedirect(w, r, auth.PortugueseMessage(err), back)
		return
	}
	p.svc.ClearPending2FACookie(w)
	toast.Set(w, r, toast.Success("Verificado."))
	toast.Redirect(w, r, dashboardPath)
}

// ---------------------------------------------------------------------------
// /forgot-password, /reset-password
// ---------------------------------------------------------------------------

func (p *pages) forgotPage(w http.ResponseWriter, r *http.Request) {
	head(r, "Esqueceu a senha")
	render.Page(w, r, p.d, forgotPage(p.d.Captcha.SiteKey()))
}

func (p *pages) forgotSent(w http.ResponseWriter, r *http.Request) {
	head(r, "Link enviado")
	render.Page(w, r, p.d, forgotSentPage())
}

func (p *pages) forgotPost(w http.ResponseWriter, r *http.Request) {
	get, ok := formValue(w, r)
	if !ok {
		return
	}
	if !p.d.Captcha.CheckForm(r) {
		errorRedirect(w, r, captcha.Message, "/forgot-password")
		return
	}
	// Silent either way (anti-enumeration).
	if err := p.svc.RequestPasswordReset(r.Context(), get("email"), "/reset-password"); err != nil {
		logInternal(r, err)
	}
	httpx.Redirect(w, r, "/forgot-password/sent")
}

func (p *pages) resetPage(w http.ResponseWriter, r *http.Request) {
	head(r, "Redefinir senha")
	render.Page(w, r, p.d, resetPage(r.URL.Query().Get("token")))
}

func (p *pages) resetPost(w http.ResponseWriter, r *http.Request) {
	get, ok := formValue(w, r)
	if !ok {
		return
	}
	token := strings.TrimSpace(get("token"))
	if token == "" {
		toast.Redirect(w, r, "/reset-password")
		return
	}
	back := "/reset-password?" + url.Values{"token": {token}}.Encode()
	if get("new_password") != get("confirm_password") {
		errorRedirect(w, r, "As senhas não conferem.", back)
		return
	}
	if err := p.svc.ResetPassword(r.Context(), token, get("new_password")); err != nil {
		logInternal(r, err)
		errorRedirect(w, r, auth.PortugueseMessage(err), back)
		return
	}
	toast.Set(w, r, toast.Success("Senha redefinida."))
	toast.Redirect(w, r, "/login?mode=signin")
}

// ---------------------------------------------------------------------------
// /verify-email, /check-email
// ---------------------------------------------------------------------------

func (p *pages) verifyEmailPage(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	state := verifyPrompt
	if q.Has("error") {
		state = verifyFailed
	}
	if token := q.Get("token"); token != "" {
		if err := p.svc.ConsumeEmailVerification(r.Context(), token); err != nil {
			logInternal(r, err)
			state = verifyFailed
		} else {
			state = verifyDone
		}
	}
	if q.Get("error") == "resent" {
		state = verifyResent
	}
	email := ""
	if u, ok := auth.CurrentUser(r.Context()); ok {
		email = u.Email
	}
	head(r, "Verificar e-mail")
	render.Page(w, r, p.d, verifyEmailPage(state, email))
}

func (p *pages) verifyEmailResend(w http.ResponseWriter, r *http.Request) {
	get, ok := formValue(w, r)
	if !ok {
		return
	}
	email := get("email")
	if strings.TrimSpace(email) == "" {
		u, signed := auth.CurrentUser(r.Context())
		if !signed {
			httpx.Redirect(w, r, "/verify-email?error=1")
			return
		}
		email = u.Email
	}
	if err := p.svc.SendVerificationEmail(r.Context(), email, "/verify-email"); err != nil {
		logInternal(r, err)
	}
	httpx.Redirect(w, r, "/verify-email?error=resent")
}

func (p *pages) checkEmailPage(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	head(r, "Verifique seu e-mail")
	render.Page(w, r, p.d, checkEmailPage(q.Get("email"), q.Has("resent")))
}

func (p *pages) checkEmailResend(w http.ResponseWriter, r *http.Request) {
	get, ok := formValue(w, r)
	if !ok {
		return
	}
	email := strings.TrimSpace(get("email"))
	if email == "" {
		email = strings.TrimSpace(r.URL.Query().Get("email"))
	}
	if email == "" {
		httpx.Redirect(w, r, "/check-email")
		return
	}
	if err := p.svc.SendVerificationEmail(r.Context(), email, "/verify-email"); err != nil {
		logInternal(r, err)
	}
	httpx.Redirect(w, r, "/check-email?"+url.Values{"email": {email}, "resent": {"1"}}.Encode())
}

// ---------------------------------------------------------------------------
// /confirmar-email, /excluir-conta
// ---------------------------------------------------------------------------

func (p *pages) confirmEmailPage(w http.ResponseWriter, r *http.Request) {
	head(r, "Confirmar novo e-mail")
	render.Page(w, r, p.d, confirmEmailPage(r.URL.Query().Get("token")))
}

func (p *pages) confirmEmailPost(w http.ResponseWriter, r *http.Request) {
	get, ok := formValue(w, r)
	if !ok {
		return
	}
	token := strings.TrimSpace(get("token"))
	if token == "" {
		toast.Redirect(w, r, "/confirmar-email")
		return
	}
	if err := p.svc.CompleteChangeEmail(r.Context(), token); err != nil {
		logInternal(r, err)
		errorRedirect(w, r, auth.PortugueseMessage(err), "/confirmar-email")
		return
	}
	toast.Set(w, r, toast.Success("E-mail alterado. Entre novamente."))
	toast.Redirect(w, r, "/login?mode=signin")
}

func (p *pages) deleteAccountPage(w http.ResponseWriter, r *http.Request) {
	head(r, "Excluir conta")
	render.Page(w, r, p.d, deleteAccountPage(r.URL.Query().Get("token")))
}

func (p *pages) deleteAccountPost(w http.ResponseWriter, r *http.Request) {
	get, ok := formValue(w, r)
	if !ok {
		return
	}
	token := strings.TrimSpace(get("token"))
	if token == "" {
		toast.Redirect(w, r, "/excluir-conta")
		return
	}
	if err := p.svc.CompleteDelete(w, r, token); err != nil {
		logInternal(r, err)
		errorRedirect(w, r, auth.PortugueseMessage(err), "/excluir-conta")
		return
	}
	toast.Set(w, r, toast.Success("Conta excluída."))
	toast.Redirect(w, r, "/")
}

// ---------------------------------------------------------------------------
// POST /logout, POST /stop-impersonating
// ---------------------------------------------------------------------------

// logout stops the session, expires the cart cookie and goes to a safe
// local `redirect` (default /).
func (p *pages) logout(w http.ResponseWriter, r *http.Request) {
	_ = r.ParseForm()
	if err := p.svc.SignOut(w, r); err != nil {
		logInternal(r, err)
	}
	httpx.Redirect(w, r, httpx.SafeLocalRedirectOr(r.PostFormValue("redirect"), "/"))
}

// stopImpersonating returns an impersonating admin to their own session
// and goes to a safe local `redirect` (default /dashboard).
func (p *pages) stopImpersonating(w http.ResponseWriter, r *http.Request) {
	_ = r.ParseForm()
	u, uok := auth.CurrentUser(r.Context())
	s, sok := auth.CurrentSession(r.Context())
	if uok && sok && s.ImpersonatedBy != nil {
		if _, _, err := p.svc.StopImpersonating(w, r, auth.Actor{User: u, Session: s}); err != nil {
			logInternal(r, err)
		}
	}
	httpx.Redirect(w, r, httpx.SafeLocalRedirectOr(r.PostFormValue("redirect"), dashboardPath))
}
