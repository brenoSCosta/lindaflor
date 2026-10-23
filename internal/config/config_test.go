package config

import (
	"errors"
	"fmt"
	"os"
	"path/filepath"
	"slices"
	"strings"
	"testing"
	"time"
)

// unset clears key for the test (empty counts as unset everywhere except
// APP_ENV, which tests always set explicitly).
func unset(t *testing.T, keys ...string) {
	t.Helper()
	for _, k := range keys {
		t.Setenv(k, "")
		_ = os.Unsetenv(k)
	}
}

func prodBase(t *testing.T) {
	t.Helper()
	t.Setenv("APP_ENV", "production")
	t.Setenv("APP_ORIGIN", "https://loja.example.com")
	t.Setenv("DATABASE_URL", "postgres://localhost/test")
	t.Setenv("TOTP_ENCRYPTION_KEY", "AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA=")
	unset(t, "TOTP_ENCRYPTION_KEY_FILE", "METRICS_TOKEN_FILE", "TOKEN_PEPPER_FILE", "PORT")
	t.Setenv("METRICS_TOKEN", "scrape-secret")
	t.Setenv("TOKEN_PEPPER", "prod-random-pepper")
}

func TestNormalizeAppOriginAcceptsHTTPOrigins(t *testing.T) {
	for raw, want := range map[string]string{
		"https://loja.example.com":        "https://loja.example.com",
		"http://localhost:4200/":          "http://localhost:4200",
		"  https://loja.example.com///  ": "https://loja.example.com",
	} {
		got, err := NormalizeAppOrigin(raw)
		if err != nil || got != want {
			t.Errorf("NormalizeAppOrigin(%q) = %q, %v; want %q", raw, got, err, want)
		}
	}
}

func TestNormalizeAppOriginRejectsBadValues(t *testing.T) {
	for _, raw := range []string{
		"", "   ", "loja.example.com", "ftp://loja.example.com", "https://",
		"https:///path", "https://example.com/app", "https://example.com?q=1",
		"https://exam ple.com",
	} {
		if _, err := NormalizeAppOrigin(raw); err == nil {
			t.Errorf("expected error for %q", raw)
		}
	}
	if _, err := NormalizeAppOrigin(""); !errors.Is(err, ErrMissingAppOrigin) {
		t.Errorf("empty: got %v", err)
	}
	var inv *InvalidAppOriginError
	if _, err := NormalizeAppOrigin("notaurl"); !errors.As(err, &inv) {
		t.Errorf("notaurl: got %v", err)
	}
}

