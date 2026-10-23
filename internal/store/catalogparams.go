package store

import (
	"context"
	"math"
	"net/url"
	"strconv"
	"strings"
)

// Catalog URL state (src/app/produtos.rs + src/app/store/catalog_filters.rs):
// query parsing, facet normalization and the chip "remove this filter"
// links. Shared by /produtos, /busca/produtos and /sets/{slug}.

// SortOptions are (value, label) in menu order.
var SortOptions = [][2]string{
	{"destaque", "Em destaque"},
	{"relevantes", "Mais relevantes"},
	{"mais_vendidos", "Mais vendidos"},
	{"maior_desconto", "Maior desconto"},
	{"menor_preco", "Menor preço"},
	{"maior_preco", "Maior preço"},
	{"novos", "Mais novos"},
	{"antigos", "Mais antigos"},
}

const (
	searchMax      = 80
	colorFilterMax = 20
	colorNameMax   = 40
)

// QueryPair is one decoded key/value of a query string.
type QueryPair struct{ Key, Value string }

// QueryPairs decodes a raw query string leniently (form-urlencoded: '+'
// is a space; malformed escapes stay literal), keeping order and repeats.
func QueryPairs(raw string) []QueryPair {
	var out []QueryPair
	for part := range strings.SplitSeq(raw, "&") {
		if part == "" {
			continue
		}
		k, v, _ := strings.Cut(part, "=")
		out = append(out, QueryPair{formDecode(k), formDecode(v)})
	}
	return out
}

func formDecode(s string) string {
	s = strings.ReplaceAll(s, "+", " ")
	var b strings.Builder
	for i := 0; i < len(s); i++ {
		if s[i] == '%' && i+2 < len(s) && isHex(s[i+1]) && isHex(s[i+2]) {
			v, _ := strconv.ParseUint(s[i+1:i+3], 16, 8)
			b.WriteByte(byte(v))
			i += 2
			continue
		}
		b.WriteByte(s[i])
	}
	return strings.ToValidUTF8(b.String(), "�")
}

// RepeatedValues collects the non-empty values of key, in order.
func RepeatedValues(pairs []QueryPair, key string) []string {
	out := []string{}
	for _, p := range pairs {
		if p.Key == key && p.Value != "" {
			out = append(out, p.Value)
		}
	}
	return out
}

func firstValue(pairs []QueryPair, key string) (string, bool) {
	for _, p := range pairs {
		if p.Key == key {
			return p.Value, true
		}
	}
	return "", false
}

// CatalogCategory normalizes ?category= ("all" for unknown values).
func CatalogCategory(raw string) string {
	c := strings.TrimSpace(raw)
	if contains(ValidCategories, c) {
		return c
	}
	return "all"
}

// CatalogSearch trims and caps the search term.
func CatalogSearch(raw string) string { return truncateRunes(strings.TrimSpace(raw), searchMax) }

// PriceParamCents parses "89,90" / "100" (0..=100000 reais) to cents.
func PriceParamCents(raw *string) *int64 {
	if raw == nil {
		return nil
	}
	s := strings.ReplaceAll(strings.TrimSpace(*raw), ",", ".")
	if strings.ContainsAny(s, "xXpP_") {
		return nil
	}
	v, err := strconv.ParseFloat(s, 64)
	if err != nil || math.IsNaN(v) || v < 0 || v > 100_000 {
		return nil
	}
	c := int64(math.Round(v * 100))
	return &c
}

// NormalizeCatalogFilters drops unknown sizes (deduping adjacent repeats),
// caps colors (20 values, 40 chars each) and parses prices.
func NormalizeCatalogFilters(sizes, colors []string, precoMin, precoMax *string, availableOnly, saleOnly bool) CatalogFilters {
	f := CatalogFilters{Sizes: []string{}, Colors: []string{}, AvailableOnly: availableOnly, SaleOnly: saleOnly}
	for _, s := range sizes {
		s = strings.ToLower(strings.TrimSpace(s))
		if !contains(ValidSizes, s) {
			continue
		}
		if n := len(f.Sizes); n > 0 && f.Sizes[n-1] == s {
			continue
		}
		f.Sizes = append(f.Sizes, s)
	}
	for _, c := range colors {
		c = truncateRunes(strings.TrimSpace(c), colorNameMax)
		if c == "" {
			continue
		}
		if len(f.Colors) == colorFilterMax {
			break
		}
		f.Colors = append(f.Colors, c)
	}
	f.PriceMinCents = PriceParamCents(precoMin)
	f.PriceMaxCents = PriceParamCents(precoMax)
	return f
}

