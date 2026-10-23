package storefront

import (
	"context"
	"log/slog"
	"net/http"
	"net/url"
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/format"
)

// handlers carries the dependencies every storefront handler needs.
type handlers struct{ d *app.Deps }

func (h *handlers) secure() bool { return h.d.Cfg != nil && h.d.Cfg.SecureCookies }

// userID is the signed-in user's id (nil for guests).
func userID(r *http.Request) *uuid.UUID {
	if u, ok := auth.CurrentUser(r.Context()); ok && u != nil {
		id := u.ID
		return &id
	}
	return nil
}

// serverError logs err and answers 500 (plain text; the logging middleware
// records the request).
func serverError(w http.ResponseWriter, r *http.Request, msg string, err error) {
	slog.ErrorContext(r.Context(), msg, "error", err, "path", r.URL.Path)
	http.Error(w, "Erro interno.", http.StatusInternalServerError)
}

// parseForm parses the body; a too-large body answers 413 and returns false.
func parseForm(w http.ResponseWriter, r *http.Request) bool {
	if err := r.ParseForm(); err != nil {
		if httpx.IsBodyTooLarge(err) {
			httpx.TooLarge(w)
			return false
		}
		http.Error(w, "Formulário inválido.", http.StatusBadRequest)
		return false
	}
	return true
}

// refererPath is the path+query of a same-origin Referer (used as the
// "back" target of no-JS form posts), or "".
func refererPath(r *http.Request) string {
	ref := r.Header.Get("Referer")
	if ref == "" {
		return ""
	}
	u, err := url.Parse(ref)
	if err != nil || u.Host == "" || !strings.EqualFold(u.Host, r.Host) {
		return ""
	}
	p := u.EscapedPath()
	if p == "" {
		p = "/"
	}
	if u.RawQuery != "" {
		p += "?" + u.RawQuery
	}
	if safe, ok := httpx.SafeLocalRedirect(p); ok {
		return safe
	}
	return ""
}

// backOr is the Referer path or fallback.
func backOr(r *http.Request, fallback string) string {
	if p := refererPath(r); p != "" {
		return p
	}
	return fallback
}

func price(c int64) string { return format.Price(c) }

func pixText(cents int64) string { return format.Price(cents) + " no Pix" }

func deref(s *string) string {
	if s == nil {
		return ""
	}
	return *s
}

// productPath is /produtos/{slug} with the slug escaped as a segment.
func productPath(slug string) string { return "/produtos/" + url.PathEscape(slug) }

// setPath is /sets/{slug}.
func setPath(slug string) string { return "/sets/" + url.PathEscape(slug) }

// productVariantHref is the variant picker link.
func productVariantHref(slug string, variantID uuid.UUID) string {
	return productPath(slug) + "?" + url.Values{"variant": {variantID.String()}}.Encode()
}

// withQuery appends an encoded query to path.
func withQuery(path string, v url.Values) string {
	if q := v.Encode(); q != "" {
		return path + "?" + q
	}
	return path
}

// resolveCover resolves a set cover or returns the fallback image.
func resolveCover(ctx context.Context, d *app.Deps, key *string) string {
	if u := store.ResolveStorageURL(ctx, d.Storage, key); u != "" {
		return u
	}
	return productFallbackImage
}

// productFallbackImage is the placeholder of sets without a cover (same
// path as the Rust app).
const productFallbackImage = "/static/product-fallback.svg"
