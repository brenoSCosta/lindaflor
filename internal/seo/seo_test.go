package seo

import (
	"encoding/json"
	"strings"
	"testing"
	"unicode/utf8"
)

func TestPrivatePathsMatchWholeSegments(t *testing.T) {
	for _, p := range []string{"/admin", "/admin/produtos", "/conta/pedidos", "/checkout", "/pedido/0199c1a0", "/login", "/settings/two-factor"} {
		if !IsPrivatePath(p) {
			t.Errorf("%s must be private", p)
		}
	}
	for _, p := range []string{"/", "/produtos", "/contato", "/sets/verao", "/termos", "/loginx"} {
		if IsPrivatePath(p) {
			t.Errorf("%s must be public", p)
		}
	}
}

func TestJSONLDBodyCannotCloseTheScriptElement(t *testing.T) {
	doc := map[string]string{"name": "Biquíni </script><script>alert(1)</script> & <!-- \u2028"}
	body := JSONLDScriptBody(doc)
	if strings.ContainsAny(body, "<>&\u2028") || !strings.Contains(body, `\u003c/script\u003e`) {
		t.Fatal(body)
	}
	var back map[string]string
	if err := json.Unmarshal([]byte(body), &back); err != nil || back["name"] != doc["name"] {
		t.Fatal("not the same JSON")
	}
}

func TestDecimalPriceAndSummarize(t *testing.T) {
	for c, want := range map[int64]string{8990: "89.90", 5: "0.05", 100000: "1000.00", -150: "-1.50"} {
		if DecimalPrice(c) != want {
			t.Errorf("%d → %s", c, DecimalPrice(c))
		}
	}
	if Summarize("  a \n b  ", 10) != "a b" {
		t.Fatal("collapse")
	}
	out := Summarize(strings.Repeat("á", 200), 160)
	if utf8.RuneCountInString(out) != 160 || !strings.HasSuffix(out, "…") {
		t.Fatal("truncate")
	}
}

func TestResolveDefaultsAndNoIndex(t *testing.T) {
	r := Resolve(Head{}, false, false, "/sets", "https://loja.example.com")
	if r.Title != DefaultTitle || r.Robots != "" {
		t.Fatalf("%+v", r)
	}
	h := New("Biquíni").WithCanonical("/produtos/x").WithOpenGraph(OpenGraph{Image: "/img.png"})
	r = Resolve(h, true, false, "/produtos/x", "https://loja.example.com")
	if r.Canonical != "https://loja.example.com/produtos/x" || r.OG == nil || r.OG.Image != "https://loja.example.com/img.png" || r.OG.Kind != "website" {
		t.Fatalf("%+v", r)
	}
	r = Resolve(h, true, false, "/conta", "https://x")
	if r.Robots != "noindex, nofollow" || r.Canonical != "" || r.OG != nil {
		t.Fatalf("private: %+v", r)
	}
	r = Resolve(h, true, true, "/x", "https://x")
	if r.Title != "Página não encontrada | Linda Flor" || r.Robots == "" {
		t.Fatalf("failed: %+v", r)
	}
}
