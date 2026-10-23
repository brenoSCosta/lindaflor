package account_test

import (
	"bytes"
	"image"
	"image/color"
	"image/png"
	"mime/multipart"
	"net/http"
	"net/textproto"
	"net/url"
	"strings"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func settingsPost(c *testutil.Client, tab string, form url.Values) *testutil.Response {
	return c.PostForm("/settings?tab="+tab, form)
}

func changeEmailTokens(t *testing.T, s *testutil.Server, userID uuid.UUID) int64 {
	t.Helper()
	return scalar[int64](t, s.Pool, `SELECT COUNT(*) FROM verifications WHERE identifier LIKE $1`, "change-email:"+userID.String()+":%")
}

// pendingSecret decrypts the user's (pending or verified) TOTP secret; the
// HTML flow never shows it as text.
func pendingSecret(t *testing.T, s *testutil.Server, userID uuid.UUID) string {
	t.Helper()
	var nonce, ct []byte
	var ver int16
	if err := s.Pool.QueryRow(t.Context(), `SELECT secret_nonce, secret_ciphertext, secret_key_version FROM two_factor WHERE user_id = $1`, userID).
		Scan(&nonce, &ct, &ver); err != nil {
		t.Fatal(err)
	}
	secret, err := auth.DecryptTOTPSecret(nonce, ct, ver)
	if err != nil {
		t.Fatal(err)
	}
	return secret
}

func TestSettingsPageRendersEveryTab(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "settings-tabs")
	res := u.c.Get("/settings")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, "Configurações", "Perfil", "Conta", "Sessões", "Segurança", "Contas vinculadas", "Zona de perigo",
		`value="update_profile"`, `value="change_email"`, `value="change_password"`, `value="delete_user"`,
		"Alterar foto", `id="avatar-cropper-dialog"`, "js/avatar-cropper.js", "E-mail e senha", "Não conectado",
		"Este dispositivo", "A 2FA está desativada.", `action="/settings/two-factor"`, "Manter outros dispositivos conectados",
		`x-data="{ tab: &#39;profile&#39; }"`, "noindex")
	// Only the active panel is visible without JS.
	res = u.c.Get("/settings?tab=security")
	wantContains(t, res, `x-data="{ tab: &#39;security&#39; }"`)
	res = u.c.Get("/settings?tab=%3Cscript%3E")
	wantContains(t, res, `x-data="{ tab: &#39;profile&#39; }"`)
}

func TestSettingsUpdateProfile(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "settings-profile")
	res := settingsPost(u.c, "profile", url.Values{"action": {"update_profile"}, "name": {"Ana Nova"}})
	wantRedirect(t, res, "/settings?tab=profile")
	wantToast(t, res, toast.SuccessType, "Alterações salvas com sucesso.")
	if n := scalar[string](t, s.Pool, `SELECT name FROM users WHERE id = $1`, u.id); n != "Ana Nova" {
		t.Fatalf("name %q", n)
	}
	res = settingsPost(u.c, "profile", url.Values{"action": {"update_profile"}, "name": {"  "}})
	wantToast(t, res, toast.ErrorType, "Informe o nome.")
	// Unknown action: back to the tab, nothing else.
	res = settingsPost(u.c, "account", url.Values{"action": {"bogus"}})
	wantRedirect(t, res, "/settings?tab=account")
	// htmx: 200 + HX-Location, toast on X-Toast.
	res = u.c.Do(http.MethodPost, "/settings?tab=profile", strings.NewReader("action=update_profile&name=Ana+HTMX"),
		http.Header{"Content-Type": {"application/x-www-form-urlencoded"}, "HX-Request": {"true"}})
	wantStatus(t, res, http.StatusOK)
	if res.Header.Get("HX-Location") != "/settings?tab=profile" || res.Header.Get("X-Toast") == "" {
		t.Fatalf("htmx answer %v", res.Header)
	}
}

