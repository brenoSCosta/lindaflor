package config

import (
	"crypto/rand"
	"encoding/base64"
	"encoding/hex"
	"errors"
	"fmt"
	"log/slog"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"
)

// DevTokenPepper is the devenv default; never acceptable in production.
const DevTokenPepper = "dev-pepper-change-me"

// DefaultMailFrom is the From header outside production when MAIL_FROM is unset.
const DefaultMailFrom = "Lindaflor <nao-responda@lindaflor.local>"

// Errors returned by Load and the validating accessors. Messages never echo
// secret values.
var (
	ErrInvalidPort                  = errors.New("invalid PORT value")
	ErrMissingAppOrigin             = errors.New("missing APP_ORIGIN: set it to the public https origin in production")
	ErrWeakSeedPassword             = errors.New("refusing to seed with a weak SEED_ADMIN_PASSWORD in production: set a strong (12+ char) password or generate one per deploy")
	ErrMissingTotpEncryptionKey     = errors.New("missing TOTP_ENCRYPTION_KEY: set it (or TOTP_ENCRYPTION_KEY_FILE) to a base64-32B or hex-64 key")
	ErrInvalidTotpEncryptionKey     = errors.New("invalid TOTP_ENCRYPTION_KEY: expected base64-encoded 32 bytes or 64 hex chars")
	ErrMailLogTransportInProduction = errors.New("MAIL_TRANSPORT=log in production would drop every email: set MAIL_TRANSPORT=smtp, or MAIL_ALLOW_LOG_IN_PRODUCTION=1 to opt in")
	ErrInvalidSMTPTLS               = errors.New("invalid SMTP_TLS: expected starttls or tls (none is dev-only)")
	ErrMissingMetricsToken          = errors.New("missing METRICS_TOKEN: set it (or METRICS_TOKEN_FILE) so /metrics is not public in production")
	ErrMissingTokenPepper           = errors.New("missing TOKEN_PEPPER: set a random value (or TOKEN_PEPPER_FILE) in production")
)

// MissingVarError reports a required variable that is unset.
type MissingVarError struct{ Key string }

func (e *MissingVarError) Error() string { return "missing environment variable: " + e.Key }

// InvalidAppOriginError reports an APP_ORIGIN that is not http(s)://host.
type InvalidAppOriginError struct{ Raw string }

func (e *InvalidAppOriginError) Error() string {
	return fmt.Sprintf("invalid APP_ORIGIN %q: must be an http(s):// URL with a host", e.Raw)
}

// InvalidMailTransportError reports an unknown MAIL_TRANSPORT.
type InvalidMailTransportError struct{ Raw string }

func (e *InvalidMailTransportError) Error() string {
	return fmt.Sprintf("invalid MAIL_TRANSPORT %q: expected \"log\" or \"smtp\"", e.Raw)
}

// S3Config is the object storage endpoint (RustFS/MinIO/S3, path-style).
type S3Config struct {
	Endpoint        string
	Region          string
	AccessKeyID     string
	SecretAccessKey string
	Bucket          string
}

// String redacts the credentials.
func (c S3Config) String() string {
	return fmt.Sprintf("S3Config{Endpoint:%s Region:%s AccessKeyID:*** SecretAccessKey:*** Bucket:%s}",
		c.Endpoint, c.Region, c.Bucket)
}

// GoString keeps %#v redacted too.
func (c S3Config) GoString() string { return c.String() }

// Config is the validated snapshot the web process boots with.
type Config struct {
	AppEnv    string
	Host      string
	Port      int
	AppOrigin string // normalized, "" when unset (dev)

	DatabaseURL         string
	PGMaxConns          int32
	PGAcquireTimeout    time.Duration
	PGIdleTimeout       time.Duration
	MigrateOnBoot       bool
	ValkeyURL           string
	S3                  *S3Config
	Mail                MailConfig
	MailPollInterval    time.Duration
	MailMaxAttempts     int
	Payments            PaymentConfig
	MetricsToken        string
	TokenPepper         string
	TrustedProxyHops    int
	TrustedProxyNets    []string
	ReservationSweep    time.Duration
	AbandonedCartOn     bool
	AbandonedCartAfter  time.Duration
	LogSampleRate       float64
	LogSlowThreshold    time.Duration
	LogFilePath         string
	LogFileMaxBytes     uint64
	LogFileMaxFiles     uint32
	PasswordMinLen      int
	PasswordMaxLen      int
	PasswordMinZxcvbn   int
	Google              GoogleConfig
	Captcha             CaptchaConfig
	OpenAPIDocsEnabled  bool
	SecureCookies       bool
	DevCSP              bool
	HSTS                bool
	GoogleOAuthTestBase string
}

