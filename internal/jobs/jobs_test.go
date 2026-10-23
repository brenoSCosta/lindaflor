package jobs_test

import (
	"context"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/jobs"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

// One sweep pass expires an overdue static-PIX order and drops a stale
// guest cart; Start stops cleanly when its context is cancelled.
func TestSweepAndStart(t *testing.T) {
	pool := testutil.NewPool(t)
	ctx := context.Background()
	order, cart := uuid.Must(uuid.NewV7()), uuid.Must(uuid.NewV7())
	if _, err := pool.Exec(ctx, `INSERT INTO orders (id, guest_email, status, total_cents, reservation_expires_at)
		VALUES ($1, 'g@example.com', 'pending_payment', 1000, now() - interval '1 minute')`, order); err != nil {
		t.Fatal(err)
	}
	if _, err := pool.Exec(ctx, `INSERT INTO carts (id, token, updated_at) VALUES ($1, $2, now() - interval '40 days')`, cart, uuid.New()); err != nil {
		t.Fatal(err)
	}
	d := &app.Deps{Pool: pool, Cfg: testutil.TestConfig(false)}
	jobs.Sweep(ctx, d)
	var status string
	if err := pool.QueryRow(ctx, `SELECT status::text FROM orders WHERE id = $1`, order).Scan(&status); err != nil || status != "cancelled" {
		t.Fatal(status, err)
	}
	var carts int64
	if err := pool.QueryRow(ctx, `SELECT COUNT(*) FROM carts`).Scan(&carts); err != nil || carts != 0 {
		t.Fatal(carts, err)
	}

	runCtx, cancel := context.WithCancel(ctx)
	wg := jobs.Start(runCtx, d)
	cancel()
	wg.Wait()
}
