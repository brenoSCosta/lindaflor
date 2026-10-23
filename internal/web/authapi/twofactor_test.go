package authapi_test

import (
	"bytes"
	"net/http"
	"slices"
	"strings"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// --- tests/auth_two_factor.rs (API) ---

type enrollment struct {
	secret string
	uri    string
	codes  []string
}

func enroll(t *testing.T, a account) enrollment {
	t.Helper()
	res := a.c.PostJSON("/api/auth/two-factor/enable", map[string]any{"password": strongPW})
	wantStatus(t, res, http.StatusOK)
	j := res.JSON(t)
	e := enrollment{secret: j["secret"].(string), uri: j["totpUri"].(string)}
	for _, c := range j["backupCodes"].([]any) {
		e.codes = append(e.codes, c.(string))
	}
	return e
}

func verifyEnroll(t *testing.T, a account, e enrollment) {
	t.Helper()
	res := a.c.PostJSON("/api/auth/two-factor/verify-totp", map[string]any{"code": testutil.TOTPCode(t, e.secret)})
	wantStatus(t, res, http.StatusOK)
	if field(res.JSON(t), "user", "twoFactorEnabled") != true {
		t.Fatal(res.Body)
	}
}

// challenge signs in (fresh jar) expecting TWO_FACTOR_REQUIRED; returns the
// client and the pending token.
func challenge(t *testing.T, s *testutil.Server, email string) (*testutil.Client, string) {
	t.Helper()
	c, res := signIn(t, s, email, strongPW)
	wantStatus(t, res, http.StatusForbidden)
	j := res.JSON(t)
	if j["code"] != "TWO_FACTOR_REQUIRED" || j["twoFactorRedirect"] != true {
		t.Fatal(res.Body)
	}
	if _, ok := testutil.FindSetCookie(res.SetCookies(), testutil.SessionCookie); ok {
		t.Fatal("no session before the second factor")
	}
	return c, j["token"].(string)
}

func TestTOTPSecretIsEncryptedAtRest(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "2fa-enc", "Encrypted")
	e := enroll(t, a)
	var nonce, ct []byte
	var version int16
	var verified bool
	if err := s.Pool.QueryRow(t.Context(), `SELECT secret_nonce, secret_ciphertext, secret_key_version, verified FROM two_factor WHERE user_id = $1`, a.id).
		Scan(&nonce, &ct, &version, &verified); err != nil {
		t.Fatal(err)
	}
	if verified || len(nonce) != 24 || version != 1 || len(ct) == 0 || bytes.Contains(ct, []byte(e.secret)) {
		t.Fatal("ciphertext at rest")
	}
	dec, err := auth.DecryptTOTPSecret(nonce, ct, 1)
	if err != nil || dec != e.secret {
		t.Fatalf("decrypt: %v", err)
	}
	verifyEnroll(t, a, e)
	wantStatus(t, a.c.PostJSON("/api/auth/sign-out", map[string]any{}), http.StatusOK)
	c, token := challenge(t, s, a.email)
	login := c.PostJSON("/api/auth/two-factor/verify-totp", map[string]any{"code": nextStepCode(t, e.secret), "token": token})
	wantStatus(t, login, http.StatusOK)
	if c.Cookie(testutil.SessionCookie) == "" {
		t.Fatal("TOTP login sets a session")
	}
}

func TestTwoFactorSecretIndexIsGone(t *testing.T) {
	s := newServer(t)
	rows, err := s.Pool.Query(t.Context(), `SELECT indexname FROM pg_indexes WHERE schemaname = 'public' AND tablename = 'two_factor'`)
	if err != nil {
		t.Fatal(err)
	}
	var names []string
	for rows.Next() {
		var n string
		_ = rows.Scan(&n)
		names = append(names, n)
	}
	rows.Close()
	if slices.Contains(names, "two_factor_secret_idx") || !slices.Contains(names, "two_factor_user_id_idx") {
		t.Fatalf("indexes %v", names)
	}
	if scalar[bool](t, s, `SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_name = 'two_factor' AND column_name = 'secret')`) {
		t.Fatal("plaintext secret column must not exist")
	}
	for _, col := range []string{"secret_nonce", "secret_ciphertext", "secret_key_version"} {
		if n := scalar[string](t, s, `SELECT is_nullable FROM information_schema.columns WHERE table_name = 'two_factor' AND column_name = $1`, col); n != "NO" {
			t.Errorf("%s must be NOT NULL", col)
		}
	}
}

