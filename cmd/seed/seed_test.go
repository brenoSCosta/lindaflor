package main

import (
	"context"
	"strings"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

func TestSlugify(t *testing.T) {
	for in, want := range map[string]string{
		"Maré":          "mare",
		"Off-white":     "off-white",
		"  Azul  Céu! ": "azul-ceu",
		"Pêssego/Coral": "pessego-coral",
		"Verde Água 2":  "verde-agua-2",
		"Ñandú Çaçador": "nandu-cacador",
	} {
		if got := slugify(in); got != want {
			t.Errorf("slugify(%q) = %q, want %q", in, got, want)
		}
	}
}

func TestTakeProductsRoundRobins(t *testing.T) {
	ps := []catalogProduct{{Slug: "a1", Category: "a"}, {Slug: "a2", Category: "a"}, {Slug: "b1", Category: "b"}, {Slug: "c1", Category: "c"}}
	var got []string
	for _, p := range takeProducts(ps, 3) {
		got = append(got, p.Slug)
	}
	if strings.Join(got, ",") != "a1,b1,c1" {
		t.Fatalf("got %v", got)
	}
	if n := len(takeProducts(ps, 10)); n != 4 {
		t.Fatalf("over-asking returns everything, got %d", n)
	}
}

func TestHashPasswordIsPHC(t *testing.T) {
	h, err := hashPassword("admin123")
	if err != nil {
		t.Fatal(err)
	}
	if !strings.HasPrefix(h, "$argon2id$v=19$m=19456,t=2,p=1$") {
		t.Fatalf("unexpected hash %q", h)
	}
}

func TestSeedIsIdempotent(t *testing.T) {
	t.Setenv("SEED_PRODUCT_COUNT", "30")
	pool := testutil.NewPool(t)
	ctx := context.Background()
	admin := config.SeedAdmin{Email: "admin@lindaflor.com", Password: "admin123", Name: "Admin User"}
	counts := func() map[string]int64 {
		out := map[string]int64{}
		for _, table := range []string{"users", "accounts", "warehouses", "store_settings", "coupons", "home_banners", "products", "product_variants", "inventory", "product_images", "sets", "set_slots", "set_members"} {
			var n int64
			if err := pool.QueryRow(ctx, "SELECT COUNT(*) FROM "+table).Scan(&n); err != nil {
				t.Fatal(err)
			}
			out[table] = n
		}
		return out
	}
	if err := seedAll(ctx, pool, admin); err != nil {
		t.Fatalf("first seed: %v", err)
	}
	first := counts()
	for table, n := range first {
		if n == 0 && table != "sets" && table != "set_slots" && table != "set_members" {
			t.Errorf("%s is empty after seeding", table)
		}
	}
	if first["products"] != 30 {
		t.Errorf("products = %d, want 30", first["products"])
	}
	var role string
	if err := pool.QueryRow(ctx, "SELECT role FROM users WHERE email = 'admin@lindaflor.com'").Scan(&role); err != nil || role != "admin" {
		t.Fatalf("admin role = %q, %v", role, err)
	}
	if err := seedAll(ctx, pool, admin); err != nil {
		t.Fatalf("second seed: %v", err)
	}
	second := counts()
	for table, n := range first {
		if second[table] != n {
			t.Errorf("%s: %d rows after re-seed, want %d", table, second[table], n)
		}
	}
}
