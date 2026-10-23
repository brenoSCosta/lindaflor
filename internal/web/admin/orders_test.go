package admin_test

import (
	"net/http"
	"strings"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestOrdersDirectoryFragment(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	_, v, _ := seedProduct(t, s, "Peça Pedido", 5)
	paid := seedOrder(t, s, v, "paid", "paga@example.com")
	pending := seedOrder(t, s, v, "pending_payment", "pendente@example.com")

	res := a.c.Get("/admin/pedidos?status=paid")
	wantContains(t, res.Body, paid.String(), "paga@example.com", `aria-current="true"`)
	wantNotContains(t, res.Body, pending.String())

	frag := a.c.HTMX().Get("/admin/pedidos/lista?q=pendente")
	wantStatus(t, frag, http.StatusOK)
	wantContains(t, frag.Body, `id="order-results"`, pending.String(), "pending_payment", "R$ 159")
	wantNotContains(t, frag.Body, paid.String(), "<html")
	if frag.Header.Get("HX-Replace-Url") != "/admin/pedidos?q=pendente" {
		t.Fatalf("HX-Replace-Url = %q", frag.Header.Get("HX-Replace-Url"))
	}
}

func TestOrderPageAndStatusFlow(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	_, v, _ := seedProduct(t, s, "Peça Fluxo", 5)
	id := seedOrder(t, s, v, "pending_payment", "fluxo@example.com")
	path := "/admin/pedidos/" + id.String()

	page := a.c.Get(path)
	wantStatus(t, page, http.StatusOK)
	wantContains(t, page.Body, "Aguardando pagamento", "Marcar como pago", "Pagamento PIX", "00020126PIXCODE",
		"E-mail: fluxo@example.com", "WhatsApp: 79999999999", "Rua A, 10", "Biquíni Teste", "M / Coral", "R$ 159,90")
	wantNotContains(t, page.Body, "Mercado Pago", "Ações manuais")

	wantRedirect(t, a.c.PostForm(path, form("status", "bogus")), path)
	wantToast(t, a.c, toast.ErrorType, "Status inválido.")
	// Static PIX orders have no MP payment: the PSP actions refuse.
	wantRedirect(t, a.c.PostForm(path, form("status", "mp_sync")), path)
	wantToast(t, a.c, toast.ErrorType, "Pedido sem pagamento no Mercado Pago.")

	wantRedirect(t, a.c.PostForm(path, form("status", "paid")), path)
	wantToast(t, a.c, toast.SuccessType, "Pedido atualizado com sucesso.")
	page = a.c.Get(path)
	wantContains(t, page.Body, "Em separação", "Envio", "Marcar como enviado", `id="restock-cancel"`, "Estornar")

	// Ship needs a valid tracking code (validated with the transition).
	wantRedirect(t, a.c.PostForm(path, form("status", "shipped", "carrier", "Correios", "tracking_code", "")), path)
	if q := flash(t, a.c); q[len(q)-1].Type != toast.ErrorType {
		t.Fatal("ship without tracking accepted")
	}
	wantRedirect(t, a.c.PostForm(path, form("status", "shipped", "carrier", "Correios", "tracking_code", "AA123456789BR")), path)
	wantToast(t, a.c, toast.SuccessType, "Pedido atualizado com sucesso.")
	page = a.c.Get(path)
	wantContains(t, page.Body, "Rastreamento", "AA123456789BR", "Abrir rastreamento", "Enviado em", "Marcar como entregue")

	wantRedirect(t, a.c.PostForm(path, form("status", "cancelled", "restock", "1")), path)
	if st := scalar[string](t, s, `SELECT status::text FROM orders WHERE id = $1`, id); st != "cancelled" {
		t.Fatalf("status = %q", st)
	}
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM audit_events WHERE aggregate_type = 'order' AND aggregate_id = $1`, id.String()); n < 3 {
		t.Fatalf("audit rows = %d", n)
	}
	wantRedirect(t, a.c.PostForm("/admin/pedidos/nope", form("status", "paid")), "/admin/pedidos")
	wantToast(t, a.c, toast.ErrorType, "Pedido inválido.")
}

func TestMercadoPagoOrderRefusesManualMoney(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	_, v, _ := seedProduct(t, s, "Peça MP", 5)
	id := seedOrder(t, s, v, "pending_payment", "mp@example.com")
	exec(t, s, `UPDATE orders SET payment_provider = 'mercadopago', mp_payment_id = 123, mp_status = 'pending' WHERE id = $1`, id)
	path := "/admin/pedidos/" + id.String()

	page := a.c.Get(path)
	wantContains(t, page.Body, "Mercado Pago", "Pagamento:", "123", "Status MP: pending", "Sincronizar com Mercado Pago",
		"Ações manuais (avançado)", "Marcar como pago manualmente", "digite o código do pedido: "+store.OrderCode(id))
	wantNotContains(t, page.Body, ">Marcar como pago<")

	wantRedirect(t, a.c.PostForm(path, form("status", "paid")), path)
	wantToast(t, a.c, toast.ErrorType, store.MPManualBlockedMessage)
	wantRedirect(t, a.c.PostForm(path, form("status", "paid", "override_confirm", "XXXX", "override_reason", "PIX conferido no extrato")), path)
	wantToast(t, a.c, toast.ErrorType, "Confirmação incorreta: digite o código do pedido exatamente como aparece.")
	wantRedirect(t, a.c.PostForm(path, form("status", "paid", "override_confirm", "#"+strings.ToLower(store.OrderCode(id)), "override_reason", "PIX conferido no extrato")), path)
	wantToast(t, a.c, toast.SuccessType, "Ação manual registrada no pedido.")
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM audit_events WHERE aggregate_id = $1 AND metadata->>'action' = 'mp_manual_override'`, id.String()); n != 1 {
		t.Fatal("override not audited")
	}
	// The static-PIX provider cannot sync MP payments.
	wantRedirect(t, a.c.PostForm(path, form("status", "mp_sync")), path)
	wantToast(t, a.c, toast.ErrorType, "Mercado Pago não está configurado.")
}