func TestEnableVerifyTOTPThenSignInRequires2FA(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "2fa", "Two Factor")
	e := enroll(t, a)
	if !strings.Contains(e.uri, "otpauth://") || e.secret == "" || len(e.codes) != 10 {
		t.Fatalf("enrollment %+v", e)
	}
	verifyEnroll(t, a, e)
	wantStatus(t, a.c.PostJSON("/api/auth/sign-out", map[string]any{}), http.StatusOK)
	c, token := challenge(t, s, a.email)
	done := c.PostJSON("/api/auth/two-factor/verify-backup-code", map[string]any{"code": e.codes[0], "token": token})
	wantStatus(t, done, http.StatusOK)
	j := done.JSON(t)
	if c.Cookie(testutil.SessionCookie) == "" || field(j, "user", "email") != a.email || field(j, "user", "twoFactorEnabled") != true {
		t.Fatal(done.Body)
	}
	// Enabling twice is refused.
	wantStatus(t, c.PostJSON("/api/auth/two-factor/enable", map[string]any{"password": strongPW}), http.StatusBadRequest)
}

func TestTOTPCodeCannotBeReplayed(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "2fa-replay", "Replay")
	e := enroll(t, a)
	code := testutil.TOTPCode(t, e.secret)
	wantStatus(t, a.c.PostJSON("/api/auth/two-factor/verify-totp", map[string]any{"code": code}), http.StatusOK)
	if res := a.c.PostJSON("/api/auth/two-factor/verify-totp", map[string]any{"code": code}); res.StatusCode == http.StatusOK {
		t.Fatal("reused TOTP must not confirm enable twice")
	}
	wantStatus(t, a.c.PostJSON("/api/auth/sign-out", map[string]any{}), http.StatusOK)
	c, token := challenge(t, s, a.email)
	if res := c.PostJSON("/api/auth/two-factor/verify-totp", map[string]any{"code": code, "token": token}); res.StatusCode == http.StatusOK {
		t.Fatal("same TOTP must not complete sign-in after enable")
	}
	wantStatus(t, c.PostJSON("/api/auth/two-factor/verify-backup-code", map[string]any{"code": e.codes[0], "token": token}), http.StatusOK)
}

func TestBackupCodeIsSingleUse(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "2fa-once", "Once")
	e := enroll(t, a)
	verifyEnroll(t, a, e)
	wantStatus(t, a.c.PostJSON("/api/auth/sign-out", map[string]any{}), http.StatusOK)
	c, token := challenge(t, s, a.email)
	wantStatus(t, c.PostJSON("/api/auth/two-factor/verify-backup-code", map[string]any{"code": e.codes[0], "token": token}), http.StatusOK)
	c, token = challenge(t, s, a.email)
	wantStatus(t, c.PostJSON("/api/auth/two-factor/verify-backup-code", map[string]any{"code": e.codes[0], "token": token}), http.StatusUnauthorized)
	c, token = challenge(t, s, a.email)
	wantStatus(t, c.PostJSON("/api/auth/two-factor/verify-backup-code", map[string]any{"code": e.codes[1], "token": token}), http.StatusOK)
}

