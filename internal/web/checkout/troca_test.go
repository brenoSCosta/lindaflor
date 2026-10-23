package checkout_test

import (
	"net/http"
	"net/url"
	"strings"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/testutil"
	"github.com/wellington-vell/lindaflor/internal/web/checkout/qrsvg"
)

// deliveredOrder: a delivered guest order (delivered yesterday) with one
// line of 2 units.
func deliveredOrder(t *testing.T, s *testutil.Server) (orderID, token, itemID uuid.UUID) {
	t.Helper()
	orderID, token, itemID = newID(), uuid.New(), newID()
	variantID := seedVariant(t, s.Pool, "Biquíni Troca", 5)
	exec(t, s.Pool, `INSERT INTO orders (id, guest_email, status, access_token, subtotal_cents, total_cents, paid_at, shipped_at, delivered_at,
			carrier, tracking_code)
		VALUES ($1, 'cliente@example.com', 'delivered', $2, 3000, 3000, now() - interval '5 days', now() - interval '3 days',
			now() - interval '1 day', 'Correios', 'QB123456789BR')`, orderID, token)
	exec(t, s.Pool, `INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
		VALUES ($1, $2, $3, 'Biquíni Troca', 'M / azul', 2, 1500)`, itemID, orderID, variantID)
	return orderID, token, itemID
}

func TestOrderPageDeliveredShowsTimelineAndReturns(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	orderID, token, _ := deliveredOrder(t, s)
	res := s.Client(t).Get("/pedido/" + orderID.String() + "?t=" + token.String())
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, "Entregue", "Pedido recebido", "Pagamento confirmado", "Enviado",
		"QB123456789BR", "Rastrear", "Trocas e devoluções", "Solicitar troca ou devolução",
		"/pedido/"+orderID.String()+"/troca?t="+token.String(), "Troca fácil")
	if strings.Contains(res.Body, "Pague com PIX") || strings.Contains(res.Body, "Cancelar pedido") {
		t.Fatal("delivered order shows payment actions")
	}
}

func TestReturnRequestFlow(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	orderID, token, itemID := deliveredOrder(t, s)
	c := s.Client(t)
	form := "/pedido/" + orderID.String() + "/troca"

	// Wrong / missing token: not found.
	res := c.Get(form)
	wantStatus(t, res, http.StatusNotFound)
	wantContains(t, res, "Pedido não encontrado")

	res = c.Get(form + "?t=" + token.String())
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res, "Troca ou devolução", "Biquíni Troca (M / azul)", `name="qty_`+itemID.String()+`"`,
		`<option value="2">2</option>`, `name="kind"`, "Enviar solicitação", "Ver política de trocas")

	// No kind: error toast, back to the form.
	res = c.PostForm(form, url.Values{"t": {token.String()}, "reason": {"x"}})
	wantStatus(t, res, http.StatusSeeOther)
	if res.Header.Get("Location") != form+"?t="+token.String() {
		t.Fatalf("Location %q", res.Header.Get("Location"))
	}
	wantToast(t, res, toast.ErrorType, "Escolha o tipo de solicitação.")

	// Wrong token.
	res = c.PostForm(form, url.Values{
		"t": {uuid.NewString()}, "kind": {"exchange"}, "reason": {"Ficou pequeno"},
		"qty_" + itemID.String(): {"1"},
	})
	wantToast(t, res, toast.ErrorType, "Pedido não encontrado.")

	res = c.PostForm(form, url.Values{
		"t": {token.String()}, "kind": {"exchange"}, "reason": {"Ficou pequeno"},
		"desired_size": {"G"}, "qty_" + itemID.String(): {"1"},
	})
	wantStatus(t, res, http.StatusSeeOther)
	if res.Header.Get("Location") != "/pedido/"+orderID.String()+"?t="+token.String() {
		t.Fatalf("Location %q", res.Header.Get("Location"))
	}
	wantToast(t, res, toast.SuccessType, "Solicitação enviada. Responderemos por e-mail.")
	if n := scalar[int64](t, s.Pool, `SELECT COUNT(*) FROM return_requests WHERE order_id = $1`, orderID); n != 1 {
		t.Fatalf("return requests %d", n)
	}

	res = c.Get("/pedido/" + orderID.String() + "?t=" + token.String())
	wantContains(t, res, "Solicitada em")

	// A second request while one is open is refused with the store message.
	res = c.PostForm(form, url.Values{
		"t": {token.String()}, "kind": {"exchange"}, "reason": {"De novo"},
		"qty_" + itemID.String(): {"1"},
	})
	if ts := toasts(res); len(ts) == 0 || ts[0].Type != toast.ErrorType {
		t.Fatalf("duplicate request toasts %v", ts)
	}
}

func TestQRDataURI(t *testing.T) {
	uri := qrsvg.DataURI("00020126580014BR.GOV.BCB.PIX0136123e4567-e12b-12d1-a456-4266554400005204000053039865802BR5913Fulano de Tal6008BRASILIA62070503***63041D3D", 224)
	if !strings.HasPrefix(uri, "data:image/svg+xml,") || strings.ContainsAny(uri, `"<>#`) {
		t.Fatalf("bad data URI prefix/escaping: %.80s", uri)
	}
	if svg := qrsvg.SVG("hello", 100); !strings.Contains(svg, `width="100"`) || !strings.Contains(svg, "<path") {
		t.Fatalf("svg %s", svg)
	}
}