// String redacts connection strings and secrets.
func (c *Config) String() string {
	s3 := "<nil>"
	if c.S3 != nil {
		s3 = c.S3.String()
	}
	return fmt.Sprintf("Config{AppEnv:%s Port:%d AppOrigin:%s DatabaseURL:*** ValkeyURL:*** S3:%s}",
		c.AppEnv, c.Port, c.AppOrigin, s3)
}

// GoString keeps %#v redacted too.
func (c *Config) GoString() string { return c.String() }

// LogValue keeps slog output redacted.
func (c *Config) LogValue() slog.Value { return slog.StringValue(c.String()) }

// Load reads and validates the configuration for the web process.
//
// Production (APP_ENV unset or "production") requires a valid APP_ORIGIN,
// a TOTP encryption key, METRICS_TOKEN and a non-default TOKEN_PEPPER, and
// validates mail and payment settings. Outside production a `.env` file in
// the working directory is loaded first (never in production).
func Load() (*Config, error) {
	if !IsProduction() {
		loadDotEnv(".env")
	}
	cfg, err := loadCore()
	if err != nil {
		return nil, err
	}
	if cfg.Mail, err = MailFromEnv(); err != nil {
		return nil, err
	}
	if cfg.Payments, err = PaymentsFromEnv(); err != nil {
		return nil, err
	}
	return cfg, nil
}

// loadCore is the Rust `Config::from_env` equivalent: port, database,
// valkey, S3 and the production guards (no mail/payment parsing).
func loadCore() (*Config, error) {
	port := 4200
	if raw, ok := ReadEnvOrFile("PORT"); ok {
		p, err := strconv.ParseUint(raw, 10, 16)
		if err != nil {
			return nil, ErrInvalidPort
		}
		port = int(p)
	}
	dbURL, ok := ReadEnvOrFile("DATABASE_URL")
	if !ok {
		return nil, &MissingVarError{Key: "DATABASE_URL"}
	}
	valkeyURL, ok := ReadEnvOrFile("VALKEY_URL")
	if !ok {
		valkeyURL = "redis://127.0.0.1:4202"
	}
	env := AppEnv()
	if env == "production" {
		if _, err := RequireAppOrigin(); err != nil {
			return nil, err
		}
		if _, err := TotpEncryptionKey(); err != nil {
			return nil, err
		}
		if MetricsToken() == "" {
			return nil, ErrMissingMetricsToken
		}
		if p := TokenPepper(); p == "" || p == DevTokenPepper {
			return nil, ErrMissingTokenPepper
		}
	}
	host := os.Getenv("HOST")
	if host == "" {
		host = "127.0.0.1"
	}
	return &Config{
		AppEnv:              env,
		Host:                host,
		Port:                port,
		AppOrigin:           AppOrigin(),
		DatabaseURL:         dbURL,
		PGMaxConns:          PGMaxConnections(),
		PGAcquireTimeout:    secsDuration(PGAcquireTimeoutSecs()),
		PGIdleTimeout:       secsDuration(PGIdleTimeoutSecs()),
		MigrateOnBoot:       MigrateOnBoot(),
		ValkeyURL:           valkeyURL,
		S3:                  s3FromEnv(),
		MailPollInterval:    secsDuration(MailPollSecs()),
		MailMaxAttempts:     MailMaxAttempts(),
		MetricsToken:        MetricsToken(),
		TokenPepper:         TokenPepper(),
		TrustedProxyHops:    TrustedProxyHops(),
		TrustedProxyNets:    TrustedProxyNetworks(),
		ReservationSweep:    secsDuration(ReservationSweepSecs()),
		AbandonedCartOn:     AbandonedCartEnabled(),
		AbandonedCartAfter:  time.Duration(AbandonedCartAfterMinutes()) * time.Minute,
		LogSampleRate:       LogSampleRate(),
		LogSlowThreshold:    time.Duration(min(LogSlowThresholdMS(), maxSecs*1000)) * time.Millisecond,
		LogFilePath:         LogFilePath(),
		LogFileMaxBytes:     LogFileMaxBytes(),
		LogFileMaxFiles:     LogFileMaxFiles(),
		PasswordMinLen:      PasswordMinLen(),
		PasswordMaxLen:      PasswordMaxLen(),
		PasswordMinZxcvbn:   PasswordMinZxcvbnScore(),
		Google:              GoogleFromEnv(),
		Captcha:             CaptchaFromEnv(),
		OpenAPIDocsEnabled:  OpenAPIDocsEnabled(),
		SecureCookies:       SecureCookies(),
		DevCSP:              IsDevEnv(env),
		HSTS:                !IsDevEnv(env),
		GoogleOAuthTestBase: GoogleOAuthTestBaseURL(),
	}, nil
}

