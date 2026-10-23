package authpages_test

import (
	"net/http"
	"net/url"
	"strings"
	"testing"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

const (
	pw    = "Harbor-Violet-Tiger-4821-Lantern!"
	newPW = "Quilt-Mango-Brave-Pixel-7395?"
)

func wantStatus(t *testing.T, res *testutil.Response, want int) {
	t.Helper()
	if res.StatusCode != want {
		t.Fatalf("status %d, want %d: %s", res.StatusCode, want, res.Body)
	}
}

func wantRedirect(t *testing.T, res *testutil.Response, prefix string) {
	t.Helper()
	if res.StatusCode != http.StatusSeeOther || !strings.HasPrefix(res.Header.Get("Location"), prefix) {
		t.Fatalf("want 303 → %s, got %d → %q", prefix, res.StatusCode, res.Header.Get("Location"))
	}
}

// toasts decodes the flash cookie written by res.
func toasts(t *testing.T, res *testutil.Response, name string) []toast.Toast {
	t.Helper()
	line, ok := testutil.FindSetCookie(res.SetCookies(), name)
	if !ok {
		return nil
	}
	v, _, _ := strings.Cut(strings.TrimPrefix(line, name+"="), ";")
	return toast.DecodeQueue(v)
}

func wantToast(t *testing.T, res *testutil.Response, kind toast.Type, title string) {
	t.Helper()
	for _, tt := range toasts(t, res, testutil.DevToastCookie) {
		if tt.Type == kind && tt.Title == title {
			return
		}
	}
	t.Fatalf("missing %s toast %q in %v", kind, title, res.SetCookies())
}

type user struct {
	c     *testutil.Client
	id    uuid.UUID
	email string
}

func signUp(t *testing.T, s *testutil.Server, prefix string) user {
	t.Helper()
	c := s.Client(t)
	email := testutil.UniqueEmail(prefix)
	res := c.SignUp("Page User", email, pw)
	wantStatus(t, res, http.StatusOK)
	id := uuid.MustParse(res.JSON(t)["user"].(map[string]any)["id"].(string))
	return user{c: c, id: id, email: email}
}

func enable2FA(t *testing.T, u user) (string, []string) {
	t.Helper()
	res := u.c.PostJSON("/api/auth/two-factor/enable", map[string]any{"password": pw})
	wantStatus(t, res, http.StatusOK)
	j := res.JSON(t)
	secret := j["secret"].(string)
	var codes []string
	for _, c := range j["backupCodes"].([]any) {
		codes = append(codes, c.(string))
	}
	wantStatus(t, u.c.PostJSON("/api/auth/two-factor/verify-totp", map[string]any{"code": testutil.TOTPCode(t, secret)}), http.StatusOK)
	wantStatus(t, u.c.PostJSON("/api/auth/sign-out", map[string]any{}), http.StatusOK)
	return secret, codes
}

func sessionEmail(t *testing.T, c *testutil.Client) string {
	t.Helper()
	res := c.Get("/api/auth/get-session")
	if strings.TrimSpace(res.Body) == "null" {
		return ""
	}
	return res.JSON(t)["user"].(map[string]any)["email"].(string)
}

func exec(t *testing.T, pool *pgxpool.Pool, sql string, args ...any) {
	t.Helper()
	if _, err := pool.Exec(t.Context(), sql, args...); err != nil {
		t.Fatal(err)
	}
}

// --- /login ---

func TestLoginPageRendersBothModes(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	c := s.Client(t)
	in := c.Get("/login?email=a%40b.com&next=%2Fconta")
	wantStatus(t, in, http.StatusOK)
	for _, want := range []string{
		"Entre na Linda Flor", "Continuar com Google", `action="/login"`, `value="a@b.com"`, `name="next" value="/conta"`,
		"Esqueceu a senha?", "Criar conta", `name="callback_url" value="/conta"`, `hx-post="/login"`, "noindex",
	} {
		if !strings.Contains(in.Body, want) {
			t.Errorf("signin page missing %q", want)
		}
	}
	up := c.Get("/login?mode=signup&name=%3Cb%3EAna&invite_id=inv-1")
	wantStatus(t, up, http.StatusOK)
	for _, want := range []string{
		"Cadastre-se na Linda Flor", `action="/signup"`, `name="invite_id" value="inv-1"`, "Já tem uma conta?",
		`value="/accept-invitation?id=inv-1"`, "&lt;b&gt;Ana",
	} {
		if !strings.Contains(up.Body, want) {
			t.Errorf("signup page missing %q", want)
		}
	}
	if strings.Contains(up.Body, "<b>Ana") {
		t.Fatal("query values must be escaped")
	}
	// Open redirects never reach the Google callback.
	evil := c.Get("/login?next=https%3A%2F%2Fevil.test")
	if !strings.Contains(evil.Body, `name="callback_url" value="/dashboard"`) {
		t.Fatal("unsafe next must fall back to /dashboard")
	}
}

func TestHTMLSignInSuccessAndFailure(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "html-signin")
	c := s.Client(t)
	bad := c.PostForm("/login", url.Values{"email": {u.email}, "password": {"wrong-password-1!"}, "next": {"/conta"}})
	wantRedirect(t, bad, "/login?")
	loc, _ := url.Parse(bad.Header.Get("Location"))
	if loc.Query().Get("mode") != "signin" || loc.Query().Get("email") != u.email || loc.Query().Get("next") != "/conta" || loc.Query().Has("password") {
		t.Fatalf("error redirect keeps the context: %s", loc)
	}
	wantToast(t, bad, toast.ErrorType, "Credenciais inválidas.")
	ok := c.PostForm("/login", url.Values{"email": {u.email}, "password": {pw}, "next": {"/conta"}})
	wantRedirect(t, ok, "/conta")
	wantToast(t, ok, toast.SuccessType, "Bem-vinda de volta.")
	if sessionEmail(t, c) != u.email {
		t.Fatal("signed in")
	}
	evil := s.Client(t).PostForm("/login", url.Values{"email": {u.email}, "password": {pw}, "next": {"//evil.test"}})
	wantRedirect(t, evil, "/dashboard")
}

