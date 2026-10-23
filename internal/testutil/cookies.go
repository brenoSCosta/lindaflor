package testutil

import (
	"strings"
	"testing"
)

// FindSetCookie returns the Set-Cookie line for name, if present.
func FindSetCookie(lines []string, name string) (string, bool) {
	for _, l := range lines {
		if strings.HasPrefix(strings.TrimSpace(l), name+"=") {
			return l, true
		}
	}
	return "", false
}

// RedactSetCookie renders "<name>=***; <attrs>" for failure messages
// (cookie values are secrets: never print them raw).
func RedactSetCookie(line string) string {
	first, attrs, hasAttrs := strings.Cut(line, ";")
	name, _, _ := strings.Cut(first, "=")
	if hasAttrs {
		return strings.TrimSpace(name) + "=***;" + attrs
	}
	return strings.TrimSpace(name) + "=***"
}

func attrParts(line string) []string {
	parts := strings.Split(line, ";")
	if len(parts) == 0 {
		return nil
	}
	return parts[1:]
}

func hasFlag(line, flag string) bool {
	for _, a := range attrParts(line) {
		if strings.EqualFold(strings.TrimSpace(a), flag) {
			return true
		}
	}
	return false
}

func attrValue(line, attr string) (string, bool) {
	for _, a := range attrParts(line) {
		k, v, ok := strings.Cut(a, "=")
		if ok && strings.EqualFold(strings.TrimSpace(k), attr) {
			return strings.TrimSpace(v), true
		}
	}
	return "", false
}

// AssertHardenedCookie: Secure, HttpOnly, SameSite=Lax, Path=/, no Domain.
func AssertHardenedCookie(t testing.TB, line, name string) {
	t.Helper()
	r := RedactSetCookie(line)
	if !hasFlag(line, "Secure") {
		t.Errorf("%s must set Secure in production, got %s", name, r)
	}
	assertCommon(t, line, name)
	if _, ok := attrValue(line, "Domain"); ok {
		t.Errorf("%s must not set Domain (__Host- requirement), got %s", name, r)
	}
}

func assertCommon(t testing.TB, line, name string) {
	t.Helper()
	r := RedactSetCookie(line)
	if !hasFlag(line, "HttpOnly") {
		t.Errorf("%s must set HttpOnly, got %s", name, r)
	}
	if v, _ := attrValue(line, "SameSite"); !strings.EqualFold(v, "lax") {
		t.Errorf("%s must set SameSite=Lax, got %s", name, r)
	}
	if v, _ := attrValue(line, "Path"); v != "/" {
		t.Errorf("%s must set Path=/, got %s", name, r)
	}
}

func assertExpired(t testing.TB, line, name string) {
	t.Helper()
	v, _ := attrValue(line, "Max-Age")
	low := strings.ToLower(line)
	if v != "0" && !strings.Contains(low, "1970") {
		t.Errorf("%s clear must expire the cookie, got %s", name, RedactSetCookie(line))
	}
}

// AssertExpiredHardenedCookie: a clearing line that keeps the hardening.
func AssertExpiredHardenedCookie(t testing.TB, line, name string) {
	t.Helper()
	AssertHardenedCookie(t, line, name)
	assertExpired(t, line, name)
}

// AssertExpiredHardenedCookieDev: development clearing line — expiry,
// HttpOnly/SameSite=Lax/Path=/ parity and explicitly no Secure.
func AssertExpiredHardenedCookieDev(t testing.TB, line, name string) {
	t.Helper()
	if hasFlag(line, "Secure") {
		t.Errorf("%s must not set Secure in development, got %s", name, RedactSetCookie(line))
	}
	assertCommon(t, line, name)
	assertExpired(t, line, name)
}
