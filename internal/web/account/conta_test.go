package account_test

import (
	"fmt"
	"net/http"
	"net/url"
	"strings"
	"testing"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func insertOrder(t *testing.T, pool *pgxpool.Pool, userID uuid.UUID, totalCents int64) uuid.UUID {
	t.Helper()
	id := uuid.Must(uuid.NewV7())
	exec(t, pool, `INSERT INTO orders (id, user_id, status, total_cents, access_token) VALUES ($1, $2, 'paid', $3, $4)`,
		id, userID, totalCents, uuid.New())
	return id
}

// tests/dashboard.rs: anonymous_dashboard_redirects_to_login
func TestAnonymousDashboardRedirectsToLogin(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := signUp(t, s, "dash")
	orderID := insertOrder(t, s.Pool, a.id, 424242)
	for _, path := range []string{"/dashboard", "/conta/pedidos", "/conta/pedidos/lista", "/conta/enderecos", "/settings", "/settings/two-factor"} {
		res := s.Client(t).Get(path)
		wantRedirect(t, res, "/login")
		wantMissing(t, res, orderID.String(), "Pedidos recentes")
	}
}

// tests/dashboard.rs: dashboard_scopes_orders_to_current_user
func TestDashboardScopesOrdersToCurrentUser(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a, b := signUp(t, s, "dash-a"), signUp(t, s, "dash-b")
	orderA := insertOrder(t, s.Pool, a.id, 424242)
	orderB := insertOrder(t, s.Pool, b.id, 777777)
	res := a.c.Get("/dashboard")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, orderA.String()[:8], "R$ 4242", "Pedidos recentes", "Ver pedidos", "Ajustar conta", "Pago")
	wantMissing(t, res, orderB.String(), "7777")
	// App shell: sidebar navigation and account menu.
	wantContains(t, res, `id="app-sidebar"`, "Painel", "Contas vinculadas", `action="/logout"`)
	wantMissing(t, res, "/admin/usuarios")
}

// tests/dashboard.rs: load_recent_orders_is_scoped_to_owner
func TestLoadRecentOrdersIsScopedToOwner(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a, b := signUp(t, s, "dash-a"), signUp(t, s, "dash-b")
	orderA := insertOrder(t, s.Pool, a.id, 424242)
	orderB := insertOrder(t, s.Pool, b.id, 777777)
	rowsA, err := store.LoadRecentOrders(t.Context(), s.Pool, a.id)
	if err != nil {
		t.Fatal(err)
	}
	if len(rowsA) != 1 || rowsA[0].ID != orderA {
		t.Fatalf("rows a: %+v", rowsA)
	}
	rowsB, err := store.LoadRecentOrders(t.Context(), s.Pool, b.id)
	if err != nil {
		t.Fatal(err)
	}
	if len(rowsB) != 1 || rowsB[0].ID != orderB {
		t.Fatalf("rows b: %+v", rowsB)
	}
}

func TestDashboardFirstPurchaseOfferAndAdminNav(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := signUp(t, s, "dash-offer")
	exec(t, s.Pool, `INSERT INTO coupons (id, code, discount_type, discount_value, usage_type) VALUES ($1, 'BEMVINDA', 'percent', 10, 'unlimited')`, uuid.Must(uuid.NewV7()))
	exec(t, s.Pool, `INSERT INTO store_settings (id, first_purchase_coupon_code) VALUES (true, 'BEMVINDA')
		ON CONFLICT (id) DO UPDATE SET first_purchase_coupon_code = EXCLUDED.first_purchase_coupon_code`)
	res := a.c.Get("/dashboard")
	wantContains(t, res, "Nenhum pedido ainda.", "Explorar produtos")
	if offer, _ := store.FirstPurchaseOffer(t.Context(), s.Pool, a.id, a.email); offer != "" {
		wantContains(t, res, "Primeira compra? Use o cupom", offer)
	}
	exec(t, s.Pool, `UPDATE users SET role = 'admin' WHERE id = $1`, a.id)
	res = a.c.Get("/dashboard")
	wantContains(t, res, "/admin/usuarios", "Relatórios")
	// Collapsed sidebar cookie.
	res = a.c.Do(http.MethodGet, "/dashboard", nil, http.Header{"Cookie": {"sidebar_state=collapsed"}})
	wantContains(t, res, `data-state="collapsed"`)
}

func TestMyOrdersPaginatesAndFragments(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a, b := signUp(t, s, "orders-a"), signUp(t, s, "orders-b")
	other := insertOrder(t, s.Pool, b.id, 999)
	for i := range 21 {
		id := uuid.Must(uuid.NewV7())
		exec(t, s.Pool, `INSERT INTO orders (id, user_id, status, total_cents, access_token, created_at, carrier, tracking_code)
			VALUES ($1, $2, 'shipped', $3, $4, now() - make_interval(mins => $5), 'Correios', 'AA123456789BR')`,
			id, a.id, int64(1000+i), uuid.New(), i)
	}
	res := a.c.Get("/conta/pedidos")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, "Meus Pedidos", "Página 1 de 2", "Próxima", `hx-get="/conta/pedidos/lista?page=2"`,
		"Enviado", "AA123456789BR", "Rastrear")
	wantMissing(t, res, other.String(), "Anterior")

	res = a.c.Do(http.MethodGet, "/conta/pedidos/lista?page=2", nil, htmxHeaders())
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, `id="my-orders"`, "Página 2 de 2", "Anterior")
	wantMissing(t, res, "<html", "Acompanhe o status")

	// Out-of-range pages clamp to the last one; plain GET renders the page.
	res = a.c.Get("/conta/pedidos/lista?page=99")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, "<html", "Página 2 de 2")

	res = b.c.Get("/conta/pedidos")
	wantContains(t, res, other.String())
	empty := signUp(t, s, "orders-empty")
	res = empty.c.Get("/conta/pedidos")
	wantContains(t, res, "Você ainda não fez nenhum pedido.")
}