func TestHTMXSignInAnswersWithHXLocation(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "htmx-signin")
	c := s.Client(t).HTMX()
	res := c.PostForm("/login", url.Values{"email": {u.email}, "password": {pw}})
	wantStatus(t, res, http.StatusOK)
	if res.Header.Get("HX-Location") != "/dashboard" || res.Header.Get("X-Toast") == "" {
		t.Fatalf("htmx answer: %v", res.Header)
	}
	if _, ok := testutil.FindSetCookie(res.SetCookies(), testutil.DevToastCookie); ok {
		t.Fatal("htmx toasts travel on the header, not the cookie")
	}
}

func TestHTMLSignupCreatesAccount(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	c := s.Client(t)
	email := testutil.UniqueEmail("html-signup")
	weak := c.PostForm("/signup", url.Values{"name": {"Ana"}, "email": {email}, "password": {"password123"}})
	wantRedirect(t, weak, "/login?")
	wantToast(t, weak, toast.ErrorType, "Escolha uma senha mais forte.")
	if l := weak.Header.Get("Location"); !strings.Contains(l, "mode=signup") || !strings.Contains(l, "name=Ana") {
		t.Fatal(l)
	}
	res := c.PostForm("/signup", url.Values{"name": {"Ana"}, "email": {email}, "password": {pw}, "invite_id": {"abc"}})
	wantRedirect(t, res, "/accept-invitation?id=abc")
	wantToast(t, res, toast.SuccessType, "Conta criada.")
	if sessionEmail(t, c) != email {
		t.Fatal("signed in after sign-up")
	}
	dup := s.Client(t).PostForm("/signup", url.Values{"name": {"Ana"}, "email": {email}, "password": {pw}})
	wantRedirect(t, dup, "/login?")
	wantToast(t, dup, toast.ErrorType, "Não foi possível concluir a operação. Tente novamente.")
}

func TestLoginHooksRunOnHTMLLoginOnly(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	var seen []uuid.UUID
	t.Cleanup(auth.RegisterLoginHook(func(w http.ResponseWriter, _ *http.Request, _ *pgxpool.Pool, id uuid.UUID) error {
		seen = append(seen, id)
		auth.SetCookie(w, "hook_ran", "1", time.Minute, false)
		return nil
	}))
	u := signUp(t, s, "hooks") // API sign-up: no hook
	if len(seen) != 0 {
		t.Fatal("API sign-up must not run login hooks")
	}
	res := s.Client(t).PostForm("/login", url.Values{"email": {u.email}, "password": {pw}})
	wantRedirect(t, res, "/dashboard")
	if len(seen) != 1 || seen[0] != u.id {
		t.Fatalf("hook runs once for the user: %v", seen)
	}
	if _, ok := testutil.FindSetCookie(res.SetCookies(), "hook_ran"); !ok {
		t.Fatal("hooks can write cookies")
	}
}

