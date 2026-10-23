package storefront_test

import (
	"net/http"
	"net/url"
	"strings"
	"testing"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/store"
)

func TestWishlistToggle(t *testing.T) {
	s := newServer(t)
	p := seedProduct(t, s.Pool, "Biquíni Favorito", 9990, 2)

	// Guests: /favoritos and the no-JS toggle go to login with next.
	guest := s.Client(t)
	wantRedirect(t, guest.Get("/favoritos"), "/login?next=%2Ffavoritos")
	res := guest.PostForm("/favoritos/alternar", form("product_id", p.ID.String(), "next", "/produtos/"+p.Slug))
	wantRedirect(t, res, "/login?next="+url.QueryEscape("/produtos/"+p.Slug))
	// htmx guests get a full navigation to login with the current page.
	res = guest.HTMX().Do(http.MethodPost, "/favoritos/alternar-proc", formBody(form("product_id", p.ID.String())),
		http.Header{"Content-Type": {"application/x-www-form-urlencoded"}, "HX-Current-URL": {s.URL + "/produtos?sort=novos"}})
	wantStatus(t, res, http.StatusNoContent)
	if got := res.Header.Get("HX-Redirect"); got != "/login?next="+url.QueryEscape("/produtos?sort=novos") {
		t.Fatalf("HX-Redirect = %q", got)
	}

	u := signUp(t, s, "wish")
	res = u.c.PostForm("/favoritos/alternar", form("product_id", p.ID.String(), "next", "/produtos/"+p.Slug))
	wantRedirect(t, res, "/produtos/"+p.Slug)
	wantToast(t, flash(t, res), toast.SuccessType, "Adicionado aos favoritos.")
	res = u.c.Get("/favoritos")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Meus Favoritos", "Biquíni Favorito", "data-favorites-grid", `aria-pressed="true"`)

	// Invalid id and unsafe next.
	res = u.c.PostForm("/favoritos/alternar", form("product_id", "x", "next", "//evil.example"))
	wantRedirect(t, res, "/favoritos")
	wantToast(t, flash(t, res), toast.ErrorType, "Produto inválido.")

	// htmx: re-rendered heart; on /favoritos the card is removed, and the
	// page reloads when the list becomes empty.
	other := seedProduct(t, s.Pool, "Maiô Favorito", 9990, 2)
	h := u.c.HTMX()
	res = h.PostForm("/favoritos/alternar-proc", form("product_id", other.ID.String()))
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, `aria-pressed="true"`, "Remover dos favoritos", `hx-post="/favoritos/alternar-proc"`)
	res = h.Do(http.MethodPost, "/favoritos/alternar-proc", formBody(form("product_id", other.ID.String())),
		http.Header{"Content-Type": {"application/x-www-form-urlencoded"}, "HX-Current-URL": {s.URL + "/favoritos"}})
	wantContains(t, res.Body, `aria-pressed="false"`)
	if res.Header.Get("HX-Retarget") != "closest article" || res.Header.Get("HX-Reswap") != "delete" {
		t.Fatalf("retarget %q / %q", res.Header.Get("HX-Retarget"), res.Header.Get("HX-Reswap"))
	}
	res = h.Do(http.MethodPost, "/favoritos/alternar-proc", formBody(form("product_id", p.ID.String())),
		http.Header{"Content-Type": {"application/x-www-form-urlencoded"}, "HX-Current-URL": {s.URL + "/favoritos"}})
	if res.Header.Get("HX-Refresh") != "true" {
		t.Fatalf("empty wishlist should refresh, got %d %v", res.StatusCode, res.Header)
	}
	res = u.c.Get("/favoritos")
	wantContains(t, res.Body, "Você ainda não tem favoritos")
}

