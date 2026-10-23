package authapi_test

import (
	"net/http"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// --- tests/auth_sessions.rs (API) ---

func listSessions(t *testing.T, c *testutil.Client) []any {
	t.Helper()
	res := c.Get("/api/auth/list-sessions")
	wantStatus(t, res, http.StatusOK)
	var arr []any
	if err := jsonUnmarshal(res.Body, &arr); err != nil {
		t.Fatalf("sessions array: %s", res.Body)
	}
	return arr
}

func TestListRevokeSessionAndRevokeOthers(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "sessions", "Sessions User")
	b, res := signIn(t, s, a.email, strongPW)
	wantStatus(t, res, http.StatusOK)
	sessions := listSessions(t, b)
	if len(sessions) < 2 {
		t.Fatalf("expected ≥2 sessions, got %v", sessions)
	}
	var otherID string
	for _, raw := range sessions {
		m := raw.(map[string]any)
		if m["current"] == false {
			otherID = m["id"].(string)
		}
		if m["token"] != m["id"] {
			t.Fatal("token must mirror the id, never the cookie")
		}
	}
	wantStatus(t, b.PostJSON("/api/auth/revoke-session", map[string]any{"sessionId": otherID}), http.StatusOK)
	if sessionEmail(t, a.c) != nil {
		t.Fatal("revoked session cookie should be null")
	}
	c, _ := signIn(t, s, a.email, strongPW)
	wantStatus(t, c.PostJSON("/api/auth/revoke-other-sessions", map[string]any{}), http.StatusOK)
	only := listSessions(t, c)
	if len(only) != 1 || only[0].(map[string]any)["current"] != true {
		t.Fatalf("remaining %v", only)
	}
	if sessionEmail(t, b) != nil {
		t.Fatal("old session b must be revoked")
	}
}

func TestRevokeSessionValidation(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "revoke-val", "Revoke Val")
	wantStatus(t, a.c.PostJSON("/api/auth/revoke-session", map[string]any{}), http.StatusBadRequest)
	wantStatus(t, a.c.PostJSON("/api/auth/revoke-session", map[string]any{"token": "nope"}), http.StatusBadRequest)
	wantStatus(t, a.c.PostJSON("/api/auth/revoke-session", map[string]any{"sessionId": uuid.NewString()}), http.StatusBadRequest)
	// Revoking the current session expires the cookie.
	cur := listSessions(t, a.c)[0].(map[string]any)["id"].(string)
	res := a.c.PostJSON("/api/auth/revoke-session", map[string]any{"token": cur})
	wantStatus(t, res, http.StatusOK)
	if line, ok := testutil.FindSetCookie(res.SetCookies(), testutil.SessionCookie); !ok {
		t.Fatal("current revoke must expire the cookie")
	} else {
		testutil.AssertExpiredHardenedCookieDev(t, line, testutil.SessionCookie)
	}
	wantStatus(t, s.Client(t).Get("/api/auth/list-sessions"), http.StatusUnauthorized)
}

func TestExpiredTempBanLiftsForActiveSession(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "ban-expiry", "Ban Expiry")
	if sessionEmail(t, a.c) != a.email {
		t.Fatal("active")
	}
	exec(t, s, `UPDATE users SET banned = true, ban_reason = 'temp', ban_expires = now() - interval '1 hour' WHERE id = $1`, a.id)
	if sessionEmail(t, a.c) != a.email {
		t.Fatal("expired temp ban must lift for active sessions")
	}
	exec(t, s, `UPDATE users SET ban_expires = now() + interval '1 hour' WHERE id = $1`, a.id)
	if sessionEmail(t, a.c) != nil {
		t.Fatal("unexpired temp ban must block the session")
	}
	exec(t, s, `UPDATE users SET ban_expires = NULL WHERE id = $1`, a.id)
	if sessionEmail(t, a.c) != nil {
		t.Fatal("permanent ban must block the session")
	}
	exec(t, s, `UPDATE users SET banned = false, ban_reason = NULL, ban_expires = NULL WHERE id = $1`, a.id)
	if sessionEmail(t, a.c) != a.email {
		t.Fatal("lifting the ban restores the session")
	}
	exec(t, s, `UPDATE users SET banned = true, ban_expires = now() - interval '1 minute' WHERE id = $1`, a.id)
	_, ok := signIn(t, s, a.email, strongPW)
	wantStatus(t, ok, http.StatusOK)
	exec(t, s, `UPDATE users SET ban_expires = now() + interval '1 minute' WHERE id = $1`, a.id)
	_, no := signIn(t, s, a.email, strongPW)
	wantStatus(t, no, http.StatusUnauthorized)
}