// tests/auth_sessions.rs: settings_change_password_parity_with_api_default
func TestSettingsChangePasswordParityWithAPIDefault(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	// Default (no flag field): the other session is revoked.
	u := signUp(t, s, "pw-html-default")
	other := signInAgain(t, s, u)
	res := settingsPost(u.c, "account", url.Values{
		"action": {"change_password"}, "current_password": {pw},
		"new_password": {newPW}, "confirm_password": {newPW},
	})
	wantRedirect(t, res, "/settings?tab=account")
	wantToast(t, res, toast.SuccessType, "Alterações salvas com sucesso.")
	if sessionEmail(t, other) != "" {
		t.Fatal("HTML default must revoke other sessions")
	}
	if sessionEmail(t, u.c) != u.email {
		t.Fatal("current session must survive")
	}

	// Explicit opt-out: both sessions survive.
	k := signUp(t, s, "pw-html-keep")
	kOther := signInAgain(t, s, k)
	res = settingsPost(k.c, "account", url.Values{
		"action": {"change_password"}, "current_password": {pw},
		"new_password": {newPW}, "confirm_password": {newPW}, "revoke_other_sessions": {"false"},
	})
	wantRedirect(t, res, "/settings?tab=account")
	if sessionEmail(t, kOther) != k.email || sessionEmail(t, k.c) != k.email {
		t.Fatal("opt-out must keep both sessions")
	}

	// Mismatch and wrong current password.
	res = settingsPost(k.c, "account", url.Values{
		"action": {"change_password"}, "current_password": {newPW},
		"new_password": {pw}, "confirm_password": {"other"},
	})
	wantToast(t, res, toast.ErrorType, "As senhas não conferem.")
	res = settingsPost(k.c, "account", url.Values{
		"action": {"change_password"}, "current_password": {"wrong-password"},
		"new_password": {pw}, "confirm_password": {pw},
	})
	wantToast(t, res, toast.ErrorType, "Credenciais inválidas.")
}

// tests/auth_sessions.rs: settings_revoke_other_sessions_action
func TestSettingsRevokeOtherSessionsAction(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "signout-everywhere")
	other := signInAgain(t, s, u)
	res := u.c.Get("/settings?tab=sessions")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, "revoke_other_sessions", "Sair de todos", "Revogar", `hx-post="/settings/revoke-session"`)

	res = settingsPost(u.c, "sessions", url.Values{"action": {"revoke_other_sessions"}})
	wantRedirect(t, res, "/settings?tab=sessions")
	if sessionEmail(t, other) != "" {
		t.Fatal("other session must be revoked")
	}
	if sessionEmail(t, u.c) != u.email {
		t.Fatal("current session must survive")
	}
	list := u.c.Get("/api/auth/list-sessions")
	if strings.Count(list.Body, `"current"`) != 1 || !strings.Contains(list.Body, `"current":true`) {
		t.Fatalf("sessions: %s", list.Body)
	}
	res = u.c.Get("/settings?tab=sessions")
	wantMissing(t, res, "Sair de todos")
}

func TestSettingsRevokeSessionFormAndProcs(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "revoke-one")
	other := signInAgain(t, s, u)
	third := signInAgain(t, s, u)
	ids := func() []string {
		var out []string
		rows, err := s.Pool.Query(t.Context(), `SELECT id::text FROM sessions WHERE user_id = $1 ORDER BY created_at`, u.id)
		if err != nil {
			t.Fatal(err)
		}
		defer rows.Close()
		for rows.Next() {
			var id string
			if err := rows.Scan(&id); err != nil {
				t.Fatal(err)
			}
			out = append(out, id)
		}
		return out
	}
	all := ids()
	if len(all) != 3 {
		t.Fatalf("sessions %v", all)
	}
	// No-JS form path.
	res := settingsPost(u.c, "sessions", url.Values{"action": {"revoke_session"}, "session_id": {all[1]}})
	wantRedirect(t, res, "/settings?tab=sessions")
	wantToast(t, res, toast.SuccessType, "Alterações salvas com sucesso.")
	if sessionEmail(t, other) != "" {
		t.Fatal("revoked session still valid")
	}
	res = settingsPost(u.c, "sessions", url.Values{"action": {"revoke_session"}, "session_id": {"nope"}})
	wantToast(t, res, toast.ErrorType, "Sessão inválida.")

	// htmx fast path: HX-Refresh.
	res = u.c.Do(http.MethodPost, "/settings/revoke-session", strings.NewReader("session_id="+all[2]),
		http.Header{"Content-Type": {"application/x-www-form-urlencoded"}, "HX-Request": {"true"}})
	wantStatus(t, res, http.StatusNoContent)
	if res.Header.Get("HX-Refresh") != "true" || sessionEmail(t, third) != "" {
		t.Fatal("fast-path revoke failed")
	}
	res = u.c.Do(http.MethodPost, "/settings/revoke-session", strings.NewReader("session_id=bad"),
		http.Header{"Content-Type": {"application/x-www-form-urlencoded"}, "HX-Request": {"true"}})
	wantStatus(t, res, http.StatusBadRequest)

	fourth := signInAgain(t, s, u)
	res = u.c.Do(http.MethodPost, "/settings/revoke-other-sessions", nil, http.Header{"HX-Request": {"true"}})
	wantStatus(t, res, http.StatusNoContent)
	if sessionEmail(t, fourth) != "" || sessionEmail(t, u.c) != u.email {
		t.Fatal("revoke-other-sessions fast path")
	}
}

