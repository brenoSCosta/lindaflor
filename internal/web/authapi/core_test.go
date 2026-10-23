package authapi_test

import (
	"net/http"
	"strings"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// --- tests/auth_core.rs ---

func TestSignUpSignInGetSessionSignOut(t *testing.T) {
	s := newServer(t)
	c := s.Client(t)
	email := testutil.UniqueEmail("core")
	up := c.SignUp("Core User", email, strongPW)
	wantStatus(t, up, http.StatusOK)
	if c.Cookie(testutil.SessionCookie) == "" {
		t.Fatal("sign-up should set the session cookie")
	}
	if field(up.JSON(t), "user", "email") != email {
		t.Fatal(up.Body)
	}
	sess := c.Get("/api/auth/get-session")
	wantStatus(t, sess, http.StatusOK)
	j := sess.JSON(t)
	if field(j, "user", "email") != email || field(j, "session", "id") == nil {
		t.Fatal(sess.Body)
	}
	if strings.Contains(sess.Body, c.Cookie(testutil.SessionCookie)) {
		t.Fatal("session payload must not carry the raw token")
	}
	out := c.PostJSON("/api/auth/sign-out", map[string]any{})
	wantStatus(t, out, http.StatusOK)
	if out.JSON(t)["success"] != true {
		t.Fatal(out.Body)
	}
	if e := sessionEmail(t, c); e != nil {
		t.Fatalf("expected null session, got %v", e)
	}
	again := c.SignIn(email, strongPW)
	wantStatus(t, again, http.StatusOK)
	if c.Cookie(testutil.SessionCookie) == "" || field(again.JSON(t), "user", "email") != email {
		t.Fatal(again.Body)
	}
}

func TestSignUpExistingEmailDoesNotEnumerate(t *testing.T) {
	s := newServer(t)
	email := testutil.UniqueEmail("exists")
	wantStatus(t, s.Client(t).SignUp("First", email, strongPW), http.StatusOK)
	again := s.Client(t).SignUp("Second", email, strongPW)
	wantStatus(t, again, http.StatusBadRequest)
	if strings.Contains(strings.ToLower(again.Body), "already exists") {
		t.Fatalf("leaked existence: %s", again.Body)
	}
}

func TestSignInWrongPasswordIsUnauthorized(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "wrong-pw", "Wrong Pw")
	_, res := signIn(t, s, a.email, "not-the-password")
	wantStatus(t, res, http.StatusUnauthorized)
}

func TestExpiredTempBanDoesNotBlockSignInOrSession(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "core-ban-expiry", "Core Ban Expiry")
	exec(t, s, `UPDATE users SET banned = true, ban_reason = 'temp', ban_expires = now() - interval '1 hour' WHERE id = $1`, a.id)
	if sessionEmail(t, a.c) != a.email {
		t.Fatal("expired temp ban must lift for current_user")
	}
	_, res := signIn(t, s, a.email, strongPW)
	wantStatus(t, res, http.StatusOK)
}

// --- tests/auth_password.rs ---

func TestRequestAndResetPassword(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "reset", "Reset User")
	req := s.Client(t).PostJSON("/api/auth/request-password-reset", map[string]any{"email": a.email})
	wantStatus(t, req, http.StatusOK)
	if req.JSON(t)["status"] != true {
		t.Fatal(req.Body)
	}
	var value string
	var hash *string
	row := s.Pool.QueryRow(t.Context(), `SELECT value, value_hash FROM verifications WHERE identifier LIKE $1 ORDER BY created_at DESC LIMIT 1`, "reset-password:"+a.email+"%")
	if err := row.Scan(&value, &hash); err != nil {
		t.Fatal(err)
	}
	if value != "" || hash == nil || len(*hash) != 64 {
		t.Fatal("reset token must be stored as a digest only")
	}
	token := s.VerificationTokenLike(t, "reset-password:"+a.email)
	reset := s.Client(t).PostJSON("/api/auth/reset-password", map[string]any{"token": token, "newPassword": strongNewPW})
	wantStatus(t, reset, http.StatusOK)
	_, old := signIn(t, s, a.email, strongPW)
	wantStatus(t, old, http.StatusUnauthorized)
	_, ok := signIn(t, s, a.email, strongNewPW)
	wantStatus(t, ok, http.StatusOK)
	// Reset revokes every session.
	if sessionEmail(t, a.c) != nil {
		t.Fatal("reset must revoke existing sessions")
	}
}

