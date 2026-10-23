package mail_test

import (
	"context"
	"strings"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/mail"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestBackoffGrowsAndCaps(t *testing.T) {
	for attempts, want := range map[int32]int64{0: 30, 1: 30, 2: 60, 3: 120, 10: 3600, 1 << 30: 3600} {
		if got := mail.BackoffSecs(attempts); got != want {
			t.Errorf("BackoffSecs(%d) = %d, want %d", attempts, got, want)
		}
	}
}

func sample(kind, to string) mail.Msg {
	return mail.Msg{Kind: kind, To: to, Subject: "Assunto", HTML: "<p>oi</p>", Text: "oi"}
}

// recordingMailer captures sent messages.
type recordingMailer struct{ sent []mail.Msg }

func (r *recordingMailer) Kind() string { return "test" }
func (r *recordingMailer) Send(_ context.Context, m mail.Msg) error {
	r.sent = append(r.sent, m)
	return nil
}

func TestEnqueueDedupesAndDrainSendsAndScrubs(t *testing.T) {
	ctx := context.Background()
	pool := testutil.NewPool(t)
	key := "test:" + uuid.NewString()
	for range 2 {
		m := sample("test", "ana@example.com")
		m.DedupeKey = key
		if err := mail.Enqueue(ctx, pool, m); err != nil {
			t.Fatal(err)
		}
	}
	var count int
	if err := pool.QueryRow(ctx, `SELECT count(*) FROM email_outbox WHERE dedupe_key = $1`, key).Scan(&count); err != nil || count != 1 {
		t.Fatalf("count=%d err=%v", count, err)
	}
	report, err := mail.DrainOnce(ctx, pool, mail.LogMailer{}, mail.Links{}, 3)
	if err != nil || report.Sent != 1 {
		t.Fatalf("%+v %v", report, err)
	}
	var status, html, text string
	var attempts int
	if err := pool.QueryRow(ctx, `SELECT status, attempts, html_body, text_body FROM email_outbox WHERE dedupe_key = $1`, key).
		Scan(&status, &attempts, &html, &text); err != nil {
		t.Fatal(err)
	}
	if status != "sent" || attempts != 1 || html != "" || text != "" {
		t.Fatalf("status=%s attempts=%d bodies must be scrubbed", status, attempts)
	}
	again, err := mail.DrainOnce(ctx, pool, mail.LogMailer{}, mail.Links{}, 3)
	if err != nil || again != (mail.DrainReport{}) {
		t.Fatalf("%+v %v", again, err)
	}
}

func TestNewsletterKindsGetListUnsubscribe(t *testing.T) {
	ctx := context.Background()
	pool := testutil.NewPool(t)
	if err := mail.Enqueue(ctx, pool, sample(mail.KindAbandonedCart, "Ana@Example.com")); err != nil {
		t.Fatal(err)
	}
	if err := mail.Enqueue(ctx, pool, sample(mail.KindOrderCreated, "ana@example.com")); err != nil {
		t.Fatal(err)
	}
	rec := &recordingMailer{}
	links := mail.Links{Origin: "https://loja.example.com", Pepper: "p"}
	if _, err := mail.DrainOnce(ctx, pool, rec, links, 3); err != nil {
		t.Fatal(err)
	}
	if len(rec.sent) != 2 {
		t.Fatal(len(rec.sent))
	}
	for _, m := range rec.sent {
		switch m.Kind {
		case mail.KindAbandonedCart:
			if m.ListUnsubscribe != links.UnsubscribeURL("ana@example.com") {
				t.Fatal(m.ListUnsubscribe)
			}
		default:
			if m.ListUnsubscribe != "" {
				t.Fatal("transactional mail must not carry List-Unsubscribe")
			}
		}
	}
}