func seedReturn(t *testing.T, s *testutil.Server, orderID uuid.UUID, kind string) uuid.UUID {
	t.Helper()
	id := newID()
	exec(t, s, `INSERT INTO return_requests (id, order_id, kind, reason) VALUES ($1, $2, $3, 'Ficou pequeno')`, id, orderID, kind)
	item := scalar[uuid.UUID](t, s, `SELECT id FROM order_items WHERE order_id = $1`, orderID)
	exec(t, s, `INSERT INTO return_request_items (return_request_id, order_item_id, quantity) VALUES ($1, $2, 1)`, id, item)
	return id
}

func TestReturnsListAndActions(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	_, v, _ := seedProduct(t, s, "Peça Troca", 5)
	order := seedOrder(t, s, v, "delivered", "troca@example.com")
	exec(t, s, `UPDATE orders SET paid_at = now(), shipped_at = now(), delivered_at = now() WHERE id = $1`, order)
	id := seedReturn(t, s, order, "exchange")
	path := "/admin/trocas/" + id.String()

	list := a.c.Get("/admin/trocas")
	wantContains(t, list.Body, "troca@example.com", "#"+store.OrderCode(order), "Troca", path)
	wantContains(t, a.c.Get("/admin/trocas?status=resolved").Body, "Nenhuma solicitação encontrada.")

	// The order page links the request.
	wantContains(t, a.c.Get("/admin/pedidos/"+order.String()).Body, "Trocas e devoluções", path)

	page := a.c.Get(path)
	wantStatus(t, page, http.StatusOK)
	wantContains(t, page.Body, "Ficou pequeno", "Aprovar e enviar instruções", "Recusar solicitação", "Histórico", "Salvar observação")

	wantRedirect(t, a.c.PostForm(path, form("action", "wat")), path)
	wantToast(t, a.c, toast.ErrorType, "Ação inválida.")
	wantRedirect(t, a.c.PostForm(path, form("action", "approve", "text", "Poste nos Correios")), path)
	wantToast(t, a.c, toast.SuccessType, "Solicitação atualizada.")
	wantRedirect(t, a.c.PostForm(path, form("action", "note", "text", "Cliente avisada")), path)
	wantRedirect(t, a.c.PostForm(path, form("action", "receive")), path)
	page = a.c.Get(path)
	wantContains(t, page.Body, "Concluir troca (nova peça enviada)", "Aprovada", "Peça recebida", "Cliente avisada", "Admin Teste")
	wantRedirect(t, a.c.PostForm(path, form("action", "exchange")), path)
	wantToast(t, a.c, toast.SuccessType, "Solicitação atualizada.")
	if st := scalar[string](t, s, `SELECT status FROM return_requests WHERE id = $1`, id); st != "resolved" {
		t.Fatalf("status = %q", st)
	}
	// Resolved: no more actions; illegal transitions are refused.
	wantNotContains(t, a.c.Get(path).Body, "Recusar solicitação")
	wantRedirect(t, a.c.PostForm(path, form("action", "approve", "text", "x")), path)
	if q := flash(t, a.c); q[len(q)-1].Type != toast.ErrorType {
		t.Fatal("illegal transition accepted")
	}
	wantRedirect(t, a.c.PostForm("/admin/trocas/x", form("action", "note")), "/admin/trocas")
	wantToast(t, a.c, toast.ErrorType, "Solicitação inválida.")
}

func TestReportsPageAndCSV(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	_, v, _ := seedProduct(t, s, "Peça Relatório", 5)
	id := seedOrder(t, s, v, "paid", "=cmd@example.com")
	exec(t, s, `UPDATE orders SET paid_at = now() WHERE id = $1`, id)

	page := a.c.Get("/admin/relatorios")
	wantStatus(t, page, http.StatusOK)
	wantContains(t, page.Body, "Receita", "R$ 159,90", "Pedidos pagos", "Variações mais vendidas", "Biquíni Teste (M / Coral)",
		"Forma de pagamento", "PIX", `style="width: 100%`, "/admin/relatorios/pedidos.csv?de=")

	res := a.c.Get("/admin/relatorios/pedidos.csv")
	wantStatus(t, res, http.StatusOK)
	if ct := res.Header.Get("Content-Type"); ct != "text/csv; charset=utf-8" {
		t.Fatalf("Content-Type = %q", ct)
	}
	cd := res.Header.Get("Content-Disposition")
	if !strings.HasPrefix(cd, `attachment; filename="pedidos-`) || !strings.HasSuffix(cd, `.csv"`) {
		t.Fatalf("Content-Disposition = %q", cd)
	}
	if !strings.HasPrefix(res.Body, "\xEF\xBB\xBF\"pedido\",\"id\",\"criado_em\"") {
		t.Fatalf("missing BOM/header: %q", res.Body[:40])
	}
	wantContains(t, res.Body, id.String(), `"'=cmd@example.com"`, `"159.90"`)

	// A range beyond the cap is clamped; bad dates fall back to defaults.
	wantStatus(t, a.c.Get("/admin/relatorios?de=2000-01-01&ate=2030-01-01"), http.StatusOK)
	wantStatus(t, a.c.Get("/admin/relatorios/pedidos.csv?de=bad&ate=worse"), http.StatusOK)
}