func TestChangePasswordDefaultsToRevokingOtherSessions(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "pw-default-revoke", "Pw Sessions")
	b, _ := signIn(t, s, a.email, strongPW)
	wantStatus(t, b.PostJSON("/api/auth/change-password", map[string]any{"currentPassword": strongPW, "newPassword": strongNewPW}), http.StatusOK)
	if sessionEmail(t, a.c) != nil {
		t.Fatal("other session must be revoked by default")
	}
	if sessionEmail(t, b) != a.email {
		t.Fatal("current session keeps working")
	}
	only := listSessions(t, b)
	if len(only) != 1 || only[0].(map[string]any)["current"] != true {
		t.Fatalf("remaining %v", only)
	}
}

func TestChangePasswordExplicitFalseKeepsOtherSessions(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "pw-keep-others", "Pw Sessions")
	b, _ := signIn(t, s, a.email, strongPW)
	wantStatus(t, b.PostJSON("/api/auth/change-password", map[string]any{
		"currentPassword": strongPW, "newPassword": strongNewPW, "revokeOtherSessions": false,
	}), http.StatusOK)
	if sessionEmail(t, a.c) != a.email || sessionEmail(t, b) != a.email {
		t.Fatal("both sessions survive the opt-out")
	}
}

func TestChangePasswordRejectsWrongCurrentAndWeakNew(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "pw-wrong", "Pw Wrong")
	wantStatus(t, a.c.PostJSON("/api/auth/change-password", map[string]any{"currentPassword": "nope-nope-nope", "newPassword": strongNewPW}), http.StatusUnauthorized)
	wantStatus(t, a.c.PostJSON("/api/auth/change-password", map[string]any{"currentPassword": strongPW, "newPassword": "password123"}), http.StatusBadRequest)
	wantStatus(t, a.c.PostJSON("/api/auth/change-password", map[string]any{"currentPassword": "", "newPassword": strongNewPW}), http.StatusBadRequest)
}

// --- tests/auth_account.rs ---

func TestUpdateUserChangePasswordListAccounts(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "account", "Account User")
	up := a.c.PostJSON("/api/auth/update-user", map[string]any{"name": "Updated Name"})
	wantStatus(t, up, http.StatusOK)
	if field(up.JSON(t), "user", "name") != "Updated Name" {
		t.Fatal(up.Body)
	}
	wantStatus(t, a.c.PostJSON("/api/auth/change-password", map[string]any{"currentPassword": strongPW, "newPassword": strongNewPW}), http.StatusOK)
	accounts := a.c.Get("/api/auth/list-accounts")
	wantStatus(t, accounts, http.StatusOK)
	var list []map[string]any
	if err := jsonUnmarshal(accounts.Body, &list); err != nil || len(list) != 1 || list[0]["providerId"] != "credential" {
		t.Fatalf("accounts %s", accounts.Body)
	}
	googleAccount := "google-" + a.id.String()
	exec(t, s, `INSERT INTO accounts (id, account_id, provider_id, user_id) VALUES ($1, $2, 'google', $3)`, uuid.New(), googleAccount, a.id)
	unlink := a.c.PostJSON("/api/auth/unlink-account", map[string]any{"providerId": "google", "accountId": googleAccount, "password": strongNewPW})
	wantStatus(t, unlink, http.StatusOK)
	if unlink.JSON(t)["status"] != true {
		t.Fatal(unlink.Body)
	}
	if err := jsonUnmarshal(a.c.Get("/api/auth/list-accounts").Body, &list); err != nil || len(list) != 1 {
		t.Fatalf("accounts after unlink: %v", list)
	}
	// The last login method cannot be removed.
	res := a.c.PostJSON("/api/auth/unlink-account", map[string]any{"providerId": "credential", "accountId": a.id.String(), "password": strongNewPW})
	wantStatus(t, res, http.StatusBadRequest)
}

