package mail

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"strings"

	"github.com/wellington-vell/lindaflor/internal/config"
)

// DevOrigin is the origin of absolute links when APP_ORIGIN is unset
// (dev/test only — production boot requires APP_ORIGIN).
const DevOrigin = "http://localhost:4200"

// Landing pages of the links mailed to customers.
const (
	VerifyEmailPath      = "/verify-email"
	ResetPasswordPath    = "/reset-password"
	ChangeEmailPath      = "/confirmar-email"
	DeleteAccountPath    = "/excluir-conta"
	UnsubscribePath      = "/newsletter/cancelar"
	StockAlertCancelPath = "/avise-me/cancelar"
)

// Signed-link purposes. Each domain separates one link kind so a token
// minted for one purpose never verifies for another.
const (
	UnsubscribeDomain      = "newsletter-unsubscribe:v1:"
	StockAlertCancelDomain = "stock-alert-cancel:v1:"
)

// devSignedLinkKey keys the signed links when TOKEN_PEPPER is unset.
const devSignedLinkKey = "lindaflor-dev-newsletter-unsubscribe"

// Links builds absolute, signed links for mail bodies and their landing
// pages. The zero value works in development (DevOrigin, dev key).
type Links struct {
	Origin string // APP_ORIGIN; "" → DevOrigin
	Pepper string // TOKEN_PEPPER; "" → dev key
}

// LinksFromConfig uses cfg.AppOrigin and cfg.TokenPepper.
func LinksFromConfig(cfg *config.Config) Links {
	if cfg == nil {
		return Links{}
	}
	return Links{Origin: cfg.AppOrigin, Pepper: cfg.TokenPepper}
}

// AbsoluteURL joins the origin and path (which must start with "/").
func (l Links) AbsoluteURL(path string) string {
	origin := l.Origin
	if origin == "" {
		origin = DevOrigin
	}
	return origin + path
}

// TokenURL is "<origin><path>?token=<token>".
func (l Links) TokenURL(path, token string) string {
	return l.AbsoluteURL(path + "?token=" + EncodeQueryValue(token))
}

func normalizeEmail(email string) string { return strings.ToLower(strings.TrimSpace(email)) }

func (l Links) mac(domain, email string) []byte {
	key := l.Pepper
	if key == "" {
		key = devSignedLinkKey
	}
	m := hmac.New(sha256.New, []byte(key))
	m.Write([]byte(domain))
	m.Write([]byte(normalizeEmail(email)))
	return m.Sum(nil)
}

// SignedEmailToken is a non-expiring hex HMAC-SHA256 binding email
// (case-insensitive, trimmed) to domain. Rotating TOKEN_PEPPER
// invalidates outstanding links.
func (l Links) SignedEmailToken(domain, email string) string {
	return hex.EncodeToString(l.mac(domain, email))
}

// VerifySignedEmailToken checks a SignedEmailToken in constant time.
func (l Links) VerifySignedEmailToken(domain, email, token string) bool {
	raw, err := hex.DecodeString(strings.TrimSpace(token))
	if err != nil {
		return false
	}
	return hmac.Equal(raw, l.mac(domain, email))
}

// UnsubscribeToken is the newsletter unsubscribe token for email.
func (l Links) UnsubscribeToken(email string) string {
	return l.SignedEmailToken(UnsubscribeDomain, email)
}

// VerifyUnsubscribeToken checks a newsletter unsubscribe token.
func (l Links) VerifyUnsubscribeToken(email, token string) bool {
	return l.VerifySignedEmailToken(UnsubscribeDomain, email, token)
}

func (l Links) signedEmailURL(path, domain, email string) string {
	email = normalizeEmail(email)
	return l.AbsoluteURL(path + "?email=" + EncodeQueryValue(email) + "&token=" + l.SignedEmailToken(domain, email))
}

// UnsubscribeURL is the absolute one-click newsletter unsubscribe link.
func (l Links) UnsubscribeURL(email string) string {
	return l.signedEmailURL(UnsubscribePath, UnsubscribeDomain, email)
}

// StockAlertCancelURL is the absolute link cancelling every pending
// back-in-stock alert of email.
func (l Links) StockAlertCancelURL(email string) string {
	return l.signedEmailURL(StockAlertCancelPath, StockAlertCancelDomain, email)
}

// VerifyStockAlertCancelToken checks a StockAlertCancelURL token.
func (l Links) VerifyStockAlertCancelToken(email, token string) bool {
	return l.VerifySignedEmailToken(StockAlertCancelDomain, email, token)
}
