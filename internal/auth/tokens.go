package auth

import (
	"crypto/hmac"
	"crypto/rand"
	"crypto/sha256"
	"encoding/hex"
	"time"

	"github.com/wellington-vell/lindaflor/internal/config"
)

// One-time token identifiers (verifications.identifier prefixes) and TTLs.
const (
	ResetIdentifierPrefix       = "reset-password:"
	VerifyIdentifierPrefix      = "email-verification:"
	ChangeEmailIdentifierPrefix = "change-email:"
	DeleteIdentifierPrefix      = "delete-user:"
	Pending2FAPrefix            = "2fa-pending:"
	OAuthStatePrefix            = "oauth-google:"

	ResetTokenTTL       = time.Hour
	VerifyTokenTTL      = 24 * time.Hour
	ChangeEmailTokenTTL = 30 * time.Minute
	DeleteTokenTTL      = 30 * time.Minute
	Pending2FATTL       = 10 * time.Minute
	OAuthStateTTL       = 10 * time.Minute
)

// Mail landing pages for each one-time link.
const (
	VerifyEmailPath   = "/verify-email"
	ResetPasswordPath = "/reset-password"
	ChangeEmailPath   = "/confirmar-email"
	DeleteAccountPath = "/excluir-conta"
)

// RandomToken is 32 random bytes, hex encoded (one-time tokens, OAuth
// state, PKCE verifiers).
func RandomToken() string {
	b := make([]byte, 32)
	if _, err := rand.Read(b); err != nil {
		panic("crypto/rand failed: " + err.Error())
	}
	return hex.EncodeToString(b)
}

// VerificationTokenHash hashes a one-time token for storage/comparison:
// HMAC-SHA256 keyed with TOKEN_PEPPER, plain SHA-256 when no pepper is
// configured (dev/test). Raw tokens never touch the DB or logs.
func VerificationTokenHash(token string) string {
	return VerificationTokenHashWithPepper(token, config.TokenPepper())
}

// VerificationTokenHashWithPepper is the pure variant (explicit pepper).
func VerificationTokenHashWithPepper(token, pepper string) string {
	if pepper == "" {
		sum := sha256.Sum256([]byte(token))
		return hex.EncodeToString(sum[:])
	}
	mac := hmac.New(sha256.New, []byte(pepper))
	mac.Write([]byte(token))
	return hex.EncodeToString(mac.Sum(nil))
}

// VerificationTokenHashPrefix is the first 8 chars of a token hash — the
// only token-derived value allowed in logs.
func VerificationTokenHashPrefix(hash string) string {
	if len(hash) < 8 {
		return hash
	}
	return hash[:8]
}
