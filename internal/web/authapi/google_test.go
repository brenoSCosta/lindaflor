package authapi_test

import (
	"encoding/json"
	"net/http"
	"net/http/httptest"
	"net/url"
	"slices"
	"strings"
	"sync"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// --- tests/auth_google.rs: Google callback against a fake token/userinfo
// server (GOOGLE_OAUTH_TEST_BASE_URL, honored outside production). ---

type fakeGoogle struct {
	*httptest.Server
	mu       sync.Mutex
	profiles map[string]map[string]any // code → userinfo
	verifier map[string]string         // code → code_verifier presented
}

func newFakeGoogle(t *testing.T) *fakeGoogle {
	t.Helper()
	g := &fakeGoogle{profiles: map[string]map[string]any{}, verifier: map[string]string{}}
	g.Server = httptest.NewServer(http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "application/json")
		switch {
		case r.Method == http.MethodPost && r.URL.Path == "/token":
			_ = r.ParseForm()
			code := r.PostFormValue("code")
			g.mu.Lock()
			g.verifier[code] = r.PostFormValue("code_verifier")
			g.mu.Unlock()
			_ = json.NewEncoder(w).Encode(map[string]any{
				"access_token": "at-" + code, "refresh_token": "rt-" + code, "id_token": "it-" + code,
				"expires_in": 3600, "token_type": "Bearer",
			})
		case r.Method == http.MethodGet && r.URL.Path == "/userinfo":
			code := strings.TrimPrefix(strings.TrimPrefix(r.Header.Get("Authorization"), "Bearer "), "at-")
			g.mu.Lock()
			p, ok := g.profiles[code]
			g.mu.Unlock()
			if !ok {
				w.WriteHeader(http.StatusUnauthorized)
				_, _ = w.Write([]byte("{}"))
				return
			}
			_ = json.NewEncoder(w).Encode(p)
		default:
			w.WriteHeader(http.StatusNotFound)
		}
	}))
	t.Cleanup(g.Close)
	return g
}

// profile registers the userinfo Google "returns" for a new code.
func (g *fakeGoogle) profile(sub, email string, verified bool) string {
	code := "code" + strings.ReplaceAll(uuid.NewString(), "-", "")
	g.mu.Lock()
	defer g.mu.Unlock()
	g.profiles[code] = map[string]any{"sub": sub, "email": email, "email_verified": verified, "name": "Google Person"}
	return code
}

func googleServer(t *testing.T) (*testutil.Server, *fakeGoogle) {
	t.Helper()
	g := newFakeGoogle(t)
	s := testutil.NewServer(t, testutil.Options{Mutate: func(c *config.Config) {
		c.Google = config.GoogleConfig{ClientID: "test-client.apps.googleusercontent.com", ClientSecret: "test-secret"}
		c.AppOrigin = "http://127.0.0.1:4200"
		c.GoogleOAuthTestBase = g.URL
	}})
	return s, g
}

func stateFrom(t *testing.T, authorize string) string {
	t.Helper()
	u, err := url.Parse(authorize)
	if err != nil {
		t.Fatal(err)
	}
	return u.Query().Get("state")
}

// startLogin begins a login flow in c's browser; returns the state.
func startLogin(t *testing.T, c *testutil.Client) string {
	t.Helper()
	res := c.PostJSON("/api/auth/sign-in/social", map[string]any{"provider": "google"})
	wantStatus(t, res, http.StatusOK)
	if c.Cookie("oauth_bind") == "" {
		t.Fatal("oauth_bind cookie on start")
	}
	return stateFrom(t, res.JSON(t)["url"].(string))
}

func callback(c *testutil.Client, code, state string) *testutil.Response {
	return c.Get("/api/auth/callback/google?code=" + code + "&state=" + state)
}

func userIDByEmail(t *testing.T, s *testutil.Server, email string) *uuid.UUID {
	t.Helper()
	var id uuid.UUID
	if err := s.Pool.QueryRow(t.Context(), `SELECT id FROM users WHERE email = $1`, email).Scan(&id); err != nil {
		return nil
	}
	return &id
}

func TestGoogleLoginCreatesSessionInTheStartingBrowser(t *testing.T) {
	s, g := googleServer(t)
	email := testutil.UniqueEmail("google-new")
	code := g.profile("sub-"+uuid.NewString(), email, true)
	c := s.Client(t)
	state := startLogin(t, c)
	res := callback(c, code, state)
	if res.StatusCode != http.StatusSeeOther || res.Header.Get("Location") != "/dashboard" {
		t.Fatalf("%d %s", res.StatusCode, res.Header.Get("Location"))
	}
	if _, ok := testutil.FindSetCookie(res.SetCookies(), testutil.SessionCookie); !ok {
		t.Fatal("expected a session")
	}
	if userIDByEmail(t, s, email) == nil {
		t.Fatal("user created")
	}
	if sessionEmail(t, c) != email {
		t.Fatal("signed in as the Google user")
	}
	// The bind cookie is single use.
	if line, ok := testutil.FindSetCookie(res.SetCookies(), "oauth_bind"); !ok || !strings.Contains(line, "Max-Age=0") {
		t.Fatal("bind cookie cleared on callback")
	}
	if !scalar[bool](t, s, `SELECT email_verified FROM users WHERE email = $1`, email) {
		t.Fatal("verified Google address creates a verified user")
	}
}

