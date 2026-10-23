package account

import (
	"encoding/json"
	"errors"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/web/checkout/qrsvg"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

const (
	msgSaved           = "Alterações salvas com sucesso."
	msgTooMany         = "Muitas tentativas. Aguarde um momento e tente novamente."
	msgStepUpWrong     = "Senha ou código do autenticador incorreto." //nolint:gosec // G101: a UI message, not a credential
	msgStepUpMissing   = "Confirme com sua senha atual ou o código do autenticador."
	msgAvatarFailed    = "Falha ao atualizar avatar"
	msgRevokeOthersErr = "Não foi possível encerrar as outras sessões."
	// deleteFreshLogin: how recently the session must have started for an
	// immediate account deletion (a stolen long-lived cookie alone is not
	// enough).
	deleteFreshLogin = 30 * time.Minute
)

var settingsTabs = []string{"profile", "account", "sessions", "security", "linked-accounts", "danger"}

// normalizeTab maps ?tab= to a known tab (default "profile").
func normalizeTab(raw string) string {
	raw = strings.TrimSpace(raw)
	for _, t := range settingsTabs {
		if raw == t {
			return t
		}
	}
	return "profile"
}

func tabURL(tab string) string { return "/settings?" + url.Values{"tab": {normalizeTab(tab)}}.Encode() }

// settingsView is the /settings template input.
type settingsView struct {
	Tab        string
	User       *auth.User
	AvatarURL  string
	HasAvatar  bool
	Sessions   []auth.ListedSession
	Accounts   []auth.LinkedAccount
	Pending    bool
	Factors    auth.StepUpFactors
	HasGoogle  bool
	Credential bool
}

// GET /settings?tab=…: never renders TOTP secret material (the one-time
// QR lives on /settings/two-factor).
func (h *handlers) settingsPage(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	u, s := current(r)
	v := settingsView{Tab: normalizeTab(r.URL.Query().Get("tab")), User: u}
	var err error
	if v.Sessions, err = h.svc.ListSessions(ctx, u.ID, s.TokenHash); err != nil {
		h.fail(w, r, "list sessions", err)
		return
	}
	if v.Accounts, err = h.svc.ListLinkedAccounts(ctx, u.ID); err != nil {
		h.fail(w, r, "list accounts", err)
		return
	}
	if !u.TwoFactorEnabled {
		if v.Pending, err = h.svc.HasUnverifiedTwoFactor(ctx, u.ID); err != nil {
			h.fail(w, r, "pending 2fa", err)
			return
		}
	}
	if v.Factors, err = h.svc.StepUpFactorsFor(ctx, u.ID); err != nil {
		h.fail(w, r, "step-up factors", err)
		return
	}
	for _, a := range v.Accounts {
		switch a.ProviderID {
		case auth.GoogleProviderID:
			v.HasGoogle = true
		case auth.CredentialProviderID:
			v.Credential = true
		}
	}
	v.AvatarURL = auth.ResolveAvatarURL(ctx, h.d.Storage, u.ID, u.Image)
	v.HasAvatar = u.Image != nil && *u.Image != ""
	seo.SetHead(ctx, seo.New("Configurações").WithNoIndex())
	render.Page(w, r, h.d, settingsPage(v)) //nolint:contextcheck // templ components get ctx from Render
}

// --- POST /settings: the single action dispatcher ---

// stepUp is VerifyStepUp with the settings toast messages.
func (h *handlers) stepUp(r *http.Request, userID uuid.UUID, password, code string) (string, bool) {
	err := h.svc.VerifyStepUp(r, userID, password, code)
	if err == nil {
		return "", true
	}
	if msg := auth.PortugueseMessage(err); strings.HasPrefix(msg, "Muitas tentativas") {
		return msg, false
	}
	if !auth.IsClientError(err) {
		slog.ErrorContext(r.Context(), "settings step-up", "error", err)
	}
	if password != "" || strings.TrimSpace(code) != "" {
		return msgStepUpWrong, false
	}
	return msgStepUpMissing, false
}

// revokeOthers reports whether a change-password submission revokes the
// other sessions: absent (or anything but an explicit opt-out) revokes,
// like the JSON API default.
func revokeOthers(raw string, present bool) bool {
	if !present {
		return true
	}
	switch strings.ToLower(strings.TrimSpace(raw)) {
	case "false", "0", "off", "no", "n":
		return false
	}
	return true
}

func (h *handlers) settingsPost(w http.ResponseWriter, r *http.Request) {
	if !parseForm(w, r) {
		return
	}
	ctx := r.Context()
	u, s := current(r)
	f := r.PostForm
	tab := r.URL.Query().Get("tab")
	back := tabURL(tab)
	fail := func(msg string) {
		toast.Set(w, r, toast.Error(msg))
		toast.Redirect(w, r, back)
	}
	ok := func() {
		toast.Set(w, r, toast.Success(msgSaved))
		toast.Redirect(w, r, back)
	}
	// An impersonation session is for looking, not for changing the
	// customer's profile, credentials or sessions.
	if s.ImpersonatedBy != nil {
		fail(auth.MsgImpersonating)
		return
	}
	password, code := f.Get("password"), f.Get("code")

	switch f.Get("action") {
	case "remove_avatar":
		if err := h.svc.RemoveAvatar(ctx, u.ID); err != nil {
			fail(avatarMessage(r, err))
			return
		}
		ok()
	case "update_profile":
		if _, err := h.svc.UpdateUserName(ctx, u.ID, f.Get("name")); err != nil {
			fail(auth.PortugueseMessage(err))
			return
		}
		ok()
	case "change_email":
		if msg, passed := h.stepUp(r, u.ID, password, code); !passed {
			fail(msg)
			return
		}
		if err := h.svc.RequestChangeEmail(ctx, u, f.Get("new_email"), tabURL("account")); err != nil {
			fail(auth.PortugueseMessage(err))
			return
		}
		// Credential change: drop other sessions, keep the authorizing one.
		if err := h.svc.RevokeOtherSessions(ctx, u.ID, s.TokenHash); err != nil {
			slog.ErrorContext(ctx, "revoke other sessions after email change", "error", err)
			fail("E-mail atualizado, mas não foi possível encerrar as outras sessões.")
			return
		}
		ok()
	case "change_password":
		if auth.CheckStepUpRate(r) != nil {
			fail(msgTooMany)
			return
		}
		next := f.Get("new_password")
		if next != f.Get("confirm_password") {
			fail("As senhas não conferem.")
			return
		}
		revoke := revokeOthers(f.Get("revoke_other_sessions"), f.Has("revoke_other_sessions"))
		if err := h.svc.ChangePassword(ctx, u, s.ID, f.Get("current_password"), next, revoke); err != nil {
			fail(auth.PortugueseMessage(err))
			return
		}
		ok()
	case "revoke_session":
		id, valid := parseID(f.Get("session_id"))
		if !valid {
			fail("Sessão inválida.")
			return
		}
		if err := h.svc.RevokeSession(w, r, u, s, id); err != nil {
			fail(auth.PortugueseMessage(err))
			return
		}
		ok()
	case "revoke_other_sessions":
		if err := h.svc.RevokeOtherSessions(ctx, u.ID, s.TokenHash); err != nil {
			slog.ErrorContext(ctx, "revoke other sessions", "error", err)
			fail(msgRevokeOthersErr)
			return
		}
		ok()
	case "enable_2fa":
		h.beginEnrollment(w, r, password, code)
	case "confirm_2fa":
		back = tabURL("security")
		if err := h.svc.ConfirmEnableTwoFactor(ctx, u, f.Get("code"), "call enable first"); err != nil {
			fail(auth.PortugueseMessage(err))
			return
		}
		toast.Set(w, r, toast.Success("2FA ativada."))
		toast.Redirect(w, r, back)
	case "disable_2fa":
		if msg, passed := h.stepUp(r, u.ID, password, code); !passed {
			fail(msg)
			return
		}
		if err := h.svc.DisableTwoFactor(ctx, u.ID); err != nil {
			fail(auth.PortugueseMessage(err))
			return
		}
		toast.Set(w, r, toast.Success("2FA desativada."))
		toast.Redirect(w, r, back)
	case "link_google":
		if msg, passed := h.stepUp(r, u.ID, password, code); !passed {
			fail(msg)
			return
		}
		if s.TokenHash == "" {
			fail("Sessão expirada. Entre novamente.")
			return
		}
		id := u.ID
		target, err := h.svc.StartGoogleOAuth(w, r, tabURL("linked-accounts"), &id, s.TokenHash)
		if err != nil {
			fail(auth.PortugueseMessage(err))
			return
		}
		externalRedirect(w, r, target)
	case "unlink":
		if msg, passed := h.stepUp(r, u.ID, password, code); !passed {
			fail(msg)
			return
		}
		if err := h.svc.UnlinkAccount(ctx, u.ID, f.Get("provider_id"), f.Get("account_id")); err != nil {
			fail(auth.PortugueseMessage(err))
			return
		}
		ok()
	case "delete_user":
		if auth.CheckStepUpRate(r) != nil {
			fail(msgTooMany)
			return
		}
		if msg := h.checkDeleteOrigin(r); msg != "" {
			fail(msg)
			return
		}
		if msg := h.authorizeImmediateDelete(r, u, s, password, code); msg != "" {
			fail(msg)
			return
		}
		if err := h.svc.DeleteUserConfirmed(w, r, u, f.Get("confirm_email")); err != nil {
			fail(auth.PortugueseMessage(err))
			return
		}
		toast.Redirect(w, r, "/")
	default:
		toast.Redirect(w, r, back)
	}
}

// externalRedirect leaves the site (Google consent): 303 for forms,
// HX-Redirect (full navigation) for htmx.
func externalRedirect(w http.ResponseWriter, r *http.Request, target string) {
	if httpx.IsHTMX(r) {
		w.Header().Set("HX-Redirect", target)
		w.WriteHeader(http.StatusOK)
		return
	}
	http.Redirect(w, r, target, http.StatusSeeOther) //nolint:gosec // G710: target is the Google consent URL built by auth.StartGoogleOAuth
}

func avatarMessage(r *http.Request, err error) string {
	var um *auth.UserMessageError
	if errors.As(err, &um) {
		return um.Msg
	}
	slog.ErrorContext(r.Context(), "avatar update", "error", err)
	return msgAvatarFailed
}

// checkDeleteOrigin: same-origin gate for immediate deletion in
// production (defense in depth on top of CrossOriginProtection): the
// Origin or Referer authority must match the Host.
func (h *handlers) checkDeleteOrigin(r *http.Request) string {
	if h.d.Cfg == nil || h.d.Cfg.AppEnv != "production" {
		return ""
	}
	host := strings.ToLower(strings.TrimSpace(r.Host))
	authority := func(v string) string {
		v = strings.TrimSpace(v)
		if _, after, found := strings.Cut(v, "://"); found {
			v = after
		}
		a, _, _ := strings.Cut(v, "/")
		return strings.ToLower(strings.TrimSpace(a))
	}
	for _, name := range []string{"Origin", "Referer"} {
		if a := authority(r.Header.Get(name)); a != "" && host != "" && a == host {
			return ""
		}
	}
	return "Origem da requisição inválida para esta ação."
}

// authorizeImmediateDelete: fresh session (30 min), then the current
// password (credential accounts) or a TOTP code (2FA); OAuth-only accounts
// without 2FA rely on the e-mail confirmation alone. "" = authorized.
func (h *handlers) authorizeImmediateDelete(r *http.Request, u *auth.User, s *auth.Session, password, code string) string {
	ctx := r.Context()
	var createdAt time.Time
	err := h.d.Pool.QueryRow(ctx, `SELECT created_at FROM sessions WHERE id = $1`, s.ID).Scan(&createdAt)
	switch {
	case errors.Is(err, pgx.ErrNoRows):
		return "Sessão inválida. Entre novamente."
	case err != nil:
		slog.ErrorContext(ctx, "delete: load session", "error", err)
		return "Não foi possível verificar a sessão."
	case time.Since(createdAt) > deleteFreshLogin:
		return "Sessão expirada para esta ação. Entre novamente e tente outra vez."
	}
	stored, err := h.d.Q.AuthCredentialPassword(ctx, u.ID)
	if err != nil && !errors.Is(err, pgx.ErrNoRows) {
		return "Não foi possível verificar a conta."
	}
	hasPassword := stored != nil && *stored != ""
	tf, err := h.d.Q.AuthVerifiedTwoFactor(ctx, u.ID)
	hasTOTP := err == nil
	if err != nil && !errors.Is(err, pgx.ErrNoRows) {
		return "Não foi possível verificar a conta."
	}
	if !hasPassword && !hasTOTP {
		return ""
	}
	password = strings.TrimSpace(password)
	if hasPassword && password != "" {
		ok, err := auth.VerifyPassword(password, *stored)
		if err != nil {
			return "Não foi possível verificar a senha."
		}
		if ok {
			return ""
		}
	}
	if code = strings.TrimSpace(code); hasTOTP && code != "" {
		secret, err := auth.DecryptTOTPSecret(tf.SecretNonce, tf.SecretCiphertext, tf.SecretKeyVersion)
		if err != nil {
			return "Não foi possível verificar o código."
		}
		ok, err := h.svc.CheckTOTPCode(ctx, u.ID, secret, code)
		switch {
		case err != nil:
			return auth.PortugueseMessage(err)
		case ok:
			return ""
		}
		return "Código do autenticador inválido."
	}
	if hasPassword && password != "" {
		return "Senha incorreta."
	}
	return "Informe sua senha atual ou o código do autenticador para excluir a conta."
}

// --- 2FA enrollment (one-time secret display) ---

// beginEnrollment: step-up, new pending enrollment, secret into the
// session-scoped flash, 303 to the one-time setup page.
func (h *handlers) beginEnrollment(w http.ResponseWriter, r *http.Request, password, code string) {
	ctx := r.Context()
	u, s := current(r)
	back := tabURL("security")
	fail := func(msg string) {
		toast.Set(w, r, toast.Error(msg))
		toast.Redirect(w, r, back)
	}
	if s.ImpersonatedBy != nil {
		fail(auth.MsgImpersonating)
		return
	}
	if msg, passed := h.stepUp(r, u.ID, password, code); !passed {
		fail(msg)
		return
	}
	enrollment, err := h.svc.EnableTwoFactor(ctx, u)
	if err != nil {
		fail(auth.PortugueseMessage(err))
		return
	}
	if s.TokenHash != "" {
		if payload, err := json.Marshal(enrollment); err == nil { //nolint:gosec // G117: the one-time setup flash must carry the secret (session-scoped, consumed on read)
			h.d.Valkey.TotpEnrollmentFlashPut(ctx, s.TokenHash, string(payload))
		}
	}
	httpx.Redirect(w, r, "/settings/two-factor")
}

// POST /settings/two-factor (password/code): HTML enable, 303 to the
// setup page with no toast.
func (h *handlers) twoFactorSetupEnable(w http.ResponseWriter, r *http.Request) {
	if !parseForm(w, r) {
		return
	}
	h.beginEnrollment(w, r, r.PostForm.Get("password"), r.PostForm.Get("code"))
}

// twoFactorView is the /settings/two-factor template input.
type twoFactorView struct {
	Enabled bool
	Pending bool
	Flash   *auth.TwoFactorEnrollment
	QR      string
	Factors auth.StepUpFactors
}

// ManualSecret is the base32 secret from the otpauth URI, grouped in
// blocks of four for typing into an authenticator ("" when unavailable).
func (v twoFactorView) ManualSecret() string {
	if v.Flash == nil {
		return ""
	}
	u, err := url.Parse(v.Flash.TOTPURI)
	if err != nil {
		return ""
	}
	secret := strings.ToUpper(u.Query().Get("secret"))
	var b strings.Builder
	for i, r := range secret {
		if i > 0 && i%4 == 0 {
			b.WriteByte(' ')
		}
		b.WriteRune(r)
	}
	return b.String()
}

// GET /settings/two-factor: consumes the enrollment flash (a second GET
// cannot re-read the encrypted secret).
func (h *handlers) twoFactorSetup(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	u, s := current(r)
	v := twoFactorView{Enabled: u.TwoFactorEnabled}
	var err error
	if !v.Enabled {
		if s.TokenHash != "" {
			if raw, found := h.d.Valkey.TotpEnrollmentFlashTake(ctx, s.TokenHash); found {
				var e auth.TwoFactorEnrollment
				if json.Unmarshal([]byte(raw), &e) == nil {
					v.Flash = &e
					v.QR = qrsvg.DataURI(e.TOTPURI, 192)
				}
			}
		}
		if v.Pending, err = h.svc.HasUnverifiedTwoFactor(ctx, u.ID); err != nil {
			h.fail(w, r, "pending 2fa", err)
			return
		}
	}
	if v.Factors, err = h.svc.StepUpFactorsFor(ctx, u.ID); err != nil {
		h.fail(w, r, "step-up factors", err)
		return
	}
	w.Header().Set("Cache-Control", "no-store")
	seo.SetHead(ctx, seo.New("Ativar autenticação de dois fatores").WithNoIndex())
	render.Page(w, r, h.d, twoFactorSetupPage(v)) //nolint:contextcheck // templ components get ctx from Render
}

// --- POST /settings/avatar (multipart "file", 2 MiB) ---

func (h *handlers) uploadAvatar(w http.ResponseWriter, r *http.Request) {
	u, s := current(r)
	back := tabURL("profile")
	if s.ImpersonatedBy != nil {
		toast.Set(w, r, toast.Error(auth.MsgImpersonating))
		toast.Redirect(w, r, back)
		return
	}
	var body []byte
	var contentType string
	mr, err := r.MultipartReader()
	if err != nil {
		http.Error(w, "Formulário inválido.", http.StatusBadRequest)
		return
	}
	for {
		part, err := mr.NextPart()
		if errors.Is(err, io.EOF) {
			break
		}
		if err != nil {
			if httpx.IsBodyTooLarge(err) {
				httpx.TooLarge(w)
				return
			}
			http.Error(w, "Formulário inválido.", http.StatusBadRequest)
			return
		}
		if part.FormName() != "file" {
			_ = part.Close()
			continue
		}
		contentType = part.Header.Get("Content-Type")
		body, err = io.ReadAll(part)
		_ = part.Close()
		if err != nil {
			if httpx.IsBodyTooLarge(err) {
				httpx.TooLarge(w)
				return
			}
			http.Error(w, "Formulário inválido.", http.StatusBadRequest)
			return
		}
	}
	if _, err := h.svc.UpdateAvatar(r.Context(), u.ID, body, contentType); err != nil {
		toast.Set(w, r, toast.Error(avatarMessage(r, err)))
		toast.Redirect(w, r, back)
		return
	}
	toast.Set(w, r, toast.Success(msgSaved))
	toast.Redirect(w, r, back)
}

// --- htmx fast paths of the sessions tab ---

// POST /settings/revoke-session (session_id): revokes one session and
// reloads the page (HX-Refresh); no-JS forms use POST /settings.
func (h *handlers) revokeSessionProc(w http.ResponseWriter, r *http.Request) {
	if !parseForm(w, r) {
		return
	}
	u, s := current(r)
	if auth.ForbidImpersonated(s) != nil {
		toast.Set(w, r, toast.Error(auth.MsgImpersonating))
		w.WriteHeader(http.StatusForbidden)
		return
	}
	id, valid := parseID(r.PostForm.Get("session_id"))
	if !valid {
		toast.Set(w, r, toast.Error("Sessão inválida"))
		w.WriteHeader(http.StatusBadRequest)
		return
	}
	if err := h.svc.RevokeSession(w, r, u, s, id); err != nil {
		if !auth.IsClientError(err) {
			h.fail(w, r, "revoke session", err)
			return
		}
		toast.Set(w, r, toast.Error(auth.PortugueseMessage(err)))
		w.WriteHeader(auth.StatusOf(err))
		return
	}
	h.refresh(w, r)
}

// POST /settings/revoke-other-sessions: signs every other device out.
func (h *handlers) revokeOtherSessionsProc(w http.ResponseWriter, r *http.Request) {
	u, s := current(r)
	if auth.ForbidImpersonated(s) != nil {
		toast.Set(w, r, toast.Error(auth.MsgImpersonating))
		w.WriteHeader(http.StatusForbidden)
		return
	}
	if err := h.svc.RevokeOtherSessions(r.Context(), u.ID, s.TokenHash); err != nil {
		h.fail(w, r, "revoke other sessions", err)
		return
	}
	h.refresh(w, r)
}

// refresh answers a fast-path call: HX-Refresh for htmx, back to the
// sessions tab otherwise.
func (h *handlers) refresh(w http.ResponseWriter, r *http.Request) {
	if httpx.IsHTMX(r) {
		w.Header().Set("HX-Refresh", "true")
		w.WriteHeader(http.StatusNoContent)
		return
	}
	toast.Set(w, r, toast.Success(msgSaved))
	toast.Redirect(w, r, tabURL("sessions"))
}
