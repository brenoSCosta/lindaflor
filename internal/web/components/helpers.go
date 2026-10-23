// Package components is the templ UI kit (port of src/components/*.rs):
// the same Tailwind classes and variants, with Alpine.js for client state.
//
// Conventions:
//   - Every component takes a Props struct; zero values are the defaults
//     (e.g. ButtonProps{} is a primary, medium button).
//   - Props.Class is appended to the component's classes; Props.Attrs is
//     spread onto the main element (templ.Attributes), e.g.
//     templ.Attributes{"type": "submit", "hx-post": "/carrinho"}.
//   - Children go in the component body: @components.Button(p) { Salvar }.
//   - Use the *Variants helpers to style other elements (a link as a button).
package components

import (
	"strconv"
	"strings"

	"github.com/a-h/templ"
)

// Cn joins non-empty class lists with single spaces.
func Cn(classes ...string) string {
	var b strings.Builder
	for _, c := range classes {
		for f := range strings.FieldsSeq(c) {
			if b.Len() > 0 {
				b.WriteByte(' ')
			}
			b.WriteString(f)
		}
	}
	return b.String()
}

// If returns s when cond holds, else "".
func If(cond bool, s string) string {
	if cond {
		return s
	}
	return ""
}

// boolStr renders "true"/"false" for aria-* and data-* attributes.
func boolStr(b bool) string {
	if b {
		return "true"
	}
	return "false"
}

// attrs returns a non-nil attribute map.
func attrs(a templ.Attributes) templ.Attributes {
	if a == nil {
		return templ.Attributes{}
	}
	return a
}

// Attr builds templ.Attributes from key/value pairs:
// components.Attr("type", "submit", "name", "intent").
func Attr(kv ...string) templ.Attributes {
	a := templ.Attributes{}
	for i := 0; i+1 < len(kv); i += 2 {
		a[kv[i]] = kv[i+1]
	}
	return a
}

// withAttr returns a copy of a with k=v set.
func withAttr(a templ.Attributes, k string, v any) templ.Attributes {
	out := templ.Attributes{}
	for key, val := range a {
		out[key] = val
	}
	out[k] = v
	return out
}

func itoa(n int) string { return strconv.Itoa(n) }

func labelOr(label, def string) string {
	if label == "" {
		return def
	}
	return label
}

func paginationLinkClass(active bool, extra string) string {
	v := ButtonGhost
	if active {
		v = ButtonOutline
	}
	return ButtonVariants(v, ButtonIcon, extra)
}

func ftoa(f float64) string { return strconv.FormatFloat(f, 'f', -1, 64) }

func maxOr(v, def float64) float64 {
	if v <= 0 {
		return def
	}
	return v
}

func (s SidebarSide) sheet() SheetSide {
	if s == SidebarRight {
		return SheetRight
	}
	return SheetLeft
}
