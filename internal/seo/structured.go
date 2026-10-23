package seo

import (
	"fmt"
	"strings"
)

// Brand is the brand name in the Product schema.
const Brand = "Lindaflor"

// InstagramURL is linked from the Organization schema.
const InstagramURL = "https://www.instagram.com/biquinislindaflor"

// EncodeSegment percent-encodes one path segment (slugs are admin-entered).
func EncodeSegment(segment string) string {
	var b strings.Builder
	for i := 0; i < len(segment); i++ {
		c := segment[i]
		if ('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z') || ('0' <= c && c <= '9') || strings.IndexByte("-_.~", c) >= 0 {
			b.WriteByte(c)
		} else {
			fmt.Fprintf(&b, "%%%02X", c)
		}
	}
	return b.String()
}

// ProductPath is "/produtos/<slug>".
func ProductPath(slug string) string { return "/produtos/" + EncodeSegment(slug) }

// SetPath is "/sets/<slug>".
func SetPath(slug string) string { return "/sets/" + EncodeSegment(slug) }

// OGImageURL is the image URL for og:image / JSON-LD: absolute http(s)
// URLs pass through (presigned S3 URLs: valid 24h, reused ~20h — scrapers
// cache the image at share time), root-relative paths are joined to the
// origin, anything else is dropped ("").
func OGImageURL(origin, url string) string {
	url = strings.TrimSpace(url)
	switch {
	case url == "":
		return ""
	case strings.HasPrefix(url, "https://") || strings.HasPrefix(url, "http://"):
		return url
	case strings.HasPrefix(url, "/"):
		return AbsoluteURL(origin, url)
	}
	return ""
}

func categoryLabel(category string) string {
	switch category {
	case "biquini":
		return "Biquíni"
	case "biquini_top":
		return "Top de biquíni"
	case "biquini_bottom":
		return "Calcinha de biquíni"
	case "maio":
		return "Maiô"
	case "saida_praia":
		return "Saída de praia"
	case "acessorio":
		return "Acessório"
	}
	return "Peça"
}

func availabilityURL(inStock bool) string {
	if inStock {
		return "https://schema.org/InStock"
	}
	return "https://schema.org/OutOfStock"
}

// ProductImage is one product image (URL already resolved by the page).
type ProductImage struct {
	URL string
	Alt string
}

// ProductVariant is what the head needs from a variant.
type ProductVariant struct {
	SKU       string
	Available int64 // live available units
}

// ProductInfo is the product page data the head is built from.
type ProductInfo struct {
	Name        string
	Slug        string
	Description string
	Category    string // product_category enum value
	PriceCents  int64
	Images      []ProductImage
	Variants    []ProductVariant
}

// AggregateRating is the approved-reviews summary.
type AggregateRating struct {
	Average float64
	Count   int64
}

// ProductHead is the head of /produtos/{slug}: title, description,
// canonical, Open Graph product card and a JSON-LD Product with a BRL
// Offer (availability: any variant with stock) and, when reviewed, an
// aggregateRating.
func ProductHead(origin string, p ProductInfo, rating *AggregateRating) Head {
	path := ProductPath(p.Slug)
	url := AbsoluteURL(origin, path)
	inStock := false
	sku := ""
	for _, v := range p.Variants {
		if v.Available > 0 {
			inStock = true
			if sku == "" {
				sku = v.SKU
			}
		}
	}
	if sku == "" && len(p.Variants) > 0 {
		sku = p.Variants[0].SKU
	}
	description := strings.TrimSpace(p.Description)
	if description == "" {
		description = fmt.Sprintf("%s — %s Linda Flor. Moda praia feita em Aracaju com envio para todo o Brasil.", p.Name, categoryLabel(p.Category))
	}
	var images []string
	for _, img := range p.Images {
		if u := OGImageURL(origin, img.URL); u != "" {
			images = append(images, u)
		}
	}
	schema := map[string]any{
		"@context":    "https://schema.org",
		"@type":       "Product",
		"name":        p.Name,
		"description": Summarize(description, 5000),
		"brand":       map[string]any{"@type": "Brand", "name": Brand},
		"category":    categoryLabel(p.Category),
		"url":         url,
		"offers": map[string]any{
			"@type":         "Offer",
			"url":           url,
			"price":         DecimalPrice(p.PriceCents),
			"priceCurrency": "BRL",
			"availability":  availabilityURL(inStock),
			"itemCondition": "https://schema.org/NewCondition",
		},
	}
	if len(images) > 0 {
		schema["image"] = images
	}
	if sku != "" {
		schema["sku"] = sku
	}
	if rating != nil {
		schema["aggregateRating"] = map[string]any{
			"@type":       "AggregateRating",
			"ratingValue": rating.Average,
			"reviewCount": rating.Count,
			"bestRating":  5,
			"worstRating": 1,
		}
	}
	og := OpenGraph{Kind: "product", ImageAlt: p.Name, PriceCents: &p.PriceCents, InStock: &inStock}
	if len(images) > 0 {
		og.Image = images[0]
	}
	if len(p.Images) > 0 && p.Images[0].Alt != "" {
		og.ImageAlt = p.Images[0].Alt
	}
	return New(p.Name).WithDescription(description).WithCanonical(path).WithOpenGraph(og).WithJSONLD(schema)
}

// SetInfo is the set (bundle) page data the head is built from.
type SetInfo struct {
	Name        string
	Slug        string
	Description string
	PriceCents  *int64 // bundle price, nil when priced per piece
}

// SetPageHead is the head of /sets/{slug}: title, description, canonical
// and an Open Graph product card.
func SetPageHead(origin string, s SetInfo, coverURL string, inStock bool) Head {
	description := strings.TrimSpace(s.Description)
	if description == "" {
		description = fmt.Sprintf("Conjunto %s Linda Flor: top e calcinha combinando, com preço especial.", s.Name)
	}
	return New(s.Name).WithDescription(description).WithCanonical(SetPath(s.Slug)).WithOpenGraph(OpenGraph{
		Kind: "product", Image: OGImageURL(origin, coverURL), ImageAlt: s.Name, PriceCents: s.PriceCents, InStock: &inStock,
	})
}

// HomeHead is the home page head: default title/description, a website
// card and JSON-LD Organization + WebSite (SearchAction into the catalog
// search, query parameter "search").
func HomeHead(origin string) Head {
	home := AbsoluteURL(origin, "/")
	return SiteDefault().WithCanonical("/").WithOpenGraph(OpenGraph{Kind: "website"}).
		WithJSONLD(map[string]any{
			"@context": "https://schema.org",
			"@type":    "Organization",
			"name":     SiteName,
			"url":      home,
			"sameAs":   []string{InstagramURL},
		}).
		WithJSONLD(map[string]any{
			"@context":   "https://schema.org",
			"@type":      "WebSite",
			"name":       SiteName,
			"url":        home,
			"inLanguage": "pt-BR",
			"potentialAction": map[string]any{
				"@type": "SearchAction",
				"target": map[string]any{
					"@type":       "EntryPoint",
					"urlTemplate": AbsoluteURL(origin, "/produtos") + "?search={search_term_string}",
				},
				"query-input": "required name=search_term_string",
			},
		})
}
