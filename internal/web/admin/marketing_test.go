package admin_test

import (
	"net/http"
	"strings"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestCouponsLifecycle(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	u := signUp(t, s, "cupom", "Cliente Cupom")

	create := form("code", " verao10 ", "discount_type", "percent", "discount_value", "10", "min_subtotal_cents", "0",
		"usage_type", "unlimited", "max_uses", "5", "q", "")
	wantRedirect(t, a.c.PostForm("/admin/cupons/create", create), "/admin/cupons")
	wantRedirect(t, a.c.PostForm("/admin/cupons/create", create), "/admin/cupons")
	wantToast(t, a.c, toast.ErrorType, "Este código já está em uso.")
	bad := form("code", "X", "discount_type", "percent", "discount_value", "80", "usage_type", "unlimited", "q", "VER")
	wantRedirect(t, a.c.PostForm("/admin/cupons/create", bad), "/admin/cupons?q=VER")
	wantToast(t, a.c, toast.ErrorType, "Percentual deve ser entre 1 e 50.")

	id := scalar[uuid.UUID](t, s, `SELECT id FROM coupons WHERE code = 'VERAO10'`)
	page := a.c.Get("/admin/cupons")
	wantContains(t, page.Body, "VERAO10", "10%", "Até 5 usos", "Sem validade", "Ativo", "Desativar",
		`hx-post="/admin/cupons/toggle-proc"`, "/admin/cupons?cupom="+id.String())

	// Search fragment.
	frag := s.Client(t)
	frag.HTTP.Jar = a.c.HTTP.Jar
	frag.HTMX()
	res := frag.Get("/admin/cupons/lista?q=nada")
	wantContains(t, res.Body, `id="coupon-results"`, "Nenhum cupom cadastrado.")
	wantNotContains(t, res.Body, "<html")
	wantContains(t, frag.Get("/admin/cupons/lista?q=ver").Body, "VERAO10")

	// htmx toggle-proc answers the refreshed list.
	res = frag.PostForm("/admin/cupons/toggle-proc", form("coupon_id", id.String(), "q", ""))
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, `id="coupon-results"`, "Inativo", "Ativar")
	if scalar[bool](t, s, `SELECT active FROM coupons WHERE id = $1`, id) {
		t.Fatal("toggle-proc did not toggle")
	}
	// No-JS toggle.
	wantRedirect(t, a.c.PostForm("/admin/cupons/toggle", form("coupon_id", id.String())), "/admin/cupons")
	if !scalar[bool](t, s, `SELECT active FROM coupons WHERE id = $1`, id) {
		t.Fatal("toggle did not toggle")
	}
	wantRedirect(t, a.c.PostForm("/admin/cupons/toggle", form("coupon_id", uuid.NewString())), "/admin/cupons")
	wantToast(t, a.c, toast.ErrorType, "Cupom não encontrado.")

	// Assignment card + user search fragment.
	page = a.c.Get("/admin/cupons?cupom=" + id.String() + "&uq=Cupom")
	wantContains(t, page.Body, "Atribuir VERAO10", "Nenhum usuário atribuído ainda.", "Cliente Cupom", `name="user_ids"`,
		"usuário(s) encontrado(s)", `hx-get="/admin/cupons/usuarios"`)
	back := "/admin/cupons?cupom=" + id.String() + "&uq=Cupom"
	wantRedirect(t, a.c.PostForm("/admin/cupons/assign", form("coupon_id", id.String(), "cupom", id.String(), "uq", "Cupom")), back)
	wantToast(t, a.c, toast.ErrorType, "Selecione ao menos um usuário.")
	wantRedirect(t, a.c.PostForm("/admin/cupons/assign", form("coupon_id", id.String(), "user_ids", u.id.String(), "cupom", id.String(), "uq", "Cupom")), back)
	users := frag.Get("/admin/cupons/usuarios?cupom=" + id.String() + "&uq=Cupom")
	wantContains(t, users.Body, `id="coupon-user-results"`, "Cliente Cupom", "Atribuído", "disabled")
	wantContains(t, a.c.Get(back).Body, u.email, "Remover")
	wantRedirect(t, a.c.PostForm("/admin/cupons/unassign", form("coupon_id", id.String(), "user_id", u.id.String(), "cupom", id.String())), "/admin/cupons?cupom="+id.String())
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM coupon_assignments WHERE coupon_id = $1`, id); n != 0 {
		t.Fatal("unassign failed")
	}
	wantRedirect(t, a.c.PostForm("/admin/cupons/unassign", form("coupon_id", "x", "user_id", "y")), "/admin/cupons")
	wantToast(t, a.c, toast.ErrorType, "Identificador inválido.")

	// Deleting the selected coupon drops it from the URL.
	wantRedirect(t, a.c.PostForm("/admin/cupons/delete", form("coupon_id", id.String(), "cupom", id.String(), "q", "ver")), "/admin/cupons?q=ver")
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM coupons`); n != 0 {
		t.Fatal("coupon not deleted")
	}
	wantRedirect(t, a.c.PostForm("/admin/cupons/delete", form("coupon_id", "nope")), "/admin/cupons")
	wantToast(t, a.c, toast.ErrorType, "Cupom inválido.")
}