// tests/auth_hardening.rs: html_change_email_requires_step_up_before_revoking_sessions
func TestHTMLChangeEmailRequiresStepUpBeforeRevokingSessions(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "stepup-html")
	other := signInAgain(t, s, u)
	newEmail := testutil.UniqueEmail("stepup-html-new")

	res := settingsPost(u.c, "account", url.Values{"action": {"change_email"}, "new_email": {newEmail}})
	wantRedirect(t, res, "/settings?tab=account")
	wantToast(t, res, toast.ErrorType, "Confirme com sua senha atual ou o código do autenticador.")
	if changeEmailTokens(t, s, u.id) != 0 || sessionEmail(t, other) != u.email {
		t.Fatal("session cookie alone must not change the email or revoke sessions")
	}
	res = settingsPost(u.c, "account", url.Values{"action": {"change_email"}, "new_email": {newEmail}, "password": {"wrong"}})
	wantToast(t, res, toast.ErrorType, "Senha ou código do autenticador incorreto.")

	res = settingsPost(u.c, "account", url.Values{"action": {"change_email"}, "new_email": {newEmail}, "password": {pw}})
	wantRedirect(t, res, "/settings?tab=account")
	wantToast(t, res, toast.SuccessType, "Alterações salvas com sucesso.")
	if changeEmailTokens(t, s, u.id) != 1 {
		t.Fatal("change-email token not issued")
	}
	if sessionEmail(t, other) != "" || sessionEmail(t, u.c) != u.email {
		t.Fatal("credential change must revoke other sessions only")
	}
}

// tests/auth_hardening.rs: impersonation HTML settings refusal
func TestImpersonationHTMLSettingsRefusal(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	admin := signUp(t, s, "imp-admin")
	exec(t, s.Pool, `UPDATE users SET role = 'admin' WHERE id = $1`, admin.id)
	target := signUp(t, s, "imp-target")
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/impersonate-user", map[string]any{"userId": target.id.String()}), http.StatusOK)

	for _, form := range []url.Values{
		{"action": {"change_email"}, "new_email": {testutil.UniqueEmail("imp-html")}, "password": {pw}},
		{"action": {"update_profile"}, "name": {"Hijacked"}},
		{"action": {"revoke_other_sessions"}},
		{"action": {"delete_user"}, "confirm_email": {target.email}, "password": {pw}},
	} {
		res := settingsPost(admin.c, "account", form)
		wantRedirect(t, res, "/settings?tab=account")
		wantToast(t, res, toast.ErrorType, auth.MsgImpersonating)
	}
	res := admin.c.PostForm("/settings/two-factor", url.Values{"password": {pw}})
	wantToast(t, res, toast.ErrorType, auth.MsgImpersonating)
	res = admin.c.Do(http.MethodPost, "/settings/revoke-other-sessions", nil, http.Header{"HX-Request": {"true"}})
	wantStatus(t, res, http.StatusForbidden)
	if changeEmailTokens(t, s, target.id) != 0 {
		t.Fatal("impersonated change-email queued a token")
	}
	if n := scalar[string](t, s.Pool, `SELECT name FROM users WHERE id = $1`, target.id); n == "Hijacked" {
		t.Fatal("impersonation renamed the user")
	}
	// The banner is shown in the app shell.
	res = admin.c.Get("/settings")
	wantContains(t, res, "Você está atuando como", `action="/stop-impersonating"`)
}

