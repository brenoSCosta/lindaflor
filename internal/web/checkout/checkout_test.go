package checkout_test

import (
	"context"
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

var ctx = context.Background()

const unitPrice = 15990

func exec(t testing.TB, pool *pgxpool.Pool, sql string, args ...any) {
	t.Helper()
	if _, err := pool.Exec(ctx, sql, args...); err != nil {
		t.Fatalf("exec %q: %v", sql, err)
	}
}

func scalar[T any](t testing.TB, pool *pgxpool.Pool, sql string, args ...any) T {
	t.Helper()
	var v T
	if err := pool.QueryRow(ctx, sql, args...).Scan(&v); err != nil {
		t.Fatalf("query %q: %v", sql, err)
	}
	return v
}

func newID() uuid.UUID { return uuid.Must(uuid.NewV7()) }

func pixSettings(t testing.TB, pool *pgxpool.Pool) {
	t.Helper()
	exec(t, pool, `INSERT INTO store_settings (id, pix_key, pix_key_type, pix_merchant_name, pix_merchant_city, whatsapp_number)
		VALUES (true, '79999816511', 'phone', 'Linda Flor', 'Aracaju', '5579999816511')
		ON CONFLICT (id) DO UPDATE SET pix_key = EXCLUDED.pix_key, pix_key_type = EXCLUDED.pix_key_type,
		  pix_merchant_name = EXCLUDED.pix_merchant_name, pix_merchant_city = EXCLUDED.pix_merchant_city,
		  whatsapp_number = EXCLUDED.whatsapp_number`)
}

// seedVariant: an active product with one variant and stock in a default
// warehouse.
func seedVariant(t testing.TB, pool *pgxpool.Pool, name string, stock int32) uuid.UUID {
	t.Helper()
	productID, variantID, whID := newID(), newID(), newID()
	exec(t, pool, `INSERT INTO products (id, name, slug, price_in_cents, active) VALUES ($1, $2, $3, $4, true)`,
		productID, name, "p-"+productID.String(), unitPrice)
	exec(t, pool, `INSERT INTO product_variants (id, product_id, sku, size, color) VALUES ($1, $2, $3, 'm', 'azul')`,
		variantID, productID, "sku-"+variantID.String())
	exec(t, pool, `INSERT INTO warehouses (id, code, name, is_default, active) VALUES ($1, $2, 'W', true, true)
		ON CONFLICT DO NOTHING`, whID, "wh-"+strings.ReplaceAll(whID.String(), "-", ""))
	wh := scalar[uuid.UUID](t, pool, `SELECT id FROM warehouses WHERE is_default AND active ORDER BY created_at LIMIT 1`)
	exec(t, pool, `INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved) VALUES ($1, $2, $3, $4, 0)`,
		newID(), variantID, wh, stock)
	return variantID
}

// cartFor creates a cart with one line and hands its cookie to c.
func cartFor(t testing.TB, s *testutil.Server, c *testutil.Client, variantID uuid.UUID, qty int32) uuid.UUID {
	t.Helper()
	cartID, token := newID(), uuid.New()
	exec(t, s.Pool, `INSERT INTO carts (id, token) VALUES ($1, $2)`, cartID, token)
	exec(t, s.Pool, `INSERT INTO cart_items (id, cart_id, variant_id, quantity) VALUES ($1, $2, $3, $4)`, newID(), cartID, variantID, qty)
	u, _ := url.Parse(s.URL)
	c.HTTP.Jar.SetCookies(u, []*http.Cookie{{Name: testutil.DevCartCookie, Value: token.String(), Path: "/"}})
	return cartID
}

func payForm(email string) url.Values {
	return url.Values{
		"intent": {"pay"}, "guest_email": {email}, "name": {"Maria Silva"}, "phone": {"(79) 99981-6511"},
		"street": {"Rua A"}, "number": {"10"}, "neighborhood": {"Centro"}, "city": {"Aracaju"},
		"state": {"SE"}, "zip_code": {"49000-000"}, "notes": {""},
	}
}

func wantStatus(t *testing.T, res *testutil.Response, want int) {
	t.Helper()
	if res.StatusCode != want {
		t.Fatalf("status %d, want %d: %s", res.StatusCode, want, res.Body)
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
	t.Fatalf("missing %s toast %q in %v", kind, title, res.SetCookies())
}

func htmx(c *testutil.Client, path string) *testutil.Response {
	return c.Do(http.MethodGet, path, nil, http.Header{"HX-Request": {"true"}, "HX-Request-Type": {"partial"}})
}

// Seeded product → cart → checkout (static PIX) → order page with the
// guest token → status poll → cancel.
func TestCheckoutEndToEndStaticPix(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	pixSettings(t, s.Pool)
	variantID := seedVariant(t, s.Pool, "Biquíni Coral", 5)
	c := s.Client(t)
	cartFor(t, s, c, variantID, 2)

	res := c.Get("/checkout")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, "Checkout", "Biquíni Coral × 2", "Pagar com PIX", `name="website"`, "Resumo")

	// Coupon preview: unknown codes are reported inline, nothing is created.
	form := payForm("cliente@example.com")
	form.Set("intent", "apply")
	form.Set("coupon_code", "NAOEXISTE")
	res = c.PostForm("/checkout", form)
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, "Maria Silva")
	if n := scalar[int64](t, s.Pool, `SELECT COUNT(*) FROM orders`); n != 0 {
		t.Fatalf("apply created %d orders", n)
	}

	// Invalid pay form: inline error, 422.
	bad := payForm("sem-arroba")
	res = c.PostForm("/checkout", bad)
	wantStatus(t, res, http.StatusUnprocessableEntity)
	wantContains(t, res, "Informe um e-mail válido.")

	res = c.PostForm("/checkout", payForm("cliente@example.com"))
	wantStatus(t, res, http.StatusSeeOther)
	loc := res.Header.Get("Location")
	if !strings.HasPrefix(loc, "/pedido/") || !strings.Contains(loc, "?t=") {
		t.Fatalf("Location %q", loc)
	}
	orderPath, rawQuery, _ := strings.Cut(loc, "?")
	orderID := uuid.MustParse(strings.TrimPrefix(orderPath, "/pedido/"))
	token := uuid.MustParse(strings.TrimPrefix(rawQuery, "t="))

	// The cart is emptied.
	res = c.Get("/checkout")
	wantContains(t, res, "Seu carrinho está vazio")

	// Guest link (fresh client, no session).
	guest := s.Client(t)
	res = guest.Get(loc)
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, "Aguardando pagamento", "Pague com PIX", "PIX copia e cola:", "data:image/svg+xml",
		"Copiar código PIX", "Avise no WhatsApp", "https://wa.me/5579999816511", "Cancelar pedido",
		"Pedido recebido", "/pedido/status-proc?order_id="+orderID.String(), "every 10s", "Biquíni Coral")

	// Without (or with a wrong) token the order does not exist.
	res = guest.Get(orderPath)
	wantStatus(t, res, http.StatusNotFound)
	wantContains(t, res, "Pedido não encontrado")
	res = guest.Get(orderPath + "?t=" + uuid.NewString())
	wantStatus(t, res, http.StatusNotFound)

	// Status poll: plain text for scripts, poller fragment for htmx.
	poll := "/pedido/status-proc?order_id=" + orderID.String() + "&access_token=" + token.String()
	res = guest.Get(poll)
	wantStatus(t, res, http.StatusOK)
	if res.Body != "pending_payment" {
		t.Fatalf("poll body %q", res.Body)
	}
	res = htmx(guest, poll)
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, `id="order-status-poll"`, `hx-trigger="every 10s"`)
	res = guest.Get("/pedido/status-proc?order_id=" + orderID.String() + "&access_token=" + uuid.NewString())
	if res.Body != "unknown" {
		t.Fatalf("wrong token poll %q", res.Body)
	}
	res = htmx(guest, "/pedido/status-proc?order_id=nope")
	wantStatus(t, res, http.StatusOK)
	if strings.Contains(res.Body, "hx-get") {
		t.Fatalf("unknown order keeps polling: %s", res.Body)
	}

	// Cancel with a wrong token: not found, untouched.
	res = guest.PostForm(orderPath+"/cancelar", url.Values{"t": {uuid.NewString()}})
	wantStatus(t, res, http.StatusSeeOther)
	wantToast(t, res, toast.ErrorType, "Pedido não encontrado.")

	res = guest.PostForm(orderPath+"/cancelar", url.Values{"t": {token.String()}})
	wantStatus(t, res, http.StatusSeeOther)
	if got := res.Header.Get("Location"); got != loc {
		t.Fatalf("cancel Location %q, want %q", got, loc)
	}
	wantToast(t, res, toast.SuccessType, "Pedido cancelado.")
	if st := scalar[string](t, s.Pool, `SELECT status::text FROM orders WHERE id = $1`, orderID); st != "cancelled" {
		t.Fatalf("status %s", st)
	}
	// The stock hold is released.
	if r := scalar[int32](t, s.Pool, `SELECT COALESCE(SUM(reserved), 0)::int FROM inventory WHERE variant_id = $1`, variantID); r != 0 {
		t.Fatalf("reserved %d after cancel", r)
	}

	// The poller sees the change and reloads the page.
	res = htmx(guest, poll)
	if res.Header.Get("HX-Refresh") != "true" {
		t.Fatalf("no HX-Refresh after status change: %d %v", res.StatusCode, res.Header)
	}
	res = guest.Get(loc)
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, "Cancelado", "Pedido cancelado.")
	if strings.Contains(res.Body, "Pague com PIX") || strings.Contains(res.Body, "order-status-poll") {
		t.Fatal("cancelled order still shows payment / poller")
	}

	// A second cancel is refused.
	res = guest.PostForm(orderPath+"/cancelar", url.Values{"t": {token.String()}})
	wantToast(t, res, toast.ErrorType, "Este pedido já foi pago. Fale conosco no WhatsApp para cancelar.")
}