// Port of tests/reviews.rs moderation_is_admin_only_and_audited and the
// admin half of review_text_is_escaped.
func TestReviewModerationIsAdminOnlyAndAudited(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	product, _, _ := seedProduct(t, s, "Biquíni Avaliado", 3)
	author := signUp(t, s, "review-mod-author", "Cliente Teste")
	user := signUp(t, s, "review-mod-user", "Cliente Teste")
	admin := adminAccount(t, s)
	id := newID()
	exec(t, s, `INSERT INTO product_reviews (id, product_id, user_id, rating, title, body, status) VALUES ($1, $2, $3, 5, 'Moderar', 'texto', 'pending')`,
		id, product, author.id)
	approve := form("review_id", id.String(), "action", "approve")
	status := func() string {
		return scalar[string](t, s, `SELECT status::text FROM product_reviews WHERE id = $1`, id)
	}

	// Guests and customers can neither see nor act.
	anon := s.Client(t)
	for _, c := range []*testutil.Client{anon, user.c} {
		if res := c.Get("/admin/avaliacoes"); res.StatusCode != http.StatusSeeOther {
			t.Fatalf("GET status = %d", res.StatusCode)
		}
		if res := c.PostForm("/admin/avaliacoes", approve); res.StatusCode != http.StatusSeeOther {
			t.Fatalf("POST status = %d", res.StatusCode)
		}
	}
	if status() != "pending" {
		t.Fatal("non-admin moderated")
	}

	// Admin sees the queue and approves; the change is audited.
	page := admin.c.Get("/admin/avaliacoes")
	wantStatus(t, page, http.StatusOK)
	wantContains(t, page.Body, "Moderar", "1 pendentes de moderação", "Pendente", "Aprovar", "Rejeitar", `aria-label="Nota 5,0 de 5"`)
	wantRedirect(t, admin.c.PostForm("/admin/avaliacoes", approve), "/admin/avaliacoes")
	wantToast(t, admin.c, toast.SuccessType, "Avaliação aprovada.")
	if status() != "approved" {
		t.Fatal("not approved")
	}
	actor := scalar[uuid.UUID](t, s, `SELECT actor_user_id FROM audit_events WHERE aggregate_type = 'product_review' AND aggregate_id = $1`, id.String())
	action := scalar[string](t, s, `SELECT metadata->>'action' FROM audit_events WHERE aggregate_type = 'product_review' AND aggregate_id = $1`, id.String())
	if actor != admin.id || action != "review.approve" {
		t.Fatalf("audit = %v %q", actor, action)
	}
	wantRedirect(t, admin.c.PostForm("/admin/avaliacoes", form("review_id", id.String(), "action", "reject")), "/admin/avaliacoes")
	wantToast(t, admin.c, toast.SuccessType, "Avaliação rejeitada.")
	if status() != "rejected" {
		t.Fatal("not rejected")
	}
	wantRedirect(t, admin.c.PostForm("/admin/avaliacoes", form("review_id", uuid.NewString(), "action", "approve")), "/admin/avaliacoes")
	wantToast(t, admin.c, toast.ErrorType, "Avaliação não encontrada.")
	wantRedirect(t, admin.c.PostForm("/admin/avaliacoes", form("review_id", id.String(), "action", "delete")), "/admin/avaliacoes")
	wantToast(t, admin.c, toast.ErrorType, "Ação inválida.")
}