// CatalogParams is the parsed catalog URL state.
type CatalogParams struct {
	Category          string
	Search            string
	FeaturedOnly      bool
	IncludeOutOfStock bool
	Page              int64
	Sort              string
	Sizes             []string // tamanho
	Colors            []string // cor
	PrecoMin          *string
	PrecoMax          *string
	Disponivel        bool
	Sale              bool
}

// ParseCatalogParams parses a raw query string.
func ParseCatalogParams(raw string) CatalogParams {
	pairs := QueryPairs(raw)
	first := func(k string) *string {
		if v, ok := firstValue(pairs, k); ok {
			return &v
		}
		return nil
	}
	flag := func(k string) bool { v := first(k); return v != nil && *v == "true" }
	p := CatalogParams{
		Category:          CatalogCategory(orDefault(first("category"), "all")),
		Search:            CatalogSearch(orDefault(first("search"), "")),
		FeaturedOnly:      flag("destaque"),
		IncludeOutOfStock: flag("esgotados"),
		Page:              1,
		Sort:              orDefault(first("sort"), "destaque"),
		Sizes:             RepeatedValues(pairs, "tamanho"),
		Colors:            RepeatedValues(pairs, "cor"),
		PrecoMin:          first("preco_min"),
		PrecoMax:          first("preco_max"),
		Disponivel:        flag("disponivel"),
		Sale:              flag("sale"),
	}
	if v := first("page"); v != nil {
		if n, err := strconv.ParseInt(strings.TrimSpace(*v), 10, 64); err == nil {
			p.Page = n
		}
	}
	p.Page = ClampPage(p.Page)
	return p
}

// HasFacets reports any active facet filter.
func (p CatalogParams) HasFacets() bool {
	return len(p.Sizes) > 0 || len(p.Colors) > 0 || p.PrecoMin != nil || p.PrecoMax != nil || p.Disponivel || p.Sale
}

// FilterChip is an active filter with the link that removes it.
type FilterChip struct {
	Label string
	Href  string
}

type dropKind int

const (
	dropNone dropKind = iota
	dropSize
	dropColor
	dropPrice
	dropDisponivel
	dropSale
	dropAllFacets
)

// FilterHref rebuilds base?query without the dropped filter.
func (p CatalogParams) filterHref(base string, drop dropKind, value string) string {
	var pairs [][2]string
	if p.Category != "" && p.Category != "all" {
		pairs = append(pairs, [2]string{"category", p.Category})
	}
	if p.Search != "" {
		pairs = append(pairs, [2]string{"search", p.Search})
	}
	if p.Sort != "" && p.Sort != "destaque" {
		pairs = append(pairs, [2]string{"sort", p.Sort})
	}
	if drop != dropAllFacets {
		for _, s := range p.Sizes {
			if !(drop == dropSize && s == value) {
				pairs = append(pairs, [2]string{"tamanho", s})
			}
		}
		for _, c := range p.Colors {
			if !(drop == dropColor && c == value) {
				pairs = append(pairs, [2]string{"cor", c})
			}
		}
		if drop != dropPrice {
			if p.PrecoMin != nil {
				pairs = append(pairs, [2]string{"preco_min", *p.PrecoMin})
			}
			if p.PrecoMax != nil {
				pairs = append(pairs, [2]string{"preco_max", *p.PrecoMax})
			}
		}
		if p.Disponivel && drop != dropDisponivel {
			pairs = append(pairs, [2]string{"disponivel", "true"})
		}
		if p.Sale && drop != dropSale {
			pairs = append(pairs, [2]string{"sale", "true"})
		}
	}
	u := url.URL{Path: base}
	path := u.EscapedPath()
	if len(pairs) == 0 {
		return path
	}
	var b strings.Builder
	for i, kv := range pairs {
		if i > 0 {
			b.WriteByte('&')
		}
		b.WriteString(url.QueryEscape(kv[0]))
		b.WriteByte('=')
		b.WriteString(url.QueryEscape(kv[1]))
	}
	return path + "?" + b.String()
}