// --- /two-factor ---

func TestHTMLTwoFactorFlow(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "html-2fa")
	secret, codes := enable2FA(t, u)
	c := s.Client(t)
	expired := c.Get("/two-factor")
	wantStatus(t, expired, http.StatusOK)
	if !strings.Contains(expired.Body, "Sessão expirada") {
		t.Fatal("no pending cookie → expired card")
	}
	wantRedirect(t, c.PostForm("/two-factor", url.Values{"code": {"123456"}}), "/login")

	challenged := c.PostForm("/login", url.Values{"email": {u.email}, "password": {pw}, "next": {"/login"}})
	wantRedirect(t, challenged, "/two-factor")
	if _, ok := testutil.FindSetCookie(challenged.SetCookies(), testutil.SessionCookie); ok {
		t.Fatal("no session before the second factor")
	}
	page := c.Get("/two-factor")
	if !strings.Contains(page.Body, "Verificação em duas etapas") || !strings.Contains(page.Body, `autocomplete="one-time-code"`) {
		t.Fatal("totp tab")
	}
	if b := c.Get("/two-factor?method=backup"); !strings.Contains(b.Body, `name="backup_code"`) {
		t.Fatal("backup tab")
	}
	wrong := c.PostForm("/two-factor", url.Values{"code": {"000000"}})
	wantRedirect(t, wrong, "/two-factor")
	wantToast(t, wrong, toast.ErrorType, "Credenciais inválidas.")
	wrongBackup := c.PostForm("/two-factor?method=backup", url.Values{"backup_code": {"0000-0000"}})
	wantRedirect(t, wrongBackup, "/two-factor?method=backup")
	ok := c.PostForm("/two-factor?method=backup", url.Values{"backup_code": {codes[0]}})
	wantRedirect(t, ok, "/dashboard")
	wantToast(t, ok, toast.SuccessType, "Verificado.")
	if line, found := testutil.FindSetCookie(ok.SetCookies(), testutil.DevPending2FACookie); !found || !strings.Contains(line, "Max-Age=0") {
		t.Fatal("pending cookie cleared")
	}
	if sessionEmail(t, c) != u.email {
		t.Fatal("signed in")
	}
	// TOTP path in another browser.
	c2 := s.Client(t)
	wantRedirect(t, c2.PostForm("/login", url.Values{"email": {u.email}, "password": {pw}}), "/two-factor")
	wantRedirect(t, c2.PostForm("/two-factor", url.Values{"code": {testutil.TOTPCodeAt(t, secret, time.Now().Add(30*time.Second))}}), "/dashboard")
	if sessionEmail(t, c2) != u.email {
		t.Fatal("signed in with TOTP")
	}
}

// --- /forgot-password, /reset-password ---

func TestHTMLForgotAndResetPassword(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "html-reset")
	c := s.Client(t)
	if p := c.Get("/forgot-password"); !strings.Contains(p.Body, "Esqueceu a senha") || !strings.Contains(p.Body, `action="/forgot-password"`) {
		t.Fatal("forgot page")
	}
	wantRedirect(t, c.PostForm("/forgot-password", url.Values{"email": {"nobody@example.com"}}), "/forgot-password/sent")
	wantRedirect(t, c.PostForm("/forgot-password", url.Values{"email": {u.email}}), "/forgot-password/sent")
	if p := c.Get("/forgot-password/sent"); !strings.Contains(p.Body, "Link enviado") {
		t.Fatal("sent page")
	}
	if p := c.Get("/reset-password"); !strings.Contains(p.Body, "Link inválido") {
		t.Fatal("no token → invalid link card")
	}
	token := s.VerificationTokenLike(t, "reset-password:"+u.email)
	if p := c.Get("/reset-password?token=" + token); !strings.Contains(p.Body, `name="token" value="`+token+`"`) {
		t.Fatal("token carried in the form")
	}
	mismatch := c.PostForm("/reset-password", url.Values{"token": {token}, "new_password": {newPW}, "confirm_password": {"x"}})
	wantRedirect(t, mismatch, "/reset-password?token=")
	wantToast(t, mismatch, toast.ErrorType, "As senhas não conferem.")
	bad := c.PostForm("/reset-password", url.Values{"token": {"bogus"}, "new_password": {newPW}, "confirm_password": {newPW}})
	wantToast(t, bad, toast.ErrorType, "Link inválido ou expirado.")
	ok := c.PostForm("/reset-password", url.Values{"token": {token}, "new_password": {newPW}, "confirm_password": {newPW}})
	wantRedirect(t, ok, "/login?mode=signin")
	wantToast(t, ok, toast.SuccessType, "Senha redefinida.")
	if sessionEmail(t, u.c) != "" {
		t.Fatal("reset revokes sessions")
	}
	wantStatus(t, s.Client(t).SignIn(u.email, newPW), http.StatusOK)
}