func s3FromEnv() *S3Config {
	endpoint := envNonEmpty("S3_ENDPOINT")
	key := envNonEmpty("S3_ACCESS_KEY_ID")
	secret := envNonEmpty("S3_SECRET_ACCESS_KEY")
	if endpoint == "" || key == "" || secret == "" {
		return nil
	}
	region := envNonEmpty("S3_REGION")
	if region == "" {
		region = "us-east-1"
	}
	bucket := envNonEmpty("S3_BUCKET")
	if bucket == "" {
		bucket = "lindaflor"
	}
	return &S3Config{Endpoint: endpoint, Region: region, AccessKeyID: key, SecretAccessKey: secret, Bucket: bucket}
}

// TokenPepper is the HMAC pepper for one-time verification tokens and
// signed newsletter links (TOKEN_PEPPER, trimmed). Empty when unset.
func TokenPepper() string { return strings.TrimSpace(envNonEmpty("TOKEN_PEPPER")) }

// ParseTotpEncryptionKey parses standard base64 (32 bytes) or 64 hex chars.
// Surrounding whitespace is ignored. Errors never echo the key material.
func ParseTotpEncryptionKey(raw string) ([32]byte, error) {
	var key [32]byte
	trimmed := strings.TrimSpace(raw)
	if trimmed == "" {
		return key, ErrMissingTotpEncryptionKey
	}
	if b, err := base64.StdEncoding.Strict().DecodeString(trimmed); err == nil && len(b) == 32 {
		copy(key[:], b)
		return key, nil
	}
	if b, err := hex.DecodeString(trimmed); err == nil && len(b) == 32 {
		copy(key[:], b)
		return key, nil
	}
	return key, ErrInvalidTotpEncryptionKey
}

// SecretKey wraps key material so fmt/slog never print it.
type SecretKey [32]byte

func (SecretKey) String() string               { return "SecretKey(***)" }
func (SecretKey) GoString() string             { return "SecretKey(***)" }
func (SecretKey) LogValue() slog.Value         { return slog.StringValue("***") }
func (k *SecretKey) Bytes() *[32]byte          { return (*[32]byte)(k) }
func (SecretKey) MarshalText() ([]byte, error) { return []byte("***"), nil }

var (
	ephemeralTotpOnce sync.Once
	ephemeralTotpKey  SecretKey
)

// TotpEncryptionKey is the XChaCha20-Poly1305 key for two_factor secrets
// at rest (TOTP_ENCRYPTION_KEY / _FILE). Missing in production is an error;
// in dev/test a per-process ephemeral key is generated once (with a
// warning). Explicit keys are re-parsed on every call so rotation needs no
// restart.
func TotpEncryptionKey() (SecretKey, error) {
	if raw, ok := ReadEnvOrFile("TOTP_ENCRYPTION_KEY"); ok {
		k, err := ParseTotpEncryptionKey(raw)
		return SecretKey(k), err
	}
	if IsProduction() {
		return SecretKey{}, ErrMissingTotpEncryptionKey
	}
	ephemeralTotpOnce.Do(func() {
		slog.Warn("TOTP_ENCRYPTION_KEY is unset: using an ephemeral in-memory key. " +
			"TOTP secrets encrypted now cannot be decrypted after a restart or on another replica. " +
			"Set TOTP_ENCRYPTION_KEY (or TOTP_ENCRYPTION_KEY_FILE) for any durable environment.")
		if _, err := rand.Read(ephemeralTotpKey[:]); err != nil {
			panic("crypto/rand failed: " + err.Error())
		}
	})
	return ephemeralTotpKey, nil
}

// LogSampleRate from LOG_SAMPLE_RATE (default 1.0, clamped to [0,1]).
func LogSampleRate() float64 {
	r := envParseFloat("LOG_SAMPLE_RATE", 1.0)
	return min(max(r, 0), 1)
}

// LogSlowThresholdMS from LOG_SLOW_THRESHOLD_MS (default 1000).
func LogSlowThresholdMS() uint64 { return envParseUint("LOG_SLOW_THRESHOLD_MS", 1000) }