func TestGoogleLoginStoresNoProviderTokens(t *testing.T) {
	s, g := googleServer(t)
	email := testutil.UniqueEmail("google-notokens")
	sub := "sub-" + uuid.NewString()
	code := g.profile(sub, email, true)
	c := s.Client(t)
	res := c.PostJSON("/api/auth/sign-in/social", map[string]any{"provider": "google", "callbackURL": "/conta"})
	wantStatus(t, res, http.StatusOK)
	authorize := res.JSON(t)["url"].(string)
	if !strings.Contains(authorize, "code_challenge=") || strings.Contains(authorize, "nonce=") {
		t.Fatal(authorize)
	}
	cb := callback(c, code, stateFrom(t, authorize))
	if cb.StatusCode != http.StatusSeeOther || cb.Header.Get("Location") != "/conta" {
		t.Fatalf("%d %s", cb.StatusCode, cb.Header.Get("Location"))
	}
	// PKCE: the stored verifier is what reached the token endpoint.
	g.mu.Lock()
	verifier := g.verifier[code]
	g.mu.Unlock()
	if verifier == "" {
		t.Fatal("code_verifier sent at token exchange")
	}
	again := s.Client(t)
	cb = callback(again, code, startLogin(t, again))
	wantStatus(t, cb, http.StatusSeeOther)
	var at, rt, it *string
	var expNull bool
	if err := s.Pool.QueryRow(t.Context(), `SELECT access_token, refresh_token, id_token, access_token_expires_at IS NULL
		FROM accounts WHERE provider_id = 'google' AND account_id = $1`, sub).Scan(&at, &rt, &it, &expNull); err != nil {
		t.Fatal(err)
	}
	if at != nil || rt != nil || it != nil || !expNull {
		t.Fatal("provider tokens must never be stored")
	}
}

// migration_0026_clears_stored_google_tokens: the consolidated Go schema
// has no legacy rows to scrub; the token columns exist only as NULLs (see
// TestGoogleLoginStoresNoProviderTokens). Kept as a schema guard.
func TestMigration0026ClearsStoredGoogleTokens(t *testing.T) {
	s, _ := googleServer(t)
	if n := scalar[int64](t, s, `SELECT count(*) FROM accounts WHERE access_token IS NOT NULL OR refresh_token IS NOT NULL OR id_token IS NOT NULL`); n != 0 {
		t.Fatalf("%d rows carry provider tokens", n)
	}
}

func TestGoogleCallbackWithoutTheBindingCookieIsRejected(t *testing.T) {
	s, g := googleServer(t)
	email := testutil.UniqueEmail("google-csrf")
	code := g.profile("sub-"+uuid.NewString(), email, true)
	attacker := s.Client(t)
	state := startLogin(t, attacker)
	victim := s.Client(t)
	res := callback(victim, code, state)
	wantStatus(t, res, http.StatusBadRequest)
	if _, ok := testutil.FindSetCookie(res.SetCookies(), testutil.SessionCookie); ok {
		t.Fatal("no session")
	}
	attacker2 := s.Client(t)
	state = startLogin(t, attacker2)
	victim2 := s.Client(t)
	_ = startLogin(t, victim2) // victim holds a bind cookie of another flow
	res = callback(victim2, code, state)
	wantStatus(t, res, http.StatusBadRequest)
	if _, ok := testutil.FindSetCookie(res.SetCookies(), testutil.SessionCookie); ok {
		t.Fatal("no session")
	}
	if userIDByEmail(t, s, email) != nil {
		t.Fatal("no user created")
	}
	// The state is single use.
	wantStatus(t, callback(attacker, code, "bogus-state"), http.StatusBadRequest)
}

func TestGoogleLoginForA2FAUserGoesToThe2FAStep(t *testing.T) {
	s, g := googleServer(t)
	a := signUp(t, s, "google-2fa", "Two Factor")
	exec(t, s, `UPDATE users SET email_verified = true WHERE id = $1`, a.id)
	enable2FA(t, a)
	code := g.profile("sub-"+uuid.NewString(), a.email, true)
	c := s.Client(t)
	res := callback(c, code, startLogin(t, c))
	wantStatus(t, res, http.StatusSeeOther)
	if _, ok := testutil.FindSetCookie(res.SetCookies(), testutil.SessionCookie); ok {
		t.Fatal("2FA user must not get a session from Google alone")
	}
	if _, ok := testutil.FindSetCookie(res.SetCookies(), testutil.DevPending2FACookie); !ok {
		t.Fatal("pending 2FA cookie")
	}
	if !strings.Contains(res.Header.Get("Location"), "/two-factor") {
		t.Fatal(res.Header.Get("Location"))
	}
}

