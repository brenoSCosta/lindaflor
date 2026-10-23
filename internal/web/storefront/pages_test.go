package storefront_test

import (
	"net/http"
	"strings"
	"testing"
)

func TestHomeRendersSectionsShellAndHead(t *testing.T) {
	s := newServer(t)
	p := seedProduct(t, s.Pool, "Biquíni Lançamento", 15990, 3)
	sale := seedProduct(t, s.Pool, "Maiô Liquida", 9990, 2)
	exec(t, s.Pool, `UPDATE products SET compare_at_price_in_cents = 19990 WHERE id = $1`, sale.ID)
	st := seedSet(t, s.Pool, "Conjunto Verão", 19990, p)
	exec(t, s.Pool, `INSERT INTO store_settings (id, whatsapp_number, announcement_text) VALUES (true, '5579999999999', 'FRETE PARA TODO O BRASIL')
		ON CONFLICT (id) DO UPDATE SET whatsapp_number = EXCLUDED.whatsapp_number, announcement_text = EXCLUDED.announcement_text`)

	res := s.Client(t).Get("/")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body,
		"Elegância tropical", "Explore a coleção", "Lançamentos", "Mais vendidos", "Liquida", "-50%",
		"Biquíni Lançamento", "/produtos/"+p.Slug, `id="sobre"`, "data-reveal", "/static/js/sobre.js",
		"Chamar no WhatsApp", "https://wa.me/5579999999999",
		// Shell: announcement, header sets menu, footer, cart badge + sheet.
		"FRETE PARA TODO O BRASIL", "/sets/"+st.Slug, "Conjunto Verão", "Institucional", `id="cart-count"`,
		`id="cart-sheet-body"`, `hx-get="/carrinho/sheet"`,
		// Home head: canonical + JSON-LD Organization/WebSite.
		`rel="canonical"`, `"@type":"Organization"`, `"@type":"WebSite"`,
	)
}

func TestHomeShowsBannerCarouselInsteadOfStaticHero(t *testing.T) {
	s := newServer(t)
	exec(t, s.Pool, `INSERT INTO home_banners (id, title, subtitle, image_url, active) VALUES ($1, 'Coleção Sol', 'Peças novas', 'https://cdn.example.com/b.jpg', true)`, newID())
	res := s.Client(t).Get("/")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, `aria-label="Destaques"`, "Coleção Sol", "Peças novas", "Ver coleção")
	wantNotContains(t, res.Body, "Elegância tropical")
}

func TestCatalogPageAndBuscaShard(t *testing.T) {
	s := newServer(t)
	seedProduct(t, s.Pool, "Biquíni Azul", 12990, 5)
	seedProduct(t, s.Pool, "Biquíni Verde", 13990, 5)
	exec(t, s.Pool, `UPDATE products SET category = 'maio' WHERE name = 'Biquíni Verde'`)
	c := s.Client(t)

	res := c.Get("/produtos")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "<html", `id="catalog-results"`, "Catálogo", "Biquíni Azul", "Biquíni Verde",
		`id="catalog-search"`, `hx-get="/busca/produtos?category=maio"`, "Ordenar:", "Filtrar",
		"<title>Catálogo de Moda Praia | Linda Flor</title>")

	res = c.Get("/produtos?category=maio")
	wantContains(t, res.Body, "Biquíni Verde", `/produtos?category=maio"`)
	wantNotContains(t, res.Body, "Biquíni Azul")

	// Search results are noindex; the heading echoes the term.
	res = c.Get("/produtos?search=azul")
	wantContains(t, res.Body, "Busca: azul", "Biquíni Azul", "noindex")
	wantNotContains(t, res.Body, "Biquíni Verde")

	// The shard: fragment for htmx, with the /produtos URL to show.
	res = s.Client(t).HTMX().Get("/busca/produtos?category=maio&search=")
	wantStatus(t, res, http.StatusOK)
	wantNotContains(t, res.Body, "<html", "<header")
	wantContains(t, res.Body, `id="catalog-results"`, "Biquíni Verde")
	if got := res.Header.Get("HX-Replace-Url"); got != "/produtos?category=maio" {
		t.Fatalf("HX-Replace-Url = %q", got)
	}
	// Without htmx the shard path renders the whole page.
	res = c.Get("/busca/produtos?search=verde")
	wantContains(t, res.Body, "<html", "Biquíni Verde")

	// Facets: chips with removal links, empty state.
	res = c.Get("/produtos?tamanho=m&sale=true")
	wantContains(t, res.Body, "Limpar tudo", "Em promoção", "Nenhum produto encontrado.", "Liquida")
}

