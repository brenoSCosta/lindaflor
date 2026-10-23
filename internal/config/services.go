package config

import (
	"bufio"
	"errors"
	"fmt"
	"os"
	"strconv"
	"strings"
)

// ---------------------------------------------------------------------------
// Mail
// ---------------------------------------------------------------------------

// MailTransportKind selects how outgoing email leaves the process.
type MailTransportKind string

// Mail transports.
const (
	MailTransportLog  MailTransportKind = "log"
	MailTransportSMTP MailTransportKind = "smtp"
)

// SMTPTLS is the SMTP_TLS mode.
type SMTPTLS string

// SMTP TLS modes.
const (
	SMTPStartTLS SMTPTLS = "starttls" // port 587, STARTTLS required
	SMTPTLSOnly  SMTPTLS = "tls"      // implicit TLS, port 465
	SMTPNoTLS    SMTPTLS = "none"     // dev only (mailpit)
)

// SMTPConfig holds the relay settings.
type SMTPConfig struct {
	Host     string
	Port     int
	TLS      SMTPTLS
	Username string
	Password string
}

// String redacts the password.
func (c SMTPConfig) String() string {
	pw := ""
	if c.Password != "" {
		pw = "***"
	}
	return fmt.Sprintf("SMTPConfig{Host:%s Port:%d TLS:%s Username:%s Password:%s}", c.Host, c.Port, c.TLS, c.Username, pw)
}

// GoString keeps %#v redacted too.
func (c SMTPConfig) GoString() string { return c.String() }

// MailConfig is the outgoing mail setup.
type MailConfig struct {
	Transport MailTransportKind
	SMTP      *SMTPConfig // set when Transport == smtp
	From      string
}

// String redacts the SMTP password.
func (c MailConfig) String() string {
	smtp := "<nil>"
	if c.SMTP != nil {
		smtp = c.SMTP.String()
	}
	return fmt.Sprintf("MailConfig{Transport:%s SMTP:%s From:%s}", c.Transport, smtp, c.From)
}

// GoString keeps %#v redacted too.
func (c MailConfig) GoString() string { return c.String() }

// MailFromEnv reads MAIL_TRANSPORT (log|smtp, default log), SMTP_HOST,
// SMTP_PORT, SMTP_TLS (starttls|tls|none), SMTP_USERNAME, SMTP_PASSWORD,
// MAIL_FROM. Production refuses the log transport unless
// MAIL_ALLOW_LOG_IN_PRODUCTION=1, refuses SMTP_TLS=none and requires MAIL_FROM.
func MailFromEnv() (MailConfig, error) {
	production := IsProduction()
	raw := "log"
	if v, ok := ReadEnvOrFile("MAIL_TRANSPORT"); ok {
		raw = v
	}
	raw = strings.ToLower(strings.TrimSpace(raw))
	var cfg MailConfig
	switch raw {
	case "log":
		if production && !EnvFlag("MAIL_ALLOW_LOG_IN_PRODUCTION") {
			return cfg, ErrMailLogTransportInProduction
		}
		cfg.Transport = MailTransportLog
	case "smtp":
		host := envNonEmpty("SMTP_HOST")
		if host == "" {
			return cfg, &MissingVarError{Key: "SMTP_HOST"}
		}
		tlsRaw := "starttls"
		if v, ok := ReadEnvOrFile("SMTP_TLS"); ok {
			tlsRaw = v
		}
		var tls SMTPTLS
		switch strings.ToLower(strings.TrimSpace(tlsRaw)) {
		case "starttls":
			tls = SMTPStartTLS
		case "tls", "implicit":
			tls = SMTPTLSOnly
		case "none", "off":
			if production {
				return cfg, ErrInvalidSMTPTLS
			}
			tls = SMTPNoTLS
		default:
			return cfg, ErrInvalidSMTPTLS
		}
		port := map[SMTPTLS]int{SMTPStartTLS: 587, SMTPTLSOnly: 465, SMTPNoTLS: 1025}[tls]
		if v, ok := ReadEnvOrFile("SMTP_PORT"); ok {
			p, err := strconv.ParseUint(strings.TrimSpace(v), 10, 16)
			if err != nil {
				return cfg, ErrInvalidPort
			}
			port = int(p)
		}
		cfg.Transport = MailTransportSMTP
		cfg.SMTP = &SMTPConfig{
			Host: host, Port: port, TLS: tls,
			Username: envNonEmpty("SMTP_USERNAME"),
			Password: envNonEmpty("SMTP_PASSWORD"),
		}
	default:
		return cfg, &InvalidMailTransportError{Raw: raw}
	}
	cfg.From = envNonEmpty("MAIL_FROM")
	if cfg.From == "" {
		if production {
			return cfg, &MissingVarError{Key: "MAIL_FROM"}
		}
		cfg.From = DefaultMailFrom
	}
	return cfg, nil
}

// ---------------------------------------------------------------------------
// Payments
// ---------------------------------------------------------------------------

// MPDefaultBaseURL is the Mercado Pago API origin (MP_API_BASE_URL overrides).
const MPDefaultBaseURL = "https://api.mercadopago.com"

