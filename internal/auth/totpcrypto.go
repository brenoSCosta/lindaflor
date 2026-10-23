package auth

import (
	"crypto/rand"
	"crypto/subtle"
	"errors"
	"fmt"
	"unicode/utf8"

	"golang.org/x/crypto/chacha20poly1305"

	"github.com/wellington-vell/lindaflor/internal/config"
)

// TOTPSecretKeyVersion is the current two_factor.secret_key_version. Bump
// when rotating TOTP_ENCRYPTION_KEY.
const TOTPSecretKeyVersion int16 = 1

// TOTPNonceLen is the XChaCha20 nonce size.
const TOTPNonceLen = chacha20poly1305.NonceSizeX

// TOTP crypto errors. None carries secret material.
var (
	ErrTOTPDecryptFailed = errors.New("totp secret authentication failed")
	ErrTOTPBadNonce      = fmt.Errorf("totp secret nonce must be %d bytes", TOTPNonceLen)
	ErrTOTPInvalidUTF8   = errors.New("decrypted totp secret is not valid UTF-8")
)

// UnsupportedKeyVersionError reports a ciphertext from another key version.
type UnsupportedKeyVersionError struct{ Version int16 }

func (e *UnsupportedKeyVersionError) Error() string {
	return fmt.Sprintf("unsupported totp secret key version %d (expected %d)", e.Version, TOTPSecretKeyVersion)
}

// EncryptedTOTP is a freshly encrypted secret ready for INSERT. String/
// GoString never print bytes.
type EncryptedTOTP struct {
	Nonce      []byte
	Ciphertext []byte
	KeyVersion int16
}

func (e EncryptedTOTP) String() string {
	return fmt.Sprintf("EncryptedTOTP{nonce_len:%d ciphertext_len:%d key_version:%d}", len(e.Nonce), len(e.Ciphertext), e.KeyVersion)
}

// GoString keeps %#v redacted.
func (e EncryptedTOTP) GoString() string { return e.String() }

// EncryptTOTPSecret encrypts a base32 secret with TOTP_ENCRYPTION_KEY and a
// fresh random nonce (XChaCha20-Poly1305).
func EncryptTOTPSecret(plaintextB32 string) (EncryptedTOTP, error) {
	key, err := config.TotpEncryptionKey()
	if err != nil {
		return EncryptedTOTP{}, fmt.Errorf("totp encryption key is not configured: %w", err)
	}
	return encryptWithKey(plaintextB32, key[:])
}

// DecryptTOTPSecret decrypts a two_factor row. Fails hard on wrong key,
// tampered nonce/tag or version mismatch — no plaintext fallback.
func DecryptTOTPSecret(nonce, ciphertext []byte, version int16) (string, error) {
	key, err := config.TotpEncryptionKey()
	if err != nil {
		return "", fmt.Errorf("totp encryption key is not configured: %w", err)
	}
	return decryptWithKey(nonce, ciphertext, version, key[:])
}

func encryptWithKey(plaintext string, key []byte) (EncryptedTOTP, error) {
	aead, err := chacha20poly1305.NewX(key)
	if err != nil {
		return EncryptedTOTP{}, fmt.Errorf("totp aead: %w", err)
	}
	nonce := make([]byte, TOTPNonceLen)
	if _, err := rand.Read(nonce); err != nil {
		return EncryptedTOTP{}, fmt.Errorf("totp nonce: %w", err)
	}
	return EncryptedTOTP{
		Nonce:      nonce,
		Ciphertext: aead.Seal(nil, nonce, []byte(plaintext), nil),
		KeyVersion: TOTPSecretKeyVersion,
	}, nil
}

func decryptWithKey(nonce, ciphertext []byte, version int16, key []byte) (string, error) {
	if version != TOTPSecretKeyVersion {
		return "", &UnsupportedKeyVersionError{Version: version}
	}
	if len(nonce) != TOTPNonceLen {
		return "", ErrTOTPBadNonce
	}
	aead, err := chacha20poly1305.NewX(key)
	if err != nil {
		return "", fmt.Errorf("totp aead: %w", err)
	}
	plain, err := aead.Open(nil, nonce, ciphertext, nil)
	if err != nil {
		return "", ErrTOTPDecryptFailed
	}
	if !utf8.Valid(plain) {
		return "", ErrTOTPInvalidUTF8
	}
	return string(plain), nil
}

func constantTimeEqual(a, b []byte) bool { return subtle.ConstantTimeCompare(a, b) == 1 }