func TestRequestPasswordResetQueuesMailWithWorkingLink(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "reset-mail", "Mail User")
	wantStatus(t, s.Client(t).PostJSON("/api/auth/request-password-reset", map[string]any{"email": a.email}), http.StatusOK)
	var subject, html, text string
	err := s.Pool.QueryRow(t.Context(), `SELECT subject, html_body, text_body FROM email_outbox
		WHERE to_address = $1 AND kind = 'password_reset' AND status = 'pending' ORDER BY created_at DESC LIMIT 1`, a.email).Scan(&subject, &html, &text)
	if err != nil {
		t.Fatal(err)
	}
	if !strings.Contains(subject, "Redefinição de senha") || !strings.Contains(text, "Olá, Mail User!") || !strings.Contains(html, "/reset-password?token=") {
		t.Fatalf("mail: %s / %s", subject, text)
	}
	_, rest, _ := strings.Cut(text, "/reset-password?token=")
	token := strings.Fields(rest)[0]
	reset := s.Client(t).PostJSON("/api/auth/reset-password", map[string]any{"token": token, "newPassword": strongNewPW})
	wantStatus(t, reset, http.StatusOK)
	_, ok := signIn(t, s, a.email, strongNewPW)
	wantStatus(t, ok, http.StatusOK)
}

func TestSignUpRejectsWeakPassword(t *testing.T) {
	s := newServer(t)
	wantStatus(t, s.Client(t).SignUp("Weak User", testutil.UniqueEmail("weak"), "password123"), http.StatusBadRequest)
}

func TestSignUpRejectsShortPassword(t *testing.T) {
	s := newServer(t)
	wantStatus(t, s.Client(t).SignUp("Short User", testutil.UniqueEmail("short"), "Ab1!"), http.StatusBadRequest)
}

func TestSignUpRejectsPasswordOverMaxChars(t *testing.T) {
	s := newServer(t)
	wantStatus(t, s.Client(t).SignUp("Long User", testutil.UniqueEmail("long"), strings.Repeat("é", 129)), http.StatusBadRequest)
}

func TestResetPasswordRejectsWeakNewPassword(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "reset-weak", "Reset Weak")
	wantStatus(t, s.Client(t).PostJSON("/api/auth/request-password-reset", map[string]any{"email": a.email}), http.StatusOK)
	token := s.VerificationTokenLike(t, "reset-password:"+a.email)
	res := s.Client(t).PostJSON("/api/auth/reset-password", map[string]any{"token": token, "newPassword": "password123"})
	wantStatus(t, res, http.StatusBadRequest)
}

// --- tests/auth_email.rs ---

func TestSendAndVerifyEmail(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "verify", "Verify User")
	if field(a.json, "user", "emailVerified") != false {
		t.Fatal("new accounts start unverified")
	}
	send := a.c.PostJSON("/api/auth/send-verification-email", map[string]any{"email": a.email})
	wantStatus(t, send, http.StatusOK)
	if v := scalar[string](t, s, `SELECT value FROM verifications WHERE identifier LIKE $1 ORDER BY created_at DESC LIMIT 1`, "email-verification:"+a.email+"%"); v != "" {
		t.Fatal("verify token must not be plaintext")
	}
	token := s.VerificationTokenLike(t, "email-verification:"+a.email)
	res := s.Client(t).PostJSON("/api/auth/verify-email", map[string]any{"token": token})
	wantStatus(t, res, http.StatusOK)
	if !scalar[bool](t, s, `SELECT email_verified FROM users WHERE email = $1`, a.email) {
		t.Fatal("email must be verified")
	}
	// The token is single use.
	wantStatus(t, s.Client(t).Get("/api/auth/verify-email?token="+token), http.StatusBadRequest)
}