func TestGoogleLoginTakesOverAnUnverifiedSquattedAccount(t *testing.T) {
	s, g := googleServer(t)
	squat := signUp(t, s, "google-victim", "Attacker")
	code := g.profile("sub-"+uuid.NewString(), squat.email, true)
	c := s.Client(t)
	res := callback(c, code, startLogin(t, c))
	wantStatus(t, res, http.StatusSeeOther)
	if _, ok := testutil.FindSetCookie(res.SetCookies(), testutil.SessionCookie); !ok {
		t.Fatal("owner gets a session")
	}
	if id := userIDByEmail(t, s, squat.email); id == nil || *id != squat.id {
		t.Fatal("same row is handed over")
	}
	if sessionEmail(t, squat.c) != nil {
		t.Fatal("attacker session revoked")
	}
	_, relogin := signIn(t, s, squat.email, strongPW)
	wantStatus(t, relogin, http.StatusUnauthorized)
	if !scalar[bool](t, s, `SELECT email_verified FROM users WHERE id = $1`, squat.id) {
		t.Fatal("verified")
	}
	if n := scalar[string](t, s, `SELECT name FROM users WHERE id = $1`, squat.id); n != "Google Person" {
		t.Fatal(n)
	}
	rows, _ := s.Pool.Query(t.Context(), `SELECT provider_id FROM accounts WHERE user_id = $1`, squat.id)
	var providers []string
	for rows.Next() {
		var p string
		_ = rows.Scan(&p)
		providers = append(providers, p)
	}
	rows.Close()
	if !slices.Equal(providers, []string{"google"}) {
		t.Fatal(providers)
	}
}

func TestGoogleUnverifiedEmailNeverLinksOrCreates(t *testing.T) {
	s, g := googleServer(t)
	victim := signUp(t, s, "google-unverified", "Victim")
	code := g.profile("sub-"+uuid.NewString(), victim.email, false)
	c := s.Client(t)
	wantStatus(t, callback(c, code, startLogin(t, c)), http.StatusBadRequest)
	fresh := testutil.UniqueEmail("google-unverified-new")
	code = g.profile("sub-"+uuid.NewString(), fresh, false)
	c2 := s.Client(t)
	wantStatus(t, callback(c2, code, startLogin(t, c2)), http.StatusBadRequest)
	if userIDByEmail(t, s, fresh) != nil {
		t.Fatal("unverified Google email must not create a user")
	}
	if sessionEmail(t, victim.c) != victim.email {
		t.Fatal("victim untouched")
	}
}

func TestLinkingGoogleWithADifferentEmailDoesNotVerifyLocalEmail(t *testing.T) {
	s, g := googleServer(t)
	a := signUp(t, s, "google-link-local", "Linker")
	code := g.profile("sub-"+uuid.NewString(), testutil.UniqueEmail("google-link-other"), true)
	res := a.c.PostJSON("/api/auth/link-social", map[string]any{"provider": "google", "password": strongPW})
	wantStatus(t, res, http.StatusOK)
	cb := callback(a.c, code, stateFrom(t, res.JSON(t)["url"].(string)))
	wantStatus(t, cb, http.StatusSeeOther)
	if n := scalar[int64](t, s, `SELECT count(*) FROM accounts WHERE user_id = $1 AND provider_id = 'google'`, a.id); n != 1 {
		t.Fatal(n)
	}
	if scalar[bool](t, s, `SELECT email_verified FROM users WHERE id = $1`, a.id) {
		t.Fatal("a different Google address must not verify this one")
	}
}

func TestLinkFlowMustCompleteInTheStartingSession(t *testing.T) {
	s, g := googleServer(t)
	a := signUp(t, s, "google-link-csrf", "Linker")
	code := g.profile("sub-"+uuid.NewString(), a.email, true)
	wantStatus(t, a.c.PostJSON("/api/auth/link-social", map[string]any{"provider": "google"}), http.StatusUnauthorized)
	res := a.c.PostJSON("/api/auth/link-social", map[string]any{"provider": "google", "password": strongPW})
	wantStatus(t, res, http.StatusOK)
	state := stateFrom(t, res.JSON(t)["url"].(string))
	// Signed out (cookie gone): the link flow is refused before any call.
	wantStatus(t, a.c.PostJSON("/api/auth/sign-out", map[string]any{}), http.StatusOK)
	wantStatus(t, callback(a.c, code, state), http.StatusBadRequest)
	if n := scalar[int64](t, s, `SELECT count(*) FROM accounts WHERE user_id = $1 AND provider_id = 'google'`, a.id); n != 0 {
		t.Fatal("nothing linked")
	}
}

func TestGoogleCallbackErrorParamRedirectsToLogin(t *testing.T) {
	s, _ := googleServer(t)
	res := s.Client(t).Get("/api/auth/callback/google?error=access_denied")
	if res.StatusCode != http.StatusSeeOther || res.Header.Get("Location") != "/login" {
		t.Fatalf("%d %s", res.StatusCode, res.Header.Get("Location"))
	}
	wantStatus(t, s.Client(t).Get("/api/auth/callback/google?state=x"), http.StatusBadRequest)
	wantStatus(t, s.Client(t).Get("/api/auth/callback/google?code=x"), http.StatusBadRequest)
}