func TestBackupCodesArePHCOnly(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "2fa-phc-only", "PhcOnly")
	e := enroll(t, a)
	verifyEnroll(t, a, e)
	stored := scalar[string](t, s, `SELECT backup_codes FROM two_factor WHERE user_id = $1`, a.id)
	if strings.Contains(stored, e.codes[0]) {
		t.Fatal("plaintext must not be stored")
	}
	entries := strings.Split(strings.TrimSpace(stored), "\n")
	if len(entries) != 10 {
		t.Fatalf("%d entries", len(entries))
	}
	for _, en := range entries {
		if !strings.HasPrefix(en, "$argon2id$") {
			t.Fatalf("PHC only, got %s", en)
		}
	}
	exec(t, s, `UPDATE two_factor SET backup_codes = $1 WHERE user_id = $2`,
		"00112233445566778899aabbccddeeff$fc21e2dc2665f7e70258365faaac64fd04b9d327395956095b4bfdc4982a492d", a.id)
	wantStatus(t, a.c.PostJSON("/api/auth/sign-out", map[string]any{}), http.StatusOK)
	c, token := challenge(t, s, a.email)
	wantStatus(t, c.PostJSON("/api/auth/two-factor/verify-backup-code", map[string]any{"code": "1234-5678", "token": token}), http.StatusUnauthorized)
}

func TestPendingTokenAttemptCapBurnsTheToken(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "2fa-cap", "Cap")
	e := enroll(t, a)
	verifyEnroll(t, a, e)
	wantStatus(t, a.c.PostJSON("/api/auth/sign-out", map[string]any{}), http.StatusOK)
	c, token := challenge(t, s, a.email)
	for range 5 {
		wantStatus(t, c.PostJSON("/api/auth/two-factor/verify-backup-code", map[string]any{"code": "0000-0000", "token": token}), http.StatusUnauthorized)
	}
	// Sixth attempt (even with a right code) burns the token.
	wantStatus(t, c.PostJSON("/api/auth/two-factor/verify-backup-code", map[string]any{"code": e.codes[0], "token": token}), http.StatusUnauthorized)
	wantStatus(t, c.PostJSON("/api/auth/two-factor/verify-backup-code", map[string]any{"code": e.codes[0], "token": token}), http.StatusUnauthorized)
	// A different user agent cannot use a fresh token bound to another client.
	_, fresh := challenge(t, s, a.email)
	other := s.Client(t)
	other.Headers.Set("User-Agent", "other-agent")
	wantStatus(t, other.PostJSON("/api/auth/two-factor/verify-backup-code", map[string]any{"code": e.codes[0], "token": fresh}), http.StatusUnauthorized)
}

func TestSignedInBackupCodeAndDisable(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "2fa-signed", "Signed")
	wantStatus(t, a.c.PostJSON("/api/auth/two-factor/verify-backup-code", map[string]any{"code": "1111-1111"}), http.StatusBadRequest)
	e := enroll(t, a)
	wantStatus(t, a.c.PostJSON("/api/auth/two-factor/verify-backup-code", map[string]any{"code": e.codes[0]}), http.StatusBadRequest)
	verifyEnroll(t, a, e)
	wantStatus(t, a.c.PostJSON("/api/auth/two-factor/verify-backup-code", map[string]any{"code": e.codes[0]}), http.StatusOK)
	wantStatus(t, a.c.PostJSON("/api/auth/two-factor/verify-backup-code", map[string]any{"code": e.codes[0]}), http.StatusUnauthorized)
	wantStatus(t, a.c.PostJSON("/api/auth/two-factor/disable", map[string]any{"password": "wrong-wrong-wrong"}), http.StatusUnauthorized)
	wantStatus(t, a.c.PostJSON("/api/auth/two-factor/disable", map[string]any{"password": strongPW}), http.StatusOK)
	if field(a.c.Get("/api/auth/get-session").JSON(t), "user", "twoFactorEnabled") != false {
		t.Fatal("2FA must be off")
	}
	if scalar[int64](t, s, `SELECT count(*) FROM two_factor WHERE user_id = $1`, a.id) != 0 {
		t.Fatal("row removed")
	}
	_, res := signIn(t, s, a.email, strongPW)
	wantStatus(t, res, http.StatusOK)
}

func TestVerifyTOTPWithoutEnrollment(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "2fa-none", "None")
	wantStatus(t, a.c.PostJSON("/api/auth/two-factor/verify-totp", map[string]any{"code": "123456"}), http.StatusBadRequest)
	wantStatus(t, a.c.PostJSON("/api/auth/two-factor/verify-totp", map[string]any{"code": " "}), http.StatusBadRequest)
	wantStatus(t, s.Client(t).PostJSON("/api/auth/two-factor/verify-totp", map[string]any{"code": "123456"}), http.StatusUnauthorized)
	wantStatus(t, s.Client(t).PostJSON("/api/auth/two-factor/verify-totp", map[string]any{"code": "123456", "token": "bogus"}), http.StatusUnauthorized)
}

