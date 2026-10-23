package seo_test

import (
	"bytes"
	"context"
	"encoding/json"
	"strings"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/web/layout"
)

const origin = "https://loja.example.com"

// renderHead renders the root layout's <head> for h (as the layout does
// for a page that called SetHead).
func renderHead(t *testing.T, h *seo.Head, failed bool, path string) string {
	t.Helper()
	var head seo.Head
	if h != nil {
		head = *h
	}
	p := layout.Page{
		Head: seo.Resolve(head, h != nil, failed, path, origin), Path: path,
		Shell: layout.ShellData{Settings: layout.DefaultStoreSettings()},
	}
	var buf bytes.Buffer
	if err := layout.Root(p).Render(context.Background(), &buf); err != nil {
		t.Fatal(err)
	}
	html := buf.String()
	start, end := strings.Index(html, "<head>"), strings.Index(html, "</head>")
	if start < 0 || end < 0 {
		t.Fatal(html)
	}
	return html[start : end+len("</head>")]
}

func jsonLDBlocks(t *testing.T, html string) []map[string]any {
	t.Helper()
	const open = `<script type="application/ld+json">`
	var out []map[string]any
	for _, rest := range strings.Split(html, open)[1:] {
		body, _, _ := strings.Cut(rest, "</script>")
		var v map[string]any
		if err := json.Unmarshal([]byte(body), &v); err != nil {
			t.Fatalf("invalid JSON-LD %q: %v", body, err)
		}
		out = append(out, v)
	}
	return out
}

func contains(t *testing.T, html string, needles ...string) {
	t.Helper()
	for _, n := range needles {
		if !strings.Contains(html, n) {
			t.Fatalf("missing %q in\n%s", n, html)
		}
	}
}

func TestDefaultHeadHasSiteTitleAndNoRobots(t *testing.T) {
	html := renderHead(t, nil, false, "/sets")
	contains(t, html, "<title>Linda Flor — Moda Praia</title>")
	if strings.Contains(html, "robots") {
		t.Fatal(html)
	}
}

func TestPrivatePathsAreNoindexEvenWithAHead(t *testing.T) {
	h := seo.New("Checkout").WithCanonical("/checkout")
	html := renderHead(t, &h, false, "/checkout")
	contains(t, html, `<meta name="robots" content="noindex, nofollow">`)
	if strings.Contains(html, "canonical") {
		t.Fatal(html)
	}
}

func TestFailedPageIsNoindex(t *testing.T) {
	h := seo.New("Produto").WithCanonical("/produtos/x")
	html := renderHead(t, &h, true, "/produtos/x")
	contains(t, html, "noindex", "Página não encontrada")
	if strings.Contains(html, "canonical") {
		t.Fatal(html)
	}
}

func TestAttributeValuesAreEscaped(t *testing.T) {
	h := seo.New(`Biquíni "Sol" <b>`).WithDescription(`Top "lindo" & <confortável>`).WithCanonical("/produtos/sol")
	html := renderHead(t, &h, false, "/produtos/sol")
	contains(t, html, `<link rel="canonical" href="https://loja.example.com/produtos/sol">`, "&amp;", "&lt;b&gt;")
	if strings.Contains(html, "<b>") || strings.Contains(html, `"lindo"`) {
		t.Fatal(html)
	}
}

func product(available int64) seo.ProductInfo {
	return seo.ProductInfo{
		Name:        "Biquíni </script><script>alert(1)</script>",
		Slug:        "biquini-sol",
		Description: `Top "cortininha" & calcinha <lacinho>.`,
		Category:    "biquini",
		PriceCents:  8990,
		Images:      []seo.ProductImage{{URL: "https://s3.example.com/lf/p/1.jpg?X-Amz-Signature=abc&X-Amz-Expires=86400", Alt: "Biquíni Sol"}},
		Variants:    []seo.ProductVariant{{SKU: "BQ-SOL-M", Available: available}},
	}
}

func TestProductSchemaHasOfferInBRL(t *testing.T) {
	schema := seo.ProductHead(origin, product(3), nil).JSONLD[0].(map[string]any)
	offers := schema["offers"].(map[string]any)
	if schema["@type"] != "Product" || schema["brand"].(map[string]any)["name"] != "Lindaflor" || schema["sku"] != "BQ-SOL-M" ||
		schema["url"] != "https://loja.example.com/produtos/biquini-sol" || offers["price"] != "89.90" ||
		offers["priceCurrency"] != "BRL" || offers["availability"] != "https://schema.org/InStock" ||
		!strings.HasPrefix(schema["image"].([]string)[0], "https://") || schema["category"] != "Biquíni" {
		t.Fatalf("%+v", schema)
	}
	soldOut := seo.ProductHead(origin, product(0), nil).JSONLD[0].(map[string]any)
	if soldOut["offers"].(map[string]any)["availability"] != "https://schema.org/OutOfStock" {
		t.Fatal("sold out")
	}
}