// LogFilePath from LOG_FILE_PATH ("" = no file sink).
func LogFilePath() string { return envNonEmpty("LOG_FILE_PATH") }

// LogFileMaxBytes from LOG_FILE_MAX_BYTES (default 10 MiB).
func LogFileMaxBytes() uint64 { return envParseUint("LOG_FILE_MAX_BYTES", 10*1024*1024) }

// LogFileMaxFiles from LOG_FILE_MAX_FILES (default 5).
func LogFileMaxFiles() uint32 { return uint32(min(envParseUint("LOG_FILE_MAX_FILES", 5), 1<<31)) } //nolint:gosec // clamped

// PasswordMinLen from PASSWORD_MIN_LEN (default 8).
func PasswordMinLen() int { return int(envParseUint("PASSWORD_MIN_LEN", 8)) } //nolint:gosec // small config value

// PasswordMaxLen from PASSWORD_MAX_LEN (default 128).
func PasswordMaxLen() int { return int(envParseUint("PASSWORD_MAX_LEN", 128)) } //nolint:gosec // small config value

// PasswordMinZxcvbnScore from PASSWORD_MIN_ZXCVBN_SCORE (default 3).
func PasswordMinZxcvbnScore() int {
	v, ok := ReadEnvOrFile("PASSWORD_MIN_ZXCVBN_SCORE")
	if !ok {
		return 3
	}
	n, err := strconv.ParseUint(v, 10, 8)
	if err != nil {
		return 3
	}
	return int(n)
}

// MetricsToken is the bearer token for GET /metrics ("" = open in dev,
// 401 in production where boot requires it).
func MetricsToken() string { return envNonEmpty("METRICS_TOKEN") }

// PGMaxConnections from PG_MAX_CONN; otherwise 20 in production, 5 elsewhere.
func PGMaxConnections() int32 {
	if v, ok := ReadEnvOrFile("PG_MAX_CONN"); ok {
		if n, err := strconv.ParseUint(v, 10, 31); err == nil && n > 0 {
			return int32(n) //nolint:gosec // bounded by bitSize 31
		}
	}
	if IsProduction() {
		return 20
	}
	return 5
}

// PGAcquireTimeoutSecs from PG_ACQUIRE_TIMEOUT_SECS (default 5, >0).
func PGAcquireTimeoutSecs() uint64 { return positiveOr("PG_ACQUIRE_TIMEOUT_SECS", 5) }

// PGIdleTimeoutSecs from PG_IDLE_TIMEOUT_SECS (default 600, >0).
func PGIdleTimeoutSecs() uint64 { return positiveOr("PG_IDLE_TIMEOUT_SECS", 600) }

func positiveOr(key string, def uint64) uint64 {
	if v, ok := ReadEnvOrFile(key); ok {
		if n, err := strconv.ParseUint(v, 10, 64); err == nil && n > 0 {
			return n
		}
	}
	return def
}

// MigrateOnBoot: explicit MIGRATE_ON_BOOT=1/0 wins; otherwise on outside
// production and off in production (the migrate job owns the schema).
func MigrateOnBoot() bool {
	if v, ok := ReadEnvOrFile("MIGRATE_ON_BOOT"); ok {
		return truthy(v)
	}
	return !IsProduction()
}

// ReservationSweepSecs from RESERVATION_SWEEP_SECS (default 60, min 1).
func ReservationSweepSecs() uint64 { return max(envParseUint("RESERVATION_SWEEP_SECS", 60), 1) }

// AbandonedCartEnabled from ABANDONED_CART_ENABLED (default on; 0/false/no/off disables).
func AbandonedCartEnabled() bool {
	v, ok := ReadEnvOrFile("ABANDONED_CART_ENABLED")
	if !ok {
		return true
	}
	switch strings.ToLower(strings.TrimSpace(v)) {
	case "0", "false", "no", "off":
		return false
	}
	return true
}

// AbandonedCartAfterMinutes from ABANDONED_CART_AFTER_MINUTES (default 180, min 1).
func AbandonedCartAfterMinutes() int64 {
	return max(envParseInt("ABANDONED_CART_AFTER_MINUTES", 180), 1)
}

// TrustedProxyHops from TRUSTED_PROXY_HOPS (default 0 = directly exposed).
// Only set above zero when every request passes through that many proxies,
// otherwise clients can spoof their IP via X-Forwarded-For.
func TrustedProxyHops() int {
	v, ok := ReadEnvOrFile("TRUSTED_PROXY_HOPS")
	if !ok {
		return 0
	}
	n, err := strconv.ParseUint(v, 10, 16)
	if err != nil {
		return 0
	}
	return int(n)
}