// forgot_password_post_is_scoped_but_page_is_not (tests/security_misc.rs)
func TestForgotPasswordPostIsScopedButPageIsNot(t *testing.T) {
	ratelimit.Reset()
	t.Cleanup(ratelimit.Reset)
	t.Setenv("RATE_LIMIT_AUTH_PASSWORD_RESET_PER_MIN", "2")
	s := testutil.NewServer(t, testutil.Options{KeepRateLimits: true})
	c := s.Client(t)
	for range 4 {
		if res := c.Get("/forgot-password"); res.StatusCode == http.StatusTooManyRequests {
			t.Fatal("GET pages never touch the strict POST budget")
		}
	}
	form := url.Values{"email": {"nobody@example.com"}}
	for i := range 2 {
		if res := c.PostForm("/forgot-password", form); res.StatusCode != http.StatusSeeOther {
			t.Fatalf("POST %d within budget: %d", i+1, res.StatusCode)
		}
	}
	wantStatus(t, c.PostForm("/forgot-password", form), http.StatusTooManyRequests)
	// Shared bucket with the JSON twin.
	wantStatus(t, c.PostForm("/api/auth/request-password-reset", form), http.StatusTooManyRequests)
}

func TestHTMLLoginRateLimitAnswersWithToast(t *testing.T) {
	ratelimit.Reset()
	t.Cleanup(ratelimit.Reset)
	t.Setenv("RATE_LIMIT_AUTH_SIGN_IN_PER_MIN", "1")
	s := testutil.NewServer(t, testutil.Options{KeepRateLimits: true})
	c := s.Client(t)
	form := url.Values{"email": {"nobody@example.com"}, "password": {"whatever-123"}}
	wantRedirect(t, c.PostForm("/login", form), "/login?")
	limited := c.PostForm("/login", form)
	wantRedirect(t, limited, "/login?")
	wantToast(t, limited, toast.ErrorType, "Muitas tentativas. Aguarde um momento e tente novamente.")
}

// --- /verify-email, /check-email ---

func TestVerifyEmailPageStates(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "html-verify")
	c := u.c
	if p := c.Get("/verify-email"); !strings.Contains(p.Body, "Verificar e-mail") || !strings.Contains(p.Body, `value="`+u.email+`"`) {
		t.Fatal("prompt prefilled with the session email")
	}
	if p := s.Client(t).Get("/verify-email?token=bogus"); !strings.Contains(p.Body, "Falha na verificação") {
		t.Fatal("bad token")
	}
	wantRedirect(t, c.PostForm("/verify-email/resend", url.Values{}), "/verify-email?error=resent")
	if p := c.Get("/verify-email?error=resent"); !strings.Contains(p.Body, "Link reenviado") {
		t.Fatal("resent state")
	}
	wantRedirect(t, s.Client(t).PostForm("/verify-email/resend", url.Values{}), "/verify-email?error=1")
	token := s.VerificationTokenLike(t, "email-verification:"+u.email)
	if p := s.Client(t).Get("/verify-email?token=" + token); !strings.Contains(p.Body, "E-mail verificado") {
		t.Fatal("verified state")
	}
	var verified bool
	if err := s.Pool.QueryRow(t.Context(), `SELECT email_verified FROM users WHERE id = $1`, u.id).Scan(&verified); err != nil || !verified {
		t.Fatal("email verified")
	}
}

func TestCheckEmailPageAndResend(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "html-check")
	c := s.Client(t)
	if p := c.Get("/check-email?email=" + url.QueryEscape(u.email)); !strings.Contains(p.Body, "Verifique sua caixa de entrada") || !strings.Contains(p.Body, "<strong>"+u.email+"</strong>") {
		t.Fatal("check page")
	}
	res := c.PostForm("/check-email/resend", url.Values{"email": {u.email}})
	wantRedirect(t, res, "/check-email?")
	if l := res.Header.Get("Location"); !strings.Contains(l, "resent=1") {
		t.Fatal(l)
	}
	if p := c.Get("/check-email?resent=1"); !strings.Contains(p.Body, "E-mail reenviado") {
		t.Fatal("resent card")
	}
	wantRedirect(t, c.PostForm("/check-email/resend", url.Values{}), "/check-email")
	var n int64
	if err := s.Pool.QueryRow(t.Context(), `SELECT count(*) FROM email_outbox WHERE to_address = $1 AND kind = 'verify_email'`, u.email).Scan(&n); err != nil || n != 1 {
		t.Fatalf("verify mail queued: %d %v", n, err)
	}
}

