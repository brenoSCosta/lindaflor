package auth

import (
	"fmt"
	"strings"
	"unicode"
	"unicode/utf8"
)

const (
	nameMax  = 100
	emailMax = 254
)

// Validation errors (wording of src/auth/validation.rs).
var (
	ErrNameRequired  = BadRequest("name is required")
	ErrNameTooLong   = BadRequest(fmt.Sprintf("name must be at most %d characters", nameMax))
	ErrEmailInvalid  = BadRequest("valid email is required")
	ErrEmailTooLong  = BadRequest(fmt.Sprintf("email must be at most %d characters", emailMax))
	ErrEmailRequired = BadRequest("email is required")
)

// ValidateName trims and checks 1..=100 Unicode characters.
func ValidateName(raw string) (string, error) {
	name := strings.TrimSpace(raw)
	n := utf8.RuneCountInString(name)
	if n < 1 {
		return "", ErrNameRequired
	}
	if n > nameMax {
		return "", ErrNameTooLong
	}
	return name, nil
}

// NormalizeEmail trims and ASCII-lowercases.
func NormalizeEmail(email string) string { return asciiLower(strings.TrimSpace(email)) }

func asciiLower(s string) string {
	return strings.Map(func(r rune) rune {
		if r >= 'A' && r <= 'Z' {
			return r + ('a' - 'A')
		}
		return r
	}, s)
}

func isASCIIAlnum(r rune) bool {
	return (r >= 'a' && r <= 'z') || (r >= 'A' && r <= 'Z') || (r >= '0' && r <= '9')
}

// ValidateEmail trims + ASCII-lowercases and checks: max 254 chars, one
// '@', non-empty local/domain, no whitespace or "..", no leading/trailing
// dots, a dot in the domain, conservative character sets.
func ValidateEmail(raw string) (string, error) {
	email := NormalizeEmail(raw)
	if email == "" {
		return "", ErrEmailInvalid
	}
	if utf8.RuneCountInString(email) > emailMax {
		return "", ErrEmailTooLong
	}
	if strings.ContainsFunc(email, unicode.IsSpace) || strings.Contains(email, "..") {
		return "", ErrEmailInvalid
	}
	local, domain, ok := strings.Cut(email, "@")
	if !ok || local == "" || domain == "" ||
		strings.HasPrefix(local, ".") || strings.HasSuffix(local, ".") ||
		strings.HasPrefix(domain, ".") || strings.HasSuffix(domain, ".") ||
		!strings.Contains(domain, ".") || strings.Count(email, "@") != 1 {
		return "", ErrEmailInvalid
	}
	for _, c := range local {
		if !isASCIIAlnum(c) && !strings.ContainsRune("._%+-", c) {
			return "", ErrEmailInvalid
		}
	}
	for _, c := range domain {
		if !isASCIIAlnum(c) && c != '.' && c != '-' {
			return "", ErrEmailInvalid
		}
	}
	return email, nil
}