// TrustedProxyNetworks from TRUSTED_PROXY_NETWORKS (comma-separated CIDRs
// or IPs; blanks dropped).
func TrustedProxyNetworks() []string {
	v, ok := ReadEnvOrFile("TRUSTED_PROXY_NETWORKS")
	if !ok {
		return nil
	}
	var out []string
	for part := range strings.SplitSeq(v, ",") {
		if p := strings.TrimSpace(part); p != "" {
			out = append(out, p)
		}
	}
	return out
}

// NormalizeAppOrigin validates an APP_ORIGIN: http(s):// plus a host, no
// path/query/fragment/whitespace; trailing slashes stripped.
func NormalizeAppOrigin(raw string) (string, error) {
	trimmed := strings.TrimSpace(raw)
	if trimmed == "" {
		return "", ErrMissingAppOrigin
	}
	normalized := strings.TrimRight(trimmed, "/")
	rest, ok := strings.CutPrefix(normalized, "https://")
	if !ok {
		rest, ok = strings.CutPrefix(normalized, "http://")
	}
	valid := ok && rest != "" && !strings.ContainsAny(rest, "/?#") &&
		!strings.ContainsFunc(rest, isSpace)
	if !valid {
		return "", &InvalidAppOriginError{Raw: raw}
	}
	return normalized, nil
}

func isSpace(r rune) bool {
	switch r {
	case ' ', '\t', '\n', '\r', '\v', '\f', 0x85, 0xA0:
		return true
	}
	return false
}

// RequireAppOrigin returns the validated APP_ORIGIN or an error.
func RequireAppOrigin() (string, error) {
	raw, ok := ReadEnvOrFile("APP_ORIGIN")
	if !ok {
		return "", ErrMissingAppOrigin
	}
	return NormalizeAppOrigin(raw)
}

// AppOrigin is the optional public origin; invalid values count as unset
// (production boot still fails through RequireAppOrigin).
func AppOrigin() string {
	o, err := NormalizeAppOrigin(os.Getenv("APP_ORIGIN"))
	if err != nil {
		return ""
	}
	return o
}

var weakSeedPasswords = map[string]bool{"": true, "admin123": true, "password": true, "changeme": true}

// RejectWeakSeedPasswordInProduction fails closed when the seed or
// create-admin path would use a weak password in production.
func RejectWeakSeedPasswordInProduction(password string) error {
	if !IsProduction() {
		return nil
	}
	if len(password) < 12 || weakSeedPasswords[strings.TrimSpace(password)] {
		return ErrWeakSeedPassword
	}
	return nil
}

// MailPollSecs from MAIL_POLL_SECS (default 5, min 1).
func MailPollSecs() uint64 { return max(envParseUint("MAIL_POLL_SECS", 5), 1) }

// MailMaxAttempts from MAIL_MAX_ATTEMPTS (default 8, min 1).
func MailMaxAttempts() int { return int(max(envParseInt("MAIL_MAX_ATTEMPTS", 8), 1)) }

// RateLimitFor resolves RATE_LIMIT_<SCOPE>_PER_MIN (scope ':'/'-' → '_',
// uppercased; e.g. auth:sign-in → RATE_LIMIT_AUTH_SIGN_IN_PER_MIN), else def.
// Read with plain os env (no _FILE), like the Rust code.
func RateLimitFor(scope string, def uint32) uint32 {
	return rateLimitEnv("RATE_LIMIT_"+scopeKey(scope)+"_PER_MIN", def)
}

// WindowRateLimitFor resolves RATE_LIMIT_<SCOPE>_PER_WINDOW, else def.
func WindowRateLimitFor(scope string, def uint32) uint32 {
	return rateLimitEnv("RATE_LIMIT_"+scopeKey(scope)+"_PER_WINDOW", def)
}

func scopeKey(scope string) string {
	return strings.ToUpper(strings.NewReplacer(":", "_", "-", "_").Replace(scope))
}

func rateLimitEnv(key string, def uint32) uint32 {
	n, err := strconv.ParseUint(os.Getenv(key), 10, 32)
	if err != nil || n == 0 {
		return def
	}
	return uint32(n)
}

// maxSecs caps second-based settings so Durations cannot overflow.
const maxSecs = 1 << 32

func secsDuration(n uint64) time.Duration { return time.Duration(min(n, maxSecs)) * time.Second }