// --- /confirmar-email, /excluir-conta ---

func TestConfirmarEmailPage(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "html-confirm")
	newEmail := testutil.UniqueEmail("html-confirm-new")
	wantStatus(t, u.c.PostJSON("/api/auth/change-email", map[string]any{"newEmail": newEmail, "password": pw}), http.StatusOK)
	token := s.VerificationTokenLike(t, "change-email:%:"+newEmail)
	c := s.Client(t)
	if p := c.Get("/confirmar-email"); !strings.Contains(p.Body, "Link inválido") {
		t.Fatal("no token")
	}
	// GET never consumes the token (link scanners).
	if p := c.Get("/confirmar-email?token=" + token); !strings.Contains(p.Body, "Confirmar novo e-mail") {
		t.Fatal("confirm page")
	}
	bad := c.PostForm("/confirmar-email", url.Values{"token": {"bogus"}})
	wantRedirect(t, bad, "/confirmar-email")
	wantToast(t, bad, toast.ErrorType, "Link inválido ou expirado.")
	ok := c.PostForm("/confirmar-email", url.Values{"token": {token}})
	wantRedirect(t, ok, "/login?mode=signin")
	wantToast(t, ok, toast.SuccessType, "E-mail alterado. Entre novamente.")
	if sessionEmail(t, u.c) != "" {
		t.Fatal("all sessions revoked")
	}
	wantStatus(t, s.Client(t).SignIn(newEmail, pw), http.StatusOK)
}

func TestExcluirContaPage(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "html-delete")
	wantStatus(t, u.c.PostJSON("/api/auth/delete-user", map[string]any{"password": pw}), http.StatusOK)
	token := s.VerificationTokenLike(t, "delete-user:"+u.id.String())
	if p := u.c.Get("/excluir-conta?token=" + token); !strings.Contains(p.Body, "Excluir minha conta") {
		t.Fatal("delete page")
	}
	if p := u.c.Get("/excluir-conta"); !strings.Contains(p.Body, "Link inválido") {
		t.Fatal("no token")
	}
	wantRedirect(t, u.c.PostForm("/excluir-conta", url.Values{}), "/excluir-conta")
	ok := u.c.PostForm("/excluir-conta", url.Values{"token": {token}})
	wantRedirect(t, ok, "/")
	wantToast(t, ok, toast.SuccessType, "Conta excluída.")
	var exists bool
	if err := s.Pool.QueryRow(t.Context(), `SELECT EXISTS (SELECT 1 FROM users WHERE id = $1)`, u.id).Scan(&exists); err != nil || exists {
		t.Fatal("user deleted")
	}
}

// --- POST /logout, POST /stop-impersonating ---

// logout_expires_cart_cookie (tests/auth_sessions.rs)
func TestLogoutExpiresCartCookie(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	u := signUp(t, s, "logout-cart")
	res := u.c.Do(http.MethodPost, "/logout", strings.NewReader("redirect=%2F"), http.Header{
		"Content-Type": {"application/x-www-form-urlencoded"},
		"Cookie":       {testutil.DevCartCookie + "=" + uuid.NewString()},
	})
	wantRedirect(t, res, "/")
	cleared, ok := testutil.FindSetCookie(res.SetCookies(), testutil.DevCartCookie)
	if !ok {
		t.Fatal("logout must expire the cart cookie")
	}
	testutil.AssertExpiredHardenedCookieDev(t, cleared, testutil.DevCartCookie)
	if sessionEmail(t, u.c) != "" {
		t.Fatal("signed out")
	}
	var n int64
	if err := s.Pool.QueryRow(t.Context(), `SELECT count(*) FROM sessions WHERE user_id = $1`, u.id).Scan(&n); err != nil || n != 0 {
		t.Fatal("session row deleted")
	}
	evil := s.Client(t).PostForm("/logout", url.Values{"redirect": {"https://evil.test"}})
	wantRedirect(t, evil, "/")
	if evil.Header.Get("Location") != "/" {
		t.Fatal("open redirect")
	}
}