func TestAuthMailIsCappedPerRecipient(t *testing.T) {
	ctx := context.Background()
	pool := testutil.NewPool(t)
	to := "Bombed@Example.com"
	accepted := 0
	for i := range 8 {
		addr := to
		if i%2 == 1 {
			addr = strings.ToLower(to)
		}
		ok, err := mail.EnqueueThrottled(ctx, pool, sample(mail.RecipientThrottledKinds[i%3], addr))
		if err != nil {
			t.Fatal(err)
		}
		if ok {
			accepted++
		}
	}
	if accepted != mail.RecipientMaxPerWindow {
		t.Fatalf("accepted %d", accepted)
	}
	var queued int
	if err := pool.QueryRow(ctx, `SELECT count(*) FROM email_outbox WHERE lower(to_address) = 'bombed@example.com'`).Scan(&queued); err != nil || queued != mail.RecipientMaxPerWindow {
		t.Fatalf("queued %d %v", queued, err)
	}
	if ok, err := mail.EnqueueThrottled(ctx, pool, sample(mail.KindPasswordReset, "other@example.com")); err != nil || !ok {
		t.Fatal("other recipients are unaffected", err)
	}
	if err := mail.Enqueue(ctx, pool, sample(mail.KindOrderCreated, to)); err != nil {
		t.Fatal(err)
	}
	if _, err := pool.Exec(ctx, `UPDATE email_outbox SET created_at = now() - interval '2 hours' WHERE lower(to_address) = 'bombed@example.com'`); err != nil {
		t.Fatal(err)
	}
	if ok, err := mail.EnqueueThrottled(ctx, pool, sample(mail.KindVerifyEmail, to)); err != nil || !ok {
		t.Fatal("rows older than the window no longer count", err)
	}
}

func TestFailingSendBacksOffThenFails(t *testing.T) {
	ctx := context.Background()
	pool := testutil.NewPool(t)
	// Unparseable recipient: the SMTP mailer rejects it before any network
	// I/O, which exercises the retry path deterministically.
	if _, err := pool.Exec(ctx, `INSERT INTO email_outbox (id, kind, to_address, subject, html_body, text_body)
		VALUES ($1, 'test', 'not-an-address', 's', 'h', 't')`, uuid.New()); err != nil {
		t.Fatal(err)
	}
	mailer, err := mail.FromConfig(config.MailConfig{
		Transport: config.MailTransportSMTP,
		SMTP:      &config.SMTPConfig{Host: "127.0.0.1", Port: 1, TLS: config.SMTPNoTLS},
		From:      "Lindaflor <loja@example.com>",
	}, false)
	if err != nil {
		t.Fatal(err)
	}
	first, err := mail.DrainOnce(ctx, pool, mailer, mail.Links{}, 2)
	if err != nil || first.Retried != 1 {
		t.Fatalf("%+v %v", first, err)
	}
	var status string
	var delay float64
	if err := pool.QueryRow(ctx, `SELECT status, EXTRACT(EPOCH FROM next_attempt_at - now())::float8 FROM email_outbox`).Scan(&status, &delay); err != nil {
		t.Fatal(err)
	}
	if status != "pending" || delay <= 20 || delay > 31 {
		t.Fatalf("status=%s delay=%v", status, delay)
	}
	if _, err := pool.Exec(ctx, `UPDATE email_outbox SET next_attempt_at = now()`); err != nil {
		t.Fatal(err)
	}
	second, err := mail.DrainOnce(ctx, pool, mailer, mail.Links{}, 2)
	if err != nil || second.Failed != 1 {
		t.Fatalf("%+v %v", second, err)
	}
	var html string
	var lastError *string
	if err := pool.QueryRow(ctx, `SELECT status, html_body, last_error FROM email_outbox`).Scan(&status, &html, &lastError); err != nil {
		t.Fatal(err)
	}
	if status != "failed" || html != "" || lastError == nil || *lastError == "" {
		t.Fatalf("status=%s html=%q err=%v", status, html, lastError)
	}
}

func sqlcStatus(s string) sqlc.OrderStatus { return sqlc.OrderStatus(s) }

func TestOrderCreatedMailHidesStaticPixForMercadoPago(t *testing.T) {
	ctx := context.Background()
	pool := testutil.NewPool(t)
	links := mail.Links{Origin: "https://loja.example.com"}
	seed := func(provider string) uuid.UUID {
		id := uuid.New()
		if _, err := pool.Exec(ctx, `INSERT INTO orders (id, guest_email, status, total_cents, payment_provider, payment_meta)
			VALUES ($1, 'mp@example.com', 'pending_payment', 5000, $2, jsonb_build_object('pix_copy_paste', '000201STATICPIXCODE'))`, id, provider); err != nil {
			t.Fatal(err)
		}
		return id
	}
	body := func(orderID uuid.UUID) string {
		if err := mail.EnqueueOrderCreated(ctx, pool, links, orderID); err != nil {
			t.Fatal(err)
		}
		var text string
		if err := pool.QueryRow(ctx, `SELECT text_body FROM email_outbox WHERE dedupe_key = $1`, "order:"+orderID.String()+":created").Scan(&text); err != nil {
			t.Fatal(err)
		}
		return text
	}
	mp := seed("mercadopago")
	text := body(mp)
	mustNotContain(t, text, "STATICPIXCODE")
	mustContain(t, text, "Mercado Pago", "https://loja.example.com/pedido/"+mp.String()+"?t=")
	mustContain(t, body(seed("static_pix")), "000201STATICPIXCODE")
}

