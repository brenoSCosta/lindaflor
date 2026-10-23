package authapi

import (
	"net/http"
	"strings"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/httpx"
)

func trim(s string) string { return strings.TrimSpace(s) }

func str(p *string) string {
	if p == nil {
		return ""
	}
	return *p
}

// POST /api/auth/sign-up/email {name, email, password, captchaToken?} →
// {user, session}; 400 CAPTCHA_FAILED when Turnstile rejects the token.
func (a *api) signUpEmail(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Name         *string `json:"name"`
		Email        *string `json:"email"`
		Password     *string `json:"password"`
		CaptchaToken string  `json:"captchaToken"`
	}
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	if body.Name == nil || body.Email == nil || body.Password == nil {
		writeError(w, r, auth.BadRequest("name, email and password are required"))
		return
	}
	if !a.captchaOK(w, r, body.CaptchaToken) {
		return
	}
	u, s, err := a.svc.SignUpEmail(w, r, *body.Name, *body.Email, *body.Password, false)
	if err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, toPayload(u, s))
}

// twoFactorRequired is the 403 body of a sign-in that needs a second factor.
type twoFactorRequired struct {
	Code              string `json:"code"`
	Message           string `json:"message"`
	TwoFactorRedirect bool   `json:"twoFactorRedirect"`
	Token             string `json:"token"`
}

// POST /api/auth/sign-in/email {email, password} → {user, session} or 403
// {code: TWO_FACTOR_REQUIRED, token}.
func (a *api) signInEmail(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Email    *string `json:"email"`
		Password *string `json:"password"`
	}
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	res, err := a.svc.SignInEmail(w, r, str(body.Email), str(body.Password), true, false)
	if err != nil {
		writeError(w, r, err)
		return
	}
	if res.PendingToken != "" {
		writeJSON(w, http.StatusForbidden, twoFactorRequired{
			Code: "TWO_FACTOR_REQUIRED", Message: "Two-factor authentication is required",
			TwoFactorRedirect: true, Token: res.PendingToken,
		})
		return
	}
	writeJSON(w, http.StatusOK, toPayload(res.User, res.Session))
}

func (a *api) socialUnavailable(w http.ResponseWriter, provider string) bool {
	if provider != auth.GoogleProviderID {
		writeJSON(w, http.StatusBadRequest, socialUnavailable{
			Code: "SOCIAL_SIGN_IN_UNAVAILABLE", Message: "social provider '" + provider + "' is not supported",
		})
		return true
	}
	if _, ok := a.svc.GoogleConfig(); !ok {
		writeJSON(w, http.StatusNotImplemented, socialUnavailable{
			Code: "SOCIAL_SIGN_IN_UNAVAILABLE", Message: a.svc.GoogleNotConfiguredMessage(),
		})
		return true
	}
	return false
}

// POST /api/auth/sign-in/social {provider, callbackURL?} → {url}.
func (a *api) signInSocial(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Provider string `json:"provider"`
		// Matches callbackURL / callbackUrl (encoding/json is case-insensitive).
		CallbackURL string `json:"callbackUrl"`
	}
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	if a.socialUnavailable(w, strings.ToLower(trim(body.Provider))) {
		return
	}
	url, err := a.svc.StartGoogleOAuth(w, r, body.CallbackURL, nil, "")
	if err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]string{"url": url})
}

// GET /api/auth/get-session → {user, session} or null.
func (a *api) getSession(w http.ResponseWriter, r *http.Request) {
	u, ok := auth.CurrentUser(r.Context())
	s, sok := auth.CurrentSession(r.Context())
	if !ok || !sok {
		writeJSON(w, http.StatusOK, nil)
		return
	}
	writeJSON(w, http.StatusOK, toPayload(u, s))
}

// POST /api/auth/sign-out → {success: true}; expires session + cart cookies.
func (a *api) signOut(w http.ResponseWriter, r *http.Request) {
	if err := a.svc.SignOut(w, r); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]bool{"success": true})
}

// GET /api/auth/callback/google?code&state → 303 to the callback path.
func (a *api) googleCallback(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	loc, err := a.svc.GoogleCallback(w, r, auth.GoogleCallbackParams{Code: q.Get("code"), State: q.Get("state"), Error: q.Get("error")})
	if err != nil {
		writeError(w, r, err)
		return
	}
	httpx.Redirect(w, r, loc)
}