func TestProductPageNoJSAddToCartAndHead(t *testing.T) {
	s := newServer(t)
	p := seedProduct(t, s.Pool, "Biquíni Coral", 15990, 2)
	c := s.Client(t)

	res := c.Get("/produtos/" + p.Slug)
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Biquíni Coral", "Selecione o tamanho", "M · Coral", "2 disponíveis",
		"Adicionar ao carrinho", `hx-post="/carrinho/adicionar"`, `id="add-to-cart-feedback"`,
		"Frete e entrega", "O que dizem as clientes", "Este produto ainda não tem avaliações.",
		`"@type":"Product"`, `rel="canonical"`, `property="og:type" content="product"`)

	// No-JS add: POST to the page itself, re-rendered with the message.
	res = c.PostForm("/produtos/"+p.Slug, form("variant_id", p.VariantID.String()))
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Adicionado ao carrinho!")
	if c.Cookie("lindaflor_cart") == "" {
		t.Fatal("cart cookie not set")
	}
	// The header badge counts the line on the next page.
	res = c.Get("/produtos")
	wantContains(t, res.Body, `data-cart-count`)
	if !strings.Contains(res.Body, ">1</span>") {
		t.Fatalf("badge should show 1")
	}

	res = c.PostForm("/produtos/"+p.Slug, form("variant_id", "nope"))
	wantContains(t, res.Body, "Variação inválida.")

	// Unknown product: 200 with the not-found block, noindex.
	res = c.Get("/produtos/nao-existe")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Produto não encontrado", "noindex")
}

func TestSoldOutVariantShowsStockAlertForm(t *testing.T) {
	s := newServer(t)
	p := seedProduct(t, s.Pool, "Saída Brisa", 8990, 0)
	res := s.Client(t).Get("/produtos/" + p.Slug)
	wantContains(t, res.Body, "Indisponível", "Avise-me quando chegar", `action="/avise-me"`, `name="website"`, p.VariantID.String())
}

func TestSetsPages(t *testing.T) {
	s := newServer(t)
	top := seedProduct(t, s.Pool, "Top Sol", 7000, 3)
	bottom := seedProduct(t, s.Pool, "Calcinha Sol", 6000, 3)
	st := seedSet(t, s.Pool, "Conjunto Sol", 11990, top, bottom)
	c := s.Client(t)

	res := c.Get("/sets")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Conjuntos", "Conjunto Sol", "/sets/"+st.Slug, "peças")

	res = c.Get("/sets/" + st.Slug)
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Conjunto Sol", "Peças inclusas", "Top Sol", "Calcinha Sol", "Adicionar conjunto",
		`hx-post="/conjuntos/adicionar"`, "peças neste conjunto", `rel="canonical"`)

	res = c.PostForm("/sets/"+st.Slug, form("quantity", "1"))
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Adicionado ao carrinho!")
	if n := scalar[int64](t, s.Pool, `SELECT COALESCE(SUM(quantity),0) FROM cart_items`); n != 2 {
		t.Fatalf("set lines = %d, want 2", n)
	}

	res = c.Get("/sets/nao-existe")
	wantStatus(t, res, http.StatusOK)
	wantContains(t, res.Body, "Conjunto não encontrado")
}

func TestStaticPagesAnd404(t *testing.T) {
	s := newServer(t)
	c := s.Client(t)
	for path, want := range map[string]string{
		"/termos":               "1. Aceitação dos Termos",
		"/politica-privacidade": "3. Compartilhamento de Dados",
		"/trocas-devolucoes":    "Como Solicitar uma Troca ou Devolução",
		"/guia-de-medidas":      "100 - 106",
	} {
		res := c.Get(path)
		wantStatus(t, res, http.StatusOK)
		wantContains(t, res.Body, want, `rel="canonical" href="http`)
	}
	res := c.Get("/nao/existe")
	wantStatus(t, res, http.StatusNotFound)
	wantContains(t, res.Body, "Página não encontrada", "Voltar ao início")
}
