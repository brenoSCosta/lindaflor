package authapi

import (
	"encoding/json"
	"net/http"
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/auth"
)

// POST /api/auth/request-password-reset {email, redirectTo?} → {status}.
// Always succeeds (anti-enumeration).
func (a *api) requestPasswordReset(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Email      *string `json:"email"`
		RedirectTo string  `json:"redirectTo"`
	}
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	if body.Email == nil {
		writeError(w, r, auth.ErrEmailRequired)
		return
	}
	if err := a.svc.RequestPasswordReset(r.Context(), *body.Email, body.RedirectTo); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

// POST /api/auth/reset-password {token, newPassword} → {status}.
func (a *api) resetPassword(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Token       *string `json:"token"`
		NewPassword *string `json:"newPassword"`
	}
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	if body.Token == nil || body.NewPassword == nil {
		writeError(w, r, auth.BadRequest("token and newPassword are required"))
		return
	}
	if trim(*body.Token) == "" {
		writeError(w, r, auth.ErrTokenRequired)
		return
	}
	// Early weak-input reject; the service re-validates with the account email.
	if err := auth.ValidatePassword(*body.NewPassword, "lindaflor"); err != nil {
		writeError(w, r, err)
		return
	}
	if err := a.svc.ResetPassword(r.Context(), *body.Token, *body.NewPassword); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

// POST /api/auth/send-verification-email {email?, callbackURL?} → {status}.
// Without an email the signed-in user's address is used.
func (a *api) sendVerificationEmail(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Email       string `json:"email"`
		CallbackURL string `json:"callbackUrl"`
	}
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	email := body.Email
	if trim(email) == "" {
		u, ok := auth.CurrentUser(r.Context())
		if !ok {
			writeError(w, r, auth.ErrEmailRequired)
			return
		}
		email = u.Email
	}
	if err := a.svc.SendVerificationEmail(r.Context(), email, body.CallbackURL); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

