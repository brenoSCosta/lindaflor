package components

import (
	"embed"
	"fmt"
	"html"
	"regexp"
	"strings"
	"sync"

	"github.com/a-h/templ"
)

// Lucide icons (lucide-static 0.469.0, ISC), embedded as SVG files.
// Add an icon: drop icons/<name>.svg from
// https://cdn.jsdelivr.net/npm/lucide-static@0.469.0/icons/<name>.svg
//
//go:embed icons/*.svg
var iconFS embed.FS

var (
	iconOnce   sync.Once
	iconBodies map[string]string
	svgInner   = regexp.MustCompile(`(?s)<svg[^>]*>(.*)</svg>`)
)

func loadIcons() {
	iconBodies = map[string]string{}
	entries, err := iconFS.ReadDir("icons")
	if err != nil {
		panic(err)
	}
	for _, e := range entries {
		b, err := iconFS.ReadFile("icons/" + e.Name())
		if err != nil {
			panic(err)
		}
		m := svgInner.FindSubmatch(b)
		if m == nil {
			panic("icon without <svg>: " + e.Name())
		}
		body := strings.Join(strings.Fields(string(m[1])), " ")
		iconBodies[strings.TrimSuffix(e.Name(), ".svg")] = body
	}
}

// IconSVG renders a lucide icon as inline SVG markup. Unknown names panic
// in tests (see icons_test.go) and render an empty box otherwise.
func IconSVG(name, class string) string {
	iconOnce.Do(loadIcons)
	body := iconBodies[name]
	cls := "lucide lucide-" + name
	if class != "" {
		cls += " " + class
	}
	return fmt.Sprintf(`<svg xmlns="http://www.w3.org/2000/svg" width="1em" height="1em" viewBox="0 0 24 24" fill="none" stroke="currentColor" stroke-width="2" stroke-linecap="round" stroke-linejoin="round" class="%s" aria-hidden="true">%s</svg>`,
		html.EscapeString(cls), body)
}

// HasIcon reports whether name is embedded.
func HasIcon(name string) bool {
	iconOnce.Do(loadIcons)
	_, ok := iconBodies[name]
	return ok
}

// Icon is a lucide icon component: @components.Icon("heart", "size-4").
func Icon(name, class string) templ.Component {
	return templ.Raw(IconSVG(name, class))
}

// IconDataURL is a CSS url() value of an icon (used as a mask image).
func IconDataURL(name string) string {
	iconOnce.Do(loadIcons)
	svg := `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 24 24" fill="none" stroke="black" stroke-width="2" stroke-linecap="round" stroke-linejoin="round">` + iconBodies[name] + `</svg>`
	r := strings.NewReplacer("%", "%25", `"`, "%22", "#", "%23")
	return `url("data:image/svg+xml,` + r.Replace(svg) + `")`
}
