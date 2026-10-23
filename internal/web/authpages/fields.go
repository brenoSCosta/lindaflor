package authpages

import "github.com/a-h/templ"

// textField is one Alpine-validated form field. Model is the Alpine data
// key (x-model); "<Model>Touched" flips on blur and "<Model>Error" is the
// getter rendered under the input.
type textField struct {
	ID, Name, Type, Label, Placeholder, Autocomplete string
	Value                                            string
	Model                                            string
	Extra                                            templ.Attributes
	InputClass                                       string
}

func (f textField) errorExpr() string { return f.Model + "Error" }

func (f textField) inputAttrs() templ.Attributes {
	a := templ.Attributes{
		"id": f.ID, "name": f.Name, "type": f.Type, "aria-describedby": f.ID + "-error",
		"x-ref": f.Model, "x-model": f.Model, "@blur": f.Model + "Touched = true",
		":aria-invalid": f.errorExpr() + " ? 'true' : 'false'",
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

func (f textField) fieldAttrs() templ.Attributes {
	return templ.Attributes{":data-invalid": f.errorExpr() + " ? 'true' : null"}
}

func (f textField) errorAttrs() templ.Attributes {
	return templ.Attributes{"id": f.ID + "-error", "x-text": f.errorExpr()}
}

// Alpine form state (client hints only; the server re-validates).
const (
	jsLen = `[...(this.password || '')].length`

	signinData = `{ email: '', emailTouched: false, password: '', passwordTouched: false,
  get emailError() { if (!this.emailTouched) return ''; if (!this.email.trim()) return 'Informe o e-mail.'; if (!this.email.includes('@')) return 'E-mail inválido.'; return ''; },
  get passwordError() { if (!this.passwordTouched) return ''; if (!this.password) return 'Informe a senha.'; if (` + jsLen + ` < 8) return 'A senha deve ter pelo menos 8 caracteres.'; if (` + jsLen + ` > 128) return 'A senha deve ter no máximo 128 caracteres.'; return ''; },
  get blocked() { return !this.email.trim() || !this.email.includes('@') || !this.password || ` + jsLen + ` < 8 || ` + jsLen + ` > 128; } }`

	signupData = `{ name: '', nameTouched: false, email: '', emailTouched: false, password: '', passwordTouched: false,
  get nameError() { if (!this.nameTouched) return ''; if (!this.name.trim()) return 'Informe o nome.'; return ''; },
  get emailError() { if (!this.emailTouched) return ''; if (!this.email.trim()) return 'Informe o e-mail.'; if (!this.email.includes('@')) return 'E-mail inválido.'; return ''; },
  get passwordError() { if (!this.passwordTouched) return ''; if (!this.password) return 'Informe a senha.'; if (` + jsLen + ` < 8) return 'A senha deve ter pelo menos 8 caracteres.'; if (` + jsLen + ` > 128) return 'A senha deve ter no máximo 128 caracteres.'; return ''; },
  get blocked() { return !this.name.trim() || !this.email.trim() || !this.email.includes('@') || !this.password || ` + jsLen + ` < 8 || ` + jsLen + ` > 128; } }`

	forgotData = `{ email: '', emailTouched: false,
  get emailError() { if (!this.emailTouched) return ''; if (!this.email.trim()) return 'Informe o e-mail.'; if (!this.email.includes('@')) return 'E-mail inválido.'; return ''; },
  get blocked() { return !this.email.trim() || !this.email.includes('@'); } }`

	resetData = `{ newPassword: '', newPasswordTouched: false, confirmPassword: '', confirmPasswordTouched: false,
  len(v) { return [...(v || '')].length; },
  get newPasswordError() { if (!this.newPasswordTouched) return ''; if (!this.newPassword) return 'Informe a nova senha.'; if (this.len(this.newPassword) < 8) return 'A senha deve ter pelo menos 8 caracteres.'; if (this.len(this.newPassword) > 128) return 'A senha deve ter no máximo 128 caracteres.'; return ''; },
  get confirmPasswordError() { if (!this.confirmPasswordTouched) return ''; if (!this.confirmPassword) return 'Confirme a nova senha.'; if (this.confirmPassword !== this.newPassword) return 'As senhas não conferem.'; return ''; },
  get blocked() { return !this.newPassword || this.len(this.newPassword) < 8 || this.len(this.newPassword) > 128 || !this.confirmPassword || this.confirmPassword !== this.newPassword; } }`

	totpData = `{ code: '', codeTouched: false,
  get codeError() { if (!this.codeTouched) return ''; if (!this.code.trim()) return 'Informe o código de 6 dígitos.'; if (this.code.trim().length !== 6) return 'O código deve ter 6 dígitos.'; return ''; },
  get blocked() { return this.code.trim().length !== 6; } }`

	backupData = `{ backupCode: '', backupCodeTouched: false,
  get backupCodeError() { if (!this.backupCodeTouched) return ''; if (!this.backupCode.trim()) return 'Informe o código de backup.'; return ''; },
  get blocked() { return !this.backupCode.trim(); } }`
)

// formAttrs is a no-JS POST form enhanced with htmx (toast + HX-Location
// redirect, no swap) and Alpine state.
func formAttrs(action, xData, xInit, loading string) templ.Attributes {
	a := templ.Attributes{
		"method": "post", "action": action, "novalidate": true,
		"hx-post": action, "hx-swap": "none", "x-data": xData,
	}
	if xInit != "" {
		a["x-init"] = xInit
	}
	if loading != "" {
		a["data-toast-loading"] = loading
	}
	return a
}

func submitAttrs() templ.Attributes {
	return templ.Attributes{"type": "submit", ":aria-disabled": "blocked ? 'true' : 'false'"}
}
