package components

import (
	"context"
	"os"
	"path/filepath"
	"regexp"
	"strings"
	"testing"

	"github.com/a-h/templ"

	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
)

func render(t *testing.T, c templ.Component) string {
	t.Helper()
	var b strings.Builder
	if err := c.Render(context.Background(), &b); err != nil {
		t.Fatal(err)
	}
	return b.String()
}

func TestButtonKeepsArbitraryVariantClasses(t *testing.T) {
	html := render(t, Button(ButtonProps{Variant: ButtonGhost, Class: "[&>svg]:size-4 md:group-data-[collapsible=icon]/sidebar:hidden", Attrs: Attr("type", "submit")}))
	for _, want := range []string{`[&amp;&gt;svg]:size-4`, `md:group-data-[collapsible=icon]/sidebar:hidden`, `type="submit"`, `aria-disabled="false"`, "hover:bg-foreground/5"} {
		if !strings.Contains(html, want) {
			t.Fatalf("missing %q in %s", want, html)
		}
	}
}

func TestEveryReferencedIconIsEmbedded(t *testing.T) {
	re := regexp.MustCompile(`Icon\("([a-z0-9-]+)"`)
	files, _ := filepath.Glob("../*/*.templ")
	more, _ := filepath.Glob("../*/*/*.templ")
	files = append(files, more...)
	seen := 0
	for _, f := range files {
		b, err := os.ReadFile(f)
		if err != nil {
			t.Fatal(err)
		}
		for _, m := range re.FindAllStringSubmatch(string(b), -1) {
			seen++
			if !HasIcon(m[1]) {
				t.Errorf("%s: icon %q is not embedded (add icons/%s.svg)", f, m[1], m[1])
			}
		}
	}
	if seen == 0 {
		t.Fatal("no icon references found")
	}
}

func TestToasterRendersQueuedToasts(t *testing.T) {
	html := render(t, Toaster([]toast.Toast{toast.Success("Salvo").WithDescription("ok"), toast.Error("Falhou")}, DefaultToasterOptions()))
	for _, want := range []string{`id="toaster"`, `hx-preserve="true"`, `data-type="error"`, `data-front="true"`, "Salvo", "Falhou", `data-toast-template`} {
		if !strings.Contains(html, want) {
			t.Fatalf("missing %q", want)
		}
	}
	m := PresentToasts([]toast.Toast{toast.Info("a"), toast.Info("b"), toast.Info("c"), toast.Info("d")}, DefaultToasterOptions())
	if m[0].Title != "d" || !m[0].Front || !m[3].Hidden || m[1].Style != "--offset:-14px;--scale:0.950;--z:3;" {
		t.Fatalf("%+v", m)
	}
}

func TestDialogAlpineBindings(t *testing.T) {
	html := render(t, Dialog(OverlayProps{XOpen: "open", OnClose: "open = false", Attrs: Attr("aria-label", "x")}))
	for _, want := range []string{`x-bind:open="open"`, `x-on:keydown.escape.window="open = false"`, `x-on:click.self="open = false"`, `aria-label="x"`} {
		if !strings.Contains(html, want) {
			t.Fatalf("missing %q in %s", want, html)
		}
	}
}

func TestSelectCheckmarkStyleIsNotSanitizedAway(t *testing.T) {
	html := render(t, Select(InputProps{Attrs: Attr("name", "size")}))
	if !strings.Contains(html, "--select-checkmark: url(") || strings.Contains(html, "ZgotmplZ") {
		t.Fatalf("style lost: %s", html)
	}
}
