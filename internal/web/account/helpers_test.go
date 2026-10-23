package account_test

import (
	"net/http"
	"strings"
	"testing"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

const (
	pw    = "Harbor-Violet-Tiger-4821-Lantern!"
	newPW = "Quilt-Mango-Brave-Pixel-7395?"
)

type user struct {
	c     *testutil.Client
	id    uuid.UUID
	email string
}

func signUp(t *testing.T, s *testutil.Server, prefix string) user {
	t.Helper()
	c := s.Client(t)
	email := testutil.UniqueEmail(prefix)
	res := c.SignUp("Conta Teste", email, pw)
	wantStatus(t, res, http.StatusOK)
	id := uuid.MustParse(res.JSON(t)["user"].(map[string]any)["id"].(string))
	return user{c: c, id: id, email: email}
}

// signInAgain opens a second session for u.
func signInAgain(t *testing.T, s *testutil.Server, u user) *testutil.Client {
	t.Helper()
	c := s.Client(t)
	wantStatus(t, c.SignIn(u.email, pw), http.StatusOK)
	return c
}

func sessionEmail(t *testing.T, c *testutil.Client) string {
	t.Helper()
	res := c.Get("/api/auth/get-session")
	if strings.TrimSpace(res.Body) == "null" {
		return ""
	}
	return res.JSON(t)["user"].(map[string]any)["email"].(string)
}

func wantStatus(t *testing.T, res *testutil.Response, want int) {
	t.Helper()
	if res.StatusCode != want {
		t.Fatalf("status %d, want %d: %s", res.StatusCode, want, res.Body)
	}
}

func wantRedirect(t *testing.T, res *testutil.Response, prefix string) {
	t.Helper()
	if res.StatusCode != http.StatusSeeOther || !strings.HasPrefix(res.Header.Get("Location"), prefix) {
		t.Fatalf("want 303 → %s, got %d → %q: %s", prefix, res.StatusCode, res.Header.Get("Location"), res.Body)
	}
}

func wantContains(t *testing.T, res *testutil.Response, subs ...string) {
	t.Helper()
	for _, s := range subs {
		if !strings.Contains(res.Body, s) {
			t.Fatalf("body missing %q:\n%s", s, res.Body)
		}
	}
}

func wantMissing(t *testing.T, res *testutil.Response, subs ...string) {
	t.Helper()
	for _, s := range subs {
		if strings.Contains(res.Body, s) {
			t.Fatalf("body must not contain %q", s)
		}
	}
}

func toasts(res *testutil.Response) []toast.Toast {
	line, ok := testutil.FindSetCookie(res.SetCookies(), testutil.DevToastCookie)
	if !ok {
		return nil
	}
	v, _, _ := strings.Cut(strings.TrimPrefix(line, testutil.DevToastCookie+"="), ";")
	return toast.DecodeQueue(v)
}

func wantToast(t *testing.T, res *testutil.Response, kind toast.Type, title string) {
	t.Helper()
	for _, tt := range toasts(res) {
		if tt.Type == kind && tt.Title == title {
			return
		}
	}
	t.Fatalf("missing %s toast %q in %v (%d %s)", kind, title, res.SetCookies(), res.StatusCode, res.Header.Get("Location"))
}

func exec(t *testing.T, pool *pgxpool.Pool, sql string, args ...any) {
	t.Helper()
	if _, err := pool.Exec(t.Context(), sql, args...); err != nil {
		t.Fatalf("exec %q: %v", sql, err)
	}
}

func scalar[T any](t *testing.T, pool *pgxpool.Pool, sql string, args ...any) T {
	t.Helper()
	var v T
	if err := pool.QueryRow(t.Context(), sql, args...).Scan(&v); err != nil {
		t.Fatalf("query %q: %v", sql, err)
	}
	return v
}

func htmxHeaders() http.Header {
	return http.Header{"HX-Request": {"true"}, "HX-Request-Type": {"partial"}}
}
