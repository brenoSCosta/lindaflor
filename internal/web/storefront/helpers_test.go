package storefront_test

import (
	"context"
	"io"
	"net/http"
	"net/url"
	"strings"
	"testing"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

const pw = "Harbor-Violet-Tiger-4821-Lantern!"

var ctx = context.Background()

// newServer is a dev test server with fresh in-process rate-limit buckets
// (newsletter/avise-me/review budgets are per process).
func newServer(t *testing.T) *testutil.Server {
	t.Helper()
	ratelimit.Reset()
	t.Cleanup(ratelimit.Reset)
	return testutil.NewServer(t, testutil.Options{})
}

func exec(t testing.TB, pool *pgxpool.Pool, sql string, args ...any) {
	t.Helper()
	if _, err := pool.Exec(ctx, sql, args...); err != nil {
		t.Fatalf("%s: %v", sql, err)
	}
}

func scalar[T any](t testing.TB, pool *pgxpool.Pool, sql string, args ...any) T {
	t.Helper()
	var v T
	if err := pool.QueryRow(ctx, sql, args...).Scan(&v); err != nil {
		t.Fatalf("%s: %v", sql, err)
	}
	return v
}

func newID() uuid.UUID { return uuid.Must(uuid.NewV7()) }

type product struct {
	ID        uuid.UUID
	Slug      string
	Name      string
	VariantID uuid.UUID
}

// seedProduct inserts an active product with one variant (size m) and
// stock units in a fresh active warehouse.
func seedProduct(t testing.TB, pool *pgxpool.Pool, name string, price int64, stock int32) product {
	t.Helper()
	p := product{ID: newID(), Name: name, VariantID: newID()}
	p.Slug = "p-" + strings.ReplaceAll(p.ID.String(), "-", "")
	exec(t, pool, `INSERT INTO products (id, name, slug, price_in_cents, category) VALUES ($1, $2, $3, $4, 'biquini')`, p.ID, name, p.Slug, price)
	exec(t, pool, `INSERT INTO product_variants (id, product_id, sku, size, color, color_hex) VALUES ($1, $2, $3, 'm', 'Coral', '#FF7F50')`,
		p.VariantID, p.ID, "sku-"+p.VariantID.String())
	wh := newID()
	exec(t, pool, `INSERT INTO warehouses (id, code, name, is_default, active) VALUES ($1, $2, 'Principal', false, true)`,
		wh, "wh-"+strings.ReplaceAll(wh.String(), "-", ""))
	exec(t, pool, `INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved) VALUES ($1, $2, $3, $4, 0)`, newID(), p.VariantID, wh, stock)
	return p
}

type set struct {
	ID       uuid.UUID
	Slug     string
	Variants []uuid.UUID
}

// seedSet inserts an active priced set whose slots are the given products.
func seedSet(t testing.TB, pool *pgxpool.Pool, name string, price int64, members ...product) set {
	t.Helper()
	s := set{ID: newID()}
	s.Slug = "conj-" + strings.ReplaceAll(s.ID.String(), "-", "")
	exec(t, pool, `INSERT INTO sets (id, name, slug, active, price_in_cents, description) VALUES ($1, $2, $3, true, $4, 'Top e calcinha')`, s.ID, name, s.Slug, price)
	for i, m := range members {
		slot := newID()
		exec(t, pool, `INSERT INTO set_slots (id, set_id, sort_order) VALUES ($1, $2, $3)`, slot, s.ID, int32(i)) //nolint:gosec // tiny
		exec(t, pool, `INSERT INTO set_slot_options (slot_id, variant_id) VALUES ($1, $2)`, slot, m.VariantID)
		exec(t, pool, `INSERT INTO set_members (set_id, product_id, sort_order) VALUES ($1, $2, $3)`, s.ID, m.ID, int32(i)) //nolint:gosec // tiny
		s.Variants = append(s.Variants, m.VariantID)
	}
	return s
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
	res := c.SignUp("Cliente Teste", email, pw)
	wantStatus(t, res, http.StatusOK)
	id := uuid.MustParse(res.JSON(t)["user"].(map[string]any)["id"].(string))
	return user{c: c, id: id, email: email}
}

func wantStatus(t *testing.T, res *testutil.Response, want int) {
	t.Helper()
	if res.StatusCode != want {
		t.Fatalf("status %d, want %d: %.2000s", res.StatusCode, want, res.Body)
	}
}

func wantRedirect(t *testing.T, res *testutil.Response, prefix string) {
	t.Helper()
	if res.StatusCode != http.StatusSeeOther || !strings.HasPrefix(res.Header.Get("Location"), prefix) {
		t.Fatalf("want 303 → %s, got %d → %q", prefix, res.StatusCode, res.Header.Get("Location"))
	}
}

func wantContains(t *testing.T, body string, subs ...string) {
	t.Helper()
	for _, s := range subs {
		if !strings.Contains(body, s) {
			t.Fatalf("body misses %q:\n%.3000s", s, body)
		}
	}
}

func wantNotContains(t *testing.T, body string, subs ...string) {
	t.Helper()
	for _, s := range subs {
		if strings.Contains(body, s) {
			t.Fatalf("body unexpectedly has %q", s)
		}
	}
}

// flash decodes the toast cookie written by a no-JS response.
func flash(t *testing.T, res *testutil.Response) []toast.Toast {
	t.Helper()
	line, ok := testutil.FindSetCookie(res.SetCookies(), testutil.DevToastCookie)
	if !ok {
		return nil
	}
	v, _, _ := strings.Cut(strings.TrimPrefix(line, testutil.DevToastCookie+"="), ";")
	return toast.DecodeQueue(v)
}

// headerToasts decodes the X-Toast header of an htmx response.
func headerToasts(t *testing.T, res *testutil.Response) []toast.Toast {
	t.Helper()
	return toast.DecodeQueue(res.Header.Get(toast.Header))
}

func wantToast(t *testing.T, toasts []toast.Toast, kind toast.Type, title string) {
	t.Helper()
	for _, tt := range toasts {
		if tt.Type == kind && tt.Title == title {
			return
		}
	}
	t.Fatalf("missing %s toast %q in %+v", kind, title, toasts)
}

func formBody(v url.Values) io.Reader { return strings.NewReader(v.Encode()) }

func form(kv ...string) url.Values {
	v := url.Values{}
	for i := 0; i+1 < len(kv); i += 2 {
		v.Add(kv[i], kv[i+1])
	}
	return v
}
