// Package storefront: home, catalog (/produtos + busca shard + filters),
// product and set pages, cart sheet + cart procedures, favoritos, reviews,
// avise-me, newsletter, static pages (termos, politica-privacidade,
// trocas-devolucoes, guia-de-medidas) and the shell data loader.
//
// Owner: wave2-D (UI) on top of wave1-C (store domain). router.go calls
// Mount and installs ShellLoader.
package storefront

import (
	"net/http"

	"github.com/a-h/templ"
	"github.com/go-chi/chi/v5"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// Mount registers the storefront routes.
func Mount(r chi.Router, d *app.Deps) {
	h := &handlers{d: d}

	r.Get("/", h.home)

	// Catalog + the /busca/produtos shard.
	r.Get("/produtos", h.catalog)
	r.Get("/busca/produtos", h.catalog)
	r.Get("/produtos/{slug}", h.productPage)
	r.Post("/produtos/{slug}", h.productPage)

	// Sets.
	r.Get("/sets", h.setsIndex)
	r.Get("/sets/{slug}", h.setPage)
	r.Post("/sets/{slug}", h.setPage)

	// Wishlist.
	r.With(auth.RequireUser).Get("/favoritos", h.favoritos)
	r.Post("/favoritos/alternar", h.wishlistToggle)
	r.Post("/favoritos/alternar-proc", h.wishlistToggleProc)

	// Cart sheet + procedures.
	r.Get("/carrinho/sheet", h.cartSheet)
	r.Post("/carrinho/adicionar", h.cartAdd)
	r.Post("/conjuntos/adicionar", h.setAdd)
	r.Post("/carrinho/remover", h.cartLineMutation("variant_id", "Variação inválida.", false, h.removeItem))
	r.Post("/carrinho/quantidade", h.cartLineMutation("variant_id", "Variação inválida.", true, h.setItemQuantity))
	r.Post("/carrinho/conjunto/quantidade", h.cartLineMutation("set_id", "Conjunto inválido.", true, h.setGroupQuantity))
	r.Post("/carrinho/conjunto/remover", h.cartLineMutation("set_id", "Conjunto inválido.", false, h.removeGroup))
	r.With(ratelimit.MutationsOnly(ratelimit.ScopeCoupon, ratelimit.DefaultCouponPerMin)).Post("/carrinho/cupom", h.cartCoupon)

	// Reviews.
	r.Post("/avaliacoes/enviar", h.reviewSubmit)
	r.Get("/avaliacoes/lista", h.reviewListFragment)

	// Back-in-stock alerts.
	r.Post("/avise-me", h.stockAlertSubscribe)
	r.Get("/avise-me/cancelar", h.stockAlertCancelPage)
	r.Post("/avise-me/cancelar", h.stockAlertCancel)

	// Newsletter.
	r.Post("/newsletter", h.newsletterSubscribe)
	r.Get("/newsletter/cancelar", h.unsubscribePage)
	r.Post("/newsletter/cancelar", h.unsubscribe)

	// Static pages.
	r.Get("/termos", h.static(termosPage(), seo.New("Termos de Uso").
		WithDescription("Termos de uso da loja Linda Flor Moda Praia: compras, preços, pagamentos, entregas e responsabilidades.").
		WithCanonical("/termos")))
	r.Get("/politica-privacidade", h.static(privacidadePage(), seo.New("Política de Privacidade").
		WithDescription("Como a Linda Flor coleta, usa e protege seus dados pessoais, conforme a LGPD.").
		WithCanonical("/politica-privacidade")))
	r.Get("/trocas-devolucoes", h.static(trocasPage(), seo.New("Trocas e Devoluções").
		WithDescription("Política de trocas e devoluções da Linda Flor: prazo de 7 dias após o recebimento, condições e como solicitar.").
		WithCanonical("/trocas-devolucoes")))
	r.Get("/guia-de-medidas", h.static(guiaPage(), seo.New("Guia de Medidas").
		WithDescription("Guia de medidas Linda Flor: tabela de tamanhos PP a GG para biquínis, maiôs e saídas de praia.").
		WithCanonical("/guia-de-medidas")))
}

func (h *handlers) static(page templ.Component, head seo.Head) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		seo.SetHead(r.Context(), head)
		render.Page(w, r, h.d, page)
	}
}
