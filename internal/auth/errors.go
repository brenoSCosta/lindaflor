package auth

import (
	"errors"
	"fmt"
	"net/http"
	"strings"
)

// Error is a client-facing auth failure: an HTTP status plus a short
// English message (the JSON API body; HTML pages map it to pt-BR with
// PortugueseMessage). Anything that is not an *Error is an internal error.
type Error struct {
	Status int
	Msg    string
}

func (e *Error) Error() string { return e.Msg }

// BadRequest is a 400 with msg.
func BadRequest(msg string) error { return &Error{Status: http.StatusBadRequest, Msg: msg} }

// Common errors.
var (
	ErrUnauthorized    = &Error{Status: http.StatusUnauthorized, Msg: "unauthorized"}
	ErrForbidden       = &Error{Status: http.StatusForbidden, Msg: "forbidden"}
	ErrTooManyRequests = &Error{Status: http.StatusTooManyRequests, Msg: "too many requests"}
)

// StatusOf is the HTTP status of err (500 for internal errors).
func StatusOf(err error) int {
	var ae *Error
	if errors.As(err, &ae) {
		return ae.Status
	}
	return http.StatusInternalServerError
}

// CodeOf is the machine-readable code of err for JSON bodies.
func CodeOf(err error) string {
	switch StatusOf(err) {
	case http.StatusBadRequest:
		return "BAD_REQUEST"
	case http.StatusUnauthorized:
		return "UNAUTHORIZED"
	case http.StatusForbidden:
		return "FORBIDDEN"
	case http.StatusTooManyRequests:
		return "TOO_MANY_REQUESTS"
	case http.StatusNotImplemented:
		return "NOT_IMPLEMENTED"
	default:
		return "INTERNAL_SERVER_ERROR"
	}
}

// IsClientError reports an *Error (as opposed to an internal failure).
func IsClientError(err error) bool {
	var ae *Error
	return errors.As(err, &ae)
}

// MinPasswordLen / MaxPasswordLen are the documented defaults (the live
// bounds come from PASSWORD_MIN_LEN / PASSWORD_MAX_LEN).
const (
	MinPasswordLen = 8
	MaxPasswordLen = 128
)

// PortugueseMessage maps an auth error to a short pt-BR UI message for the
// HTML pages (port of service::portuguese_error_message).
func PortugueseMessage(err error) string {
	if err == nil {
		return ""
	}
	msg := strings.ToLower(err.Error())
	if !IsClientError(err) {
		msg = "internal"
	}
	switch {
	case strings.Contains(msg, "already exists"):
		return "Já existe uma conta com este e-mail."
	case strings.Contains(msg, "at least"):
		return fmt.Sprintf("A senha deve ter pelo menos %d caracteres.", MinPasswordLen)
	case strings.Contains(msg, "at most"):
		return fmt.Sprintf("A senha deve ter no máximo %d caracteres.", MaxPasswordLen)
	case strings.Contains(msg, "too weak"):
		return "Escolha uma senha mais forte."
	case strings.Contains(msg, "invalid or expired"):
		return "Link inválido ou expirado."
	case strings.Contains(msg, "does not match"):
		return "O e-mail informado não confere."
	case strings.Contains(msg, "already in use"):
		return "Este e-mail já está em uso."
	case strings.Contains(msg, "must differ"):
		return "O novo e-mail deve ser diferente do atual."
	case strings.Contains(msg, "not configured"):
		return "Login com Google não está configurado."
	case strings.Contains(msg, "too many") || strings.Contains(msg, "429"):
		return "Muitas tentativas. Aguarde um momento e tente novamente."
	case strings.Contains(msg, "unauthorized") || strings.Contains(msg, "forbidden"):
		return "Credenciais inválidas."
	case strings.Contains(msg, "name") && strings.Contains(msg, "required"):
		return "Informe o nome."
	case strings.Contains(msg, "email") && strings.Contains(msg, "required"):
		return "Informe um e-mail válido."
	default:
		return "Não foi possível concluir a operação. Tente novamente."
	}
}