// GET /api/auth/verify-email?token=…&callbackURL=… → {status}.
func (a *api) verifyEmailGet(w http.ResponseWriter, r *http.Request) {
	token := r.URL.Query().Get("token")
	if token == "" {
		writeError(w, r, auth.ErrTokenRequired)
		return
	}
	if err := a.svc.ConsumeEmailVerification(r.Context(), token); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

// POST /api/auth/verify-email {token} → {status}.
func (a *api) verifyEmailPost(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Token *string `json:"token"`
	}
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	if body.Token == nil || trim(*body.Token) == "" {
		writeError(w, r, auth.ErrTokenRequired)
		return
	}
	if err := a.svc.ConsumeEmailVerification(r.Context(), *body.Token); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

// POST /api/auth/change-email {newEmail, password?|code?, callbackURL?} →
// {status}. Step-up required; revokes every other session.
func (a *api) changeEmail(w http.ResponseWriter, r *http.Request) {
	u, s, ok := requireSession(w, r)
	if !ok {
		return
	}
	var body struct {
		NewEmail    *string `json:"newEmail"`
		NewEmail2   *string `json:"new_email"`
		CallbackURL string  `json:"callbackUrl"`
		Password    string  `json:"password"`
		Code        string  `json:"code"`
	}
	if err := auth.ForbidImpersonated(s); err != nil {
		writeError(w, r, err)
		return
	}
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	if body.NewEmail == nil {
		body.NewEmail = body.NewEmail2
	}
	if body.NewEmail == nil {
		writeError(w, r, auth.BadRequest("newEmail is required"))
		return
	}
	if err := a.svc.ChangeEmail(r, u, s, *body.NewEmail, body.Password, body.Code, body.CallbackURL); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

// POST /api/auth/change-email/confirm {token} → {status} (works logged out).
func (a *api) confirmChangeEmail(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Token *string `json:"token"`
	}
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	if body.Token == nil {
		writeError(w, r, auth.ErrTokenRequired)
		return
	}
	if err := a.svc.CompleteChangeEmail(r.Context(), *body.Token); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

// POST /api/auth/change-password {currentPassword, newPassword,
// revokeOtherSessions? (default true)} → {status}.
func (a *api) changePassword(w http.ResponseWriter, r *http.Request) {
	u, s, ok := requireSession(w, r)
	if !ok {
		return
	}
	if err := auth.ForbidImpersonated(s); err != nil {
		writeError(w, r, err)
		return
	}
	if err := auth.CheckStepUpRate(r); err != nil {
		writeError(w, r, err)
		return
	}
	var body struct {
		CurrentPassword     *string `json:"currentPassword"`
		NewPassword         *string `json:"newPassword"`
		RevokeOtherSessions *bool   `json:"revokeOtherSessions"`
	}
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	if body.CurrentPassword == nil || body.NewPassword == nil {
		writeError(w, r, auth.BadRequest("currentPassword and newPassword are required"))
		return
	}
	revoke := body.RevokeOtherSessions == nil || *body.RevokeOtherSessions
	if err := a.svc.ChangePassword(r.Context(), u, s.ID, *body.CurrentPassword, *body.NewPassword, revoke); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

// POST /api/auth/delete-user {token} completes a deletion; {password?|code?,
// callbackURL?} (signed in, step-up) requests one by mail. → {status}.
func (a *api) deleteUser(w http.ResponseWriter, r *http.Request) {
	var body struct {
		Password    string `json:"password"`
		Code        string `json:"code"`
		Token       string `json:"token"`
		CallbackURL string `json:"callbackUrl"`
	}
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	if t := trim(body.Token); t != "" {
		if err := a.svc.CompleteDelete(w, r, t); err != nil {
			writeError(w, r, err)
			return
		}
		writeJSON(w, http.StatusOK, statusOK)
		return
	}
	u, s, ok := requireSession(w, r)
	if !ok {
		return
	}
	if err := auth.ForbidImpersonated(s); err != nil {
		writeError(w, r, err)
		return
	}
	if err := a.svc.VerifyStepUp(r, u.ID, body.Password, body.Code); err != nil {
		writeError(w, r, err)
		return
	}
	if err := a.svc.RequestDeleteUser(r.Context(), u, body.CallbackURL); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

// POST /api/auth/update-user {name?, image?: null} → {user}. Avatars are
// uploaded in settings; image may only be cleared here.
func (a *api) updateUser(w http.ResponseWriter, r *http.Request) {
	u, s, ok := requireSession(w, r)
	if !ok {
		return
	}
	if err := auth.ForbidImpersonated(s); err != nil {
		writeError(w, r, err)
		return
	}
	var body map[string]json.RawMessage
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	var name, image *string
	imageSet := false
	for k, raw := range body {
		switch strings.ToLower(k) {
		case "name":
			if err := json.Unmarshal(raw, &name); err != nil {
				writeError(w, r, auth.BadRequest("name must be a string"))
				return
			}
		case "image":
			imageSet = true
			if err := json.Unmarshal(raw, &image); err != nil {
				writeError(w, r, auth.BadRequest("image must be a string or null"))
				return
			}
		}
	}
	updated, err := a.svc.UpdateUser(r.Context(), u.ID, name, imageSet, image)
	if err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"user": toUserJSON(updated)})
}

// listedSessionJSON: token mirrors id (never the cookie token).
type listedSessionJSON struct {
	ID        string  `json:"id"`
	Token     string  `json:"token"`
	UserAgent *string `json:"userAgent"`
	IPAddress *string `json:"ipAddress"`
	CreatedAt string  `json:"createdAt"`
	ExpiresAt string  `json:"expiresAt"`
	Current   bool    `json:"current"`
}

// GET /api/auth/list-sessions → [session…] (current flagged).
func (a *api) listSessions(w http.ResponseWriter, r *http.Request) {
	u, s, ok := requireSession(w, r)
	if !ok {
		return
	}
	list, err := a.svc.ListSessions(r.Context(), u.ID, s.TokenHash)
	if err != nil {
		writeError(w, r, err)
		return
	}
	out := make([]listedSessionJSON, 0, len(list))
	for _, l := range list {
		id := l.ID.String()
		out = append(out, listedSessionJSON{
			ID: id, Token: id, UserAgent: l.UserAgent, IPAddress: l.IPAddress,
			CreatedAt: formatTime(l.CreatedAt), ExpiresAt: formatTime(l.ExpiresAt), Current: l.Current,
		})
	}
	writeJSON(w, http.StatusOK, out)
}

// POST /api/auth/revoke-session {sessionId|token} → {status}.
func (a *api) revokeSession(w http.ResponseWriter, r *http.Request) {
	u, s, ok := requireSession(w, r)
	if !ok {
		return
	}
	if err := auth.ForbidImpersonated(s); err != nil {
		writeError(w, r, err)
		return
	}
	var body struct {
		SessionID string `json:"sessionId"`
		Token     string `json:"token"`
	}
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	raw := trim(body.SessionID)
	if raw == "" {
		raw = trim(body.Token)
	}
	if raw == "" {
		writeError(w, r, auth.BadRequest("sessionId or token is required"))
		return
	}
	id, err := uuid.Parse(raw)
	if err != nil {
		writeError(w, r, auth.BadRequest("sessionId or token must be a valid session id"))
		return
	}
	if err := a.svc.RevokeSession(w, r, u, s, id); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

// POST /api/auth/revoke-other-sessions → {status}.
func (a *api) revokeOtherSessions(w http.ResponseWriter, r *http.Request) {
	u, s, ok := requireSession(w, r)
	if !ok {
		return
	}
	if err := auth.ForbidImpersonated(s); err != nil {
		writeError(w, r, err)
		return
	}
	if err := a.svc.RevokeOtherSessions(r.Context(), u.ID, s.TokenHash); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

type accountJSON struct {
	ID         string `json:"id"`
	ProviderID string `json:"providerId"`
	AccountID  string `json:"accountId"`
	CreatedAt  string `json:"createdAt"`
}

// GET /api/auth/list-accounts → [account…].
func (a *api) listAccounts(w http.ResponseWriter, r *http.Request) {
	u, _, ok := requireSession(w, r)
	if !ok {
		return
	}
	list, err := a.svc.ListLinkedAccounts(r.Context(), u.ID)
	if err != nil {
		writeError(w, r, err)
		return
	}
	out := make([]accountJSON, 0, len(list))
	for _, l := range list {
		out = append(out, accountJSON{ID: l.ID.String(), ProviderID: l.ProviderID, AccountID: l.AccountID, CreatedAt: formatTime(l.CreatedAt)})
	}
	writeJSON(w, http.StatusOK, out)
}

// POST /api/auth/link-social {provider, callbackURL?, password?|code?} →
// {url}. Step-up required; the state is bound to this session.
func (a *api) linkSocial(w http.ResponseWriter, r *http.Request) {
	u, s, ok := requireSession(w, r)
	if !ok {
		return
	}
	if err := auth.ForbidImpersonated(s); err != nil {
		writeError(w, r, err)
		return
	}
	var body struct {
		Provider    string `json:"provider"`
		CallbackURL string `json:"callbackUrl"`
		Password    string `json:"password"`
		Code        string `json:"code"`
	}
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	if a.socialUnavailable(w, strings.ToLower(trim(body.Provider))) {
		return
	}
	if s.TokenHash == "" {
		writeError(w, r, auth.ErrUnauthorized)
		return
	}
	if err := a.svc.VerifyStepUp(r, u.ID, body.Password, body.Code); err != nil {
		writeError(w, r, err)
		return
	}
	id := u.ID
	url, err := a.svc.StartGoogleOAuth(w, r, body.CallbackURL, &id, s.TokenHash)
	if err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]string{"url": url})
}

// POST /api/auth/unlink-account {providerId, accountId, password?|code?} →
// {status}. The last usable login method cannot be removed.
func (a *api) unlinkAccount(w http.ResponseWriter, r *http.Request) {
	u, s, ok := requireSession(w, r)
	if !ok {
		return
	}
	if err := auth.ForbidImpersonated(s); err != nil {
		writeError(w, r, err)
		return
	}
	var body struct {
		ProviderID *string `json:"providerId"`
		AccountID  *string `json:"accountId"`
		Password   string  `json:"password"`
		Code       string  `json:"code"`
	}
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	provider, account := trim(str(body.ProviderID)), trim(str(body.AccountID))
	if provider == "" || account == "" {
		writeError(w, r, auth.BadRequest("providerId and accountId are required"))
		return
	}
	if err := a.svc.VerifyStepUp(r, u.ID, body.Password, body.Code); err != nil {
		writeError(w, r, err)
		return
	}
	if err := a.svc.UnlinkAccount(r.Context(), u.ID, provider, account); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}
