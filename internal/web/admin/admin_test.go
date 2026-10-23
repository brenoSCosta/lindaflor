package admin_test

import (
	"bytes"
	"context"
	"fmt"
	"mime/multipart"
	"net/http"
	"net/textproto"
	"net/url"
	"strings"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestMain(m *testing.M) { testutil.Main(m) }

const strongPW = "Harbor-Violet-Tiger-4821-Lantern!"

// account is a signed-up user with its own cookie jar.
type account struct {
	c     *testutil.Client
	id    uuid.UUID
	email string
	name  string
}

func signUp(t *testing.T, s *testutil.Server, prefix, name string) account {
	t.Helper()
	c := s.Client(t)
	email := testutil.UniqueEmail(prefix)
	res := c.SignUp(name, email, strongPW)
	if res.StatusCode != http.StatusOK {
		t.Fatalf("sign-up: %d %s", res.StatusCode, res.Body)
	}
	user, _ := res.JSON(t)["user"].(map[string]any)
	return account{c: c, id: uuid.MustParse(user["id"].(string)), email: email, name: name}
}

func makeAdmin(t *testing.T, s *testutil.Server, id uuid.UUID) {
	t.Helper()
	exec(t, s, `UPDATE users SET role = 'admin' WHERE id = $1`, id)
}

func adminAccount(t *testing.T, s *testutil.Server) account {
	t.Helper()
	a := signUp(t, s, "admin", "Admin Teste")
	makeAdmin(t, s, a.id)
	return a
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

func newID() uuid.UUID { return uuid.Must(uuid.NewV7()) }

func wantStatus(t *testing.T, res *testutil.Response, want int) {
	t.Helper()
	if res.StatusCode != want {
		t.Fatalf("status = %d, want %d; body: %.600s", res.StatusCode, want, res.Body)
	}
}

// wantRedirect asserts a 303 to location (prefix match when it ends in *).
func wantRedirect(t *testing.T, res *testutil.Response, location string) {
	t.Helper()
	if res.StatusCode != http.StatusSeeOther {
		t.Fatalf("status = %d, want 303; body: %.600s", res.StatusCode, res.Body)
	}
	got := res.Header.Get("Location")
	if p, ok := strings.CutSuffix(location, "*"); ok {
		if !strings.HasPrefix(got, p) {
			t.Fatalf("Location = %q, want prefix %q", got, p)
		}
		return
	}
	if got != location {
		t.Fatalf("Location = %q, want %q", got, location)
	}
}

func wantContains(t *testing.T, body string, parts ...string) {
	t.Helper()
	for _, p := range parts {
		if !strings.Contains(body, p) {
			t.Fatalf("body missing %q:\n%.3000s", p, body)
		}
	}
}

func wantNotContains(t *testing.T, body string, parts ...string) {
	t.Helper()
	for _, p := range parts {
		if strings.Contains(body, p) {
			t.Fatalf("body unexpectedly contains %q", p)
		}
	}
}

// flash returns the toasts queued in the client's toast cookie.
func flash(t *testing.T, c *testutil.Client) []toast.Toast {
	t.Helper()
	raw := c.Cookie(toast.CookieName(false))
	if raw == "" {
		return nil
	}
	return toast.DecodeQueue(raw)
}

// wantToast asserts the newest queued toast (cookie flash).
func wantToast(t *testing.T, c *testutil.Client, kind toast.Type, title string) {
	t.Helper()
	q := flash(t, c)
	if len(q) == 0 {
		t.Fatalf("no toast queued, want %q", title)
	}
	last := q[len(q)-1]
	if last.Type != kind || last.Title != title {
		t.Fatalf("toast = %s %q, want %s %q", last.Type, last.Title, kind, title)
	}
}

// form builds url.Values from k, v pairs.
func form(kv ...string) url.Values {
	v := url.Values{}
	for i := 0; i+1 < len(kv); i += 2 {
		v.Add(kv[i], kv[i+1])
	}
	return v
}

// filePart is one uploaded file of a multipart request.
type filePart struct {
	field, name, contentType string
	body                     []byte
}

func pngBytes() []byte { return []byte{0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0} }

// postMultipart sends a multipart form.
func postMultipart(t *testing.T, c *testutil.Client, path string, fields url.Values, files ...filePart) *testutil.Response {
	t.Helper()
	var buf bytes.Buffer
	mw := multipart.NewWriter(&buf)
	for k, vs := range fields {
		for _, v := range vs {
			if err := mw.WriteField(k, v); err != nil {
				t.Fatal(err)
			}
		}
	}
	for _, f := range files {
		h := textproto.MIMEHeader{}
		h.Set("Content-Disposition", fmt.Sprintf(`form-data; name=%q; filename=%q`, f.field, f.name))
		h.Set("Content-Type", f.contentType)
		w, err := mw.CreatePart(h)
		if err != nil {
			t.Fatal(err)
		}
		if _, err := w.Write(f.body); err != nil {
			t.Fatal(err)
		}
	}
	if err := mw.Close(); err != nil {
		t.Fatal(err)
	}
	return c.Do(http.MethodPost, path, &buf, http.Header{"Content-Type": {mw.FormDataContentType()}})
}

// seedProduct inserts an active product with one variant stocked in a
// default warehouse; returns product, variant and inventory ids.
func seedProduct(t *testing.T, s *testutil.Server, name string, qty int) (productID, variantID, inventoryID uuid.UUID) {
	t.Helper()
	productID, variantID, inventoryID = newID(), newID(), newID()
	exec(t, s, `INSERT INTO products (id, name, slug, price_in_cents, category, active) VALUES ($1, $2, $3, 15990, 'biquini', true)`,
		productID, name, "p-"+productID.String())
	exec(t, s, `INSERT INTO product_variants (id, product_id, sku, size, color) VALUES ($1, $2, $3, 'm', 'Coral')`,
		variantID, productID, "SKU-"+variantID.String())
	wh := scalarOrNew(t, s)
	exec(t, s, `INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved) VALUES ($1, $2, $3, $4, 0)`,
		inventoryID, variantID, wh, qty)
	return productID, variantID, inventoryID
}

func scalarOrNew(t *testing.T, s *testutil.Server) uuid.UUID {
	t.Helper()
	var id uuid.UUID
	err := s.Pool.QueryRow(context.Background(), `SELECT id FROM warehouses ORDER BY created_at LIMIT 1`).Scan(&id)
	if err == nil {
		return id
	}
	id = newID()
	exec(t, s, `INSERT INTO warehouses (id, code, name, is_default, active) VALUES ($1, 'PRINCIPAL', 'Depósito Principal', true, true)`, id)
	return id
}

// seedOrder inserts an order with one item of variantID.
func seedOrder(t *testing.T, s *testutil.Server, variantID uuid.UUID, status, email string) uuid.UUID {
	t.Helper()
	id := newID()
	exec(t, s, `INSERT INTO orders (id, guest_email, status, subtotal_cents, total_cents, shipping_address, payment_meta)
		VALUES ($1, $2, $3::order_status, 15990, 15990,
		'{"street":"Rua A","number":"10","neighborhood":"Centro","city":"Aracaju","state":"SE","zip_code":"49000000","phone":"79999999999"}',
		'{"pix_copy_paste":"00020126PIXCODE"}')`, id, email, status)
	exec(t, s, `INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
		VALUES ($1, $2, $3, 'Biquíni Teste', 'M / Coral', 1, 15990)`, newID(), id, variantID)
	return id
}

// adminRoutes lists every admin route (method, path) for the guard test.
var adminRoutes = [][2]string{
	{"GET", "/admin"},
	{"GET", "/admin/usuarios"},
	{"GET", "/admin/usuarios/lista"},
	{"POST", "/admin/usuarios/papel"},
	{"POST", "/admin/usuarios/nome"},
	{"POST", "/admin/usuarios/banir"},
	{"POST", "/admin/usuarios/desbanir"},
	{"POST", "/admin/usuarios/atuar"},
	{"POST", "/admin/usuarios/sessoes/revogar"},
	{"POST", "/admin/usuarios/sessoes/revogar-todas"},
	{"POST", "/admin/usuarios/remover"},
	{"GET", "/admin/produtos"},
	{"GET", "/admin/produtos/lista"},
	{"GET", "/admin/produtos/novo"},
	{"POST", "/admin/produtos/create"},
	{"POST", "/admin/produtos/update"},
	{"GET", "/admin/produtos/" + uuid.NewString()},
	{"GET", "/admin/sets"},
	{"POST", "/admin/sets/create"},
	{"POST", "/admin/sets/toggle"},
	{"POST", "/admin/sets/delete"},
	{"GET", "/admin/sets/" + uuid.NewString()},
	{"POST", "/admin/sets/" + uuid.NewString() + "/update"},
	{"GET", "/admin/pedidos"},
	{"GET", "/admin/pedidos/lista"},
	{"GET", "/admin/pedidos/" + uuid.NewString()},
	{"POST", "/admin/pedidos/" + uuid.NewString()},
	{"GET", "/admin/trocas"},
	{"GET", "/admin/trocas/" + uuid.NewString()},
	{"POST", "/admin/trocas/" + uuid.NewString()},
	{"GET", "/admin/estoque"},
	{"POST", "/admin/estoque"},
	{"GET", "/admin/estoque/lista"},
	{"GET", "/admin/configuracoes"},
	{"POST", "/admin/configuracoes"},
	{"GET", "/admin/cupons"},
	{"GET", "/admin/cupons/lista"},
	{"GET", "/admin/cupons/usuarios"},
	{"POST", "/admin/cupons/create"},
	{"POST", "/admin/cupons/toggle"},
	{"POST", "/admin/cupons/toggle-proc"},
	{"POST", "/admin/cupons/delete"},
	{"POST", "/admin/cupons/assign"},
	{"POST", "/admin/cupons/unassign"},
	{"GET", "/admin/avaliacoes"},
	{"POST", "/admin/avaliacoes"},
	{"GET", "/admin/banners"},
	{"POST", "/admin/banners/create"},
	{"POST", "/admin/banners/toggle"},
	{"POST", "/admin/banners/delete"},
	{"GET", "/admin/newsletter"},
	{"GET", "/admin/newsletter/export-csv"},
	{"GET", "/admin/relatorios"},
	{"GET", "/admin/relatorios/pedidos.csv"},
}

func send(c *testutil.Client, method, path string) *testutil.Response {
	if method == http.MethodGet {
		return c.Get(path)
	}
	return c.PostForm(path, url.Values{"x": {"1"}})
}

func TestAdminRoutesRequireAdmin(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	anon := s.Client(t)
	user := signUp(t, s, "customer", "Cliente")
	for _, rt := range adminRoutes {
		res := send(anon, rt[0], rt[1])
		if res.StatusCode != http.StatusSeeOther || !strings.HasPrefix(res.Header.Get("Location"), "/login") {
			t.Errorf("anonymous %s %s = %d %q, want 303 /login", rt[0], rt[1], res.StatusCode, res.Header.Get("Location"))
		}
		res = send(user.c, rt[0], rt[1])
		if res.StatusCode != http.StatusSeeOther || res.Header.Get("Location") != "/dashboard" {
			t.Errorf("customer %s %s = %d %q, want 303 /dashboard", rt[0], rt[1], res.StatusCode, res.Header.Get("Location"))
		}
	}
	// htmx fragment requests are redirected with HX-Location, never 303.
	res := user.c.HTMX().Get("/admin/produtos/lista?q=x")
	wantStatus(t, res, http.StatusOK)
	if res.Header.Get("HX-Location") != "/dashboard" {
		t.Fatalf("HX-Location = %q", res.Header.Get("HX-Location"))
	}
	wantNotContains(t, res.Body, "product-results")
}

func TestAdminPagesRender(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	pages := map[string][]string{
		"/admin":               {"Bem-vinda ao painel", "Total de Produtos", "Estoque Baixo", "Pedidos Recentes"},
		"/admin/usuarios":      {"Gerenciamento de usuários", `id="user-search"`, "Admin Teste"},
		"/admin/produtos":      {"Lista de produtos cadastrados no catálogo.", "Novo produto", `id="product-search"`},
		"/admin/produtos/novo": {"Cadastre um produto com variantes", `enctype="multipart/form-data"`, "Criar produto"},
		"/admin/sets":          {"Conjuntos", "Nenhum conjunto cadastrado ainda.", "Criar conjunto"},
		"/admin/pedidos":       {"Acompanhe pedidos da loja", "Em separação", "Nenhum pedido encontrado."},
		"/admin/trocas":        {"Trocas e devoluções", "Em aberto", "Nenhuma solicitação encontrada."},
		"/admin/estoque":       {"Saldo por depósito.", "Saldo por Depósito", "Depósitos"},
		"/admin/configuracoes": {"Configurações da loja", "PIX (chave fixa)", "Salvar configurações"},
		"/admin/cupons":        {"Novo cupom", "Nenhum cupom cadastrado.", "Criar cupom"},
		"/admin/avaliacoes":    {"Avaliações", "pendentes de moderação", "Nenhuma avaliação ainda"},
		"/admin/banners":       {"Banners da home", "Nenhum banner cadastrado ainda.", "Criar banner"},
		"/admin/newsletter":    {"Newsletter", "Nenhum inscritor ainda", "Exportar CSV (ativas)"},
		"/admin/relatorios":    {"Relatórios", "Receita por dia", "Exportar pedidos (CSV)", "horário de Brasília"},
	}
	for path, want := range pages {
		res := a.c.Get(path)
		wantStatus(t, res, http.StatusOK)
		wantContains(t, res.Body, want...)
		wantContains(t, res.Body, "<!doctype html>", "noindex")
	}
	// Missing records.
	wantContains(t, a.c.Get("/admin/produtos/"+uuid.NewString()).Body, "Produto não encontrado", "Voltar para produtos")
	wantContains(t, a.c.Get("/admin/produtos/not-a-uuid").Body, "Produto não encontrado")
	wantContains(t, a.c.Get("/admin/sets/"+uuid.NewString()).Body, "Conjunto não encontrado", "Voltar para conjuntos")
	wantStatus(t, a.c.Get("/admin/pedidos/"+uuid.NewString()), http.StatusNotFound)
	wantStatus(t, a.c.Get("/admin/pedidos/nope"), http.StatusNotFound)
	wantStatus(t, a.c.Get("/admin/trocas/"+uuid.NewString()), http.StatusNotFound)
}

func TestDashboardFigures(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	_, variant, _ := seedProduct(t, s, "Biquíni Painel", 2)
	order := seedOrder(t, s, variant, "paid", "painel@example.com")
	res := a.c.Get("/admin")
	wantStatus(t, res, http.StatusOK)
	// 2 available <= threshold 5: one low-stock variant; R$ 159 revenue.
	wantContains(t, res.Body, order.String(), "painel@example.com", "R$ 159", `text-destructive">1<`)
}
