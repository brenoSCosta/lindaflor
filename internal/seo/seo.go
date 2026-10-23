// Package seo: per-page <head> metadata (title, description, canonical,
// robots, Open Graph/Twitter, JSON-LD), robots.txt and sitemap.xml.
//
// Pages call SetHead before render.Page; the root layout renders Resolve's
// output. Builders for the storefront heads live in structured.go
// (ProductHead, SetPageHead, HomeHead); robots.txt and sitemap.xml in
// sitemap.go.
package seo

import (
	"context"
	"encoding/json"
	"fmt"
	"net/http"
	"strings"
	"unicode/utf8"
)

// Site constants.
const (
	SiteName           = "Linda Flor"
	DefaultTitle       = "Linda Flor — Moda Praia"
	DefaultDescription = "Moda praia feita com carinho em Aracaju: biquínis, maiôs, saídas de praia e acessórios Linda Flor. Frete para todo o Brasil e desconto no Pix."
	descriptionMax     = 160
)

var privatePrefixes = []string{
	"/admin", "/carrinho", "/check-email", "/checkout", "/confirmar-email", "/conta",
	"/dashboard", "/excluir-conta", "/favoritos", "/forgot-password", "/login", "/logout",
	"/newsletter", "/pedido", "/reset-password", "/settings", "/signup", "/two-factor",
	"/verify-email",
}

// IsPrivatePath reports never-indexed areas (whole path segments).
func IsPrivatePath(path string) bool {
	for _, p := range privatePrefixes {
		if rest, ok := strings.CutPrefix(path, p); ok && (rest == "" || strings.HasPrefix(rest, "/")) {
			return true
		}
	}
	return false
}

// OpenGraph is the share-card data of a page.
type OpenGraph struct {
	Kind       string // "website" (default) or "product"
	Image      string // absolute or root-relative
	ImageAlt   string
	PriceCents *int64
	InStock    *bool
}

// Head is what a page hands the root layout through SetHead.
type Head struct {
	Title       string
	Description string
	Canonical   string // path (+query) relative to the site root
	NoIndex     bool
	OpenGraph   *OpenGraph
	JSONLD      []any
}

// New is a head titled "<title> | Linda Flor".
func New(title string) Head { return Head{Title: title + " | " + SiteName} }

// SiteDefault is the home-page head.
func SiteDefault() Head { return Head{Title: DefaultTitle, Description: DefaultDescription} }

// WithDescription sets a summarized meta description.
func (h Head) WithDescription(text string) Head {
	h.Description = Summarize(text, descriptionMax)
	return h
}

// WithCanonical sets the canonical path.
func (h Head) WithCanonical(path string) Head { h.Canonical = path; return h }

// WithNoIndex marks the page noindex.
func (h Head) WithNoIndex() Head { h.NoIndex = true; return h }

// WithOpenGraph sets the share card.
func (h Head) WithOpenGraph(og OpenGraph) Head { h.OpenGraph = &og; return h }

// WithJSONLD appends a JSON-LD document.
func (h Head) WithJSONLD(doc any) Head { h.JSONLD = append(h.JSONLD, doc); return h }

// Summarize collapses whitespace and truncates to maxChars (with "…").
func Summarize(text string, maxChars int) string {
	collapsed := strings.Join(strings.Fields(text), " ")
	if utf8.RuneCountInString(collapsed) <= maxChars {
		return collapsed
	}
	r := []rune(collapsed)[:max(maxChars-1, 0)]
	return strings.TrimRight(string(r), " ") + "…"
}

type slot struct{ head *Head }

type ctxKey struct{}

// Middleware installs the per-request head slot.
func Middleware(next http.Handler) http.Handler {
	return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
		next.ServeHTTP(w, r.WithContext(context.WithValue(r.Context(), ctxKey{}, &slot{})))
	})
}

// SetHead records the page head. Call it in the handler before rendering.
func SetHead(ctx context.Context, h Head) {
	if s, ok := ctx.Value(ctxKey{}).(*slot); ok {
		s.head = &h
	}
}

// HeadFrom returns the head set for this request, if any.
func HeadFrom(ctx context.Context) (Head, bool) {
	if s, ok := ctx.Value(ctxKey{}).(*slot); ok && s.head != nil {
		return *s.head, true
	}
	return Head{}, false
}

// SiteOrigin is APP_ORIGIN when set, else http://<Host> (dev), else
// http://localhost.
func SiteOrigin(appOrigin string, r *http.Request) string {
	if appOrigin != "" {
		return appOrigin
	}
	host := r.Host
	if host != "" && !strings.ContainsFunc(host, func(c rune) bool { return !hostChar(c) }) {
		return "http://" + host
	}
	return "http://localhost"
}

// AbsoluteURL joins origin and a root-relative path (absolute URLs pass).
func AbsoluteURL(origin, path string) string {
	if strings.Contains(path, "://") {
		return path
	}
	if strings.HasPrefix(path, "/") {
		return origin + path
	}
	return origin + "/" + path
}

// JSONLDScriptBody serializes doc with <, >, &, U+2028/9 escaped so data
// cannot close the <script> element.
func JSONLDScriptBody(doc any) string {
	b, err := json.Marshal(doc) // escapes <, >, & and U+2028/9 as < etc.
	if err != nil {
		return "{}"
	}
	return string(b)
}

// DecimalPrice formats cents for schema.org / OG (8990 → "89.90").
func DecimalPrice(cents int64) string {
	sign := ""
	if cents < 0 {
		sign, cents = "-", -cents
	}
	return fmt.Sprintf("%s%d.%02d", sign, cents/100, cents%100)
}

// ResolvedOG is the share card ready to render.
type ResolvedOG struct {
	Kind, Title, Description, URL, Image, ImageAlt, Price, Availability string
}

// Resolved is everything the layout renders in <head>.
type Resolved struct {
	Title       string
	Description string
	Canonical   string
	Robots      string
	OG          *ResolvedOG
	JSONLD      []string
}

// Resolve turns the page head into final strings. failed marks an error
// page (404): never indexed. Private paths are always noindex.
func Resolve(h Head, set, failed bool, path, origin string) Resolved {
	switch {
	case failed:
		h = New("Página não encontrada")
		h.NoIndex = true
	case !set:
		h = Head{Title: DefaultTitle}
	}
	noindex := h.NoIndex || IsPrivatePath(path)
	out := Resolved{Title: h.Title, Description: h.Description}
	if noindex {
		out.Robots = "noindex, nofollow"
		return out
	}
	if h.Canonical != "" {
		out.Canonical = AbsoluteURL(origin, h.Canonical)
	}
	if og := h.OpenGraph; og != nil {
		r := &ResolvedOG{Kind: og.Kind, Title: h.Title, Description: h.Description, URL: out.Canonical, ImageAlt: og.ImageAlt}
		if r.Kind == "" {
			r.Kind = "website"
		}
		if og.Image != "" {
			r.Image = AbsoluteURL(origin, og.Image)
		}
		if og.PriceCents != nil {
			r.Price = DecimalPrice(*og.PriceCents)
		}
		if og.InStock != nil {
			r.Availability = "out of stock"
			if *og.InStock {
				r.Availability = "in stock"
			}
		}
		out.OG = r
	}
	for _, d := range h.JSONLD {
		out.JSONLD = append(out.JSONLD, JSONLDScriptBody(d))
	}
	return out
}

func hostChar(c rune) bool {
	return ('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z') || ('0' <= c && c <= '9') || strings.ContainsRune(".-:[]", c)
}
