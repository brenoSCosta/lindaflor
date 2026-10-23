package authapi_test

import (
	"context"
	"encoding/json"
	"net/http"
	"testing"
	"time"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

const (
	strongPW    = "Harbor-Violet-Tiger-4821-Lantern!"
	strongNewPW = "Quilt-Mango-Brave-Pixel-7395?"
)

func newServer(t *testing.T) *testutil.Server {
	t.Helper()
	return testutil.NewServer(t, testutil.Options{})
}

// account is a signed-up user with its own cookie jar.
type account struct {
	c     *testutil.Client
	id    uuid.UUID
	email string
	json  map[string]any
}

func signUp(t *testing.T, s *testutil.Server, prefix, name string) account {
	t.Helper()
	c := s.Client(t)
	email := testutil.UniqueEmail(prefix)
	res := c.SignUp(name, email, strongPW)
	if res.StatusCode != http.StatusOK {
		t.Fatalf("sign-up: %d %s", res.StatusCode, res.Body)
	}
	j := res.JSON(t)
	id := uuid.MustParse(j["user"].(map[string]any)["id"].(string))
	return account{c: c, id: id, email: email, json: j}
}

// signIn opens a new session (own jar) for email.
func signIn(t *testing.T, s *testutil.Server, email, password string) (*testutil.Client, *testutil.Response) {
	t.Helper()
	c := s.Client(t)
	return c, c.SignIn(email, password)
}

func field(j map[string]any, path ...string) any {
	var cur any = j
	for _, p := range path {
		m, ok := cur.(map[string]any)
		if !ok {
			return nil
		}
		cur = m[p]
	}
	return cur
}

func sessionEmail(t *testing.T, c *testutil.Client) any {
	t.Helper()
	res := c.Get("/api/auth/get-session")
	if res.StatusCode != http.StatusOK {
		t.Fatalf("get-session: %d %s", res.StatusCode, res.Body)
	}
	if res.Body == "null\n" || res.Body == "null" {
		return nil
	}
	return field(res.JSON(t), "user", "email")
}

func wantStatus(t *testing.T, res *testutil.Response, want int) {
	t.Helper()
	if res.StatusCode != want {
		t.Fatalf("status %d, want %d: %s", res.StatusCode, want, res.Body)
	}
}

func exec(t *testing.T, s *testutil.Server, sql string, args ...any) {
	t.Helper()
	if _, err := s.Pool.Exec(context.Background(), sql, args...); err != nil {
		t.Fatalf("exec %q: %v", sql, err)
	}
}

func scalar[T any](t *testing.T, s *testutil.Server, sql string, args ...any) T {
	t.Helper()
	var v T
	if err := s.Pool.QueryRow(context.Background(), sql, args...).Scan(&v); err != nil {
		t.Fatalf("query %q: %v", sql, err)
	}
	return v
}

func makeAdmin(t *testing.T, s *testutil.Server, id uuid.UUID) {
	t.Helper()
	exec(t, s, "UPDATE users SET role = 'admin' WHERE id = $1", id)
}

// enable2FA enrolls + verifies TOTP for a; returns the secret.
func enable2FA(t *testing.T, a account) string {
	t.Helper()
	res := a.c.PostJSON("/api/auth/two-factor/enable", map[string]any{"password": strongPW})
	wantStatus(t, res, http.StatusOK)
	secret := res.JSON(t)["secret"].(string)
	v := a.c.PostJSON("/api/auth/two-factor/verify-totp", map[string]any{"code": testutil.TOTPCode(t, secret)})
	wantStatus(t, v, http.StatusOK)
	return secret
}

func nextStepCode(t *testing.T, secret string) string {
	t.Helper()
	return testutil.TOTPCodeAt(t, secret, time.Now().Add(30*time.Second))
}

// wrongCode is a six-digit code that is not valid.
func wrongCode(valid string) string {
	n := 0
	for _, c := range valid {
		n = n*10 + int(c-'0')
	}
	return pad6((n + 500000) % 1000000)
}

func pad6(n int) string {
	b := []byte("000000")
	for i := 5; i >= 0 && n > 0; i-- {
		b[i] = byte('0' + n%10)
		n /= 10
	}
	return string(b)
}

func jsonUnmarshal(body string, v any) error { return json.Unmarshal([]byte(body), v) }