func TestCheckoutEmptyCart(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	res := s.Client(t).Get("/checkout")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, "Seu carrinho está vazio", "Ver catálogo")
	if !strings.Contains(res.Body, `content="noindex`) && !strings.Contains(res.Body, "noindex") {
		t.Fatal("checkout must be noindex")
	}
}

func TestCheckoutHoneypotDropsOrder(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	pixSettings(t, s.Pool)
	c := s.Client(t)
	cartFor(t, s, c, seedVariant(t, s.Pool, "Saída", 3), 1)
	form := payForm("bot@example.com")
	form.Set("website", "http://spam.example")
	res := c.PostForm("/checkout", form)
	wantStatus(t, res, http.StatusSeeOther)
	if res.Header.Get("Location") != "/checkout" {
		t.Fatalf("Location %q", res.Header.Get("Location"))
	}
	if n := scalar[int64](t, s.Pool, `SELECT COUNT(*) FROM orders`); n != 0 {
		t.Fatalf("honeypot created %d orders", n)
	}
}

func TestCheckoutPixUnavailable(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	c := s.Client(t)
	cartFor(t, s, c, seedVariant(t, s.Pool, "Maiô", 3), 1)
	res := c.Get("/checkout")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, "Pagamento via PIX indisponível no momento.")
	res = c.PostForm("/checkout", payForm("cliente@example.com"))
	wantStatus(t, res, http.StatusUnprocessableEntity)
	if n := scalar[int64](t, s.Pool, `SELECT COUNT(*) FROM orders`); n != 0 {
		t.Fatalf("created %d orders without PIX", n)
	}
}

