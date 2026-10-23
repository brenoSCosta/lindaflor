package httpx

import "testing"

func TestSafeLocalRedirectAcceptsSameSitePaths(t *testing.T) {
	for raw, want := range map[string]string{
		"/":                            "/",
		"/conta":                       "/conta",
		"  /conta  ":                   "/conta",
		"/produtos?q=biquini&cor=azul": "/produtos?q=biquini&cor=azul",
		"/produtos?q=biquini%20azul":   "/produtos?q=biquini%20azul",
		"/favoritos#topo":              "/favoritos#topo",
		"/pedido/abc?token=x":          "/pedido/abc?token=x",
	} {
		if got, ok := SafeLocalRedirect(raw); !ok || got != want {
			t.Errorf("SafeLocalRedirect(%q) = %q, %v; want %q", raw, got, ok, want)
		}
	}
}

func TestSafeLocalRedirectRejectsOffSiteAndAmbiguousTargets(t *testing.T) {
	for _, raw := range []string{
		"", "   ", "conta", "//evil.com", "///evil.com", "/\\evil.com", "/\\/evil.com",
		"\\\\evil.com", "/\tevil.com", "/\t/evil.com", "/\n/evil.com", "/\r/evil.com",
		"/ /evil.com", "/con\x00ta", "/conta\\x", "%2F%2Fevil.com", "/%2Fevil.com",
		"/%5Cevil.com", "/%09/evil.com", "/%0d%0aSet-Cookie:x", "/%zz", "/%ff",
		"https://evil.com", "http://evil.com/conta", "javascript:alert(1)",
		"data:text/html,x", " //evil.com", "/%", "/%4",
	} {
		if got, ok := SafeLocalRedirect(raw); ok {
			t.Errorf("expected %q to be rejected, got %q", raw, got)
		}
	}
	if SafeLocalRedirectOr("//evil", "/x") != "/x" {
		t.Fatal("fallback")
	}
}