func TestVerifyEmailGetVariant(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "verify-get", "Verify Get")
	// Without a body email the session's address is used.
	wantStatus(t, a.c.PostJSON("/api/auth/send-verification-email", map[string]any{}), http.StatusOK)
	token := s.VerificationTokenLike(t, "email-verification:"+a.email)
	wantStatus(t, s.Client(t).Get("/api/auth/verify-email?token="+token+"&callbackURL=/x"), http.StatusOK)
	wantStatus(t, s.Client(t).Get("/api/auth/verify-email"), http.StatusBadRequest)
	wantStatus(t, s.Client(t).PostJSON("/api/auth/send-verification-email", map[string]any{}), http.StatusBadRequest)
}

func TestChangeEmailAndConfirm(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "change-from", "Change Email")
	newEmail := testutil.UniqueEmail("change-to")
	res := a.c.PostJSON("/api/auth/change-email", map[string]any{"newEmail": newEmail, "password": strongPW})
	wantStatus(t, res, http.StatusOK)
	token := s.VerificationTokenLike(t, "change-email:%:"+newEmail)
	confirm := s.Client(t).PostJSON("/api/auth/change-email/confirm", map[string]any{"token": token})
	wantStatus(t, confirm, http.StatusOK)
	if got := scalar[string](t, s, `SELECT email FROM users WHERE id = $1`, a.id); got != newEmail {
		t.Fatal(got)
	}
}

func TestChangeEmailRequestRevokesOtherSessions(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "change-req-from", "Change Req")
	b, res := signIn(t, s, a.email, strongPW)
	wantStatus(t, res, http.StatusOK)
	change := b.PostJSON("/api/auth/change-email", map[string]any{"newEmail": testutil.UniqueEmail("change-req-to"), "password": strongPW})
	wantStatus(t, change, http.StatusOK)
	if sessionEmail(t, a.c) != nil {
		t.Fatal("other session must be revoked on change-email request")
	}
	if sessionEmail(t, b) != a.email {
		t.Fatal("requesting session keeps working")
	}
}

func TestChangeEmailConfirmRevokesAllSessionsLoggedOut(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "change-confirm-from", "Change Confirm")
	b, _ := signIn(t, s, a.email, strongPW)
	newEmail := testutil.UniqueEmail("change-confirm-to")
	wantStatus(t, b.PostJSON("/api/auth/change-email", map[string]any{"newEmail": newEmail, "password": strongPW}), http.StatusOK)
	token := s.VerificationTokenLike(t, "change-email:%:"+newEmail)
	wantStatus(t, s.Client(t).PostJSON("/api/auth/change-email/confirm", map[string]any{"token": token}), http.StatusOK)
	if scalar[string](t, s, `SELECT email FROM users WHERE id = $1`, a.id) != newEmail {
		t.Fatal("email not changed")
	}
	for name, c := range map[string]*testutil.Client{"requester": b, "other": a.c} {
		if sessionEmail(t, c) != nil {
			t.Fatalf("%s session must be revoked on confirm", name)
		}
	}
}

func TestChangeEmailValidation(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "change-val", "Change Val")
	wantStatus(t, a.c.PostJSON("/api/auth/change-email", map[string]any{"newEmail": a.email, "password": strongPW}), http.StatusBadRequest)
	wantStatus(t, a.c.PostJSON("/api/auth/change-email", map[string]any{"newEmail": "nope", "password": strongPW}), http.StatusBadRequest)
	wantStatus(t, a.c.PostJSON("/api/auth/change-email", map[string]any{"newEmail": testutil.UniqueEmail("x")}), http.StatusUnauthorized)
	wantStatus(t, s.Client(t).PostJSON("/api/auth/change-email", map[string]any{"newEmail": testutil.UniqueEmail("x")}), http.StatusUnauthorized)
	// A taken address answers success without issuing a token.
	other := signUp(t, s, "change-taken", "Taken")
	wantStatus(t, a.c.PostJSON("/api/auth/change-email", map[string]any{"newEmail": other.email, "password": strongPW}), http.StatusOK)
	if n := scalar[int64](t, s, `SELECT count(*) FROM verifications WHERE identifier LIKE $1`, "change-email:"+a.id.String()+":%"); n != 0 {
		t.Fatalf("taken address issued %d tokens", n)
	}
}
