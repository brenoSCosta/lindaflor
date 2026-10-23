package seo

import (
	"context"
	"log/slog"
	"net/http"
	"strings"
	"sync"
	"time"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// Crawler routes.
const (
	RobotsPath  = "/robots.txt"
	SitemapPath = "/sitemap.xml"
)

// SitemapCacheTTL is how long a built sitemap is served before a rebuild,
// so crawler bursts cost one query set.
const SitemapCacheTTL = 5 * time.Minute

// Disallowed are the areas crawlers must not fetch (private pages also
// carry noindex).
var Disallowed = []string{"/admin", "/conta", "/checkout", "/carrinho", "/pedido", "/api", "/settings"}

// staticPages are the indexable static pages: path, changefreq, priority.
var staticPages = [][3]string{
	{"/", "daily", "1.0"},
	{"/produtos", "daily", "0.9"},
	{"/sets", "weekly", "0.7"},
	{"/guia-de-medidas", "yearly", "0.4"},
	{"/trocas-devolucoes", "yearly", "0.3"},
	{"/termos", "yearly", "0.2"},
	{"/politica-privacidade", "yearly", "0.2"},
}

// RobotsBody is robots.txt for origin.
func RobotsBody(origin string) string {
	var b strings.Builder
	b.WriteString("User-agent: *\n")
	for _, p := range Disallowed {
		b.WriteString("Disallow: " + p + "\n")
	}
	b.WriteString("\nSitemap: " + origin + SitemapPath + "\n")
	return b.String()
}

// SitemapEntry is one <url>.
type SitemapEntry struct {
	Path       string // root-relative path (+ query)
	LastMod    string // YYYY-MM-DD, "" when unknown
	ChangeFreq string
	Priority   string
}

var xmlEscaper = strings.NewReplacer("&", "&amp;", "<", "&lt;", ">", "&gt;", `"`, "&quot;", "'", "&apos;")

// RenderSitemap renders the urlset.
func RenderSitemap(origin string, entries []SitemapEntry) string {
	var b strings.Builder
	b.WriteString("<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n")
	for _, e := range entries {
		b.WriteString("  <url><loc>")
		b.WriteString(xmlEscaper.Replace(AbsoluteURL(origin, e.Path)))
		b.WriteString("</loc>")
		if e.LastMod != "" {
			b.WriteString("<lastmod>" + xmlEscaper.Replace(e.LastMod) + "</lastmod>")
		}
		b.WriteString("<changefreq>" + e.ChangeFreq + "</changefreq><priority>" + e.Priority + "</priority></url>\n")
	}
	b.WriteString("</urlset>\n")
	return b.String()
}

// SitemapEntries lists every indexable URL: static pages, category pages
// with active products, then active products and active sets (lastmod).
func SitemapEntries(ctx context.Context, q *sqlc.Queries) ([]SitemapEntry, error) {
	entries := make([]SitemapEntry, 0, len(staticPages))
	for _, p := range staticPages {
		entries = append(entries, SitemapEntry{Path: p[0], ChangeFreq: p[1], Priority: p[2]})
	}
	cats, err := q.SeoActiveCategories(ctx)
	if err != nil {
		return nil, err
	}
	for _, c := range cats {
		entries = append(entries, SitemapEntry{Path: "/produtos?category=" + c, ChangeFreq: "daily", Priority: "0.8"})
	}
	products, err := q.SeoActiveProducts(ctx)
	if err != nil {
		return nil, err
	}
	for _, p := range products {
		entries = append(entries, SitemapEntry{Path: ProductPath(p.Slug), LastMod: p.Lastmod, ChangeFreq: "weekly", Priority: "0.8"})
	}
	sets, err := q.SeoActiveSets(ctx)
	if err != nil {
		return nil, err
	}
	for _, s := range sets {
		entries = append(entries, SitemapEntry{Path: SetPath(s.Slug), LastMod: s.Lastmod, ChangeFreq: "weekly", Priority: "0.6"})
	}
	return entries, nil
}

// RobotsHandler serves GET /robots.txt.
func RobotsHandler(appOrigin string) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		w.Header().Set("Content-Type", "text/plain; charset=utf-8")
		w.Header().Set("Cache-Control", "public, max-age=3600")
		_, _ = w.Write([]byte(RobotsBody(SiteOrigin(appOrigin, r))))
	}
}

// SitemapCache keeps the last built sitemap per origin for SitemapCacheTTL.
type SitemapCache struct {
	mu      sync.Mutex
	origin  string
	builtAt time.Time
	xml     string
}

func (c *SitemapCache) get(origin string, now time.Time) (string, bool) {
	c.mu.Lock()
	defer c.mu.Unlock()
	if c.xml == "" || c.origin != origin || now.Sub(c.builtAt) >= SitemapCacheTTL {
		return "", false
	}
	return c.xml, true
}

func (c *SitemapCache) put(origin, xml string, now time.Time) {
	c.mu.Lock()
	defer c.mu.Unlock()
	c.origin, c.xml, c.builtAt = origin, xml, now
}

// SitemapHandler serves GET /sitemap.xml from Postgres through cache.
func SitemapHandler(appOrigin string, q *sqlc.Queries, cache *SitemapCache) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		origin := SiteOrigin(appOrigin, r)
		xml, ok := cache.get(origin, time.Now())
		if !ok {
			entries, err := SitemapEntries(r.Context(), q)
			if err != nil {
				slog.ErrorContext(r.Context(), "sitemap build failed", "error", err)
				http.Error(w, "Erro interno do servidor.", http.StatusInternalServerError)
				return
			}
			xml = RenderSitemap(origin, entries)
			cache.put(origin, xml, time.Now())
		}
		w.Header().Set("Content-Type", "application/xml; charset=utf-8")
		w.Header().Set("Cache-Control", "public, max-age=300")
		_, _ = w.Write([]byte(xml))
	}
}
