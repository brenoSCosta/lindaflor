package authapi

import (
	"net/http"

	"github.com/wellington-vell/lindaflor/internal/auth"
)

type stepUpBody struct {
	Password string `json:"password"`
	Code     string `json:"code"`
}

// POST /api/auth/two-factor/enable {password?|code?} → {totpUri, secret,
// backupCodes}. The only response that ever carries the secret.
func (a *api) twoFactorEnable(w http.ResponseWriter, r *http.Request) {
	var body stepUpBody
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
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
	if u.TwoFactorEnabled {
		writeError(w, r, auth.ErrTwoFactorAlreadyEnabled)
		return
	}
	// A stolen session must not enroll its own authenticator.
	if err := a.svc.VerifyStepUp(r, u.ID, body.Password, body.Code); err != nil {
		writeError(w, r, err)
		return
	}
	enr, err := a.svc.EnableTwoFactor(r.Context(), u)
	if err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"totpUri": enr.TOTPURI, "secret": enr.Secret, "backupCodes": enr.BackupCodes})
}

type codeBody struct {
	Code  *string `json:"code"`
	Token string  `json:"token"`
}

// POST /api/auth/two-factor/verify-totp {code, token?}: with the pending
// token from sign-in it completes the login ({user, session}); signed in,
// it confirms a pending enrollment ({user, session} with 2FA on).
func (a *api) twoFactorVerifyTOTP(w http.ResponseWriter, r *http.Request) {
	var body codeBody
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	if body.Code == nil {
		writeError(w, r, auth.BadRequest("code is required"))
		return
	}
	code := trim(*body.Code)
	if code == "" {
		writeError(w, r, auth.BadRequest("code is required"))
		return
	}
	if t := trim(body.Token); t != "" {
		u, s, err := a.svc.Complete2FA(w, r, t, code, false, false)
		if err != nil {
			writeError(w, r, err)
			return
		}
		writeJSON(w, http.StatusOK, toPayload(u, s))
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
	if err := a.svc.ConfirmEnableTwoFactor(r.Context(), u, code, "call /two-factor/enable first"); err != nil {
		writeError(w, r, err)
		return
	}
	updated := *u
	updated.TwoFactorEnabled = true
	writeJSON(w, http.StatusOK, toPayload(&updated, s))
}

// POST /api/auth/two-factor/disable {password?|code?} → {status}.
func (a *api) twoFactorDisable(w http.ResponseWriter, r *http.Request) {
	var body stepUpBody
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
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
	if err := a.svc.DisableTwoFactor(r.Context(), u.ID); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

// POST /api/auth/two-factor/verify-backup-code {code, token?}: completes a
// pending login ({user, session}) or burns a code while signed in
// ({status}).
func (a *api) twoFactorVerifyBackupCode(w http.ResponseWriter, r *http.Request) {
	var body codeBody
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	if body.Code == nil || trim(*body.Code) == "" {
		writeError(w, r, auth.BadRequest("code is required"))
		return
	}
	code := trim(*body.Code)
	if t := trim(body.Token); t != "" {
		u, s, err := a.svc.Complete2FA(w, r, t, code, true, false)
		if err != nil {
			writeError(w, r, err)
			return
		}
		writeJSON(w, http.StatusOK, toPayload(u, s))
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
	if err := a.svc.ConsumeBackupCodeSignedIn(r.Context(), u.ID, code); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

// POST /api/auth/two-factor/generate-backup-codes {password?|code?} →
// {backupCodes}. Replaces every code.
func (a *api) twoFactorGenerateBackupCodes(w http.ResponseWriter, r *http.Request) {
	var body stepUpBody
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	u, s, ok := requireSession(w, r)
	if !ok {
		return
	}
	if !u.TwoFactorEnabled {
		writeError(w, r, auth.BadRequest("two-factor authentication is not enabled"))
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
	codes, err := a.svc.RegenerateBackupCodes(r.Context(), u.ID)
	if err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"backupCodes": codes})
}