// ProviderKind is the payment strategy for new orders (PAYMENT_PROVIDER).
type ProviderKind string

// Payment providers.
const (
	ProviderStaticPix   ProviderKind = "static_pix"
	ProviderMercadoPago ProviderKind = "mercadopago"
)

// MercadoPagoConfig holds the MP credentials.
type MercadoPagoConfig struct {
	AccessToken     string
	WebhookSecret   string
	BaseURL         string
	NotificationURL string // "<APP_ORIGIN>/api/webhooks/mercadopago" or ""
}

// String redacts the secrets.
func (c MercadoPagoConfig) String() string {
	return fmt.Sprintf("MercadoPagoConfig{AccessToken:*** WebhookSecret:*** BaseURL:%s NotificationURL:%s}", c.BaseURL, c.NotificationURL)
}

// GoString keeps %#v redacted too.
func (c MercadoPagoConfig) GoString() string { return c.String() }

// PaymentConfig selects and configures the payment provider.
type PaymentConfig struct {
	Kind        ProviderKind
	MercadoPago *MercadoPagoConfig
}

// String redacts nested secrets.
func (c PaymentConfig) String() string {
	mp := "<nil>"
	if c.MercadoPago != nil {
		mp = c.MercadoPago.String()
	}
	return fmt.Sprintf("PaymentConfig{Kind:%s MercadoPago:%s}", c.Kind, mp)
}

// GoString keeps %#v redacted too.
func (c PaymentConfig) GoString() string { return c.String() }

// Payment configuration errors.
var (
	ErrStaticPixInProduction = errors.New("refusing to boot in production with the static PIX provider: set PAYMENT_PROVIDER=mercadopago, or ALLOW_STATIC_PIX=1 to accept manual confirmation")
	ErrInsecureMPBaseURL     = errors.New("MP_API_BASE_URL must be an https:// URL in production")
)

// InvalidProviderError reports an unknown PAYMENT_PROVIDER.
type InvalidProviderError struct{ Raw string }

func (e *InvalidProviderError) Error() string {
	return fmt.Sprintf("invalid PAYMENT_PROVIDER %q: expected static_pix or mercadopago", e.Raw)
}

// PaymentMissingVarError reports a missing MP secret.
type PaymentMissingVarError struct{ Key string }

func (e *PaymentMissingVarError) Error() string {
	return fmt.Sprintf("PAYMENT_PROVIDER=mercadopago requires %s (or %s_FILE) to be set", e.Key, e.Key)
}

// PaymentsFromEnv reads PAYMENT_PROVIDER, MP_ACCESS_TOKEN, MP_WEBHOOK_SECRET,
// MP_API_BASE_URL and ALLOW_STATIC_PIX. Unset PAYMENT_PROVIDER means
// mercadopago when MP_ACCESS_TOKEN is present, else static_pix. Production:
// MP needs both secrets and an https base URL; static PIX needs ALLOW_STATIC_PIX=1.
func PaymentsFromEnv() (PaymentConfig, error) {
	production := IsProduction()
	token, hasToken := ReadEnvOrFile("MP_ACCESS_TOKEN")
	kind := ProviderStaticPix
	if raw, ok := ReadEnvOrFile("PAYMENT_PROVIDER"); ok {
		switch strings.ToLower(strings.TrimSpace(raw)) {
		case "static_pix", "static", "pix", "manual_pix":
			kind = ProviderStaticPix
		case "mercadopago", "mercado_pago", "mp":
			kind = ProviderMercadoPago
		default:
			return PaymentConfig{}, &InvalidProviderError{Raw: raw}
		}
	} else if hasToken {
		kind = ProviderMercadoPago
	}
	if kind == ProviderStaticPix {
		if production && !EnvFlag("ALLOW_STATIC_PIX") {
			return PaymentConfig{}, ErrStaticPixInProduction
		}
		return PaymentConfig{Kind: ProviderStaticPix}, nil
	}
	if !hasToken {
		return PaymentConfig{}, &PaymentMissingVarError{Key: "MP_ACCESS_TOKEN"}
	}
	secret, ok := ReadEnvOrFile("MP_WEBHOOK_SECRET")
	if !ok {
		return PaymentConfig{}, &PaymentMissingVarError{Key: "MP_WEBHOOK_SECRET"}
	}
	base := MPDefaultBaseURL
	if v, ok := ReadEnvOrFile("MP_API_BASE_URL"); ok {
		base = strings.TrimRight(strings.TrimSpace(v), "/")
	}
	if production && !strings.HasPrefix(base, "https://") {
		return PaymentConfig{}, ErrInsecureMPBaseURL
	}
	notify := ""
	if o := AppOrigin(); o != "" {
		notify = o + "/api/webhooks/mercadopago"
	}
	return PaymentConfig{Kind: kind, MercadoPago: &MercadoPagoConfig{
		AccessToken: token, WebhookSecret: secret, BaseURL: base, NotificationURL: notify,
	}}, nil
}

// ---------------------------------------------------------------------------
// Google OAuth, CAPTCHA
// ---------------------------------------------------------------------------

