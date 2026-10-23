package checkout

import (
	"net/url"
	"strconv"
	"strings"

	"github.com/a-h/templ"
	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/payments/pix"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/checkout/ui"
	"github.com/wellington-vell/lindaflor/internal/web/components"
	"github.com/wellington-vell/lindaflor/internal/web/format"
)

// checkoutGetters are the Alpine client hints of the checkout form (same
// messages as the Topcoat signals; the server re-validates everything).
const checkoutGetters = `digits(v) { return String(v || '').replace(/\D/g, '').length; },
  get emailError() { if (!this.emailTouched) return ''; if (!this.email.trim()) return 'Informe o e-mail.'; if (!this.email.includes('@')) return 'E-mail inválido.'; return ''; },
  get nameError() { if (!this.nameTouched) return ''; if (!this.name.trim()) return 'Informe o nome completo.'; return ''; },
  get streetError() { if (!this.streetTouched) return ''; if (!this.street.trim()) return 'Informe a rua.'; return ''; },
  get numberError() { if (!this.numberTouched) return ''; if (!this.number.trim()) return 'Informe o número.'; return ''; },
  get neighborhoodError() { if (!this.neighborhoodTouched) return ''; if (!this.neighborhood.trim()) return 'Informe o bairro.'; return ''; },
  get cityError() { if (!this.cityTouched) return ''; if (!this.city.trim()) return 'Informe a cidade.'; return ''; },
  get zipError() { if (!this.zipTouched) return ''; if (!this.zip.trim()) return 'Informe o CEP.'; if (this.digits(this.zip) !== 8) return 'Informe um CEP válido com 8 dígitos.'; return ''; },
  get phoneError() { if (!this.phoneTouched) return ''; const n = this.digits(this.phone); if (n < 10 || n > 11) return 'Informe um WhatsApp válido (DDD + número).'; return ''; },
  get blocked() { const p = this.digits(this.phone); return !this.pixAvailable || !this.email.trim() || !this.email.includes('@') || !this.name.trim() || !this.street.trim() || !this.number.trim() || !this.neighborhood.trim() || !this.city.trim() || this.digits(this.zip) !== 8 || p < 10 || p > 11; }`

func (v checkoutView) xData() string {
	return ui.AlpineData("pixAvailable: "+strconv.FormatBool(v.Sum.PixAvailable)+", "+checkoutGetters,
		"email", v.Email, "name", v.Name, "phone", v.Phone, "street", v.Street, "number", v.Number,
		"neighborhood", v.Neighborhood, "city", v.City, "zip", v.Zip)
}

func (v checkoutView) fields() map[string]ui.TextField {
	return map[string]ui.TextField{
		"email": {
			ID: "email", Name: "guest_email", Type: "email", Label: "E-mail", Autocomplete: "email",
			Value: v.Email, Model: "email", Extra: templ.Attributes{"maxlength": "254"},
		},
		"name": {
			ID: "name", Name: "name", Type: "text", Label: "Nome completo", Autocomplete: "name",
			Value: v.Name, Model: "name", Extra: templ.Attributes{"maxlength": "100"},
		},
		"phone": {
			ID: "phone", Name: "phone", Label: "WhatsApp", Autocomplete: "tel", Placeholder: "79999816511",
			Value: v.Phone, Model: "phone", Extra: templ.Attributes{"maxlength": "20", "inputmode": "tel"},
		},
		"street": {
			ID: "street", Name: "street", Label: "Rua", Value: v.Street, Model: "street",
			Extra: templ.Attributes{"maxlength": "120"},
		},
		"number": {
			ID: "number", Name: "number", Label: "Número", Value: v.Number, Model: "number",
			Extra: templ.Attributes{"maxlength": "20"},
		},
		"complement": {
			ID: "complement", Name: "complement", Label: "Complemento", Value: v.Complement, Plain: true,
			Extra: templ.Attributes{"maxlength": "80"},
		},
		"neighborhood": {
			ID: "neighborhood", Name: "neighborhood", Label: "Bairro", Value: v.Neighborhood,
			Model: "neighborhood", Extra: templ.Attributes{"maxlength": "80"},
		},
		"city": {
			ID: "city", Name: "city", Label: "Cidade", Value: v.City, Model: "city",
			Extra: templ.Attributes{"maxlength": "80"},
		},
		"zip": {ID: "zip", Name: "zip_code", Label: "CEP", Placeholder: "49000-000", Value: v.Zip, Model: "zip"},
	}
}

func (v checkoutView) savedVariant(id uuid.UUID) components.ButtonVariant {
	if v.Sum.SelectedAddressID != nil && *v.Sum.SelectedAddressID == id {
		return components.ButtonPrimary
	}
	return components.ButtonOutline
}

func savedAddressHref(a store.SavedAddress) templ.SafeURL {
	return templ.SafeURL("/checkout?endereco=" + a.ID.String())
}

func price(c int64) string { return format.Price(c) }

func itoa(n int) string { return strconv.Itoa(n) }

func pixUnavailableMessage() string { return pix.UnavailableMessage }

func saveAddressAttrs(checked bool) templ.Attributes {
	a := templ.Attributes{"id": "save_address", "name": "save_address", "value": "1"}
	if checked {
		a["checked"] = true
	}
	return a
}

// blocked is the server-rendered twin of the Alpine `blocked` getter.
func (v checkoutView) blocked() bool {
	p := store.PhoneDigitCount(v.Phone)
	return !v.Sum.PixAvailable || blank(v.Email) || !strings.Contains(v.Email, "@") || blank(v.Name) ||
		blank(v.Street) || blank(v.Number) || blank(v.Neighborhood) || blank(v.City) ||
		len(store.CEPDigits(v.Zip)) != 8 || p < 10 || p > 11
}

func blank(s string) bool { return strings.TrimSpace(s) == "" }

func urlQueryEscape(s string) string { return url.QueryEscape(s) }