func TestNewsletterSubscribeAndUnsubscribe(t *testing.T) {
	s := newServer(t)
	c := s.Client(t)

	res := c.PostForm("/newsletter", form("email", ""))
	wantRedirect(t, res, "/")
	wantToast(t, flash(t, res), toast.ErrorType, "E-mail é obrigatório")
	res = c.PostForm("/newsletter", form("email", "a.b@"))
	wantToast(t, flash(t, res), toast.ErrorType, "E-mail inválido")

	res = c.PostForm("/newsletter", form("email", "News@Example.com", "name", "Ana"))
	wantToast(t, flash(t, res), toast.SuccessType, "Inscrição realizada! Você vai receber nossas novidades por e-mail.")
	if n := scalar[int64](t, s.Pool, `SELECT count(*) FROM email_outbox WHERE to_address = 'news@example.com' AND kind = 'newsletter_welcome'`); n != 1 {
		t.Fatalf("welcome mails = %d", n)
	}
	res = c.PostForm("/newsletter", form("email", "news@example.com"))
	wantToast(t, flash(t, res), toast.ErrorType, "E-mail já cadastrado na newsletter")
	// Fifth request of the minute is the last within budget.
	c.PostForm("/newsletter", form("email", "x@example.com"))
	res = c.PostForm("/newsletter", form("email", "y@example.com"))
	wantToast(t, flash(t, res), toast.ErrorType, "Muitas tentativas. Tente novamente em alguns minutos.")
	if n := scalar[int64](t, s.Pool, `SELECT count(*) FROM newsletter_subscribers WHERE email = 'y@example.com'`); n != 0 {
		t.Fatal("rate-limited subscribe stored")
	}

	// Unsubscribe page: GET only confirms; bad tokens are refused.
	link, err := url.Parse(store.UnsubscribeURL("news@example.com"))
	if err != nil {
		t.Fatal(err)
	}
	q := link.Query()
	res = c.Get("/newsletter/cancelar?" + link.RawQuery)
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Cancelar newsletter", "news@example.com", "Cancelar inscrição")
	res = c.Get("/newsletter/cancelar?email=news%40example.com&token=bad")
	wantContains(t, res.Body, "Link inválido")
	if !active(t, s.Pool, "news@example.com") {
		t.Fatal("GET must not unsubscribe")
	}
	res = c.PostForm("/newsletter/cancelar", form("email", "news@example.com", "token", "bad"))
	wantRedirect(t, res, "/newsletter/cancelar")
	wantToast(t, flash(t, res), toast.ErrorType, "Link de cancelamento inválido.")

	res = c.PostForm("/newsletter/cancelar", form("email", q.Get("email"), "token", q.Get("token")))
	wantRedirect(t, res, "/newsletter/cancelar?feito=1")
	if active(t, s.Pool, "news@example.com") {
		t.Fatal("still subscribed")
	}
	wantContains(t, c.Get("/newsletter/cancelar?feito=1").Body, "Inscrição cancelada")

	// RFC 8058 one-click: cross-site POST, token in the query string.
	exec(t, s.Pool, `UPDATE newsletter_subscribers SET unsubscribed_at = NULL`)
	res = c.Do(http.MethodPost, "/newsletter/cancelar?"+link.RawQuery, strings.NewReader("List-Unsubscribe=One-Click"),
		http.Header{"Content-Type": {"application/x-www-form-urlencoded"}, "Sec-Fetch-Site": {"cross-site"}, "Origin": {"https://mail.example"}})
	wantStatus(t, res, http.StatusOK)
	if res.Body != "unsubscribed" || active(t, s.Pool, "news@example.com") {
		t.Fatalf("one-click: %q", res.Body)
	}
	res = c.Do(http.MethodPost, "/newsletter/cancelar?email=news%40example.com&token=00", strings.NewReader("List-Unsubscribe=One-Click"),
		http.Header{"Content-Type": {"application/x-www-form-urlencoded"}})
	wantStatus(t, res, http.StatusBadRequest)
}

func active(t *testing.T, pool *pgxpool.Pool, email string) bool {
	t.Helper()
	return scalar[bool](t, pool, `SELECT unsubscribed_at IS NULL FROM newsletter_subscribers WHERE lower(email) = $1`, email)
}

