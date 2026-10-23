package metrics

import (
	"slices"
	"strings"

	"github.com/google/uuid"
)

// Route labels for collapsed series.
const (
	AuthRouteLabel    = "/api/auth/*"
	UnknownRouteLabel = "other"
)

// knownTemplates are the route templates kept as labels (sorted; anything
// else collapses into UnknownRouteLabel so label cardinality stays bounded).
var knownTemplates = []string{
	"/",
	"/admin",
	"/admin/configuracoes",
	"/admin/cupons",
	"/admin/cupons/assign",
	"/admin/cupons/create",
	"/admin/cupons/delete",
	"/admin/cupons/toggle",
	"/admin/cupons/unassign",
	"/admin/estoque",
	"/admin/pedidos",
	"/admin/pedidos/{id}",
	"/admin/produtos",
	"/admin/produtos/create",
	"/admin/produtos/novo",
	"/admin/produtos/update",
	"/admin/produtos/{id}",
	"/admin/relatorios",
	"/admin/trocas",
	"/admin/trocas/{id}",
	"/admin/usuarios",
	"/admin/usuarios/atuar",
	"/admin/usuarios/banir",
	"/admin/usuarios/desbanir",
	"/admin/usuarios/nome",
	"/admin/usuarios/papel",
	"/admin/usuarios/remover",
	"/admin/usuarios/sessoes/revogar",
	"/admin/usuarios/sessoes/revogar-todas",
	"/api/health",
	"/check-email",
	"/check-email/resend",
	"/checkout",
	"/conta/enderecos",
	"/conta/enderecos/padrao",
	"/conta/enderecos/remover",
	"/conta/enderecos/salvar",
	"/conta/pedidos",
	"/dashboard",
	"/forgot-password",
	"/forgot-password/sent",
	"/login",
	"/login/google",
	"/logout",
	"/pedido",
	"/pedido/status-proc",
	"/pedido/{id}",
	"/pedido/{id}/cancelar",
	"/pedido/{id}/troca",
	"/politica-privacidade",
	"/produtos",
	"/produtos/{slug}",
	"/reset-password",
	"/robots.txt",
	"/sets",
	"/sets/{slug}",
	"/settings",
	"/settings/avatar",
	"/settings/two-factor",
	"/signup",
	"/sitemap.xml",
	"/stop-impersonating",
	"/termos",
	"/trocas-devolucoes",
	"/two-factor",
	"/verify-email",
	"/verify-email/resend",
}

// NormalizeRouteLabel maps a request path to a bounded metrics label:
// dynamic segments (UUIDs, numeric ids, product/set slugs) become {id} /
// {slug}, /api/auth/* collapses, assets and unknown paths become "other".
func NormalizeRouteLabel(path string) string {
	if path == "/" {
		return "/"
	}
	if path == "/api/auth" || strings.HasPrefix(path, "/api/auth/") {
		return AuthRouteLabel
	}
	// Crawler files look like assets but are real routes.
	if path == "/robots.txt" || path == "/sitemap.xml" {
		return path
	}
	if looksLikeAsset(path) {
		return UnknownRouteLabel
	}
	t := templatize(path)
	if _, ok := slices.BinarySearch(knownTemplates, t); ok {
		return t
	}
	return UnknownRouteLabel
}

func looksLikeAsset(path string) bool {
	last := path[strings.LastIndexByte(path, '/')+1:]
	dot := strings.LastIndexByte(last, '.')
	if dot < 0 {
		return false
	}
	ext := last[dot+1:]
	if ext == "" {
		return false
	}
	for i := 0; i < len(ext); i++ {
		c := ext[i]
		if (c < 'a' || c > 'z') && (c < 'A' || c > 'Z') && (c < '0' || c > '9') {
			return false
		}
	}
	return true
}

func templatize(path string) string {
	var b strings.Builder
	prev := ""
	for seg := range strings.SplitSeq(path, "/") {
		if seg == "" {
			continue
		}
		b.WriteByte('/')
		switch {
		case looksLikeUUID(seg) || isNumericID(seg):
			b.WriteString("{id}")
		case prev == "produtos" || prev == "sets":
			b.WriteString("{slug}")
		default:
			b.WriteString(seg)
		}
		prev = seg
	}
	if b.Len() == 0 {
		return "/"
	}
	return b.String()
}

func looksLikeUUID(seg string) bool {
	if _, err := uuid.Parse(seg); err == nil {
		return true
	}
	if len(seg) != 36 {
		return false
	}
	for i := 0; i < len(seg); i++ {
		c := seg[i]
		if i == 8 || i == 13 || i == 18 || i == 23 {
			if c != '-' {
				return false
			}
		} else if !strings.ContainsRune("0123456789abcdefABCDEF", rune(c)) {
			return false
		}
	}
	return true
}

func isNumericID(seg string) bool {
	if seg == "" || len(seg) > 20 {
		return false
	}
	for i := 0; i < len(seg); i++ {
		if seg[i] < '0' || seg[i] > '9' {
			return false
		}
	}
	return true
}