func TestUpdateUserValidation(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "update-val", "Update Val")
	wantStatus(t, a.c.PostJSON("/api/auth/update-user", map[string]any{}), http.StatusBadRequest)
	wantStatus(t, a.c.PostJSON("/api/auth/update-user", map[string]any{"name": "   "}), http.StatusBadRequest)
	wantStatus(t, s.Client(t).PostJSON("/api/auth/update-user", map[string]any{"name": "x"}), http.StatusUnauthorized)
}

func TestDeleteUserWithToken(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "delete", "Delete Me")
	wantStatus(t, a.c.PostJSON("/api/auth/delete-user", map[string]any{"password": strongPW}), http.StatusOK)
	if n := scalar[int64](t, s, `SELECT count(*) FROM email_outbox WHERE to_address = $1 AND kind = 'delete_account'`, a.email); n != 1 {
		t.Fatalf("delete mail queued %d times", n)
	}
	token := s.VerificationTokenLike(t, "delete-user:"+a.id.String())
	wantStatus(t, a.c.PostJSON("/api/auth/delete-user", map[string]any{"token": token}), http.StatusOK)
	if scalar[bool](t, s, `SELECT EXISTS (SELECT 1 FROM users WHERE id = $1)`, a.id) {
		t.Fatal("user must be gone")
	}
	_, res := signIn(t, s, a.email, strongPW)
	wantStatus(t, res, http.StatusUnauthorized)
}

func TestDeleteUserRequiresStepUp(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "delete-stepup", "Delete Step")
	wantStatus(t, a.c.PostJSON("/api/auth/delete-user", map[string]any{}), http.StatusUnauthorized)
	wantStatus(t, a.c.PostJSON("/api/auth/delete-user", map[string]any{"password": "wrong-wrong-wrong"}), http.StatusUnauthorized)
	wantStatus(t, s.Client(t).PostJSON("/api/auth/delete-user", map[string]any{"token": "nope"}), http.StatusBadRequest)
}

