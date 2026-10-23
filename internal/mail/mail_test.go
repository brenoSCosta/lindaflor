package mail_test

import (
	"context"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/mail"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

func TestEnqueueDedupes(t *testing.T) {
	ctx := context.Background()
	pool := testutil.NewPool(t)
	msg := mail.Msg{Kind: mail.KindOrderCreated, To: "  cliente@example.com ", Subject: "Pedido", HTML: "<p>x</p>", Text: "x", DedupeKey: "order_created:1"}
	for range 2 {
		if err := mail.Enqueue(ctx, pool, msg); err != nil {
			t.Fatal(err)
		}
	}
	msg.DedupeKey = ""
	if err := mail.Enqueue(ctx, pool, msg); err != nil {
		t.Fatal(err)
	}
	var n int
	var to, status string
	if err := pool.QueryRow(ctx, `SELECT count(*), min(to_address), min(status) FROM email_outbox`).Scan(&n, &to, &status); err != nil {
		t.Fatal(err)
	}
	if n != 2 || to != "cliente@example.com" || status != "pending" {
		t.Fatalf("n=%d to=%q status=%q", n, to, status)
	}
}
