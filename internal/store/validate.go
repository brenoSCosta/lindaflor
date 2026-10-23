package store

import (
	"strings"
	"unicode"
	"unicode/utf8"
)

// Local ports of src/auth/validation.rs (validate_email / validate_name),
// used by checkout, newsletter and stock alerts (the store package does not
// import auth; the rules match auth.ValidateEmail / auth.ValidateName).

const (
	emailMax = 254
	nameMax  = 100
)

// ValidateEmail trims + ASCII-lowercases; max 254 chars, one '@',
// non-empty local/domain, no spaces or "..", dotted domain, conservative
// character sets. ok=false when invalid.
func ValidateEmail(raw string) (string, bool) {
	email := strings.ToLower(strings.TrimSpace(raw))
	if email == "" || utf8.RuneCountInString(email) > emailMax {
		return "", false
	}
	if strings.ContainsFunc(email, unicode.IsSpace) || strings.Contains(email, "..") {
		return "", false
	}
	local, domain, ok := strings.Cut(email, "@")
	if !ok || local == "" || domain == "" ||
		strings.HasPrefix(local, ".") || strings.HasSuffix(local, ".") ||
		strings.HasPrefix(domain, ".") || strings.HasSuffix(domain, ".") ||
		!strings.Contains(domain, ".") || strings.Count(email, "@") != 1 {
		return "", false
	}
	for _, c := range local {
		if !isASCIIAlnum(c) && c != '.' && c != '_' && c != '%' && c != '+' && c != '-' {
			return "", false
		}
	}
	for _, c := range domain {
		if !isASCIIAlnum(c) && c != '.' && c != '-' {
			return "", false
		}
	}
	return email, true
}

// ValidateName trims and requires 1..=100 characters.
func ValidateName(raw string) (string, bool) {
	name := strings.TrimSpace(raw)
	n := utf8.RuneCountInString(name)
	if n < 1 || n > nameMax {
		return "", false
	}
	return name, true
}

func isASCIIAlnum(c rune) bool {
	return c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9'
}

// truncateRunes keeps at most n runes.
func truncateRunes(s string, n int) string {
	if utf8.RuneCountInString(s) <= n {
		return s
	}
	return string([]rune(s)[:n])
}

func runeLen(s string) int { return utf8.RuneCountInString(s) }

func trimSpace(s string) string { return strings.TrimSpace(s) }

func isHex(c byte) bool {
	return c >= '0' && c <= '9' || c >= 'a' && c <= 'f' || c >= 'A' && c <= 'F'
}

func digitsOnly(raw string) string {
	var b strings.Builder
	for _, r := range raw {
		if r >= '0' && r <= '9' {
			b.WriteRune(r)
		}
	}
	return b.String()
}

func isBlank(s string) bool { return strings.TrimSpace(s) == "" }
