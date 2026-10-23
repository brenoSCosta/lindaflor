package account

import (
	"strings"

	"github.com/a-h/templ"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/web/checkout/ui"
	"github.com/wellington-vell/lindaflor/internal/web/layout"
)

// Alpine client hints of the settings forms (Topcoat signal messages).
const (
	jsLen = `len(v) { return [...(v || '')].length; },`

	profileGetters = `get nameError() { if (!this.nameTouched) return ''; if (!this.name.trim()) return 'Informe o nome.'; return ''; },
  get blocked() { return !this.name.trim(); }`

	emailGetters = `get newEmailError() { if (!this.newEmailTouched) return ''; if (!this.newEmail.trim()) return 'Informe o e-mail.'; if (!this.newEmail.includes('@')) return 'E-mail inválido.'; return ''; },
  get blocked() { return !this.newEmail.trim() || !this.newEmail.includes('@'); }`

	passwordGetters = jsLen + `
  get currentPasswordError() { if (!this.currentPasswordTouched) return ''; if (!this.currentPassword) return 'Informe a senha atual.'; return ''; },
  get newPasswordError() { if (!this.newPasswordTouched) return ''; if (!this.newPassword) return 'Informe a nova senha.'; if (this.len(this.newPassword) < 8) return 'A senha deve ter pelo menos 8 caracteres.'; if (this.len(this.newPassword) > 128) return 'A senha deve ter no máximo 128 caracteres.'; return ''; },
  get confirmPasswordError() { if (!this.confirmPasswordTouched) return ''; if (!this.confirmPassword) return 'Confirme a nova senha.'; if (this.confirmPassword !== this.newPassword) return 'As senhas não conferem.'; return ''; },
  get blocked() { return !this.currentPassword || !this.newPassword || this.len(this.newPassword) < 8 || this.len(this.newPassword) > 128 || !this.confirmPassword || this.confirmPassword !== this.newPassword; }`

	codeGetters = `get codeError() { if (!this.codeTouched) return ''; if (!this.code.trim()) return 'Informe o código de 6 dígitos.'; if (this.code.trim().length !== 6) return 'O código deve ter 6 dígitos.'; return ''; },
  get blocked() { return this.code.trim().length !== 6; }`

	deleteGetters = `get confirmEmailError() { if (!this.confirmEmailTouched) return ''; if (!this.confirmEmail.trim()) return 'Informe o e-mail.'; if (!this.confirmEmail.includes('@')) return 'E-mail inválido.'; return ''; },
  get blocked() { return !this.confirmEmail.trim() || !this.confirmEmail.includes('@'); }`
)

func profileData(name string) string { return ui.AlpineData(profileGetters, "name", name) }

func emailData() string { return ui.AlpineData(emailGetters, "newEmail", "") }

func passwordData() string {
	return ui.AlpineData(passwordGetters, "currentPassword", "", "newPassword", "", "confirmPassword", "")
}

func codeData() string { return ui.AlpineData(codeGetters, "code", "") }

func deleteData() string { return ui.AlpineData(deleteGetters, "confirmEmail", "") }

// settingsForm: a no-JS POST /settings?tab=… form, htmx-enhanced (no swap:
// toast on X-Toast, HX-Location back to the tab) with a loading toast.
func settingsForm(tab, loading, xData string, extra ...string) templ.Attributes {
	action := tabURL(tab)
	a := templ.Attributes{"method": "post", "action": action, "hx-post": action, "hx-swap": "none"}
	if loading != "" {
		a["data-toast-loading"] = loading
	}
	if xData != "" {
		a["x-data"] = xData
		a["novalidate"] = true
	}
	for i := 0; i+1 < len(extra); i += 2 {
		a[extra[i]] = extra[i+1]
	}
	return a
}

// tabAttrs: the trigger keeps its ?tab= href (no-JS) and switches the
// Alpine tab in place.
func tabAttrs(tab string) templ.Attributes {
	return templ.Attributes{
		"x-on:click.prevent": "tab = '" + tab + "'",
		":aria-current":      "tab === '" + tab + "' ? 'page' : null",
	}
}

// panelAttrs hide inactive panels (server-side and via Alpine).
func panelAttrs(active, tab string) templ.Attributes {
	a := templ.Attributes{":hidden": "tab !== '" + tab + "'"}
	if active != tab {
		a["hidden"] = true
	}
	return a
}

func providerLabel(providerID string) string {
	switch providerID {
	case auth.CredentialProviderID:
		return "E-mail e senha"
	case auth.GoogleProviderID:
		return "Google"
	}
	return "Outro"
}

func initials(name string) string { return layout.Initials(name) }

func orDash(s *string, fallback string) string {
	if s == nil || strings.TrimSpace(*s) == "" {
		return fallback
	}
	return *s
}

func codeLabel(f auth.StepUpFactors) string {
	if f.Password {
		return "Ou código do autenticador"
	}
	return "Código do autenticador"
}

func accountSubtitle(a auth.LinkedAccount, email string) string {
	if a.ProviderID == auth.CredentialProviderID {
		return email
	}
	return a.AccountID
}
