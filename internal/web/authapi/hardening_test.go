package authapi_test

import (
	"net/http"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/testutil"
	"github.com/wellington-vell/lindaflor/internal/valkey"
)

// --- tests/auth_hardening.rs (A3 step-up + impersonation, A5 TOTP cap, A6 enumeration) ---

func TestImpersonationIsAuditedCappedAndCannotChangeCredentials(t *testing.T) {
	s := newServer(t)
	admin := adminAccount(t, s, "imp-admin", "Hardening User")
	target := signUp(t, s, "imp-target", "Hardening User")
	imp := admin.c.PostJSON("/api/auth/admin/impersonate-user", map[string]any{"userId": target.id.String()})
	wantStatus(t, imp, http.StatusOK)
	actionCount := func(action string) int64 {
		return scalar[int64](t, s, `SELECT count(*) FROM audit_events WHERE aggregate_id = $1 AND metadata->>'action' = $2`, target.id.String(), action)
	}
	if actionCount("impersonate_start") != 1 {
		t.Fatal("impersonate_start audited once")
	}
	if !scalar[bool](t, s, `SELECT bool_and(expires_at <= now() + interval '61 minutes') FROM sessions WHERE user_id = $1 AND impersonated_by = $2`, target.id, admin.id) {
		t.Fatal("impersonation session must be capped at 1h")
	}
	calls := []struct {
		path string
		body map[string]any
	}{
		{"/api/auth/change-email", map[string]any{"newEmail": testutil.UniqueEmail("imp-new"), "password": strongPW}},
		{"/api/auth/link-social", map[string]any{"provider": "google", "password": strongPW}},
		{"/api/auth/two-factor/enable", map[string]any{"password": strongPW}},
		{"/api/auth/change-password", map[string]any{"currentPassword": strongPW, "newPassword": strongNewPW}},
		{"/api/auth/unlink-account", map[string]any{"providerId": "google", "accountId": "x", "password": strongPW}},
		{"/api/auth/update-user", map[string]any{"name": "Hijacked"}},
		{"/api/auth/revoke-other-sessions", map[string]any{}},
		{"/api/auth/delete-user", map[string]any{"password": strongPW}},
		{"/api/auth/revoke-session", map[string]any{"sessionId": uuid.NewString()}},
		{"/api/auth/two-factor/disable", map[string]any{"password": strongPW}},
	}
	for _, c := range calls {
		if res := admin.c.PostJSON(c.path, c.body); res.StatusCode != http.StatusForbidden {
			t.Errorf("%s: %d %s", c.path, res.StatusCode, res.Body)
		}
	}
	if n := scalar[int64](t, s, `SELECT count(*) FROM verifications WHERE identifier LIKE $1`, "change-email:"+target.id.String()+":%"); n != 0 {
		t.Fatal("no change-email token from an impersonation")
	}
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/stop-impersonating", map[string]any{}), http.StatusOK)
	if actionCount("impersonate_stop") != 1 {
		t.Fatal("impersonate_stop audited once")
	}
	if n := scalar[string](t, s, `SELECT name FROM users WHERE id = $1`, target.id); n != "Hardening User" {
		t.Fatal(n)
	}
}

func TestAdminRenameIsAudited(t *testing.T) {
	s := newServer(t)
	admin := adminAccount(t, s, "rename-admin", "Hardening User")
	target := signUp(t, s, "rename-target", "Hardening User")
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/update-user", map[string]any{"userId": target.id.String(), "name": "Novo Nome"}), http.StatusOK)
	r := lastAudit(t, s, admin.id, target.id.String(), "rename_user")
	if r.meta["from"] != "Hardening User" || r.meta["to"] != "Novo Nome" {
		t.Fatalf("audit %+v", r.meta)
	}
}

func TestEnabling2FARequiresPassword(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "stepup-enable", "Hardening User")
	for _, body := range []map[string]any{{}, {"password": "wrong-password"}} {
		wantStatus(t, a.c.PostJSON("/api/auth/two-factor/enable", body), http.StatusUnauthorized)
	}
	wantStatus(t, a.c.PostJSON("/api/auth/two-factor/enable", map[string]any{"password": strongPW}), http.StatusOK)
}

func TestUnlinkRequiresStepUp(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "stepup-unlink", "Hardening User")
	acct := "google-" + a.id.String()
	exec(t, s, `INSERT INTO accounts (id, account_id, provider_id, user_id) VALUES ($1, $2, 'google', $3)`, uuid.New(), acct, a.id)
	wantStatus(t, a.c.PostJSON("/api/auth/unlink-account", map[string]any{"providerId": "google", "accountId": acct}), http.StatusUnauthorized)
	wantStatus(t, a.c.PostJSON("/api/auth/unlink-account", map[string]any{"providerId": "google", "accountId": acct, "password": strongPW}), http.StatusOK)
}

func TestWrongTOTPCodesLockTheUserAcrossPaths(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "totp-cap", "Hardening User")
	secret := enable2FA(t, a)
	bad := wrongCode(testutil.TOTPCode(t, secret))
	for range valkey.TotpFailMax {
		wantStatus(t, a.c.PostJSON("/api/auth/two-factor/disable", map[string]any{"code": bad}), http.StatusUnauthorized)
	}
	c, challenged := signIn(t, s, a.email, strongPW)
	if challenged.JSON(t)["code"] != "TWO_FACTOR_REQUIRED" {
		t.Fatal(challenged.Body)
	}
	token := challenged.JSON(t)["token"].(string)
	locked := c.PostJSON("/api/auth/two-factor/verify-totp", map[string]any{"code": testutil.TOTPCode(t, secret), "token": token})
	wantStatus(t, locked, http.StatusTooManyRequests)
	if _, ok := testutil.FindSetCookie(locked.SetCookies(), testutil.SessionCookie); ok {
		t.Fatal("no session while locked")
	}
}

func TestStepUpTOTPCannotBeReplayed(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "totp-replay", "Hardening User")
	secret := enable2FA(t, a)
	code := nextStepCode(t, secret)
	wantStatus(t, a.c.PostJSON("/api/auth/change-email", map[string]any{"newEmail": testutil.UniqueEmail("totp-replay-a"), "code": code}), http.StatusOK)
	wantStatus(t, a.c.PostJSON("/api/auth/change-email", map[string]any{"newEmail": testutil.UniqueEmail("totp-replay-b"), "code": code}), http.StatusUnauthorized)
}

func TestUnknownAndWrongPasswordSignInLookTheSame(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "enum", "Hardening User")
	_, wrong := signIn(t, s, a.email, "not-the-password")
	_, unknown := signIn(t, s, testutil.UniqueEmail("enum-missing"), "not-the-password")
	wantStatus(t, wrong, http.StatusUnauthorized)
	if unknown.StatusCode != wrong.StatusCode || unknown.Body != wrong.Body {
		t.Fatalf("enumeration oracle: %d %s vs %d %s", unknown.StatusCode, unknown.Body, wrong.StatusCode, wrong.Body)
	}
}

func TestPasswordLockoutAfterRepeatedFailures(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "lockout", "Lockout User")
	for range valkey.AuthFailMax {
		_, res := signIn(t, s, a.email, "not-the-password")
		wantStatus(t, res, http.StatusUnauthorized)
	}
	_, locked := signIn(t, s, a.email, strongPW)
	wantStatus(t, locked, http.StatusTooManyRequests)
	if locked.Header.Get("Retry-After") == "" {
		t.Fatal("429 carries Retry-After")
	}
}
