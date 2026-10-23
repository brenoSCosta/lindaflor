package auth

import (
	"crypto/rand"
	"fmt"
	"math/big"
	"strings"
	"time"

	"github.com/pquerna/otp"
	"github.com/pquerna/otp/totp"
	"golang.org/x/crypto/argon2"
)

// TOTPIssuer is the otpauth issuer label.
const TOTPIssuer = "lindaflor"

const backupCodeCount = 10

// Backup-code Argon2id parameters (m=8192 KiB, t=2, p=1): lighter than
// passwords on purpose — enable/regen hashes 10 codes and verify scans up
// to 10 entries, while online guessing is capped by the pending-2FA
// attempt limit. Verification reads params from each PHC string.
const (
	backupMemory  uint32 = 8192
	backupTime    uint32 = 2
	backupThreads uint8  = 1
)

var totpOpts = totp.ValidateOpts{Period: 30, Skew: 1, Digits: otp.DigitsSix, Algorithm: otp.AlgorithmSHA1}

// GenerateTOTPSecret creates a new base32 secret (160 bits) and its
// otpauth:// URI for account (usually the email).
func GenerateTOTPSecret(account string) (secret, uri string, err error) {
	key, err := totp.Generate(totp.GenerateOpts{
		Issuer: TOTPIssuer, AccountName: account, Period: 30, SecretSize: 20,
		Digits: otp.DigitsSix, Algorithm: otp.AlgorithmSHA1,
	})
	if err != nil {
		return "", "", fmt.Errorf("totp generate: %w", err)
	}
	return key.Secret(), key.URL(), nil
}

// VerifyTOTPCode checks a 6-digit code (skew ±1 step) against the base32
// secret. Malformed codes are false; a malformed secret is an error.
func VerifyTOTPCode(secretB32, code string) (bool, error) {
	return verifyTOTPAt(secretB32, code, time.Now().UTC())
}

func verifyTOTPAt(secretB32, code string, t time.Time) (bool, error) {
	code = strings.TrimSpace(code)
	if len(code) != 6 || strings.ContainsFunc(code, func(r rune) bool { return r < '0' || r > '9' }) {
		return false, nil
	}
	ok, err := totp.ValidateCustom(code, secretB32, t, totpOpts)
	if err != nil {
		return false, fmt.Errorf("totp verify: %w", err)
	}
	return ok, nil
}

// TOTPCode generates the code for secret at t (tests and tools).
func TOTPCode(secretB32 string, t time.Time) (string, error) {
	return totp.GenerateCodeCustom(secretB32, t, totpOpts)
}

// GenerateBackupCodes returns 10 printable "dddd-dddd" codes.
func GenerateBackupCodes() []string {
	codes := make([]string, backupCodeCount)
	for i := range codes {
		codes[i] = fmt.Sprintf("%04d-%04d", randBelow(10000), randBelow(10000))
	}
	return codes
}

func randBelow(n int64) int64 {
	v, err := rand.Int(rand.Reader, big.NewInt(n))
	if err != nil {
		panic("crypto/rand failed: " + err.Error())
	}
	return v.Int64()
}

func normalizeBackupCode(code string) string {
	var b strings.Builder
	for _, r := range code {
		if isASCIIAlnum(r) {
			if r >= 'a' && r <= 'z' {
				r -= 'a' - 'A'
			}
			b.WriteRune(r)
		}
	}
	return b.String()
}

// HashBackupCodes stores codes as newline-joined Argon2id PHC strings
// (PHC params contain commas, so never comma-join). CPU heavy.
func HashBackupCodes(codes []string) (string, error) {
	entries := make([]string, 0, len(codes))
	for _, c := range codes {
		h, err := hashArgon2(normalizeBackupCode(c), backupMemory, backupTime, backupThreads)
		if err != nil {
			return "", err
		}
		entries = append(entries, h)
	}
	return strings.Join(entries, "\n"), nil
}

// ConsumeBackupCode removes the entry matching presented and returns the
// remaining store; ok=false on mismatch. Malformed entries are skipped
// (never fatal) and preserved.
func ConsumeBackupCode(stored, presented string) (remaining string, ok bool) {
	needle := normalizeBackupCode(presented)
	if needle == "" {
		return "", false
	}
	var entries []string
	for line := range strings.SplitSeq(stored, "\n") {
		if line = strings.TrimSpace(line); line != "" {
			entries = append(entries, line)
		}
	}
	for i, entry := range entries {
		if match, err := verifyBackupEntry(entry, needle); err != nil || !match {
			continue
		}
		rest := append(append([]string{}, entries[:i]...), entries[i+1:]...)
		return strings.Join(rest, "\n"), true
	}
	return "", false
}

func verifyBackupEntry(entry, needle string) (bool, error) {
	p, err := parsePHC(entry)
	if err != nil {
		return false, err
	}
	key := argon2.IDKey([]byte(needle), p.salt, p.iterations, p.memory, p.threads, uint32(len(p.key))) //nolint:gosec // len ≤ 64
	return constantTimeEqual(key, p.key), nil
}