// ClearFacetsHref drops every facet (keeps category, search, sort).
func (p CatalogParams) ClearFacetsHref(base string) string {
	return p.filterHref(base, dropAllFacets, "")
}

// Chips are the active-filter chips with their removal links.
func (p CatalogParams) Chips(base string) []FilterChip {
	var chips []FilterChip
	for _, s := range p.Sizes {
		chips = append(chips, FilterChip{SizeLabel(s), p.filterHref(base, dropSize, s)})
	}
	for _, c := range p.Colors {
		chips = append(chips, FilterChip{c, p.filterHref(base, dropColor, c)})
	}
	switch {
	case p.PrecoMin != nil && p.PrecoMax != nil:
		chips = append(chips, FilterChip{"R$ " + *p.PrecoMin + " – R$ " + *p.PrecoMax, p.filterHref(base, dropPrice, "")})
	case p.PrecoMin != nil:
		chips = append(chips, FilterChip{"A partir de R$ " + *p.PrecoMin, p.filterHref(base, dropPrice, "")})
	case p.PrecoMax != nil:
		chips = append(chips, FilterChip{"Até R$ " + *p.PrecoMax, p.filterHref(base, dropPrice, "")})
	}
	if p.Disponivel {
		chips = append(chips, FilterChip{"Disponíveis", p.filterHref(base, dropDisponivel, "")})
	}
	if p.Sale {
		chips = append(chips, FilterChip{"Em promoção", p.filterHref(base, dropSale, "")})
	}
	return chips
}

// CategoryHeadLabel is the SEO label of the category ("" for all).
func (p CatalogParams) CategoryHeadLabel() string {
	for _, c := range Categories {
		if c.Key == p.Category && c.Key != "all" {
			return c.Label
		}
	}
	return ""
}

// CatalogPage is the /busca/produtos result block.
type CatalogPage struct {
	Params          CatalogParams
	Products        []ProductSummary
	Pager           Pagination
	Heading         string
	AvailableColors []ColorSwatch
}

// BrowseCatalog loads one page of the catalog for a raw query string
// (resolve product images before rendering).
func BrowseCatalog(ctx context.Context, q DBTX, rawQuery string) (CatalogPage, error) {
	p := ParseCatalogParams(rawQuery)
	return browse(ctx, q, p)
}

func browse(ctx context.Context, q DBTX, p CatalogParams) (CatalogPage, error) {
	out := CatalogPage{Params: p}
	filters := NormalizeCatalogFilters(p.Sizes, p.Colors, p.PrecoMin, p.PrecoMax, p.Disponivel, p.Sale)
	category := ""
	if p.Category != "all" {
		category = p.Category
	}
	cq := CatalogQuery{
		Category: category, Search: p.Search, FeaturedOnly: p.FeaturedOnly,
		IncludeOutOfStock: p.IncludeOutOfStock && !filters.AvailableOnly,
		Filters:           filters, Sort: p.Sort,
	}
	total, err := CountProducts(ctx, q, cq)
	if err != nil {
		return out, err
	}
	out.Pager = Paginate(p.Page, StorefrontPageSize, total)
	cq.Limit, cq.Offset = out.Pager.PageSize, out.Pager.Offset()
	if out.Products, err = ListProducts(ctx, q, cq); err != nil {
		return out, err
	}
	if out.AvailableColors, err = ListFilterColors(ctx, q, category); err != nil {
		return out, err
	}
	switch {
	case p.Sale:
		out.Heading = "Liquida"
	case p.Sort == "novos":
		out.Heading = "Novidades"
	case p.Search == "":
		out.Heading = "Catálogo"
	default:
		out.Heading = "Busca: " + p.Search
	}
	return out, nil
}
