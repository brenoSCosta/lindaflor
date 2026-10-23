package db_test

import (
	"context"
	"errors"
	"io/fs"
	"strings"
	"testing"
	"time"

	"github.com/wellington-vell/lindaflor/internal/db"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

func TestEmbeddedMigrationsAreForwardOnly(t *testing.T) {
	entries, err := fs.ReadDir(db.MigrationsFS(), ".")
	if err != nil || len(entries) == 0 {
		t.Fatalf("expected migrations: %v", err)
	}
	for _, e := range entries {
		b, _ := fs.ReadFile(db.MigrationsFS(), e.Name())
		if strings.Contains(string(b), "+goose Down") {
			t.Errorf("%s: down-migrations are forbidden (forward-fix + PITR)", e.Name())
		}
		if !strings.Contains(string(b), "+goose Up") {
			t.Errorf("%s: missing -- +goose Up", e.Name())
		}
	}
}

func TestOpenUsesPoolSettings(t *testing.T) {
	url := testutil.NewEmptyDatabase(t)
	pool, err := db.Open(context.Background(), url, db.PoolOptions{MaxConns: 7, ConnectTimeout: 8, IdleTimeout: 90, MigrateOnBoot: true})
	if err != nil {
		t.Fatal(err)
	}
	defer pool.Close()
	c := pool.Config()
	if c.MaxConns != 7 || c.ConnConfig.ConnectTimeout != 8*time.Second || c.MaxConnIdleTime != 90*time.Second {
		t.Fatalf("pool config: %d %v %v", c.MaxConns, c.ConnConfig.ConnectTimeout, c.MaxConnIdleTime)
	}
}

func TestOpenAbortsWhenMigrationsPending(t *testing.T) {
	_, err := db.Open(context.Background(), testutil.NewEmptyDatabase(t), db.PoolOptions{MigrateOnBoot: false})
	var pending *db.PendingMigrationsError
	if !errors.As(err, &pending) || len(pending.Versions) == 0 {
		t.Fatalf("want pending migrations, got %v", err)
	}
}

func TestOpenAcceptsMigratedSchemaWithoutBootMigrate(t *testing.T) {
	url := testutil.NewEmptyDatabase(t)
	pool, err := db.Open(context.Background(), url, db.PoolOptions{MigrateOnBoot: true})
	if err != nil {
		t.Fatal(err)
	}
	pool.Close()
	pool, err = db.Open(context.Background(), url, db.PoolOptions{MigrateOnBoot: false})
	if err != nil {
		t.Fatalf("current schema must boot: %v", err)
	}
	pool.Close()
}

func TestLikeHelpersEscapeWildcards(t *testing.T) {
	if db.EscapeLike("plain") != "plain" || db.EscapeLike(`50%_off\x`) != `50\%\_off\\x` ||
		db.LikeContains("a_b") != `%a\_b%` || db.LikeContains("") != "%%" {
		t.Fatal("escape")
	}
}

func TestSchemaHelpersAndTrigger(t *testing.T) {
	ctx := context.Background()
	pool := testutil.NewPool(t)
	q := sqlc.New(pool)
	if n, err := q.Ping(ctx); err != nil || n != 1 {
		t.Fatalf("ping: %v", err)
	}
	// users BEFORE DELETE keeps the contact email on orders.
	_, err := pool.Exec(ctx, `
		INSERT INTO users (id, name, email) VALUES ('00000000-0000-7000-8000-000000000001', 'A', 'a@example.com');
		INSERT INTO orders (id, user_id) VALUES ('00000000-0000-7000-8000-0000000000aa', '00000000-0000-7000-8000-000000000001');
		DELETE FROM users WHERE id = '00000000-0000-7000-8000-000000000001';`)
	if err != nil {
		t.Fatal(err)
	}
	var email *string
	if err := pool.QueryRow(ctx, `SELECT guest_email FROM orders WHERE id = '00000000-0000-7000-8000-0000000000aa'`).Scan(&email); err != nil || email == nil || *email != "a@example.com" {
		t.Fatalf("trigger: %v %v", email, err)
	}
	// live availability ignores holds of expired, unswept pending orders.
	_, err = pool.Exec(ctx, `
		INSERT INTO products (id, name, slug, price_in_cents) VALUES ('00000000-0000-7000-8000-0000000000b1', 'P', 'p', 1000);
		INSERT INTO product_variants (id, product_id, sku, size, color) VALUES ('00000000-0000-7000-8000-0000000000c1', '00000000-0000-7000-8000-0000000000b1', 'SKU', 'm', 'azul');
		INSERT INTO warehouses (id, code, name) VALUES ('00000000-0000-7000-8000-0000000000d1', 'W', 'W');
		INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved) VALUES ('00000000-0000-7000-8000-0000000000e1', '00000000-0000-7000-8000-0000000000c1', '00000000-0000-7000-8000-0000000000d1', 5, 3);
		INSERT INTO orders (id, reservation_expires_at) VALUES ('00000000-0000-7000-8000-0000000000f1', now() - interval '1 minute');
		INSERT INTO inventory_reservations (id, order_id, inventory_id, quantity) VALUES ('00000000-0000-7000-8000-000000000101', '00000000-0000-7000-8000-0000000000f1', '00000000-0000-7000-8000-0000000000e1', 2);`)
	if err != nil {
		t.Fatal(err)
	}
	avail, err := q.VariantLiveAvailable(ctx, [16]byte{0, 0, 0, 0, 0, 0, 0x70, 0, 0x80, 0, 0, 0, 0, 0, 0, 0xc1})
	if err != nil || avail != 4 {
		t.Fatalf("live available = %d (%v), want 4 (5 on hand - 3 reserved + 2 expired)", avail, err)
	}
}