// --- P2-5: generate-backup-codes step-up ---

func verified2FA(t *testing.T, s *testutil.Server, prefix string) (account, string) {
	t.Helper()
	a := signUp(t, s, prefix, "Step Up")
	return a, enable2FA(t, a)
}

func TestGenerateBackupCodesEmptyBodyIs401(t *testing.T) {
	s := newServer(t)
	a, _ := verified2FA(t, s, "2fa-gen-none")
	wantStatus(t, a.c.PostJSON("/api/auth/two-factor/generate-backup-codes", map[string]any{}), http.StatusUnauthorized)
}

func TestGenerateBackupCodesWrongPasswordIs401(t *testing.T) {
	s := newServer(t)
	a, _ := verified2FA(t, s, "2fa-gen-wrong")
	wantStatus(t, a.c.PostJSON("/api/auth/two-factor/generate-backup-codes", map[string]any{"password": "Wrong-Password-0000!"}), http.StatusUnauthorized)
}

func TestGenerateBackupCodesCorrectPasswordRotatesPHC(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "2fa-gen-ok", "Step Up")
	e := enroll(t, a)
	verifyEnroll(t, a, e)
	res := a.c.PostJSON("/api/auth/two-factor/generate-backup-codes", map[string]any{"password": strongPW})
	wantStatus(t, res, http.StatusOK)
	var fresh []string
	for _, c := range res.JSON(t)["backupCodes"].([]any) {
		fresh = append(fresh, c.(string))
	}
	if len(fresh) != 10 || slices.Contains(fresh, e.codes[0]) {
		t.Fatalf("fresh %v", fresh)
	}
	stored := scalar[string](t, s, `SELECT backup_codes FROM two_factor WHERE user_id = $1`, a.id)
	entries := strings.Split(stored, "\n")
	if len(entries) != 10 {
		t.Fatalf("%d entries", len(entries))
	}
	for _, en := range entries {
		if !strings.HasPrefix(en, "$argon2id$") {
			t.Fatal("regen must write PHC")
		}
	}
	for _, c := range fresh {
		if strings.Contains(stored, c) {
			t.Fatal("plaintext must not be stored")
		}
	}
}

func TestGenerateBackupCodesTOTPOnlyForPasswordlessUser(t *testing.T) {
	s := newServer(t)
	a, secret := verified2FA(t, s, "2fa-gen-oauth")
	exec(t, s, `DELETE FROM accounts WHERE user_id = $1 AND provider_id = 'credential'`, a.id)
	wantStatus(t, a.c.PostJSON("/api/auth/two-factor/generate-backup-codes", map[string]any{"password": strongPW}), http.StatusUnauthorized)
	res := a.c.PostJSON("/api/auth/two-factor/generate-backup-codes", map[string]any{"code": nextStepCode(t, secret)})
	wantStatus(t, res, http.StatusOK)
	if len(res.JSON(t)["backupCodes"].([]any)) != 10 {
		t.Fatal(res.Body)
	}
}

func TestGenerateBackupCodesTOTPReplayIs401(t *testing.T) {
	s := newServer(t)
	a, secret := verified2FA(t, s, "2fa-gen-replay")
	code := nextStepCode(t, secret)
	wantStatus(t, a.c.PostJSON("/api/auth/two-factor/generate-backup-codes", map[string]any{"code": code}), http.StatusOK)
	wantStatus(t, a.c.PostJSON("/api/auth/two-factor/generate-backup-codes", map[string]any{"code": code}), http.StatusUnauthorized)
}

func TestGenerateBackupCodesUnverifiedIsError(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "2fa-gen-unverified", "Unverified")
	enroll(t, a)
	if res := a.c.PostJSON("/api/auth/two-factor/generate-backup-codes", map[string]any{"password": strongPW}); res.StatusCode == http.StatusOK {
		t.Fatal("unverified enrollment must not rotate")
	}
}
