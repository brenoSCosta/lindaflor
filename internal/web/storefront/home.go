package storefront

import (
	"net/http"

	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// homeData is the home page (src/app.rs page).
type homeData struct {
	Slides      []store.HeroSlide
	Sections    store.HomeSections
	Settings    store.StoreSettings
	Cards       CardCtx
	WhatsappURL string
	SiteKey     string // Turnstile site key for the newsletter form ("" = off)
}

func (h *handlers) home(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	settings := store.StorefrontSettings(ctx, h.d.Pool)
	sections, err := store.LoadHomeSections(ctx, h.d.Pool)
	if err != nil {
		serverError(w, r, "load home sections", err)
		return
	}
	sections.Lancamentos = store.ResolveSummaryImages(ctx, h.d.Storage, sections.Lancamentos)
	sections.MaisVendidos = store.ResolveSummaryImages(ctx, h.d.Storage, sections.MaisVendidos)
	sections.Liquida = store.ResolveSummaryImages(ctx, h.d.Storage, sections.Liquida)
	slides, err := store.LoadHeroSlides(ctx, h.d.Pool, h.d.Storage)
	if err != nil {
		serverError(w, r, "load hero slides", err)
		return
	}
	data := homeData{Slides: slides, Sections: sections, Settings: settings, Cards: h.cardCtx(r, settings), SiteKey: h.d.Captcha.SiteKey()}
	if n := settings.WhatsappNumberTrimmed(); n != "" {
		data.WhatsappURL = store.WhatsappLink(n, "Olá! Quero saber mais sobre as peças da Linda Flor.")
	}
	seo.SetHead(ctx, seo.HomeHead(seo.SiteOrigin(h.d.Cfg.AppOrigin, r)))
	render.Page(w, r, h.d, homePage(data)) //nolint:contextcheck // templ components get the request ctx through Render
}

func freeShippingBenefit(threshold int64) string {
	if threshold > 0 {
		return "Acima de " + price(threshold)
	}
	return "Em todas as compras"
}

type categoryTile struct{ Href, Image, Alt, Label string }

var homeCategories = []categoryTile{
	{"/produtos?category=biquini", "https://images.unsplash.com/photo-1487252502161-75020a813bf0?auto=format&fit=crop&w=1200&q=80", "Biquínis", "Biquínis"},
	{"/produtos?category=biquini_top", "https://images.unsplash.com/photo-1631157899275-ae088292e34f?auto=format&fit=crop&w=1200&q=80", "Tops", "Tops"},
	{"/produtos?category=biquini_bottom", "https://images.unsplash.com/photo-1625563854481-b1530ba713c0?auto=format&fit=crop&w=1200&q=80", "Calcinhas", "Calcinhas"},
	{"/produtos?category=maio", "https://images.unsplash.com/photo-1571019613454-1cb2f99b2d8b?auto=format&fit=crop&w=1200&q=80", "Maiôs", "Maiôs"},
	{"/produtos?category=saida_praia", "https://images.unsplash.com/photo-1519046904884-53103b34b206?auto=format&fit=crop&w=1200&q=80", "Saídas de praia", "Saídas"},
	{"/produtos?category=acessorio", "https://images.unsplash.com/photo-1523293182086-7651a899d37f?auto=format&fit=crop&w=1200&q=80", "Acessórios", "Acessórios"},
}
