package storefront_test

import (
	"net/http"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestCartFlowHTMX(t *testing.T) {
	s := newServer(t)
	p := seedProduct(t, s.Pool, "Biquíni Mar", 10000, 3)
	exec(t, s.Pool, `INSERT INTO coupons (id, code, discount_type, discount_value) VALUES ($1, 'DEZ', 'percent', 10)`, newID())
	// Signed in: per-user coupons need an identity.
	h := signUp(t, s, "cart").c.HTMX()

	// Empty sheet: fragment, no cart created.
	res := h.Get("/carrinho/sheet")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Seu carrinho está vazio.")
	wantNotContains(t, res.Body, "<html")
	if n := scalar[int64](t, s.Pool, `SELECT count(*) FROM carts`); n != 0 {
		t.Fatalf("sheet render created %d carts", n)
	}

	// Add: sheet + badge partial + feedback partial, opens the sheet.
	res = h.PostForm("/carrinho/adicionar", form("variant_id", p.VariantID.String()))
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Biquíni Mar", "M · Coral", "1 peça", `<hx-partial hx-target="#cart-count" hx-swap="outerHTML">`,
		`id="cart-count"`, ">1</span>", `hx-target="#add-to-cart-feedback"`, "Adicionado ao carrinho!", "Finalizar compra")
	if got := res.Header.Get("HX-Trigger"); got != `{"cart-show":true}` {
		t.Fatalf("HX-Trigger = %q", got)
	}

	// Quantity +1 (capped by stock 3).
	res = h.PostForm("/carrinho/quantidade", form("variant_id", p.VariantID.String(), "quantity", "2"))
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "2 peças", ">2</span>", "R$ 200,00")
	res = h.PostForm("/carrinho/quantidade", form("variant_id", p.VariantID.String(), "quantity", "9"))
	wantContains(t, res.Body, "3 peças")

	// Coupon: refused code keeps the typed value and shows the message.
	res = h.PostForm("/carrinho/cupom", form("code", "NADA"))
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Cupom inválido.", `value="NADA"`)
	res = h.PostForm("/carrinho/cupom", form("code", "dez"))
	wantContains(t, res.Body, "Cupom DEZ aplicado.", "Desconto", "-R$ 30,00")
	// The sheet re-render keeps the stored coupon.
	res = h.Get("/carrinho/sheet")
	wantContains(t, res.Body, "Cupom DEZ aplicado.")
	res = h.PostForm("/carrinho/cupom", form("code", ""))
	wantNotContains(t, res.Body, "aplicado.")

	// Bad input: toast on the header, sheet still returned.
	res = h.PostForm("/carrinho/remover", form("variant_id", "x"))
	wantStatus(t, res, http.StatusOK)
	wantToast(t, headerToasts(t, res), toast.ErrorType, "Variação inválida.")

	// Remove: empty again, badge 0 (hidden).
	res = h.PostForm("/carrinho/remover", form("variant_id", p.VariantID.String()))
	wantContains(t, res.Body, "Seu carrinho está vazio.", "hidden")

	// Out of stock / unknown variant: inline 422 retargeted to the form.
	none := seedProduct(t, s.Pool, "Esgotado", 1000, 0)
	res = h.PostForm("/carrinho/adicionar", form("variant_id", none.VariantID.String()))
	wantStatus(t, res, http.StatusUnprocessableEntity)
	wantContains(t, res.Body, "Esta peça está esgotada.")
	if res.Header.Get("HX-Retarget") != "#add-to-cart-feedback" {
		t.Fatalf("retarget %q", res.Header.Get("HX-Retarget"))
	}
	res = h.PostForm("/carrinho/adicionar", form("variant_id", newID().String()))
	wantContains(t, res.Body, "Esta variação não está disponível.")
}

func TestCartNoJSFallback(t *testing.T) {
	s := newServer(t)
	p := seedProduct(t, s.Pool, "Maiô Onda", 20000, 5)
	c := s.Client(t)

	res := c.Do(http.MethodPost, "/carrinho/adicionar", formBody(form("variant_id", p.VariantID.String())),
		http.Header{"Content-Type": {"application/x-www-form-urlencoded"}, "Referer": {s.URL + "/produtos/" + p.Slug}})
	wantRedirect(t, res, "/produtos/"+p.Slug)
	wantToast(t, flash(t, res), toast.SuccessType, "Adicionado ao carrinho!")

	// The no-JS cart page is the sheet in a page, controls post + 303 back.
	res = c.Get("/carrinho/sheet")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "<html", "Maiô Onda", `action="/carrinho/quantidade"`, "Continuar comprando", "noindex")
	res = c.PostForm("/carrinho/quantidade", form("variant_id", p.VariantID.String(), "quantity", "2"))
	wantRedirect(t, res, "/carrinho/sheet")
	res = c.PostForm("/carrinho/cupom", form("code", "XYZ"))
	wantRedirect(t, res, "/carrinho/sheet")
	wantToast(t, flash(t, res), toast.ErrorType, "Cupom inválido.")
	res = c.Get("/carrinho/sheet")
	wantContains(t, res.Body, "2 peças")
}

func TestSetCartGroup(t *testing.T) {
	s := newServer(t)
	top := seedProduct(t, s.Pool, "Top Lua", 7000, 4)
	bottom := seedProduct(t, s.Pool, "Calcinha Lua", 6000, 2)
	st := seedSet(t, s.Pool, "Conjunto Lua", 12000, top, bottom)
	h := s.Client(t).HTMX()

	res := h.PostForm("/conjuntos/adicionar", form("set_id", st.ID.String(), "quantity", "1"))
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Conjunto Lua", "Top Lua", "Calcinha Lua", "Diminuir conjunto", "R$ 130,00", ">2</span>")

	res = h.PostForm("/carrinho/conjunto/quantidade", form("set_id", st.ID.String(), "quantity", "2"))
	wantContains(t, res.Body, "4 peças", "R$ 260,00")
	// Capped at the scarcest member.
	res = h.PostForm("/carrinho/conjunto/quantidade", form("set_id", st.ID.String(), "quantity", "5"))
	wantContains(t, res.Body, "4 peças")

	res = h.PostForm("/carrinho/conjunto/remover", form("set_id", st.ID.String()))
	wantContains(t, res.Body, "Seu carrinho está vazio.")

	res = h.PostForm("/conjuntos/adicionar", form("set_id", "x", "quantity", "1"))
	wantStatus(t, res, http.StatusUnprocessableEntity)
	wantContains(t, res.Body, "Conjunto inválido.")
}

// Port of tests/security_misc.rs coupon_apply_has_a_scoped_budget: coupon
// POSTs spend their own per-IP budget; pages from the same IP still load.
func TestCouponApplyHasAScopedBudget(t *testing.T) {
	// Other tests relax every budget process-wide; pin this one explicitly.
	t.Setenv("RATE_LIMIT_COUPON_APPLY_PER_MIN", "2")
	ratelimit.Reset()
	t.Cleanup(ratelimit.Reset)
	s := testutil.NewServer(t, testutil.Options{KeepRateLimits: true})
	c := s.Client(t)
	for i := range 2 {
		if res := c.PostForm("/carrinho/cupom", form("code", "x")); res.StatusCode == http.StatusTooManyRequests {
			t.Fatalf("request %d limited early", i+1)
		}
	}
	wantStatus(t, c.PostForm("/carrinho/cupom", form("code", "x")), http.StatusTooManyRequests)
	wantStatus(t, c.Get("/carrinho/sheet"), http.StatusOK)
	wantStatus(t, c.Get("/"), http.StatusOK)
}
