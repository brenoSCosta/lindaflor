package auth

import (
	"crypto/rand"
	"crypto/subtle"
	"encoding/base64"
	"errors"
	"fmt"
	"strconv"
	"strings"
	"sync"
	"unicode/utf8"

	"github.com/trustelem/zxcvbn"
	"golang.org/x/crypto/argon2"

	"github.com/wellington-vell/lindaflor/internal/config"
)

// Argon2id parameters for account passwords (OWASP / argon2 crate
// defaults: m=19456 KiB, t=2, p=1, 32-byte tag, 16-byte salt).
const (
	argonMemory  uint32 = 19456
	argonTime    uint32 = 2
	argonThreads uint8  = 1
	argonKeyLen  uint32 = 32
	argonSaltLen        = 16
)

// Password policy errors. Messages keep the Rust wording (PortugueseMessage
// matches on "at least" / "at most" / "too weak").
var (
	ErrPasswordTooWeak = BadRequest("password is too weak: choose a stronger password")
	errMalformedHash   = errors.New("password verify error: malformed PHC string")
)

// PasswordTooShort is returned when the password has fewer than min chars.
func PasswordTooShort(minLen int) error {
	return BadRequest(fmt.Sprintf("password must be at least %d characters", minLen))
}

// PasswordTooLong is returned when the password has more than max chars.
func PasswordTooLong(maxLen int) error {
	return BadRequest(fmt.Sprintf("password must be at most %d characters", maxLen))
}

// ValidatePassword checks min length, then max length (both in Unicode
// characters, not bytes), then zxcvbn strength with userInputs (email,
// name, "lindaflor") as dictionary words. Bounds come from
// PASSWORD_MIN_LEN / PASSWORD_MAX_LEN / PASSWORD_MIN_ZXCVBN_SCORE.
// Never log the candidate.
func ValidatePassword(password string, userInputs ...string) error {
	n := utf8.RuneCountInString(password)
	if minLen := config.PasswordMinLen(); n < minLen {
		return PasswordTooShort(minLen)
	}
	if maxLen := config.PasswordMaxLen(); n > maxLen {
		return PasswordTooLong(maxLen)
	}
	inputs := make([]string, 0, len(userInputs))
	for _, in := range userInputs {
		if in != "" {
			inputs = append(inputs, in)
		}
	}
	if strengthScore(password, inputs) < config.PasswordMinZxcvbnScore() {
		return ErrPasswordTooWeak
	}
	return nil
}

// strengthScore is the zxcvbn score (0-4). The Go port misses repeat and
// sequence patterns made of non-ASCII runes ("é" × 100 scores 4), so the
// password is also scored with each distinct non-ASCII rune folded to an
// ASCII letter (structure preserved) and the lower score wins.
func strengthScore(password string, inputs []string) int {
	score := zxcvbn.PasswordStrength(password, inputs).Score
	folded, changed := foldNonASCII(password)
	if changed {
		score = min(score, zxcvbn.PasswordStrength(folded, inputs).Score)
	}
	return score
}

func foldNonASCII(s string) (string, bool) {
	seen := map[rune]rune{}
	changed := false
	out := []rune(s)
	for i, r := range out {
		if r < utf8.RuneSelf {
			continue
		}
		f, ok := seen[r]
		if !ok {
			f = 'a' + rune(len(seen)%26)
			seen[r] = f
		}
		out[i] = f
		changed = true
	}
	return string(out), changed
}

// HashPassword hashes with Argon2id and a random salt, returning a PHC
// string ($argon2id$v=19$m=19456,t=2,p=1$salt$hash). Callers validate first.
func HashPassword(password string) (string, error) {
	return hashArgon2(password, argonMemory, argonTime, argonThreads)
}

func hashArgon2(password string, memory, iterations uint32, threads uint8) (string, error) {
	salt := make([]byte, argonSaltLen)
	if _, err := rand.Read(salt); err != nil {
		return "", fmt.Errorf("password hash error: %w", err)
	}
	key := argon2.IDKey([]byte(password), salt, iterations, memory, threads, argonKeyLen)
	b64 := base64.RawStdEncoding
	return fmt.Sprintf("$argon2id$v=%d$m=%d,t=%d,p=%d$%s$%s",
		argon2.Version, memory, iterations, threads, b64.EncodeToString(salt), b64.EncodeToString(key)), nil
}

type phc struct {
	memory, iterations uint32
	threads            uint8
	salt, key          []byte
}

func parsePHC(encoded string) (phc, error) {
	var p phc
	parts := strings.Split(encoded, "$")
	if len(parts) != 6 || parts[0] != "" || parts[1] != "argon2id" {
		return p, errMalformedHash
	}
	if parts[2] != "v="+strconv.Itoa(argon2.Version) {
		return p, errMalformedHash
	}
	for kv := range strings.SplitSeq(parts[3], ",") {
		k, v, ok := strings.Cut(kv, "=")
		if !ok {
			return p, errMalformedHash
		}
		n, err := strconv.ParseUint(v, 10, 32)
		if err != nil || n == 0 {
			return p, errMalformedHash
		}
		switch k {
		case "m":
			p.memory = uint32(n)
		case "t":
			p.iterations = uint32(n)
		case "p":
			if n > 255 {
				return p, errMalformedHash
			}
			p.threads = uint8(n)
		default:
			return p, errMalformedHash
		}
	}
	if p.memory == 0 || p.iterations == 0 || p.threads == 0 || p.memory > 1<<21 || p.iterations > 64 {
		return p, errMalformedHash
	}
	var err error
	if p.salt, err = base64.RawStdEncoding.Strict().DecodeString(parts[4]); err != nil || len(p.salt) < 8 {
		return p, errMalformedHash
	}
	if p.key, err = base64.RawStdEncoding.Strict().DecodeString(parts[5]); err != nil || len(p.key) < 16 || len(p.key) > 64 {
		return p, errMalformedHash
	}
	return p, nil
}

// VerifyPassword reports whether password matches the PHC hash. A
// malformed hash is an error (not a mismatch). Constant-time compare.
func VerifyPassword(password, encoded string) (bool, error) {
	p, err := parsePHC(encoded)
	if err != nil {
		return false, err
	}
	key := argon2.IDKey([]byte(password), p.salt, p.iterations, p.memory, p.threads, uint32(len(p.key))) //nolint:gosec // len ≤ 64
	return subtle.ConstantTimeCompare(key, p.key) == 1, nil
}

var (
	dummyOnce sync.Once
	dummyHash string
)

// DummyVerify spends one Argon2 verification against a fixed hash so an
// unknown account costs the same as a wrong password (no timing oracle).
func DummyVerify(password string) {
	dummyOnce.Do(func() { dummyHash, _ = HashPassword("lindaflor-timing-equalizer") })
	_, _ = VerifyPassword(password, dummyHash)
}
