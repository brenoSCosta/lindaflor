// Package ui holds the templ pieces shared by the checkout/order pages and
// the account pages: Alpine-validated text fields, the order timeline
// (port of src/app/store/order_timeline.rs), the tracking line and status
// badges.
package ui

import (
	"encoding/json"
	"strings"

	"github.com/a-h/templ"

	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/components"
)

// TextField is one Alpine-validated input (port of the Topcoat
// `input(value: signal, touched:, error:)` fields). Model is the Alpine
// data key: x-model binds it, "<Model>Touched" flips on blur and the
// getter "<Model>Error" renders under the input. The server value lands
// in `value` (no-JS) and in the form's x-data (AlpineData).
type TextField struct {
	ID, Name, Type, Label, Placeholder, Autocomplete string
	Value                                            string
	Model                                            string
	Extra                                            templ.Attributes
	// Plain renders a field without Alpine bindings (no client error).
	Plain bool
}

func (f TextField) errorExpr() string { return f.Model + "Error" }

// InputAttrs are the <input> attributes.
func (f TextField) InputAttrs() templ.Attributes {
	a := templ.Attributes{"id": f.ID, "name": f.Name}
	if f.Type != "" {
		a["type"] = f.Type
	}
	if !f.Plain {
		a["aria-describedby"] = f.ID + "-error"
		a["x-model"] = f.Model
		a["@blur"] = f.Model + "Touched = true"
		a[":aria-invalid"] = f.errorExpr() + " ? 'true' : 'false'"
	}
	if f.Placeholder != "" {
		a["placeholder"] = f.Placeholder
	}
	if f.Autocomplete != "" {
		a["autocomplete"] = f.Autocomplete
	}
	if f.Value != "" {
		a["value"] = f.Value
	}
	for k, v := range f.Extra {
		a[k] = v
	}
	return a
}

// FieldAttrs bind data-invalid on the field wrapper.
func (f TextField) FieldAttrs() templ.Attributes {
	if f.Plain {
		return nil
	}
	return templ.Attributes{":data-invalid": f.errorExpr() + " ? 'true' : null"}
}

// ErrorAttrs bind the error text.
func (f TextField) ErrorAttrs() templ.Attributes {
	return templ.Attributes{"id": f.ID + "-error", "x-text": f.errorExpr()}
}

// AlpineData builds an x-data object literal: every (model, value) pair
// becomes `model: "<value>", modelTouched: false`, followed by the
// getters (error messages and `blocked`). Values are JSON-encoded, so
// the server-rendered input values survive x-model's initial sync.
func AlpineData(getters string, kv ...string) string {
	var b strings.Builder
	b.WriteString("{ ")
	for i := 0; i+1 < len(kv); i += 2 {
		v, _ := json.Marshal(kv[i+1])
		b.WriteString(kv[i])
		b.WriteString(": ")
		b.Write(v)
		b.WriteString(", ")
		b.WriteString(kv[i])
		b.WriteString("Touched: false, ")
	}
	b.WriteString(getters)
	b.WriteString(" }")
	return b.String()
}

// SubmitAttrs mark the submit button aria-disabled while the client
// validation blocks it (it stays submittable: the server decides).
func SubmitAttrs(extra ...string) templ.Attributes {
	a := templ.Attributes{"type": "submit", ":aria-disabled": "blocked ? 'true' : 'false'"}
	for i := 0; i+1 < len(extra); i += 2 {
		a[extra[i]] = extra[i+1]
	}
	return a
}

// BadgeVariant maps a store badge tone to the UI kit.
func BadgeVariant(t store.BadgeTone) components.BadgeVariant {
	switch t {
	case store.TonePrimary:
		return components.BadgePrimary
	case store.ToneDestructive:
		return components.BadgeDestructive
	case store.ToneOutline:
		return components.BadgeOutline
	}
	return components.BadgeSecondary
}

// StepDone is the timeline/progress-bar class of step i.
func stepBarClass(done bool) string {
	if done {
		return "h-1.5 w-5 rounded-full bg-primary"
	}
	return "h-1.5 w-5 rounded-full bg-border"
}

func stepLabelClass(done bool) string {
	if done {
		return "text-sm font-medium"
	}
	return "text-sm text-muted-foreground"
}