func TestDeleteUserPurgesPersonalDataAndKeepsOrders(t *testing.T) {
	s := newServer(t)
	a := signUp(t, s, "lgpd", "Lgpd User")
	productID, variantID := uuid.New(), uuid.New()
	exec(t, s, `INSERT INTO products (id, name, slug, price_in_cents) VALUES ($1, 'Peça LGPD', $2, 1000)`, productID, "lgpd-"+productID.String())
	exec(t, s, `INSERT INTO product_variants (id, product_id, sku, size, color) VALUES ($1, $2, $3, 'm', 'azul')`, variantID, productID, "lgpd-"+variantID.String())
	exec(t, s, `INSERT INTO user_addresses (id, user_id, recipient_name, zip_code, street, number, neighborhood, city, state, phone)
		VALUES ($1, $2, 'Lgpd User', '49000000', 'Rua A', '1', 'Centro', 'Aracaju', 'SE', '79999999999')`, uuid.New(), a.id)
	exec(t, s, `INSERT INTO wishlist_items (user_id, product_id) VALUES ($1, $2)`, a.id, productID)
	cartID := uuid.New()
	exec(t, s, `INSERT INTO carts (id, token, user_id) VALUES ($1, $2, $3)`, cartID, uuid.New(), a.id)
	exec(t, s, `INSERT INTO cart_items (id, cart_id, variant_id, quantity) VALUES ($1, $2, $3, 1)`, uuid.New(), cartID, variantID)
	typed, bare := uuid.New(), uuid.New()
	exec(t, s, `INSERT INTO orders (id, user_id, guest_email, status, total_cents, shipping_address)
		VALUES ($1, $2, 'entrega@example.com', 'delivered', 1000, '{"name": "Lgpd User", "city": "Aracaju"}')`, typed, a.id)
	exec(t, s, `INSERT INTO orders (id, user_id, guest_email, status, total_cents, shipping_address)
		VALUES ($1, $2, NULL, 'delivered', 1000, '{"name": "Lgpd User", "city": "Aracaju"}')`, bare, a.id)

	wantStatus(t, a.c.PostJSON("/api/auth/delete-user", map[string]any{"password": strongPW}), http.StatusOK)
	token := s.VerificationTokenLike(t, "delete-user:"+a.id.String())
	wantStatus(t, a.c.PostJSON("/api/auth/delete-user", map[string]any{"token": token}), http.StatusOK)

	for _, table := range []string{"sessions", "accounts", "user_addresses", "wishlist_items", "carts"} {
		if n := scalar[int64](t, s, "SELECT count(*) FROM "+table+" WHERE user_id = $1", a.id); n != 0 {
			t.Errorf("%s rows must go with the user (%d left)", table, n)
		}
	}
	if n := scalar[int64](t, s, `SELECT count(*) FROM cart_items WHERE cart_id = $1`, cartID); n != 0 {
		t.Error("the cart is deleted, not orphaned")
	}
	rows, err := s.Pool.Query(t.Context(), `SELECT id, user_id, guest_email, shipping_address->>'city' FROM orders WHERE id = ANY($1)`, []uuid.UUID{typed, bare})
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	n := 0
	for rows.Next() {
		n++
		var id uuid.UUID
		var owner *uuid.UUID
		var guest, city *string
		if err := rows.Scan(&id, &owner, &guest, &city); err != nil {
			t.Fatal(err)
		}
		want := a.email
		if id == typed {
			want = "entrega@example.com"
		}
		if owner != nil || guest == nil || *guest != want || city == nil || *city != "Aracaju" {
			t.Errorf("order %s: owner=%v guest=%v city=%v", id, owner, guest, city)
		}
	}
	if n != 2 {
		t.Fatalf("orders are retained, got %d", n)
	}
}

func TestLinkSocialUnavailableWithoutGoogleEnv(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{Mutate: func(c *config.Config) {
		c.Google = config.GoogleConfig{}
		c.AppOrigin = ""
	}})
	a := signUp(t, s, "link", "Link User")
	res := a.c.PostJSON("/api/auth/link-social", map[string]any{"provider": "google"})
	wantStatus(t, res, http.StatusNotImplemented)
	if res.JSON(t)["code"] != "SOCIAL_SIGN_IN_UNAVAILABLE" {
		t.Fatal(res.Body)
	}
	wantStatus(t, a.c.PostJSON("/api/auth/link-social", map[string]any{"provider": "github"}), http.StatusBadRequest)
	wantStatus(t, s.Client(t).PostJSON("/api/auth/sign-in/social", map[string]any{"provider": "google"}), http.StatusNotImplemented)
}

func TestGoogleCallbackRejectsMissingPKCEVerifierBeforeNetwork(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{Mutate: func(c *config.Config) {
		c.Google = config.GoogleConfig{ClientID: "test-client.apps.googleusercontent.com", ClientSecret: "test-secret"}
		c.AppOrigin = "http://127.0.0.1:4200"
		c.GoogleOAuthTestBase = "http://127.0.0.1:1" // never reached
	}})
	state := "pkce-missing-" + uuid.NewString()
	exec(t, s, `INSERT INTO verifications (id, identifier, value, expires_at) VALUES ($1, $2, $3, now() + interval '10 minutes')`,
		uuid.New(), "oauth-google:"+state, `{"callback": "/dashboard"}`)
	res := s.Client(t).Get("/api/auth/callback/google?code=fake-code&state=" + state)
	wantStatus(t, res, http.StatusBadRequest)
}
