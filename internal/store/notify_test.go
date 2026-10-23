package store_test

import (
	"fmt"
	"strings"
	"testing"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/mail"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// Ports of the stock_alerts.rs, abandoned_cart.rs and newsletter.rs tests.

func seedAlertVariant(t *testing.T, pool *pgxpool.Pool, stock int32) (uuid.UUID, uuid.UUID) {
	t.Helper()
	productID := newID()
	exec(t, pool, `INSERT INTO products (id, name, slug, price_in_cents) VALUES ($1, 'Biquíni Sol', $2, 9990)`, productID, "biquini-"+productID.String())
	variantID := insertVariant(t, pool, productID, "m", "Azul")
	inv := insertInventory(t, pool, variantID, insertWarehouse(t, pool, true, true), stock, 0)
	return variantID, inv
}

func mailsTo(t *testing.T, pool *pgxpool.Pool, to string) [][2]string {
	t.Helper()
	rows, err := pool.Query(ctx, `SELECT kind, text_body FROM email_outbox WHERE to_address = $1 ORDER BY created_at`, to)
	mustNoErr(t, err)
	defer rows.Close()
	var out [][2]string
	for rows.Next() {
		var k, b string
		mustNoErr(t, rows.Scan(&k, &b))
		out = append(out, [2]string{k, b})
	}
	return out
}

func subscribe(t *testing.T, pool *pgxpool.Pool, variant uuid.UUID, email string) store.SubscribeOutcome {
	t.Helper()
	o, err := store.SubscribeStockAlert(ctx, pool, variant, email, nil)
	mustNoErr(t, err)
	return o
}

func TestStockAlertSubscribeValidatesAndDedupes(t *testing.T) {
	pool := testutil.NewPool(t)
	soldOut, _ := seedAlertVariant(t, pool, 0)
	inStock, _ := seedAlertVariant(t, pool, 3)
	if subscribe(t, pool, soldOut, "ana@example.com") != store.AlertSubscribed ||
		subscribe(t, pool, soldOut, "ana@example.com") != store.AlertAlreadySubscribed ||
		subscribe(t, pool, inStock, "ana@example.com") != store.AlertInStock ||
		subscribe(t, pool, newID(), "ana@example.com") != store.AlertUnknownVariant {
		t.Fatal("outcomes")
	}
	if n := scalar[int64](t, pool, `SELECT count(*) FROM stock_alerts`); n != 1 {
		t.Fatal(n)
	}
	for range store.MaxPendingAlertsPerEmail - 1 {
		v, _ := seedAlertVariant(t, pool, 0)
		if subscribe(t, pool, v, "ana@example.com") != store.AlertSubscribed {
			t.Fatal("under the cap")
		}
	}
	oneMore, _ := seedAlertVariant(t, pool, 0)
	if subscribe(t, pool, oneMore, "ana@example.com") != store.AlertTooMany || subscribe(t, pool, oneMore, "bia@example.com") != store.AlertSubscribed {
		t.Fatal("cap")
	}
	if msg, ok := store.AlertAlreadySubscribed.Toast(); !ok || msg != store.StockAlertSubscribedMessage {
		t.Fatal(msg)
	}
}

func notify(t *testing.T, pool *pgxpool.Pool) int {
	t.Helper()
	n, err := store.NotifyRestocked(ctx, pool)
	mustNoErr(t, err)
	return n
}

func TestRestockMailsEachAlertExactlyOnce(t *testing.T) {
	pool := testutil.NewPool(t)
	variant, inv := seedAlertVariant(t, pool, 0)
	subscribe(t, pool, variant, "ana@example.com")
	subscribe(t, pool, variant, "bia@example.com")
	if notify(t, pool) != 0 {
		t.Fatal("still sold out")
	}
	exec(t, pool, `UPDATE inventory SET quantity = 2 WHERE id = $1`, inv)
	if notify(t, pool) != 2 || notify(t, pool) != 0 {
		t.Fatal("exactly once")
	}
	mails := mailsTo(t, pool, "ana@example.com")
	if len(mails) != 1 || mails[0][0] != mail.KindBackInStock {
		t.Fatal(mails)
	}
	for _, want := range []string{"Biquíni Sol (M · Azul)", "?variant=" + variant.String(), store.StockAlertCancelURL("ana@example.com")} {
		if !strings.Contains(mails[0][1], want) {
			t.Errorf("mail lacks %q:\n%s", want, mails[0][1])
		}
	}
	exec(t, pool, `UPDATE inventory SET quantity = 0 WHERE id = $1`, inv)
	notify(t, pool)
	exec(t, pool, `UPDATE inventory SET quantity = 5 WHERE id = $1`, inv)
	if notify(t, pool) != 0 || len(mailsTo(t, pool, "ana@example.com")) != 1 || len(mailsTo(t, pool, "bia@example.com")) != 1 {
		t.Fatal("closed alerts never re-send")
	}
	exec(t, pool, `UPDATE inventory SET quantity = 0 WHERE id = $1`, inv)
	if subscribe(t, pool, variant, "ana@example.com") != store.AlertSubscribed {
		t.Fatal("fresh subscription")
	}
	exec(t, pool, `UPDATE inventory SET quantity = 1 WHERE id = $1`, inv)
	if notify(t, pool) != 1 || len(mailsTo(t, pool, "ana@example.com")) != 2 {
		t.Fatal("new alert")
	}
}

func TestInactiveProductsAndHeldStockDoNotTrigger(t *testing.T) {
	pool := testutil.NewPool(t)
	variant, inv := seedAlertVariant(t, pool, 0)
	subscribe(t, pool, variant, "ana@example.com")
	exec(t, pool, `UPDATE inventory SET quantity = 2, reserved = 2 WHERE id = $1`, inv)
	if notify(t, pool) != 0 {
		t.Fatal("fully reserved")
	}
	exec(t, pool, `UPDATE inventory SET quantity = 5 WHERE id = $1`, inv)
	exec(t, pool, `UPDATE products SET active = false WHERE id = (SELECT product_id FROM product_variants WHERE id = $1)`, variant)
	if notify(t, pool) != 0 || len(mailsTo(t, pool, "ana@example.com")) != 0 {
		t.Fatal("hidden product")
	}
}

func TestCancelLinkDropsPendingAlerts(t *testing.T) {
	pool := testutil.NewPool(t)
	variant, inv := seedAlertVariant(t, pool, 0)
	subscribe(t, pool, variant, "ana@example.com")
	u := store.StockAlertCancelURL("Ana@Example.com")
	if !strings.Contains(u, "email=ana%40example.com&token=") {
		t.Fatal(u)
	}
	token := u[strings.LastIndex(u, "token=")+6:]
	if !store.VerifyStockAlertCancelToken("ana@example.com", token) {
		t.Fatal("verify")
	}
	if store.MailLinks().VerifyUnsubscribeToken("ana@example.com", token) {
		t.Fatal("domain separation")
	}
	if n, err := store.CancelStockAlerts(ctx, pool, "ANA@example.com"); err != nil || n != 1 {
		t.Fatal(n, err)
	}
	exec(t, pool, `UPDATE inventory SET quantity = 3 WHERE id = $1`, inv)
	if notify(t, pool) != 0 {
		t.Fatal("cancelled alerts")
	}
	if slug, _ := store.VariantSlug(ctx, pool, variant); !strings.HasPrefix(slug, "biquini-") {
		t.Fatal(slug)
	}
}

// --- abandoned carts ---

func seedAbandonedVariant(t *testing.T, pool *pgxpool.Pool) uuid.UUID {
	t.Helper()
	productID := newID()
	exec(t, pool, `INSERT INTO products (id, name, slug, price_in_cents) VALUES ($1, 'Maiô Mar', $2, 15990)`, productID, "maio-"+productID.String())
	return insertVariant(t, pool, productID, "g", "Verde")
}

func seedIdleCart(t *testing.T, pool *pgxpool.Pool, variant *uuid.UUID, idle string, contact *string, user *uuid.UUID) uuid.UUID {
	t.Helper()
	id := newID()
	exec(t, pool, fmt.Sprintf(`INSERT INTO carts (id, token, user_id, contact_email, created_at, updated_at)
		VALUES ($1, $2, $3, $4, now() - interval '%[1]s', now() - interval '%[1]s')`, idle), id, uuid.New(), user, contact)
	if variant != nil {
		exec(t, pool, fmt.Sprintf(`INSERT INTO cart_items (id, cart_id, variant_id, quantity, created_at, updated_at)
			VALUES ($1, $2, $3, 2, now() - interval '%[1]s', now() - interval '%[1]s')`, idle), newID(), id, *variant)
	}
	return id
}

func reminders(t *testing.T, pool *pgxpool.Pool) [][2]string {
	t.Helper()
	rows, err := pool.Query(ctx, `SELECT to_address, text_body FROM email_outbox WHERE kind = 'abandoned_cart' ORDER BY to_address`)
	mustNoErr(t, err)
	defer rows.Close()
	var out [][2]string
	for rows.Next() {
		var to, b string
		mustNoErr(t, rows.Scan(&to, &b))
		out = append(out, [2]string{to, b})
	}
	return out
}

func sendReminders(t *testing.T, pool *pgxpool.Pool) int {
	t.Helper()
	n, err := store.SendAbandonedCartReminders(ctx, pool, 180)
	mustNoErr(t, err)
	return n
}

func TestRemindsOnlyEligibleCarts(t *testing.T) {
	pool := testutil.NewPool(t)
	v := seedAbandonedVariant(t, pool)
	seedIdleCart(t, pool, &v, "4 hours", str("ok@example.com"), nil)
	seedIdleCart(t, pool, nil, "4 hours", str("empty@example.com"), nil)
	seedIdleCart(t, pool, &v, "1 hour", str("recent@example.com"), nil)
	seedIdleCart(t, pool, &v, "10 days", str("old@example.com"), nil)
	seedIdleCart(t, pool, &v, "4 hours", nil, nil)
	converted := seedIdleCart(t, pool, &v, "4 hours", str("bought@example.com"), nil)
	exec(t, pool, `INSERT INTO orders (id, guest_email, cart_id, created_at) VALUES ($1, 'bought@example.com', $2, now() - interval '3 hours')`, newID(), converted)
	seedIdleCart(t, pool, &v, "4 hours", str("out@example.com"), nil)
	if _, err := store.UnsubscribeNewsletter(ctx, pool, "OUT@example.com"); err != nil {
		t.Fatal(err)
	}
	user := newID()
	exec(t, pool, `INSERT INTO users (id, name, email, email_verified) VALUES ($1, 'Ana', 'ana@example.com', true)`, user)
	seedIdleCart(t, pool, &v, "5 hours", nil, &user)
	if n := sendReminders(t, pool); n != 2 {
		t.Fatal(n)
	}
	mails := reminders(t, pool)
	if len(mails) != 2 || mails[0][0] != "ana@example.com" || mails[1][0] != "ok@example.com" {
		t.Fatal(mails)
	}
	body := mails[1][1]
	for _, want := range []string{"Maiô Mar — G · Verde (x2) — R$", "/checkout", store.UnsubscribeURL("ok@example.com")} {
		if !strings.Contains(body, want) {
			t.Errorf("body lacks %q:\n%s", want, body)
		}
	}
	if !strings.Contains(mails[0][1], "Olá, Ana!") {
		t.Fatal(mails[0][1])
	}
	if sendReminders(t, pool) != 0 || len(reminders(t, pool)) != 2 {
		t.Fatal("never twice")
	}
}

func TestAtMostOneReminderPerCartAndAddressPerWeek(t *testing.T) {
	pool := testutil.NewPool(t)
	v := seedAbandonedVariant(t, pool)
	cart := seedIdleCart(t, pool, &v, "4 hours", str("ana@example.com"), nil)
	seedIdleCart(t, pool, &v, "5 hours", str("ana@example.com"), nil)
	if sendReminders(t, pool) != 1 {
		t.Fatal("one mail for both carts")
	}
	exec(t, pool, `UPDATE carts SET abandoned_reminded_at = now() - interval '2 days', updated_at = now() - interval '4 hours' WHERE id = $1`, cart)
	exec(t, pool, `UPDATE email_outbox SET created_at = now() - interval '2 days'`)
	if sendReminders(t, pool) != 0 {
		t.Fatal("7-day cap")
	}
	exec(t, pool, `UPDATE carts SET abandoned_reminded_at = now() - interval '8 days' WHERE id = $1`, cart)
	exec(t, pool, `UPDATE email_outbox SET created_at = now() - interval '8 days'`)
	if sendReminders(t, pool) != 1 || len(reminders(t, pool)) != 2 {
		t.Fatal("a week later")
	}
}

func TestCheckoutEmailIsRememberedWithoutCountingAsActivity(t *testing.T) {
	pool := testutil.NewPool(t)
	v := seedAbandonedVariant(t, pool)
	cart := seedIdleCart(t, pool, &v, "4 hours", nil, nil)
	if sendReminders(t, pool) != 0 {
		t.Fatal("no address")
	}
	mustNoErr(t, store.RememberCheckoutEmail(ctx, pool, cart, "guest@example.com"))
	if sendReminders(t, pool) != 1 || reminders(t, pool)[0][0] != "guest@example.com" {
		t.Fatal("remembered")
	}
}

// --- newsletter ---

func TestSubscribeQueuesOneWelcomeAndFlagsDuplicates(t *testing.T) {
	pool := testutil.NewPool(t)
	first, err := store.SubscribeNewsletter(ctx, pool, "news@example.com", str("Ana"))
	mustNoErr(t, err)
	if first.Kind != store.NewsletterSubscribed {
		t.Fatal(first)
	}
	again, err := store.SubscribeNewsletter(ctx, pool, "NEWS@example.com", nil)
	mustNoErr(t, err)
	if again.Kind != store.NewsletterAlreadySubscribed {
		t.Fatal(again)
	}
	mails := mailsTo(t, pool, "news@example.com")
	if len(mails) != 1 || mails[0][0] != "newsletter_welcome" || !strings.Contains(mails[0][1], "Olá, Ana!") {
		t.Fatal(mails)
	}
}

func TestUnsubscribeTokenIsSignedPerAddress(t *testing.T) {
	l := mail.Links{Pepper: "pepper"}
	token := l.UnsubscribeToken("Ana@Example.com")
	if len(token) != 64 || !l.VerifyUnsubscribeToken(" ana@example.com ", token) || l.VerifyUnsubscribeToken("bia@example.com", token) ||
		(mail.Links{Pepper: "other-pepper"}).VerifyUnsubscribeToken("ana@example.com", token) {
		t.Fatal("token")
	}
	for _, bad := range []string{"", "zz", token[:62]} {
		if l.VerifyUnsubscribeToken("ana@example.com", bad) {
			t.Error(bad)
		}
	}
	dev := mail.Links{}.UnsubscribeToken("ana@example.com")
	if !(mail.Links{}).VerifyUnsubscribeToken("ana@example.com", dev) || dev == token {
		t.Fatal("dev key")
	}
	u := store.UnsubscribeURL("Ana+news@Example.com")
	if !strings.Contains(u, "/newsletter/cancelar") || !strings.Contains(u, "email=ana%2Bnews%40example.com&token=") {
		t.Fatal(u)
	}
	if !store.VerifyUnsubscribeToken("ana+news@example.com", u[strings.LastIndex(u, "token=")+6:]) {
		t.Fatal("verify")
	}
}

func TestUnsubscribeThenResubscribe(t *testing.T) {
	pool := testutil.NewPool(t)
	email := "leave@example.com"
	if o, _ := store.SubscribeNewsletter(ctx, pool, email, str("Ana")); o.Kind != store.NewsletterSubscribed {
		t.Fatal(o)
	}
	if ok, err := store.UnsubscribeNewsletter(ctx, pool, "LEAVE@example.com"); err != nil || !ok {
		t.Fatal(ok, err)
	}
	if ok, _ := store.UnsubscribeNewsletter(ctx, pool, email); ok {
		t.Fatal("idempotent")
	}
	if ok, _ := store.UnsubscribeNewsletter(ctx, pool, "nobody@example.com"); ok {
		t.Fatal("unknown")
	}
	if scalar[bool](t, pool, `SELECT unsubscribed_at IS NULL FROM newsletter_subscribers WHERE email = $1`, email) {
		t.Fatal("still active")
	}
	if o, _ := store.SubscribeNewsletter(ctx, pool, email, nil); o.Kind != store.NewsletterResubscribed {
		t.Fatal(o)
	}
	var active bool
	var name *string
	mustNoErr(t, pool.QueryRow(ctx, `SELECT unsubscribed_at IS NULL, name FROM newsletter_subscribers WHERE email = $1`, email).Scan(&active, &name))
	if !active || name == nil || *name != "Ana" {
		t.Fatal(active, name)
	}
	if o, _ := store.SubscribeNewsletter(ctx, pool, email, nil); o.Kind != store.NewsletterAlreadySubscribed {
		t.Fatal(o)
	}
	if n := scalar[int64](t, pool, `SELECT count(*) FROM email_outbox WHERE to_address = $1`, email); n != 1 {
		t.Fatal(n)
	}
}

func TestWelcomeMailLinksToUnsubscribe(t *testing.T) {
	pool := testutil.NewPool(t)
	_, err := store.SubscribeNewsletter(ctx, pool, "link@example.com", nil)
	mustNoErr(t, err)
	text := scalar[string](t, pool, `SELECT text_body FROM email_outbox WHERE to_address = 'link@example.com'`)
	if !strings.Contains(text, "Cancelar inscrição: "+store.UnsubscribeURL("link@example.com")) {
		t.Fatal(text)
	}
}

func TestSubscribeReportsRealDBErrors(t *testing.T) {
	pool := testutil.NewPool(t)
	exec(t, pool, `DROP TABLE newsletter_subscribers`)
	if _, err := store.SubscribeNewsletter(ctx, pool, "x@example.com", nil); err == nil {
		t.Fatal("missing table must not read as a duplicate")
	}
}

func TestNewsletterExportSkipsUnsubscribed(t *testing.T) {
	pool := testutil.NewPool(t)
	_, err := store.SubscribeNewsletter(ctx, pool, "stay@example.com", nil)
	mustNoErr(t, err)
	_, err = store.SubscribeNewsletter(ctx, pool, "=go@example.com", nil)
	mustNoErr(t, err)
	_, err = store.UnsubscribeNewsletter(ctx, pool, "=go@example.com")
	mustNoErr(t, err)
	active, err := store.ActiveSubscribers(ctx, pool)
	mustNoErr(t, err)
	if len(active) != 1 || active[0].Email != "stay@example.com" {
		t.Fatal(active)
	}
	adm, err := store.LoadNewsletterAdmin(ctx, pool)
	mustNoErr(t, err)
	if len(adm.Subscribers) != 2 || adm.Active != 1 || adm.Cancelled != 1 {
		t.Fatal(adm)
	}
	for _, s := range adm.Subscribers {
		if s.Email == "=go@example.com" && !strings.HasPrefix(s.StatusLabel(), "Cancelada em ") {
			t.Fatal(s.StatusLabel())
		}
	}
	csv, err := store.NewsletterCSV(ctx, pool)
	mustNoErr(t, err)
	if !strings.HasPrefix(csv, "email,name,created_at\n\"stay@example.com\",\"\",") {
		t.Fatal(csv)
	}
}