func TestProductPageHeadContainsExpectedTags(t *testing.T) {
	h := seo.ProductHead(origin, product(2), nil)
	html := renderHead(t, &h, false, "/produtos/biquini-sol")
	contains(t, html,
		"<title>Biquíni &lt;/script&gt;&lt;script&gt;alert(1)&lt;/script&gt; | Linda Flor</title>",
		`<link rel="canonical" href="https://loja.example.com/produtos/biquini-sol">`,
		`<meta property="og:type" content="product">`,
		`<meta property="og:url" content="https://loja.example.com/produtos/biquini-sol">`,
		`<meta property="og:image" content="https://s3.example.com/lf/p/1.jpg?X-Amz-Signature=abc&amp;X-Amz-Expires=86400">`,
		`<meta property="og:image:alt" content="Biquíni Sol">`,
		`<meta property="product:price:amount" content="89.90">`,
		`<meta property="product:price:currency" content="BRL">`,
		`<meta property="product:availability" content="in stock">`,
		`<meta name="twitter:card" content="summary_large_image">`,
		`<meta name="description" content="Top &#34;cortininha&#34; &amp; calcinha &lt;lacinho&gt;.">`,
		`<script type="application/ld+json">`,
	)
	if strings.Contains(html, "robots") {
		t.Fatal(html)
	}
	// The first </script> after the JSON-LD opening tag is its own closing
	// tag: the product name's </script> stays escaped inside the JSON.
	_, ld, _ := strings.Cut(html, `<script type="application/ld+json">`)
	body, _, _ := strings.Cut(ld, "</script>")
	if strings.Contains(body, "<") {
		t.Fatal(body)
	}
	blocks := jsonLDBlocks(t, html)
	if len(blocks) != 1 || blocks[0]["name"] != "Biquíni </script><script>alert(1)</script>" {
		t.Fatal(blocks)
	}
}

func TestHomeHeadHasOrganizationAndWebsiteSearch(t *testing.T) {
	h := seo.HomeHead(origin)
	html := renderHead(t, &h, false, "/")
	contains(t, html, "<title>Linda Flor — Moda Praia</title>", `<link rel="canonical" href="https://loja.example.com/">`,
		`<meta property="og:type" content="website">`, `<meta name="twitter:card" content="summary">`)
	blocks := jsonLDBlocks(t, html)
	if len(blocks) != 2 || blocks[0]["@type"] != "Organization" || blocks[1]["@type"] != "WebSite" {
		t.Fatal(blocks)
	}
	tmpl := blocks[1]["potentialAction"].(map[string]any)["target"].(map[string]any)["urlTemplate"]
	if tmpl != "https://loja.example.com/produtos?search={search_term_string}" {
		t.Fatal(tmpl)
	}
}

func TestSetHeadUsesBundlePriceAndCover(t *testing.T) {
	price := int64(15990)
	h := seo.SetPageHead(origin, seo.SetInfo{Name: "Verão", Slug: "verão 26", PriceCents: &price}, "/img/cover.jpg", false)
	html := renderHead(t, &h, false, "/sets/ver%C3%A3o%2026")
	contains(t, html, "<title>Verão | Linda Flor</title>", `href="https://loja.example.com/sets/ver%C3%A3o%2026"`,
		`content="Conjunto Verão Linda Flor: top e calcinha combinando, com preço especial."`,
		`<meta property="og:image" content="https://loja.example.com/img/cover.jpg">`,
		`<meta property="product:price:amount" content="159.90">`, `content="out of stock"`)
}

func TestSlugsArePercentEncoded(t *testing.T) {
	if seo.ProductPath("biquini-sol") != "/produtos/biquini-sol" || seo.SetPath("verão 26") != "/sets/ver%C3%A3o%2026" {
		t.Fatal(seo.SetPath("verão 26"))
	}
}

func TestOGImageURLKeepsAbsoluteAndJoinsRelative(t *testing.T) {
	if seo.OGImageURL(origin, "/img/a.jpg") != "https://loja.example.com/img/a.jpg" || seo.OGImageURL(origin, "products/a.jpg") != "" || seo.OGImageURL(origin, "") != "" {
		t.Fatal("og image")
	}
}

func TestProductJSONLDIncludesAggregateRatingWhenReviewed(t *testing.T) {
	agg := seo.ProductHead(origin, product(3), &seo.AggregateRating{Average: 4.5, Count: 12}).JSONLD[0].(map[string]any)["aggregateRating"].(map[string]any)
	if agg["ratingValue"] != 4.5 || agg["reviewCount"] != int64(12) || agg["@type"] != "AggregateRating" {
		t.Fatal(agg)
	}
	if _, ok := seo.ProductHead(origin, product(3), nil).JSONLD[0].(map[string]any)["aggregateRating"]; ok {
		t.Fatal("unrated")
	}
}

func TestRobotsAndSitemapRendering(t *testing.T) {
	body := seo.RobotsBody(origin)
	if !strings.HasPrefix(body, "User-agent: *\n") || !strings.Contains(body, "Sitemap: https://loja.example.com/sitemap.xml") {
		t.Fatal(body)
	}
	for _, p := range seo.Disallowed {
		contains(t, body, "Disallow: "+p+"\n")
	}
	xml := seo.RenderSitemap(origin, []seo.SitemapEntry{{Path: "/produtos?category=maio&x=<y>", LastMod: "2026-10-22", ChangeFreq: "daily", Priority: "0.8"}})
	contains(t, xml, "<loc>https://loja.example.com/produtos?category=maio&amp;x=&lt;y&gt;</loc>", "<lastmod>2026-10-22</lastmod>")
}
