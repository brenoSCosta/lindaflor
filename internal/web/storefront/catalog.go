package storefront

import (
	"net/http"
	"net/url"
	"strconv"

	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// catalogCategories are the category pills of /produtos (src/app/produtos.rs
// CATEGORIES, including its "Cantinhas" label).
var catalogCategories = [][2]string{
	{"all", "Todos"},
	{"biquini", "Biquínis"},
	{"biquini_top", "Tops"},
	{"biquini_bottom", "Cantinhas"},
	{"maio", "Maiôs"},
	{"saida_praia", "Saídas"},
	{"acessorio", "Acessórios"},
}

// catalogView is the /busca/produtos block.
type catalogView struct {
	Page  store.CatalogPage
	Cards CardCtx
	// Base is the form action / chip base path ("/produtos").
	Base string
}

// catalogHead: one canonical URL per category (filters, sorting and
// pagination fold into it); search results are noindex.
func catalogHead(p store.CatalogParams) seo.Head {
	var h seo.Head
	if label := catalogCategoryLabel(p.Category); label != "" {
		h = seo.New(label + " — Moda Praia").
			WithDescription(label + " Linda Flor: moda praia feita em Aracaju, com frete para todo o Brasil e desconto no Pix.").
			WithCanonical("/produtos?category=" + p.Category)
	} else {
		h = seo.New("Catálogo de Moda Praia").
			WithDescription("Catálogo Linda Flor: biquínis, maiôs, saídas de praia e acessórios. Moda praia feita em Aracaju, com frete para todo o Brasil.").
			WithCanonical("/produtos")
	}
	if p.Search != "" {
		h = h.WithNoIndex()
	}
	return h
}

func catalogCategoryLabel(category string) string {
	for _, c := range catalogCategories {
		if c[0] == category && c[0] != "all" {
			return c[1]
		}
	}
	return ""
}

// catalog serves /produtos and the /busca/produtos shard (the full page
// for plain requests, the #catalog-results block for htmx; htmx requests
// also get HX-Replace-Url so the address bar shows the /produtos URL).
func (h *handlers) catalog(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	page, err := store.BrowseCatalog(ctx, h.d.Pool, r.URL.RawQuery)
	if err != nil {
		serverError(w, r, "browse catalog", err)
		return
	}
	page.Products = store.ResolveSummaryImages(ctx, h.d.Storage, page.Products)
	settings := store.StorefrontSettings(ctx, h.d.Pool)
	seo.SetHead(ctx, catalogHead(page.Params))
	if httpx.IsHTMX(r) && r.URL.Path == "/busca/produtos" {
		w.Header().Set(httpx.HXReplaceURL, withQuery("/produtos", nonEmpty(r.URL.Query())))
	}
	render.Page(w, r, h.d, catalogResults(catalogView{Page: page, Cards: h.cardCtx(r, settings), Base: "/produtos"})) //nolint:contextcheck // templ components get the request ctx through Render
}

// nonEmpty drops empty values (htmx sends every form field).
func nonEmpty(v url.Values) url.Values {
	out := url.Values{}
	for k, vs := range v {
		for _, s := range vs {
			if s != "" {
				out.Add(k, s)
			}
		}
	}
	return out
}

// categoryQuery is what a category pill keeps (the Rust click handler:
// category, search, destaque, esgotados; page resets to 1).
func categoryQuery(p store.CatalogParams, category string) url.Values {
	v := url.Values{}
	if category != "all" {
		v.Set("category", category)
	}
	if p.Search != "" {
		v.Set("search", p.Search)
	}
	if p.FeaturedOnly {
		v.Set("destaque", "true")
	}
	if p.IncludeOutOfStock {
		v.Set("esgotados", "true")
	}
	return v
}

// pageQuery is the full catalog state with another page number.
func pageQuery(p store.CatalogParams, page int64) url.Values {
	v := categoryQuery(p, p.Category)
	if p.Sort != "" && p.Sort != "destaque" {
		v.Set("sort", p.Sort)
	}
	for _, s := range p.Sizes {
		v.Add("tamanho", s)
	}
	for _, c := range p.Colors {
		v.Add("cor", c)
	}
	if p.PrecoMin != nil {
		v.Set("preco_min", *p.PrecoMin)
	}
	if p.PrecoMax != nil {
		v.Set("preco_max", *p.PrecoMax)
	}
	if p.Disponivel {
		v.Set("disponivel", "true")
	}
	if p.Sale {
		v.Set("sale", "true")
	}
	if page > 1 {
		v.Set("page", strconv.FormatInt(page, 10))
	}
	return v
}

func containsStr(list []string, s string) bool {
	for _, v := range list {
		if v == s {
			return true
		}
	}
	return false
}