func TestReviewTextIsEscapedInAdmin(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	product, _, _ := seedProduct(t, s, "Biquíni XSS", 3)
	author := signUp(t, s, "review-xss", "Cliente Teste")
	admin := adminAccount(t, s)
	exec(t, s, `INSERT INTO product_reviews (id, product_id, user_id, rating, title, body, status) VALUES ($1, $2, $3, 5, $4, $5, 'approved')`,
		newID(), product, author.id, "<script>alert('t')</script>", "<img src=x onerror=alert(1)>")
	html := admin.c.Get("/admin/avaliacoes").Body
	wantNotContains(t, html, "<script>alert('t')", "<img src=x")
	wantContains(t, html, "&lt;script&gt;", "&lt;img src=x")
}

func TestNewsletterListAndCSV(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	exec(t, s, `INSERT INTO newsletter_subscribers (id, email, name) VALUES ($1, 'stay@example.com', '=Fórmula')`, newID())
	exec(t, s, `INSERT INTO newsletter_subscribers (id, email, unsubscribed_at) VALUES ($1, 'go@example.com', now())`, newID())

	page := a.c.Get("/admin/newsletter")
	wantContains(t, page.Body, "1 ativas · 1 canceladas", "stay@example.com", "go@example.com", "Ativa", "Cancelada em ")

	res := a.c.Get("/admin/newsletter/export-csv")
	wantStatus(t, res, http.StatusOK)
	if res.Header.Get("Content-Type") != "text/csv; charset=utf-8" ||
		res.Header.Get("Content-Disposition") != `attachment; filename="newsletter.csv"` {
		t.Fatalf("headers: %v", res.Header)
	}
	if !strings.HasPrefix(res.Body, "email,name,created_at\n\"stay@example.com\",\"'=Fórmula\",") {
		t.Fatalf("csv = %q", res.Body)
	}
	wantNotContains(t, res.Body, "go@example.com")
}

func TestSettingsSave(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	ok := form("pix_key", "79999816511", "pix_key_type", "phone", "pix_merchant_name", "Linda Flor",
		"pix_merchant_city", "Aracaju", "pix_discount_percent", "7", "announcement_text", "Frete grátis!",
		"whatsapp_number", "5579999816511", "whatsapp_message_template", "Pedido {{order_id}}", "free_shipping_threshold", "199,90")
	wantRedirect(t, a.c.PostForm("/admin/configuracoes", ok), "/admin/configuracoes")
	wantToast(t, a.c, toast.SuccessType, "Configurações salvas com sucesso.")
	if n := scalar[int64](t, s, `SELECT free_shipping_threshold_cents FROM store_settings`); n != 19990 {
		t.Fatalf("threshold = %d", n)
	}
	page := a.c.Get("/admin/configuracoes")
	wantContains(t, page.Body, `value="199.90"`, `value="Aracaju"`, "Frete grátis!", "Pedido {{order_id}}", `value="phone" selected`)

	bad := form("pix_key", "", "pix_key_type", "cpf", "pix_discount_percent", "5")
	wantRedirect(t, a.c.PostForm("/admin/configuracoes", bad), "/admin/configuracoes")
	wantToast(t, a.c, toast.ErrorType, "Informe a chave e o tipo da chave PIX, ou deixe ambos em branco.")
	wantRedirect(t, a.c.PostForm("/admin/configuracoes", form("pix_discount_percent", "31")), "/admin/configuracoes")
	wantToast(t, a.c, toast.ErrorType, "Desconto PIX deve estar entre 0 e 30%.")
}