func TestCheckoutRateLimitsOrders(t *testing.T) {
	t.Setenv("RATE_LIMIT_CHECKOUT_ORDER_PER_WINDOW", "1")
	ratelimit.Reset()
	t.Cleanup(ratelimit.Reset)
	s := testutil.NewServer(t, testutil.Options{KeepRateLimits: true})
	pixSettings(t, s.Pool)
	variantID := seedVariant(t, s.Pool, "Canga", 10)
	c := s.Client(t)
	cartFor(t, s, c, variantID, 1)
	res := c.PostForm("/checkout", payForm("a@example.com"))
	wantStatus(t, res, http.StatusSeeOther)
	cartFor(t, s, c, variantID, 1)
	res = c.PostForm("/checkout", payForm("b@example.com"))
	wantStatus(t, res, http.StatusUnprocessableEntity)
	wantContains(t, res, "Muitos pedidos em pouco tempo.")
}

// Signed-in buyers: the default saved address prefills the form, ?endereco
// picks another, and "Salvar este endereço" stores the typed one.
func TestCheckoutSavedAddresses(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	pixSettings(t, s.Pool)
	c := s.Client(t)
	res := c.SignUp("Compradora", testutil.UniqueEmail("buyer"), "Harbor-Violet-Tiger-4821-Lantern!")
	wantStatus(t, res, http.StatusOK)
	userID := uuid.MustParse(res.JSON(t)["user"].(map[string]any)["id"].(string))
	cartFor(t, s, c, seedVariant(t, s.Pool, "Top", 5), 1)

	res = c.Get("/checkout")
	wantContains(t, res, "Salvar este endereço")
	if strings.Contains(res.Body, "Endereços salvos:") {
		t.Fatal("no saved addresses yet")
	}
	form := payForm("compradora@example.com")
	form.Set("save_address", "1")
	res = c.PostForm("/checkout", form)
	wantStatus(t, res, http.StatusSeeOther)
	if n := scalar[int64](t, s.Pool, `SELECT COUNT(*) FROM user_addresses WHERE user_id = $1`, userID); n != 1 {
		t.Fatalf("saved %d addresses", n)
	}
	other := newID()
	exec(t, s.Pool, `INSERT INTO user_addresses (id, user_id, label, recipient_name, zip_code, street, number, complement, neighborhood, city, state, phone, is_default)
		VALUES ($1, $2, 'Trabalho', 'Quem recebe', '01001000', 'Av. Paulista', '1000', '', 'Bela Vista', 'São Paulo', 'SP', '11999998888', false)`, other, userID)

	cartFor(t, s, c, seedVariant(t, s.Pool, "Top 2", 5), 1)
	res = c.Get("/checkout")
	wantContains(t, res, "Endereços salvos:", "Rua A, 10", "Trabalho", `value="Maria Silva"`)
	res = c.Get("/checkout?endereco=" + other.String())
	wantContains(t, res, `value="Quem recebe"`, `value="01001-000"`)
	// The order page of a signed-in owner works without the token.
	if res := c.Get("/conta/pedidos"); res.StatusCode != http.StatusOK {
		t.Fatalf("conta/pedidos %d", res.StatusCode)
	}
}

func TestTrackPage(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	res := s.Client(t).Get("/pedido")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, "Acompanhe seu pedido")
	res = s.Client(t).Get("/pedido/not-a-uuid")
	wantStatus(t, res, http.StatusNotFound)
}
