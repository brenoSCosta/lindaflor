package httpx

import (
	"strings"
	"unicode"
	"unicode/utf8"
)

// SafeLocalRedirect returns raw (trimmed) when it is a same-site absolute
// path, else ok=false. Every user-controlled redirect target (next,
// redirect, callback_url, same-origin Referer paths, admin banner links)
// goes through it.
//
// Browsers treat `\` as `/` and drop TAB/CR/LF, so `/\evil.com` and
// `/\t/evil.com` resolve to `//evil.com`. Rules: a single leading `/` (the
// second byte is neither `/` nor `\`); no whitespace; no `\` or control
// characters, checked as given and after one round of percent-decoding.
func SafeLocalRedirect(raw string) (string, bool) {
	path := strings.TrimFunc(raw, unicode.IsSpace)
	if !isLocalPath(path) || strings.ContainsFunc(path, unicode.IsSpace) {
		return "", false
	}
	decoded, ok := percentDecodeStrict(path)
	if !ok || !isLocalPath(decoded) {
		return "", false
	}
	return path, true
}

// SafeLocalRedirectOr is SafeLocalRedirect with a fallback.
func SafeLocalRedirectOr(raw, fallback string) string {
	if p, ok := SafeLocalRedirect(raw); ok {
		return p
	}
	return fallback
}

func isLocalPath(p string) bool {
	if len(p) == 0 || p[0] != '/' {
		return false
	}
	if len(p) > 1 && (p[1] == '/' || p[1] == '\\') {
		return false
	}
	return !strings.ContainsFunc(p, func(r rune) bool { return r == '\\' || unicode.IsControl(r) })
}

// percentDecodeStrict decodes %XX once; malformed escapes or non-UTF-8
// results fail.
func percentDecodeStrict(s string) (string, bool) {
	out := make([]byte, 0, len(s))
	for i := 0; i < len(s); i++ {
		if s[i] != '%' {
			out = append(out, s[i])
			continue
		}
		if i+2 >= len(s) {
			return "", false
		}
		hi, ok1 := unhex(s[i+1])
		lo, ok2 := unhex(s[i+2])
		if !ok1 || !ok2 {
			return "", false
		}
		out = append(out, hi<<4|lo)
		i += 2
	}
	if !utf8.Valid(out) {
		return "", false
	}
	return string(out), true
}

func unhex(c byte) (byte, bool) {
	switch {
	case '0' <= c && c <= '9':
		return c - '0', true
	case 'a' <= c && c <= 'f':
		return c - 'a' + 10, true
	case 'A' <= c && c <= 'F':
		return c - 'A' + 10, true
	}
	return 0, false
}