func addressForm(label, street string) url.Values {
	return url.Values{
		"label": {label}, "recipient_name": {"Maria Silva"}, "phone": {"79999816511"}, "zip_code": {"49000-000"},
		"street": {street}, "number": {"10"}, "complement": {""}, "neighborhood": {"Centro"}, "city": {"Aracaju"}, "state": {"SE"},
	}
}

func TestAddressesCRUD(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := signUp(t, s, "addr")
	res := a.c.Get("/conta/enderecos")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, "Meus Endereços", "Você ainda não salvou nenhum endereço.", "Novo endereço", `action="/conta/enderecos/salvar"`)

	res = a.c.PostForm("/conta/enderecos/salvar", addressForm("Casa", "Rua A"))
	wantRedirect(t, res, "/conta/enderecos")
	wantToast(t, res, toast.SuccessType, "Endereço salvo.")
	res = a.c.PostForm("/conta/enderecos/salvar", addressForm("Trabalho", "Rua B"))
	wantToast(t, res, toast.SuccessType, "Endereço salvo.")

	casa := scalar[uuid.UUID](t, s.Pool, `SELECT id FROM user_addresses WHERE user_id = $1 AND label = 'Casa'`, a.id)
	trabalho := scalar[uuid.UUID](t, s.Pool, `SELECT id FROM user_addresses WHERE user_id = $1 AND label = 'Trabalho'`, a.id)
	res = a.c.Get("/conta/enderecos")
	wantContains(t, res, "Casa", "Trabalho", "Padrão", "Tornar padrão", "Rua A, 10 — Centro, Aracaju/SE — CEP 49000-000", "WhatsApp: 79999816511")

	// Edit form prefilled; invalid edits come back to it.
	res = a.c.Get("/conta/enderecos?editar=" + trabalho.String())
	wantContains(t, res, "Editar endereço", `value="Rua B"`, `value="`+trabalho.String()+`"`, "Cancelar")
	bad := addressForm("Trabalho", "Rua B")
	bad.Set("id", trabalho.String())
	bad.Set("zip_code", "123")
	res = a.c.PostForm("/conta/enderecos/salvar", bad)
	wantRedirect(t, res, "/conta/enderecos?editar="+trabalho.String())
	if ts := toasts(res); len(ts) == 0 || ts[0].Type != toast.ErrorType {
		t.Fatalf("invalid edit toasts %v", ts)
	}
	edit := addressForm("Escritório", "Rua C")
	edit.Set("id", trabalho.String())
	res = a.c.PostForm("/conta/enderecos/salvar", edit)
	wantToast(t, res, toast.SuccessType, "Endereço salvo.")
	if st := scalar[string](t, s.Pool, `SELECT street FROM user_addresses WHERE id = $1`, trabalho); st != "Rua C" {
		t.Fatalf("street %q", st)
	}

	res = a.c.PostForm("/conta/enderecos/padrao", url.Values{"id": {trabalho.String()}})
	wantToast(t, res, toast.SuccessType, "Endereço padrão atualizado.")
	if !scalar[bool](t, s.Pool, `SELECT is_default FROM user_addresses WHERE id = $1`, trabalho) {
		t.Fatal("default not moved")
	}

	// Someone else's id is not found.
	b := signUp(t, s, "addr-b")
	res = b.c.PostForm("/conta/enderecos/remover", url.Values{"id": {casa.String()}})
	wantToast(t, res, toast.ErrorType, "Endereço não encontrado.")
	res = b.c.Get("/conta/enderecos?editar=" + casa.String())
	wantMissing(t, res, "Editar endereço", "Rua A")
	res = b.c.PostForm("/conta/enderecos/padrao", url.Values{"id": {"nope"}})
	wantToast(t, res, toast.ErrorType, "Endereço não encontrado.")

	res = a.c.PostForm("/conta/enderecos/remover", url.Values{"id": {casa.String()}})
	wantToast(t, res, toast.SuccessType, "Endereço removido.")
	if n := scalar[int64](t, s.Pool, `SELECT COUNT(*) FROM user_addresses WHERE user_id = $1`, a.id); n != 1 {
		t.Fatalf("addresses %d", n)
	}
}

func TestAddressesLimit(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := signUp(t, s, "addr-limit")
	for i := range store.MaxAddressesPerUser {
		res := a.c.PostForm("/conta/enderecos/salvar", addressForm(fmt.Sprintf("E%d", i), fmt.Sprintf("Rua %d", i)))
		wantToast(t, res, toast.SuccessType, "Endereço salvo.")
	}
	res := a.c.Get("/conta/enderecos")
	wantContains(t, res, fmt.Sprintf("Você atingiu o limite de %d endereços.", store.MaxAddressesPerUser))
	wantMissing(t, res, "Novo endereço")
	res = a.c.PostForm("/conta/enderecos/salvar", addressForm("Extra", "Rua X"))
	if ts := toasts(res); len(ts) == 0 || ts[0].Type != toast.ErrorType || !strings.Contains(ts[0].Title, "até") {
		t.Fatalf("limit toast %v", ts)
	}
}