// tests/auth_two_factor.rs: settings_setup_page_leaks_no_secret
func TestSettingsSetupPageLeaksNoSecret(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "2fa-noleak")
	res := u.c.PostJSON("/api/auth/two-factor/enable", map[string]any{"password": pw})
	wantStatus(t, res, http.StatusOK)
	j := res.JSON(t)
	secret, uri := j["secret"].(string), j["totpUri"].(string)
	for _, path := range []string{"/settings?setup=1", "/settings?tab=security", "/settings/two-factor"} {
		res := u.c.Get(path)
		wantStatus(t, res, http.StatusOK)
		wantMissing(t, res, secret, uri, "otpauth://", "data:image/svg+xml")
	}
	res = u.c.Get("/settings?tab=security")
	wantContains(t, res, "Cadastro da 2FA iniciado.", "Gerar novo QR", `value="confirm_2fa"`)
}

// tests/auth_two_factor.rs: html_enable_shows_secret_once_on_setup_page,
// then confirm and disable through the settings forms.
func TestHTMLEnableShowsSecretOnceThenConfirmAndDisable(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "2fa-html")

	// Wrong / missing step-up.
	res := u.c.PostForm("/settings/two-factor", url.Values{"action": {"enable_2fa"}})
	wantRedirect(t, res, "/settings?tab=security")
	wantToast(t, res, toast.ErrorType, "Confirme com sua senha atual ou o código do autenticador.")

	res = u.c.PostForm("/settings/two-factor", url.Values{"action": {"enable_2fa"}, "password": {pw}})
	wantRedirect(t, res, "/settings/two-factor")
	if len(toasts(res)) != 0 {
		t.Fatal("HTML enable must not toast")
	}
	res = u.c.Get("/settings/two-factor")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, "data:image/svg+xml", "Códigos de recuperação", `value="confirm_2fa"`,
		"Segredo para digitar manualmente", groupedSecret(pendingSecret(t, s, u.id)))
	wantMissing(t, res, "otpauth://")
	res = u.c.Get("/settings/two-factor")
	wantStatus(t, res, http.StatusOK)
	wantMissing(t, res, "otpauth://", "data:image/svg+xml", "Códigos de recuperação", "Segredo para digitar manualmente")
	wantContains(t, res, "O segredo já foi exibido.")
	res = u.c.Get("/settings?tab=security")
	wantMissing(t, res, "otpauth://", "data:image/svg+xml")

	// The settings dispatcher also starts an enrollment (enable_2fa).
	res = settingsPost(u.c, "security", url.Values{"action": {"enable_2fa"}, "password": {pw}})
	wantRedirect(t, res, "/settings/two-factor")

	secret := pendingSecret(t, s, u.id)
	res = settingsPost(u.c, "security", url.Values{"action": {"confirm_2fa"}, "code": {"000000"}})
	wantRedirect(t, res, "/settings?tab=security")
	if ts := toasts(res); len(ts) == 0 || ts[0].Type != toast.ErrorType {
		t.Fatalf("wrong code toasts %v", ts)
	}
	res = settingsPost(u.c, "security", url.Values{"action": {"confirm_2fa"}, "code": {testutil.TOTPCode(t, secret)}})
	wantToast(t, res, toast.SuccessType, "2FA ativada.")
	session := u.c.Get("/api/auth/get-session").JSON(t)
	if session["user"].(map[string]any)["twoFactorEnabled"] != true {
		t.Fatal("2FA not enabled")
	}
	res = u.c.Get("/settings/two-factor")
	wantContains(t, res, "A 2FA já está ativada.")
	res = u.c.Get("/settings?tab=security")
	wantContains(t, res, "A 2FA está ativada.", "Desativar 2FA", "Ou código do autenticador")

	// tests/auth_two_factor.rs: html_disable_2fa_with_password_turns_it_off
	res = settingsPost(u.c, "security", url.Values{"action": {"disable_2fa"}, "password": {"wrong"}})
	wantToast(t, res, toast.ErrorType, "Senha ou código do autenticador incorreto.")
	res = settingsPost(u.c, "security", url.Values{"action": {"disable_2fa"}, "password": {pw}})
	wantRedirect(t, res, "/settings")
	wantToast(t, res, toast.SuccessType, "2FA desativada.")
	session = u.c.Get("/api/auth/get-session").JSON(t)
	if session["user"].(map[string]any)["twoFactorEnabled"] != false {
		t.Fatal("2FA must be off after a correct-password disable")
	}
}

