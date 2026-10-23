package storefront_test

import (
	"net/http"
	"strings"
	"testing"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
)

// Ports of the storefront parts of tests/reviews.rs (admin moderation is
// wave2-F's).

func insertOrder(t *testing.T, pool *pgxpool.Pool, userID, variantID uuid.UUID, status string) {
	t.Helper()
	id := newID()
	exec(t, pool, `INSERT INTO orders (id, user_id, status, total_cents, shipped_at) VALUES ($1, $2, $3::order_status, 15990, now())`, id, userID, status)
	exec(t, pool, `INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
		VALUES ($1, $2, $3, 'Biquíni Avaliado', 'M · Coral', 1, 15990)`, newID(), id, variantID)
}

func insertReview(t *testing.T, pool *pgxpool.Pool, productID, userID uuid.UUID, status string, rating int, title, body string) {
	t.Helper()
	exec(t, pool, `INSERT INTO product_reviews (id, product_id, user_id, rating, title, body, fit, status)
		VALUES ($1, $2, $3, $4, $5, $6, 'ideal', $7::review_status)`, newID(), productID, userID, int16(rating), title, body, status) //nolint:gosec // 1..5
}

func reviewCount(t *testing.T, pool *pgxpool.Pool, productID uuid.UUID) int64 {
	t.Helper()
	return scalar[int64](t, pool, `SELECT count(*) FROM product_reviews WHERE product_id = $1`, productID)
}

func TestReviewFormRequiresVerifiedBuyerAndIsRateLimited(t *testing.T) {
	s := newServer(t)
	p := seedProduct(t, s.Pool, "Biquíni Avaliado", 15990, 5)
	f := form("product_id", p.ID.String(), "rating", "4", "title", "Bom", "body", "Gostei muito", "fit", "grande")
	page := "/produtos/" + p.Slug

	// Guest: sent to login, nothing stored.
	res := s.Client(t).PostForm("/avaliacoes/enviar", f)
	wantRedirect(t, res, "/login")
	if reviewCount(t, s.Pool, p.ID) != 0 {
		t.Fatal("guest review stored")
	}

	// Signed in, no order: no form on the page, POST refused.
	stranger := signUp(t, s, "review-form-stranger")
	res = stranger.c.Get(page)
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, `id="avaliacoes"`)
	wantNotContains(t, res.Body, `name="rating"`)
	res = stranger.c.PostForm("/avaliacoes/enviar", f)
	wantRedirect(t, res, page+"#avaliacoes")
	wantToast(t, flash(t, res), toast.ErrorType, "Só quem recebeu este produto pode avaliá-lo.")
	if reviewCount(t, s.Pool, p.ID) != 0 {
		t.Fatal("stranger review stored")
	}

	// Verified buyer: form shown, POST stored as pending.
	buyer := signUp(t, s, "review-form-buyer")
	insertOrder(t, s.Pool, buyer.id, p.VariantID, "delivered")
	wantContains(t, buyer.c.Get(page).Body, `name="rating"`, "Avalie sua compra")
	res = buyer.c.PostForm("/avaliacoes/enviar", f)
	wantRedirect(t, res, page+"#avaliacoes")
	wantToast(t, flash(t, res), toast.SuccessType, "Obrigada pela avaliação!")
	if st := scalar[string](t, s.Pool, `SELECT status::text FROM product_reviews WHERE user_id = $1`, buyer.id); st != "pending" {
		t.Fatal(st)
	}
	res = buyer.c.Get(page)
	wantNotContains(t, res.Body, `name="rating"`)
	wantContains(t, res.Body, "Recebemos sua avaliação. Ela aparece aqui depois da moderação.")

	// Rate limit: refused submits burn the budget even once eligible.
	spammer := signUp(t, s, "review-form-spam")
	for range 5 {
		spammer.c.PostForm("/avaliacoes/enviar", f)
	}
	insertOrder(t, s.Pool, spammer.id, p.VariantID, "delivered")
	res = spammer.c.PostForm("/avaliacoes/enviar", f)
	wantToast(t, flash(t, res), toast.ErrorType, "Muitas avaliações em pouco tempo. Tente mais tarde.")
	if n := scalar[int64](t, s.Pool, `SELECT count(*) FROM product_reviews WHERE user_id = $1`, spammer.id); n != 0 {
		t.Fatal("sixth submit within the window must be refused")
	}

	// Unknown product.
	res = buyer.c.PostForm("/avaliacoes/enviar", form("product_id", "x"))
	wantRedirect(t, res, "/produtos")
}

func TestOnlyApprovedReviewsArePublicAndEscaped(t *testing.T) {
	s := newServer(t)
	p := seedProduct(t, s.Pool, "Biquíni Público", 15990, 5)
	tag := strings.ReplaceAll(p.ID.String(), "-", "")
	a, b, c := signUp(t, s, "pub-a"), signUp(t, s, "pub-b"), signUp(t, s, "pub-c")
	insertReview(t, s.Pool, p.ID, a.id, "approved", 4, "Aprovada "+tag, "<img src=x onerror=alert(1)>")
	insertReview(t, s.Pool, p.ID, b.id, "pending", 1, "Pendente "+tag, "ok")
	insertReview(t, s.Pool, p.ID, c.id, "rejected", 1, "Rejeitada "+tag, "ok")

	res := s.Client(t).Get("/produtos/" + p.Slug)
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Aprovada "+tag, "1 avaliação", "Compra verificada", "Caimento: Tamanho ideal para 100% das clientes",
		`"aggregateRating"`, "&lt;img src=x")
	wantNotContains(t, res.Body, "Pendente "+tag, "Rejeitada "+tag, "<img src=x")
}

func TestReviewListShard(t *testing.T) {
	s := newServer(t)
	p := seedProduct(t, s.Pool, "Biquíni Popular", 15990, 5)
	for i := range 7 {
		u := signUp(t, s, "list")
		insertReview(t, s.Pool, p.ID, u.id, "approved", 5, "Review "+string(rune('A'+i)), "ok")
	}
	c := s.Client(t)
	res := c.Get("/produtos/" + p.Slug)
	wantContains(t, res.Body, "7 avaliações", "Página 1 de 2", "Mais avaliações",
		`hx-get="/avaliacoes/lista?product_id=`+p.ID.String()+`&amp;page=2"`)

	res = c.HTMX().Get("/avaliacoes/lista?product_id=" + p.ID.String() + "&page=2")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Página 2 de 2", "Anteriores")
	wantNotContains(t, res.Body, "<html")

	res = s.Client(t).Get("/avaliacoes/lista?product_id=" + p.ID.String() + "&page=2")
	wantRedirect(t, res, "/produtos/"+p.Slug+"?avaliacoes=2")
	wantStatus(t, s.Client(t).Get("/produtos/"+p.Slug+"?avaliacoes=2"), http.StatusOK)
}