func TestStopImpersonatingRestoresTheAdmin(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	admin := signUp(t, s, "html-imp-admin")
	exec(t, s.Pool, `UPDATE users SET role = 'admin' WHERE id = $1`, admin.id)
	target := signUp(t, s, "html-imp-target")
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/impersonate-user", map[string]any{"userId": target.id.String()}), http.StatusOK)
	if sessionEmail(t, admin.c) != target.email {
		t.Fatal("impersonating")
	}
	if home := admin.c.Get("/login"); !strings.Contains(home.Body, "/stop-impersonating") {
		t.Fatal("impersonation banner")
	}
	res := admin.c.PostForm("/stop-impersonating", url.Values{"redirect": {"/admin/usuarios"}})
	wantRedirect(t, res, "/admin/usuarios")
	if sessionEmail(t, admin.c) != admin.email {
		t.Fatal("back to the admin")
	}
	// Not impersonating: a no-op redirect.
	wantRedirect(t, admin.c.PostForm("/stop-impersonating", url.Values{}), "/dashboard")
}

// --- tests/prod_cookies.rs (HTML half) ---

func TestProdCookiesSecure(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{Prod: true})
	c := s.Client(t)
	email := testutil.UniqueEmail("prod-cookies")
	res := c.PostForm("/signup", url.Values{"name": {"Prod Cookies"}, "email": {email}, "password": {pw}, "next": {"/login"}})
	wantRedirect(t, res, "/login")
	cookies := res.SetCookies()
	for _, pair := range [][2]string{{testutil.ProdSessionCookie, "session"}, {testutil.ProdToastCookie, "toast"}} {
		line, ok := testutil.FindSetCookie(cookies, pair[0])
		if !ok {
			t.Fatalf("sign-up: expected %s", pair[0])
		}
		testutil.AssertHardenedCookie(t, line, pair[0])
		if _, bare := testutil.FindSetCookie(cookies, pair[1]); bare {
			t.Fatalf("production must not set the bare %s cookie", pair[1])
		}
	}
	// The cart cookie is written by the store's login hook (wave 1C).
	if auth.LoginHookCount() > 0 {
		line, ok := testutil.FindSetCookie(cookies, testutil.ProdCartCookie)
		if !ok {
			t.Fatal("sign-up: expected the cart cookie")
		}
		testutil.AssertHardenedCookie(t, line, testutil.ProdCartCookie)
	}

	u := signUp(t, s, "prod-pending")
	enable2FA(t, u)
	login := s.Client(t).PostForm("/login", url.Values{"email": {u.email}, "password": {pw}, "next": {"/login"}})
	wantRedirect(t, login, "/two-factor")
	pending, ok := testutil.FindSetCookie(login.SetCookies(), testutil.ProdPending2FACookie)
	if !ok {
		t.Fatal("2fa challenge: expected the pending cookie")
	}
	testutil.AssertHardenedCookie(t, pending, testutil.ProdPending2FACookie)
	if _, bare := testutil.FindSetCookie(login.SetCookies(), testutil.DevPending2FACookie); bare {
		t.Fatal("no bare pending cookie in production")
	}

	failed := s.Client(t).PostForm("/login", url.Values{"email": {"prod-toast@example.com"}, "password": {"wrong-password-1!"}, "next": {"/login"}})
	wantRedirect(t, failed, "/login?")
	toastLine, ok := testutil.FindSetCookie(failed.SetCookies(), testutil.ProdToastCookie)
	if !ok {
		t.Fatal("failed login: expected the toast cookie")
	}
	testutil.AssertHardenedCookie(t, toastLine, testutil.ProdToastCookie)
}

// prod_logout_expires_hardened_cookies
func TestProdLogoutExpiresHardenedCookies(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{Prod: true})
	u := signUp(t, s, "prod-logout")
	res := u.c.Do(http.MethodPost, "/logout", strings.NewReader("redirect=%2F"), http.Header{
		"Content-Type": {"application/x-www-form-urlencoded"},
		"Cookie":       {testutil.ProdCartCookie + "=" + uuid.NewString()},
	})
	wantStatus(t, res, http.StatusSeeOther)
	for _, name := range []string{testutil.ProdCartCookie, testutil.ProdSessionCookie} {
		line, ok := testutil.FindSetCookie(res.SetCookies(), name)
		if !ok {
			t.Fatalf("logout: expected %s", name)
		}
		testutil.AssertExpiredHardenedCookie(t, line, name)
	}
}

// The /settings HTML tests (tests/auth_sessions.rs, auth_hardening.rs,
// auth_two_factor.rs) live with the settings page in internal/web/account.