// GoogleConfig holds the OAuth client (GOOGLE_CLIENT_ID / _SECRET, trimmed).
type GoogleConfig struct {
	ClientID     string
	ClientSecret string
}

// Enabled reports whether both credentials are present.
func (g GoogleConfig) Enabled() bool { return g.ClientID != "" && g.ClientSecret != "" }

// String redacts the secret.
func (g GoogleConfig) String() string {
	return "GoogleConfig{ClientID:" + g.ClientID + " ClientSecret:***}"
}

// GoString keeps %#v redacted too.
func (g GoogleConfig) GoString() string { return g.String() }

// GoogleFromEnv reads GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET (+ _FILE).
func GoogleFromEnv() GoogleConfig {
	return GoogleConfig{
		ClientID:     strings.TrimSpace(envNonEmpty("GOOGLE_CLIENT_ID")),
		ClientSecret: strings.TrimSpace(envNonEmpty("GOOGLE_CLIENT_SECRET")),
	}
}

// GoogleOAuthTestBaseURL is GOOGLE_OAUTH_TEST_BASE_URL (trailing slash
// trimmed), honored only when APP_ENV is development/dev/test so
// integration tests can stub Google's token/userinfo endpoints.
func GoogleOAuthTestBaseURL() string {
	if !IsDevEnv(AppEnv()) {
		return ""
	}
	return strings.TrimRight(strings.TrimSpace(os.Getenv("GOOGLE_OAUTH_TEST_BASE_URL")), "/")
}

// CaptchaConfig is the Turnstile gate (TURNSTILE_SITE_KEY,
// TURNSTILE_SECRET_KEY, TURNSTILE_ENFORCE=1|true).
type CaptchaConfig struct {
	SiteKey          string
	SecretConfigured bool
	Enforced         bool
}

// Required reports whether a CAPTCHA token must be presented.
func (c CaptchaConfig) Required() bool { return c.Enforced && c.SecretConfigured }

// CaptchaFromEnv reads the Turnstile variables (plain env, like Rust).
func CaptchaFromEnv() CaptchaConfig {
	enforce := os.Getenv("TURNSTILE_ENFORCE")
	return CaptchaConfig{
		SiteKey:          strings.TrimSpace(os.Getenv("TURNSTILE_SITE_KEY")),
		SecretConfigured: strings.TrimSpace(os.Getenv("TURNSTILE_SECRET_KEY")) != "",
		Enforced:         enforce == "1" || strings.EqualFold(enforce, "true"),
	}
}

// ---------------------------------------------------------------------------
// Seed / create-admin
// ---------------------------------------------------------------------------

// SeedAdmin holds SEED_ADMIN_EMAIL / SEED_ADMIN_PASSWORD / SEED_ADMIN_NAME
// (read by cmd/seed only; the web process never reads SEED_*).
type SeedAdmin struct {
	Email    string
	Password string
	Name     string
}

// SeedAdminFromEnv reads the seed admin credentials. Defaults:
// admin@lindaflor.com / "Admin User"; the password defaults to the
// documented dev value "admin123" outside production and is required (and
// must be strong) in production.
func SeedAdminFromEnv() (SeedAdmin, error) {
	s := SeedAdmin{
		Email:    os.Getenv("SEED_ADMIN_EMAIL"),
		Password: os.Getenv("SEED_ADMIN_PASSWORD"),
		Name:     os.Getenv("SEED_ADMIN_NAME"),
	}
	if s.Email == "" {
		s.Email = "admin@lindaflor.com"
	}
	if s.Name == "" {
		s.Name = "Admin User"
	}
	if s.Password == "" {
		if IsProduction() {
			return s, errors.New("SEED_ADMIN_PASSWORD must be set to a strong (12+ char) value when APP_ENV=production")
		}
		s.Password = "admin123"
	}
	if err := RejectWeakSeedPasswordInProduction(s.Password); err != nil {
		return s, err
	}
	return s, nil
}

// ---------------------------------------------------------------------------
// .env loading (dev only)
// ---------------------------------------------------------------------------

// loadDotEnv sets KEY=VALUE lines from path for keys not already set.
// Missing file is fine. Supports comments, `export `, and quoted values.
func loadDotEnv(path string) {
	f, err := os.Open(path) //nolint:gosec // fixed dev-only path
	if err != nil {
		return
	}
	defer func() { _ = f.Close() }()
	sc := bufio.NewScanner(f)
	for sc.Scan() {
		line := strings.TrimSpace(sc.Text())
		if line == "" || strings.HasPrefix(line, "#") {
			continue
		}
		line = strings.TrimPrefix(line, "export ")
		k, v, ok := strings.Cut(line, "=")
		if !ok {
			continue
		}
		k = strings.TrimSpace(k)
		v = strings.TrimSpace(v)
		if len(v) >= 2 && (v[0] == '"' || v[0] == '\'') && v[len(v)-1] == v[0] {
			v = v[1 : len(v)-1]
		}
		if _, exists := os.LookupEnv(k); !exists {
			_ = os.Setenv(k, v)
		}
	}
}
