// Package links builds the absolute, signed links that customer mail
// carries (newsletter unsubscribe, back-in-stock cancel) and the order page
// link. It is a leaf package (no store/mail imports) so the mail worker can
// use UnsubscribeURL for List-Unsubscribe headers without an import cycle.
//
// Ports mail::absolute_url / encode_query_value and the signed e-mail token
// helpers of src/app/newsletter.rs.
package links

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"fmt"
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/config"
)

// DevOrigin is used when APP_ORIGIN is unset (development, tests).
const DevOrigin = "http://localhost:4200"

// Paths and HMAC domains of the one-click links.
const (
	UnsubscribePath   = "/newsletter/cancelar"
	UnsubscribeDomain = "newsletter-unsubscribe:v1:"
	StockCancelPath   = "/avise-me/cancelar"
	StockCancelDomain = "stock-alert-cancel:v1:"
)

// devKey keys the HMAC when TOKEN_PEPPER is unset (dev/test).
const devKey = "lindaflor-dev-newsletter-unsubscribe"

// AbsoluteURL is path (starting with "/") on APP_ORIGIN.
func AbsoluteURL(path string) string {
	origin := config.AppOrigin()
	if origin == "" {
		origin = DevOrigin
	}
	return origin + path
}

// EncodeQueryValue percent-encodes everything but A-Z a-z 0-9 - _ . ~.
func EncodeQueryValue(value string) string {
	var b strings.Builder
	for i := 0; i < len(value); i++ {
		c := value[i]
		if c >= 'a' && c <= 'z' || c >= 'A' && c <= 'Z' || c >= '0' && c <= '9' || c == '-' || c == '_' || c == '.' || c == '~' {
			b.WriteByte(c)
			continue
		}
		fmt.Fprintf(&b, "%%%02X", c)
	}
	return b.String()
}

// OrderURL is the absolute guest link of an order page.
func OrderURL(orderID, accessToken uuid.UUID) string {
	return AbsoluteURL(fmt.Sprintf("/pedido/%s?t=%s", orderID, EncodeQueryValue(accessToken.String())))
}

func emailMAC(domain, email, pepper string) []byte {
	key := pepper
	if key == "" {
		key = devKey
	}
	mac := hmac.New(sha256.New, []byte(key))
	mac.Write([]byte(domain))
	mac.Write([]byte(strings.ToLower(strings.TrimSpace(email))))
	return mac.Sum(nil)
}

// SignedEmailTokenWithPepper is the hex HMAC-SHA256 binding email
// (case-insensitive, trimmed) to the purpose domain.
func SignedEmailTokenWithPepper(domain, email, pepper string) string {
	return hex.EncodeToString(emailMAC(domain, email, pepper))
}

// SignedEmailToken uses TOKEN_PEPPER.
func SignedEmailToken(domain, email string) string {
	return SignedEmailTokenWithPepper(domain, email, config.TokenPepper())
}

// VerifySignedEmailTokenWithPepper checks a token in constant time.
func VerifySignedEmailTokenWithPepper(domain, email, token, pepper string) bool {
	raw, err := hex.DecodeString(strings.TrimSpace(token))
	if err != nil {
		return false
	}
	return hmac.Equal(raw, emailMAC(domain, email, pepper))
}

// VerifySignedEmailToken uses TOKEN_PEPPER.
func VerifySignedEmailToken(domain, email, token string) bool {
	return VerifySignedEmailTokenWithPepper(domain, email, token, config.TokenPepper())
}

func signedURL(path, domain, email string) string {
	email = strings.ToLower(strings.TrimSpace(email))
	token := SignedEmailToken(domain, email)
	return AbsoluteURL(fmt.Sprintf("%s?email=%s&token=%s", path, EncodeQueryValue(email), token))
}

// UnsubscribeURL is the absolute one-click newsletter unsubscribe link.
func UnsubscribeURL(email string) string {
	return signedURL(UnsubscribePath, UnsubscribeDomain, email)
}

// VerifyUnsubscribeToken checks a newsletter unsubscribe token.
func VerifyUnsubscribeToken(email, token string) bool {
	return VerifySignedEmailToken(UnsubscribeDomain, email, token)
}

// StockAlertCancelURL is the absolute signed "cancel my alerts" link.
func StockAlertCancelURL(email string) string {
	return signedURL(StockCancelPath, StockCancelDomain, email)
}

// VerifyStockAlertCancelToken checks a back-in-stock cancel token.
func VerifyStockAlertCancelToken(email, token string) bool {
	return VerifySignedEmailToken(StockCancelDomain, email, token)
}