func TestOrderTransitionMails(t *testing.T) {
	ctx := context.Background()
	pool := testutil.NewPool(t)
	id := uuid.New()
	if _, err := pool.Exec(ctx, `INSERT INTO orders (id, guest_email, status, total_cents, tracking_code)
		VALUES ($1, 'cli@example.com', 'shipped', 5000, 'AA123456789BR')`, id); err != nil {
		t.Fatal(err)
	}
	links := mail.Links{}
	steps := []struct {
		from, to string
		expired  bool
		kind     string
	}{
		{"pending_payment", "paid", false, mail.KindPaymentConfirmed},
		{"paid", "shipped", false, mail.KindOrderShipped},
		{"pending_payment", "cancelled", true, mail.KindOrderExpired},
		{"paid", "cancelled", true, mail.KindOrderCancelled},
		{"paid", "refunded", false, mail.KindOrderRefunded},
		{"paid", "processing", false, ""},
	}
	for _, s := range steps {
		if err := mail.EnqueueOrderTransition(ctx, pool, links, id, sqlcStatus(s.from), sqlcStatus(s.to), s.expired); err != nil {
			t.Fatal(err)
		}
	}
	rows, err := pool.Query(ctx, `SELECT kind, text_body FROM email_outbox ORDER BY created_at, id`)
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	var kinds []string
	for rows.Next() {
		var kind, text string
		if err := rows.Scan(&kind, &text); err != nil {
			t.Fatal(err)
		}
		kinds = append(kinds, kind)
		if kind == mail.KindOrderShipped {
			mustContain(t, text, "Rastrear entrega: https://rastreamento.correios.com.br/app/index.php?objetos=AA123456789BR")
		}
	}
	if strings.Join(kinds, ",") != "payment_confirmed,order_shipped,order_expired,order_cancelled,order_refunded" {
		t.Fatal(kinds)
	}
}

func TestNewsletterWelcomeLinksToUnsubscribeAndDedupes(t *testing.T) {
	ctx := context.Background()
	pool := testutil.NewPool(t)
	links := mail.Links{Origin: "https://loja.example.com", Pepper: "pep"}
	for range 2 {
		if err := mail.SendNewsletterWelcome(ctx, pool, links, "Link@example.com", "", ""); err != nil {
			t.Fatal(err)
		}
	}
	var n int
	var text string
	if err := pool.QueryRow(ctx, `SELECT count(*), min(text_body) FROM email_outbox`).Scan(&n, &text); err != nil {
		t.Fatal(err)
	}
	if n != 1 {
		t.Fatal(n)
	}
	mustContain(t, text, "Cancelar inscrição: "+links.UnsubscribeURL("link@example.com"))
}

func TestAuthHooksUseTokenLinksAndCoupon(t *testing.T) {
	ctx := context.Background()
	pool := testutil.NewPool(t)
	if _, err := pool.Exec(ctx, `INSERT INTO store_settings (id, first_purchase_coupon_code) VALUES (true, ' BEMVINDA10 ')
		ON CONFLICT (id) DO UPDATE SET first_purchase_coupon_code = EXCLUDED.first_purchase_coupon_code`); err != nil {
		t.Fatal(err)
	}
	links := mail.Links{Origin: "https://loja.example.com"}
	if err := mail.SendVerifyEmail(ctx, pool, links, "a@example.com", "Ana", "tok/1"); err != nil {
		t.Fatal(err)
	}
	if err := mail.SendDeleteAccount(ctx, pool, links, "a@example.com", "Ana", "tok2"); err != nil {
		t.Fatal(err)
	}
	var text string
	if err := pool.QueryRow(ctx, `SELECT text_body FROM email_outbox WHERE kind = 'verify_email'`).Scan(&text); err != nil {
		t.Fatal(err)
	}
	mustContain(t, text, "https://loja.example.com/verify-email?token=tok%2F1", "cupom BEMVINDA10")
	if err := pool.QueryRow(ctx, `SELECT text_body FROM email_outbox WHERE kind = 'delete_account'`).Scan(&text); err != nil {
		t.Fatal(err)
	}
	mustContain(t, text, "https://loja.example.com/excluir-conta?token=tok2")
}