func alerts(t *testing.T, pool *pgxpool.Pool, variantID uuid.UUID) []string {
	t.Helper()
	rows, err := pool.Query(ctx, `SELECT email_norm FROM stock_alerts WHERE variant_id = $1 ORDER BY email_norm`, variantID)
	if err != nil {
		t.Fatal(err)
	}
	defer rows.Close()
	out := []string{}
	for rows.Next() {
		var e string
		if err := rows.Scan(&e); err != nil {
			t.Fatal(err)
		}
		out = append(out, e)
	}
	return out
}

// Port of tests/stock_alerts.rs subscribe_validates_dedupes_and_is_rate_limited.
func TestStockAlertSubscribeValidatesDedupesAndIsRateLimited(t *testing.T) {
	s := newServer(t)
	p := seedProduct(t, s.Pool, "Saída Brisa", 8990, 0)
	c := s.Client(t)
	post := func(kv ...string) *http.Response {
		return c.PostForm("/avise-me", form(append([]string{"variant_id", p.VariantID.String()}, kv...)...)).Response
	}

	// 1: invalid address → back to the product page, nothing stored.
	res := post("email", "not-an-email")
	if res.StatusCode != http.StatusSeeOther || !strings.Contains(res.Header.Get("Location"), "variant="+p.VariantID.String()) {
		t.Fatalf("%d %q", res.StatusCode, res.Header.Get("Location"))
	}
	if len(alerts(t, s.Pool, p.VariantID)) != 0 {
		t.Fatal("stored invalid address")
	}
	// 2 + 3: stored once, normalized; the duplicate is a no-op.
	post("email", "Ana@Example.com")
	post("email", "ana@example.com")
	if got := alerts(t, s.Pool, p.VariantID); len(got) != 1 || got[0] != "ana@example.com" {
		t.Fatal(got)
	}
	// 4: honeypot filled → silently dropped.
	post("email", "bot@example.com", "website", "x")
	// 5 is the last request within the per-minute budget; the 6th is refused.
	post("email", "bia@example.com")
	post("email", "cris@example.com")
	if got := alerts(t, s.Pool, p.VariantID); strings.Join(got, ",") != "ana@example.com,bia@example.com" {
		t.Fatal(got)
	}

	// Unknown variant.
	res2 := c.PostForm("/avise-me", form("variant_id", newID().String(), "email", "a@example.com"))
	wantRedirect(t, res2, "/produtos")
}

func TestStockAlertCancel(t *testing.T) {
	s := newServer(t)
	p := seedProduct(t, s.Pool, "Saída Mar", 8990, 0)
	c := s.Client(t)
	c.PostForm("/avise-me", form("variant_id", p.VariantID.String(), "email", "ana@example.com"))
	if len(alerts(t, s.Pool, p.VariantID)) != 1 {
		t.Fatal("not subscribed")
	}
	link, err := url.Parse(store.StockAlertCancelURL("ana@example.com"))
	if err != nil {
		t.Fatal(err)
	}
	res := c.Get("/avise-me/cancelar?" + link.RawQuery)
	wantContains(t, res.Body, "Cancelar avisos de estoque", "Cancelar avisos")
	wantContains(t, c.Get("/avise-me/cancelar?email=ana%40example.com&token=x").Body, "Link inválido")
	res = c.PostForm("/avise-me/cancelar", form("email", "ana@example.com", "token", "x"))
	wantRedirect(t, res, "/avise-me/cancelar")
	q := link.Query()
	res = c.PostForm("/avise-me/cancelar", form("email", q.Get("email"), "token", q.Get("token")))
	wantRedirect(t, res, "/avise-me/cancelar?feito=1")
	if len(alerts(t, s.Pool, p.VariantID)) != 0 {
		t.Fatal("alerts not cancelled")
	}
	wantContains(t, c.Get("/avise-me/cancelar?feito=1").Body, "Avisos cancelados")
}