func TestLoadFailsInProductionWithoutAppOrigin(t *testing.T) {
	prodBase(t)
	unset(t, "APP_ORIGIN", "APP_ORIGIN_FILE")
	_, err := loadCore()
	var inv *InvalidAppOriginError
	if !errors.Is(err, ErrMissingAppOrigin) && !errors.As(err, &inv) {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestLoadFailsInProductionWithInvalidAppOrigin(t *testing.T) {
	prodBase(t)
	t.Setenv("APP_ORIGIN", "not-a-url")
	_, err := loadCore()
	var inv *InvalidAppOriginError
	if !errors.As(err, &inv) {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestLoadOKInDevWithoutAppOrigin(t *testing.T) {
	t.Setenv("APP_ENV", "development")
	unset(t, "APP_ORIGIN", "APP_ORIGIN_FILE", "PORT")
	t.Setenv("DATABASE_URL", "postgres://localhost/test")
	if _, err := loadCore(); err != nil {
		t.Fatalf("dev boot must not require APP_ORIGIN: %v", err)
	}
}

func TestLoadOKInProductionWithValidAppOrigin(t *testing.T) {
	prodBase(t)
	t.Setenv("APP_ORIGIN", "https://loja.example.com/")
	cfg, err := loadCore()
	if err != nil {
		t.Fatalf("prod boot with valid APP_ORIGIN: %v", err)
	}
	if cfg.AppOrigin != "https://loja.example.com" || !cfg.HSTS || cfg.DevCSP || !cfg.SecureCookies {
		t.Fatalf("unexpected prod snapshot: %+v", cfg)
	}
}

func TestFullLoadInProductionValidatesMailAndPayments(t *testing.T) {
	prodBase(t)
	unset(t, "MAIL_TRANSPORT", "MAIL_ALLOW_LOG_IN_PRODUCTION", "PAYMENT_PROVIDER", "MP_ACCESS_TOKEN", "ALLOW_STATIC_PIX")
	if _, err := Load(); !errors.Is(err, ErrMailLogTransportInProduction) {
		t.Fatalf("want mail error, got %v", err)
	}
	t.Setenv("MAIL_ALLOW_LOG_IN_PRODUCTION", "1")
	t.Setenv("MAIL_FROM", "Lindaflor <loja@example.com>")
	if _, err := Load(); !errors.Is(err, ErrStaticPixInProduction) {
		t.Fatalf("want static pix error, got %v", err)
	}
	t.Setenv("ALLOW_STATIC_PIX", "1")
	cfg, err := Load()
	if err != nil {
		t.Fatalf("Load: %v", err)
	}
	if cfg.Payments.Kind != ProviderStaticPix || cfg.Mail.Transport != MailTransportLog {
		t.Fatalf("unexpected: %v %v", cfg.Payments, cfg.Mail)
	}
}

func TestMailLogTransportRefusedInProductionUnlessAllowed(t *testing.T) {
	t.Setenv("APP_ENV", "production")
	unset(t, "MAIL_TRANSPORT", "MAIL_ALLOW_LOG_IN_PRODUCTION")
	t.Setenv("MAIL_FROM", "Lindaflor <loja@example.com>")
	if _, err := MailFromEnv(); !errors.Is(err, ErrMailLogTransportInProduction) {
		t.Fatalf("got %v", err)
	}
	t.Setenv("MAIL_ALLOW_LOG_IN_PRODUCTION", "1")
	cfg, err := MailFromEnv()
	if err != nil || cfg.Transport != MailTransportLog {
		t.Fatalf("opt-in: %v %v", cfg, err)
	}
	unset(t, "MAIL_ALLOW_LOG_IN_PRODUCTION")
	t.Setenv("APP_ENV", "development")
	cfg, err = MailFromEnv()
	if err != nil || cfg.Transport != MailTransportLog {
		t.Fatalf("dev: %v %v", cfg, err)
	}
}

func TestMailSMTPConfigParsesAndRedactsPassword(t *testing.T) {
	t.Setenv("APP_ENV", "production")
	t.Setenv("MAIL_TRANSPORT", "smtp")
	t.Setenv("SMTP_HOST", "smtp.example.com")
	unset(t, "SMTP_PORT")
	t.Setenv("SMTP_TLS", "tls")
	t.Setenv("SMTP_USERNAME", "loja")
	t.Setenv("SMTP_PASSWORD", "hunter2-secret")
	t.Setenv("MAIL_FROM", "Lindaflor <loja@example.com>")
	cfg, err := MailFromEnv()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.SMTP == nil || cfg.SMTP.Port != 465 || cfg.SMTP.TLS != SMTPTLSOnly || cfg.SMTP.Password != "hunter2-secret" {
		t.Fatalf("unexpected smtp: %+v", cfg.SMTP)
	}
	for _, rendered := range []string{fmt.Sprintf("%v", cfg), fmt.Sprintf("%+v", cfg), fmt.Sprintf("%#v", cfg), fmt.Sprintf("%v", *cfg.SMTP)} {
		if strings.Contains(rendered, "hunter2") {
			t.Fatalf("leaked password: %s", rendered)
		}
	}
	t.Setenv("SMTP_TLS", "none")
	if _, err := MailFromEnv(); !errors.Is(err, ErrInvalidSMTPTLS) {
		t.Fatalf("plaintext must be dev-only: %v", err)
	}
	unset(t, "SMTP_HOST")
	var missing *MissingVarError
	if _, err := MailFromEnv(); !errors.As(err, &missing) {
		t.Fatalf("missing host: %v", err)
	}
}

func TestLoadFailsInProductionWithoutMetricsToken(t *testing.T) {
	prodBase(t)
	unset(t, "METRICS_TOKEN", "METRICS_TOKEN_FILE")
	if _, err := loadCore(); !errors.Is(err, ErrMissingMetricsToken) {
		t.Fatalf("unexpected error: %v", err)
	}
}

func TestLoadFailsInProductionWithoutTokenPepper(t *testing.T) {
	prodBase(t)
	unset(t, "TOKEN_PEPPER", "TOKEN_PEPPER_FILE")
	if _, err := loadCore(); !errors.Is(err, ErrMissingTokenPepper) {
		t.Fatalf("missing pepper: %v", err)
	}
	t.Setenv("TOKEN_PEPPER", DevTokenPepper)
	if _, err := loadCore(); !errors.Is(err, ErrMissingTokenPepper) {
		t.Fatalf("dev pepper: %v", err)
	}
}

func TestLoadFailsInProductionWithoutTotpKey(t *testing.T) {
	prodBase(t)
	unset(t, "TOTP_ENCRYPTION_KEY", "TOTP_ENCRYPTION_KEY_FILE")
	if _, err := loadCore(); !errors.Is(err, ErrMissingTotpEncryptionKey) {
		t.Fatalf("unexpected error: %v", err)
	}
}

const (
	testKeyB64 = "AAECAwQFBgcICQoLDA0ODxAREhMUFRYXGBkaGxwdHh8="
	testKeyHex = "000102030405060708090a0b0c0d0e0f101112131415161718191a1b1c1d1e1f"
)

func expectedTestKey() [32]byte {
	var k [32]byte
	for i := range k {
		k[i] = byte(i)
	}
	return k
}

func TestParseTotpKeyAcceptsBase64AndHex(t *testing.T) {
	for _, raw := range []string{testKeyB64, testKeyHex, "  " + strings.ToUpper(testKeyHex) + "  \n"} {
		k, err := ParseTotpEncryptionKey(raw)
		if err != nil || k != expectedTestKey() {
			t.Errorf("ParseTotpEncryptionKey(%q) = %v, %v", raw, k, err)
		}
	}
}

func TestParseTotpKeyRejectsBadValuesWithoutEchoingThem(t *testing.T) {
	for _, raw := range []string{
		"", "   ", "not-a-key",
		"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA",  // unpadded base64
		"AAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAAA==", // 31 bytes
		"00", strings.Repeat("zz", 32),
	} {
		_, err := ParseTotpEncryptionKey(raw)
		if !errors.Is(err, ErrInvalidTotpEncryptionKey) && !errors.Is(err, ErrMissingTotpEncryptionKey) {
			t.Errorf("%q: unexpected error %v", raw, err)
			continue
		}
		if tr := strings.TrimSpace(raw); tr != "" && strings.Contains(err.Error(), tr) {
			t.Errorf("error must not echo key material: %v", err)
		}
	}
}

func writeTemp(t *testing.T, contents string) string {
	t.Helper()
	p := filepath.Join(t.TempDir(), "secret")
	if err := os.WriteFile(p, []byte(contents), 0o600); err != nil {
		t.Fatal(err)
	}
	return p
}

func TestTotpKeyPrefersDirectEnvOverFile(t *testing.T) {
	t.Setenv("APP_ENV", "development")
	t.Setenv("TOTP_ENCRYPTION_KEY", testKeyB64)
	t.Setenv("TOTP_ENCRYPTION_KEY_FILE", writeTemp(t, strings.Repeat("ff", 32)+"\n"))
	k, err := TotpEncryptionKey()
	if err != nil || [32]byte(k) != expectedTestKey() {
		t.Fatalf("direct env wins: %v", err)
	}
}

func TestTotpKeyReadsFileWhenEnvUnset(t *testing.T) {
	t.Setenv("APP_ENV", "development")
	unset(t, "TOTP_ENCRYPTION_KEY")
	t.Setenv("TOTP_ENCRYPTION_KEY_FILE", writeTemp(t, testKeyB64+"\n"))
	k, err := TotpEncryptionKey()
	if err != nil || [32]byte(k) != expectedTestKey() {
		t.Fatalf("file fallback: %v", err)
	}
}

func TestTotpKeyEphemeralInDevAndRequiredInProduction(t *testing.T) {
	unset(t, "TOTP_ENCRYPTION_KEY", "TOTP_ENCRYPTION_KEY_FILE")
	t.Setenv("APP_ENV", "development")
	first, err1 := TotpEncryptionKey()
	second, err2 := TotpEncryptionKey()
	if err1 != nil || err2 != nil || first != second || first == (SecretKey{}) {
		t.Fatalf("ephemeral key must be stable and non-zero")
	}
	t.Setenv("APP_ENV", "production")
	if _, err := TotpEncryptionKey(); !errors.Is(err, ErrMissingTotpEncryptionKey) {
		t.Fatalf("prod requires key: %v", err)
	}
}

func TestTotpKeyFormattingNeverLeaksMaterial(t *testing.T) {
	t.Setenv("APP_ENV", "development")
	t.Setenv("TOTP_ENCRYPTION_KEY", testKeyB64)
	unset(t, "TOTP_ENCRYPTION_KEY_FILE")
	k, err := TotpEncryptionKey()
	if err != nil {
		t.Fatal(err)
	}
	for _, rendered := range []string{fmt.Sprintf("%v", k), fmt.Sprintf("%+v", k), fmt.Sprintf("%#v", k), fmt.Sprintf("%x", k.Bytes()[:0])} {
		if strings.Contains(rendered, "AAEC") || strings.Contains(rendered, "000102") || strings.Contains(rendered, "[0 1 2") {
			t.Fatalf("leaked key: %s", rendered)
		}
	}
	if strings.Contains(fmt.Sprintf("%v", ErrInvalidTotpEncryptionKey), "AAEC") {
		t.Fatal("error leaks key")
	}
}

func TestRequireAppOriginRejectsEmpty(t *testing.T) {
	t.Setenv("APP_ORIGIN", "  ")
	unset(t, "APP_ORIGIN_FILE")
	if _, err := RequireAppOrigin(); !errors.Is(err, ErrMissingAppOrigin) {
		t.Fatalf("got %v", err)
	}
}

func TestFormattingRedactsSecrets(t *testing.T) {
	cfg := &Config{
		Port:        4200,
		DatabaseURL: "postgres://user:s3cret@localhost/db",
		ValkeyURL:   "redis://:s3cret@localhost:6379",
		S3: &S3Config{
			Endpoint: "http://localhost:9000", Region: "us-east-1",
			AccessKeyID: "AKIAEXAMPLE", SecretAccessKey: "supersecret", Bucket: "lindaflor",
		},
	}
	for _, rendered := range []string{fmt.Sprintf("%v", cfg), fmt.Sprintf("%+v", cfg), fmt.Sprintf("%#v", cfg), fmt.Sprintf("%v", *cfg.S3)} {
		for _, secret := range []string{"s3cret", "supersecret", "AKIAEXAMPLE"} {
			if strings.Contains(rendered, secret) {
				t.Fatalf("leaked %s: %s", secret, rendered)
			}
		}
		if !strings.Contains(rendered, "***") || !strings.Contains(rendered, "lindaflor") {
			t.Fatalf("expected redaction marker and bucket: %s", rendered)
		}
	}
}

func TestPaymentConfigFormattingRedactsSecrets(t *testing.T) {
	cfg := PaymentConfig{Kind: ProviderMercadoPago, MercadoPago: &MercadoPagoConfig{
		AccessToken: "APP_USR-secret-token", WebhookSecret: "whsec-very-secret", BaseURL: MPDefaultBaseURL,
	}}
	for _, rendered := range []string{fmt.Sprintf("%v", cfg), fmt.Sprintf("%+v", cfg), fmt.Sprintf("%#v", cfg)} {
		if strings.Contains(rendered, "secret-token") || strings.Contains(rendered, "very-secret") ||
			!strings.Contains(rendered, "***") || !strings.Contains(rendered, "api.mercadopago.com") {
			t.Fatalf("bad rendering: %s", rendered)
		}
	}
}

func TestReadEnvOrFilePrefersDirectEnvOverFile(t *testing.T) {
	t.Setenv("LINDAFLOR_TEST_SECRET_XYZ", "direct-value")
	t.Setenv("LINDAFLOR_TEST_SECRET_XYZ_FILE", writeTemp(t, "file-value\n"))
	if v, ok := ReadEnvOrFile("LINDAFLOR_TEST_SECRET_XYZ"); !ok || v != "direct-value" {
		t.Fatalf("got %q %v", v, ok)
	}
}

func TestReadEnvOrFileReadsFileWhenEnvUnset(t *testing.T) {
	unset(t, "LINDAFLOR_TEST_SECRET2_XYZ")
	t.Setenv("LINDAFLOR_TEST_SECRET2_XYZ_FILE", writeTemp(t, "file-value\r\n"))
	if v, ok := ReadEnvOrFile("LINDAFLOR_TEST_SECRET2_XYZ"); !ok || v != "file-value" {
		t.Fatalf("got %q %v", v, ok)
	}
}

func TestWeakSeedPasswordRejectedInProductionOnly(t *testing.T) {
	t.Setenv("APP_ENV", "production")
	if RejectWeakSeedPasswordInProduction("admin123") == nil || RejectWeakSeedPasswordInProduction("short") == nil {
		t.Fatal("weak passwords must be rejected in production")
	}
	if err := RejectWeakSeedPasswordInProduction("a-strong-password-1"); err != nil {
		t.Fatal(err)
	}
	t.Setenv("APP_ENV", "development")
	if err := RejectWeakSeedPasswordInProduction("admin123"); err != nil {
		t.Fatal(err)
	}
}

func TestTrustedProxyEnvDefaultsAndParsing(t *testing.T) {
	unset(t, "TRUSTED_PROXY_HOPS", "TRUSTED_PROXY_NETWORKS")
	if TrustedProxyHops() != 0 || len(TrustedProxyNetworks()) != 0 {
		t.Fatal("defaults")
	}
	t.Setenv("TRUSTED_PROXY_HOPS", "2")
	if TrustedProxyHops() != 2 {
		t.Fatal("hops=2")
	}
	t.Setenv("TRUSTED_PROXY_HOPS", "abc")
	if TrustedProxyHops() != 0 {
		t.Fatal("bad hops")
	}
	t.Setenv("TRUSTED_PROXY_NETWORKS", "10.0.0.0/8, 192.168.0.0/16 ,,")
	if got := TrustedProxyNetworks(); !slices.Equal(got, []string{"10.0.0.0/8", "192.168.0.0/16"}) {
		t.Fatalf("got %v", got)
	}
}

func TestPGMaxConnectionsEnvAndDefaults(t *testing.T) {
	unset(t, "PG_MAX_CONN", "PG_MAX_CONN_FILE")
	t.Setenv("APP_ENV", "production")
	if PGMaxConnections() != 20 {
		t.Fatal("prod default")
	}
	t.Setenv("APP_ENV", "development")
	if PGMaxConnections() != 5 {
		t.Fatal("dev default")
	}
	t.Setenv("PG_MAX_CONN", "12")
	if PGMaxConnections() != 12 {
		t.Fatal("explicit")
	}
	t.Setenv("PG_MAX_CONN", "0")
	if PGMaxConnections() != 5 {
		t.Fatal("zero falls back")
	}
}

func TestPGTimeoutsUseDefaultsAndEnv(t *testing.T) {
	unset(t, "PG_ACQUIRE_TIMEOUT_SECS", "PG_IDLE_TIMEOUT_SECS")
	if PGAcquireTimeoutSecs() != 5 || PGIdleTimeoutSecs() != 600 {
		t.Fatal("defaults")
	}
	t.Setenv("PG_ACQUIRE_TIMEOUT_SECS", "9")
	t.Setenv("PG_IDLE_TIMEOUT_SECS", "30")
	if PGAcquireTimeoutSecs() != 9 || PGIdleTimeoutSecs() != 30 {
		t.Fatal("explicit")
	}
}

func TestMigrateOnBootDefaultsOffInProduction(t *testing.T) {
	unset(t, "MIGRATE_ON_BOOT", "MIGRATE_ON_BOOT_FILE")
	t.Setenv("APP_ENV", "production")
	if MigrateOnBoot() {
		t.Fatal("prod default off")
	}
	t.Setenv("APP_ENV", "development")
	if !MigrateOnBoot() {
		t.Fatal("dev default on")
	}
	t.Setenv("MIGRATE_ON_BOOT", "0")
	if MigrateOnBoot() {
		t.Fatal("explicit off")
	}
	t.Setenv("MIGRATE_ON_BOOT", "true")
	if !MigrateOnBoot() {
		t.Fatal("explicit on")
	}
}

func TestRateLimitEnvOverrides(t *testing.T) {
	unset(t, "RATE_LIMIT_AUTH_SIGN_IN_PER_MIN", "RATE_LIMIT_CHECKOUT_ORDER_PER_WINDOW")
	if RateLimitFor("auth:sign-in", 5) != 5 {
		t.Fatal("default")
	}
	t.Setenv("RATE_LIMIT_AUTH_SIGN_IN_PER_MIN", "42")
	if RateLimitFor("auth:sign-in", 5) != 42 {
		t.Fatal("override")
	}
	t.Setenv("RATE_LIMIT_CHECKOUT_ORDER_PER_WINDOW", "0")
	if WindowRateLimitFor("checkout:order", 5) != 5 {
		t.Fatal("zero falls back")
	}
}

func TestSnapshotDurations(t *testing.T) {
	t.Setenv("APP_ENV", "development")
	t.Setenv("DATABASE_URL", "postgres://localhost/test")
	t.Setenv("RESERVATION_SWEEP_SECS", "0")
	t.Setenv("ABANDONED_CART_AFTER_MINUTES", "-3")
	cfg, err := loadCore()
	if err != nil {
		t.Fatal(err)
	}
	if cfg.ReservationSweep != time.Second || cfg.AbandonedCartAfter != time.Minute {
		t.Fatalf("minimums not applied: %v %v", cfg.ReservationSweep, cfg.AbandonedCartAfter)
	}
}
