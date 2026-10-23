// Package layout renders the document shell: the root layout (<head> with
// SEO tags, CSS, vendored scripts, htmx config, impersonation banner,
// toaster), the storefront shell (announcement, header, cart sheet,
// footer), the authenticated app shell (sidebar) and the 404 page.
//
// Pages do not call these directly: internal/web/render.Page loads the
// ShellData (through app.Deps.Shell) and wraps the page component.
package layout

import (
	"net/url"
	"strings"

	"github.com/a-h/templ"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/seo"
)

// StoreSettings is what the shell shows from store_settings.
type StoreSettings struct {
	PixDiscountPercent         int
	FreeShippingThresholdCents int64
	AnnouncementText           string // "" builds the default announcement
	WhatsappNumber             string
}

// DefaultStoreSettings mirrors the schema defaults (5% Pix, R$ 299 frete).
func DefaultStoreSettings() StoreSettings {
	return StoreSettings{PixDiscountPercent: 5, FreeShippingThresholdCents: 29900}
}

// NavSet is one entry of the "Conjuntos" header menu (first 6 active sets).
type NavSet struct {
	Slug string
	Name string
}

// ShellData is everything the shells need. internal/web/storefront owns
// the loader that fills it (wired as app.Deps.Shell); Wave 0 renders
// placeholders.
type ShellData struct {
	User *auth.User
	// Impersonating: an admin is acting as User (banner + stop button).
	Impersonating bool
	// AvatarURL is the resolved (presigned) avatar of User, "" for initials.
	AvatarURL string
	CartCount int
	Settings  StoreSettings
	Sets      []NavSet
	// SidebarExpanded comes from the sidebar_state cookie (app shell).
	SidebarExpanded bool
	// CartSheet renders inside the cart sheet (storefront). nil renders an
	// empty container (#cart-sheet-body) for htmx to fill.
	CartSheet templ.Component
}

// Page is the root layout input.
type Page struct {
	Shell    ShellData
	Head     seo.Resolved
	Toasts   []toast.Toast
	Path     string
	AppShell bool
}

// IsAppShellPath reports paths that use the authenticated app chrome.
func IsAppShellPath(path string) bool {
	switch {
	case path == "/dashboard", path == "/accept-invitation", path == "/settings":
		return true
	case strings.HasPrefix(path, "/settings/"), strings.HasPrefix(path, "/conta"), strings.HasPrefix(path, "/admin"):
		return true
	}
	return false
}

func pathActive(pathname, to string) bool { return pathname == to || pathname == to+"/" }

func pathPrefix(pathname, prefix string) bool {
	return pathname == prefix || pathname == prefix+"/" || strings.HasPrefix(pathname, prefix+"/")
}

// Initials are the first letters of the first two words, uppercased.
func Initials(name string) string {
	var b strings.Builder
	n := 0
	for _, w := range strings.Fields(name) {
		if n == 2 {
			break
		}
		r := []rune(w)
		b.WriteRune(r[0])
		n++
	}
	return strings.ToUpper(b.String())
}

// WhatsappLink is https://wa.me/<number>?text=<text>.
func WhatsappLink(number, text string) string {
	return "https://wa.me/" + url.PathEscape(number) + "?text=" + url.QueryEscape(text)
}

func announcement(s StoreSettings) string {
	if s.AnnouncementText != "" {
		return s.AnnouncementText
	}
	var parts []string
	if s.PixDiscountPercent > 0 {
		parts = append(parts, itoa(s.PixDiscountPercent)+"% OFF NO PIX")
	}
	if s.FreeShippingThresholdCents > 0 {
		parts = append(parts, "FRETE GRÁTIS ACIMA DE "+priceStr(s.FreeShippingThresholdCents))
	} else {
		parts = append(parts, "FRETE GRÁTIS")
	}
	parts = append(parts, "ARACAJU, SE")
	return strings.Join(parts, " · ")
}

func pixText(s StoreSettings) string {
	if s.PixDiscountPercent > 0 {
		return "Pagamento via Pix com " + itoa(s.PixDiscountPercent) + "% de desconto"
	}
	return "Pagamento via Pix"
}

// htmxConfig is the <meta name="htmx-config"> payload: 5xx responses are
// not swapped (toast.js shows an error toast instead); 4xx are, so
// validation errors (422) re-render forms in place.
const htmxConfig = `{"noSwap":[204,304,"5xx"]}`