func TestSettingsLinkedAccounts(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "linked")
	exec(t, s.Pool, `INSERT INTO accounts (id, account_id, provider_id, user_id) VALUES ($1, 'g-123', 'google', $2)`, uuid.Must(uuid.NewV7()), u.id)
	res := u.c.Get("/settings?tab=linked-accounts")
	wantContains(t, res, "Google", "g-123", "Desconectar", `value="unlink"`)
	wantMissing(t, res, "Não conectado")

	res = settingsPost(u.c, "linked-accounts", url.Values{"action": {"unlink"}, "provider_id": {"google"}, "account_id": {"g-123"}})
	wantToast(t, res, toast.ErrorType, "Confirme com sua senha atual ou o código do autenticador.")
	res = settingsPost(u.c, "linked-accounts", url.Values{"action": {"unlink"}, "provider_id": {"google"}, "account_id": {"g-123"}, "password": {pw}})
	wantRedirect(t, res, "/settings?tab=linked-accounts")
	wantToast(t, res, toast.SuccessType, "Alterações salvas com sucesso.")
	if n := scalar[int64](t, s.Pool, `SELECT COUNT(*) FROM accounts WHERE user_id = $1 AND provider_id = 'google'`, u.id); n != 0 {
		t.Fatal("google account still linked")
	}

	// Google is not configured in tests: linking fails with a toast.
	res = settingsPost(u.c, "linked-accounts", url.Values{"action": {"link_google"}, "password": {pw}})
	wantRedirect(t, res, "/settings?tab=linked-accounts")
	wantToast(t, res, toast.ErrorType, "Login com Google não está configurado.")
}

func TestSettingsDeleteUser(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "delete-me")
	res := settingsPost(u.c, "danger", url.Values{"action": {"delete_user"}, "confirm_email": {u.email}})
	wantToast(t, res, toast.ErrorType, "Informe sua senha atual ou o código do autenticador para excluir a conta.")
	res = settingsPost(u.c, "danger", url.Values{"action": {"delete_user"}, "confirm_email": {u.email}, "password": {"wrong"}})
	wantToast(t, res, toast.ErrorType, "Senha incorreta.")
	res = settingsPost(u.c, "danger", url.Values{"action": {"delete_user"}, "confirm_email": {"other@example.com"}, "password": {pw}})
	wantToast(t, res, toast.ErrorType, "O e-mail informado não confere.")

	// Stale session: re-login required.
	exec(t, s.Pool, `UPDATE sessions SET created_at = now() - interval '31 minutes' WHERE user_id = $1`, u.id)
	res = settingsPost(u.c, "danger", url.Values{"action": {"delete_user"}, "confirm_email": {u.email}, "password": {pw}})
	wantToast(t, res, toast.ErrorType, "Sessão expirada para esta ação. Entre novamente e tente outra vez.")
	exec(t, s.Pool, `UPDATE sessions SET created_at = now() WHERE user_id = $1`, u.id)

	res = settingsPost(u.c, "danger", url.Values{"action": {"delete_user"}, "confirm_email": {u.email}, "password": {pw}})
	wantRedirect(t, res, "/")
	if n := scalar[int64](t, s.Pool, `SELECT COUNT(*) FROM users WHERE id = $1`, u.id); n != 0 {
		t.Fatal("user not deleted")
	}
	if sessionEmail(t, u.c) != "" {
		t.Fatal("session survived deletion")
	}
}

func TestSettingsDeleteOriginCheckInProduction(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{Prod: true})
	c := s.Client(t)
	email := testutil.UniqueEmail("delete-prod")
	wantStatus(t, c.SignUp("Prod", email, pw), http.StatusOK)
	form := url.Values{"action": {"delete_user"}, "confirm_email": {email}, "password": {pw}}
	// Same-site POST without Origin/Referer: refused by the extra gate.
	res := c.PostForm("/settings?tab=danger", form)
	wantRedirect(t, res, "/settings?tab=danger")
	if n := scalar[int64](t, s.Pool, `SELECT COUNT(*) FROM users WHERE email = $1`, email); n != 1 {
		t.Fatal("deleted without an origin")
	}
	u, _ := url.Parse(s.URL)
	res = c.Do(http.MethodPost, "/settings?tab=danger", strings.NewReader(form.Encode()), http.Header{
		"Content-Type": {"application/x-www-form-urlencoded"}, "Referer": {"https://" + u.Host + "/settings?tab=danger"},
	})
	wantRedirect(t, res, "/")
	if n := scalar[int64](t, s.Pool, `SELECT COUNT(*) FROM users WHERE email = $1`, email); n != 0 {
		t.Fatal("same-origin delete failed")
	}
}

func pngBytes(t *testing.T, size int) []byte {
	t.Helper()
	img := image.NewRGBA(image.Rect(0, 0, size, size))
	for x := range size {
		for y := range size {
			img.Set(x, y, color.RGBA{uint8(x), uint8(y), uint8(x ^ y), 255})
		}
	}
	var b bytes.Buffer
	if err := png.Encode(&b, img); err != nil {
		t.Fatal(err)
	}
	return b.Bytes()
}

func multipartFile(t *testing.T, contentType string, data []byte) (string, *bytes.Buffer) {
	t.Helper()
	var body bytes.Buffer
	mw := multipart.NewWriter(&body)
	h := textproto.MIMEHeader{}
	h.Set("Content-Disposition", `form-data; name="file"; filename="avatar"`)
	h.Set("Content-Type", contentType)
	part, err := mw.CreatePart(h)
	if err != nil {
		t.Fatal(err)
	}
	if _, err := part.Write(data); err != nil {
		t.Fatal(err)
	}
	if err := mw.Close(); err != nil {
		t.Fatal(err)
	}
	return mw.FormDataContentType(), &body
}

func TestSettingsAvatarUploadAndRemove(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "avatar")
	ct, body := multipartFile(t, "image/png", pngBytes(t, 32))
	res := u.c.Do(http.MethodPost, "/settings/avatar", body, http.Header{"Content-Type": {ct}})
	wantRedirect(t, res, "/settings?tab=profile")
	wantToast(t, res, toast.SuccessType, "Alterações salvas com sucesso.")
	key := scalar[*string](t, s.Pool, `SELECT image FROM users WHERE id = $1`, u.id)
	if key == nil || !strings.HasPrefix(*key, "avatars/"+u.id.String()+"/") {
		t.Fatalf("image %v", key)
	}
	res = u.c.Get("/settings")
	wantContains(t, res, `value="remove_avatar"`)

	// Wrong type.
	ct, body = multipartFile(t, "image/gif", []byte("GIF89a-not-really"))
	res = u.c.Do(http.MethodPost, "/settings/avatar", body, http.Header{"Content-Type": {ct}})
	wantToast(t, res, toast.ErrorType, auth.MsgImageType)
	// Over the 2 MiB body cap.
	ct, body = multipartFile(t, "image/png", bytes.Repeat([]byte{0x89}, 2<<20+1))
	res = u.c.Do(http.MethodPost, "/settings/avatar", body, http.Header{"Content-Type": {ct}})
	wantStatus(t, res, http.StatusRequestEntityTooLarge)
	if got := scalar[*string](t, s.Pool, `SELECT image FROM users WHERE id = $1`, u.id); got == nil || *got != *key {
		t.Fatal("failed uploads must keep the avatar")
	}

	res = settingsPost(u.c, "profile", url.Values{"action": {"remove_avatar"}})
	wantToast(t, res, toast.SuccessType, "Alterações salvas com sucesso.")
	if got := scalar[*string](t, s.Pool, `SELECT image FROM users WHERE id = $1`, u.id); got != nil && *got != "" {
		t.Fatalf("image after remove %v", *got)
	}
}

func TestSettingsStepUpRateLimitedMessage(t *testing.T) {
	t.Setenv("RATE_LIMIT_AUTH_STEP_UP_PER_MIN", "1")
	s := testutil.NewServer(t, testutil.Options{KeepRateLimits: true})
	ratelimit.Reset()
	t.Cleanup(ratelimit.Reset)
	u := signUp(t, s, "stepup-rate")
	settingsPost(u.c, "account", url.Values{"action": {"change_password"}, "current_password": {"x"}, "new_password": {newPW}, "confirm_password": {newPW}})
	res := settingsPost(u.c, "account", url.Values{"action": {"change_password"}, "current_password": {"x"}, "new_password": {newPW}, "confirm_password": {newPW}})
	wantToast(t, res, toast.ErrorType, "Muitas tentativas. Aguarde um momento e tente novamente.")
}

// groupedSecret formats a base32 secret as the setup page shows it.
func groupedSecret(secret string) string {
	var b strings.Builder
	for i, r := range strings.ToUpper(secret) {
		if i > 0 && i%4 == 0 {
			b.WriteByte(' ')
		}
		b.WriteRune(r)
	}
	return b.String()
}
