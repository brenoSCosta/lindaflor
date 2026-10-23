package mail

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	netmail "net/mail"
	"time"

	gomail "github.com/wneessen/go-mail"

	"github.com/wellington-vell/lindaflor/internal/config"
)

// SMTPTimeout bounds every SMTP dial/command.
const SMTPTimeout = 20 * time.Second

// Transport errors.
var (
	ErrAddress = errors.New("invalid address")
	ErrBuild   = errors.New("could not build message")
	ErrSMTP    = errors.New("smtp")
)

// LogMailer logs a one-line summary instead of sending (dev/test default).
// Bodies carry one-time links: they are logged at debug level only, and
// never in production.
type LogMailer struct {
	Production bool
}

// Kind implements Mailer.
func (LogMailer) Kind() string { return "log" }

// Send implements Mailer.
func (l LogMailer) Send(ctx context.Context, msg Msg) error {
	slog.InfoContext(ctx, "mail (log transport, not sent)",
		"to", RedactAddress(msg.To), "subject", msg.Subject, "list_unsubscribe", msg.ListUnsubscribe != "")
	if !l.Production {
		slog.DebugContext(ctx, "mail body", "to", msg.To, "body", msg.Text)
	}
	return nil
}

// SMTPMailer delivers through an SMTP relay (multipart/alternative).
type SMTPMailer struct {
	from string
	opts []gomail.Option
	host string
}

// Kind implements Mailer.
func (*SMTPMailer) Kind() string { return "smtp" }

// FromConfig builds the configured transport. production picks the
// log-transport redaction level.
func FromConfig(cfg config.MailConfig, production bool) (Mailer, error) {
	if cfg.Transport != config.MailTransportSMTP || cfg.SMTP == nil {
		return LogMailer{Production: production}, nil
	}
	from, err := netmail.ParseAddress(cfg.From)
	if err != nil {
		return nil, fmt.Errorf("%w: MAIL_FROM: %w", ErrAddress, err)
	}
	s := cfg.SMTP
	opts := []gomail.Option{gomail.WithPort(s.Port), gomail.WithTimeout(SMTPTimeout)}
	switch s.TLS {
	case config.SMTPTLSOnly:
		opts = append(opts, gomail.WithSSL())
	case config.SMTPNoTLS:
		opts = append(opts, gomail.WithTLSPolicy(gomail.NoTLS))
	default:
		opts = append(opts, gomail.WithTLSPolicy(gomail.TLSMandatory))
	}
	if s.Username != "" {
		auth := gomail.SMTPAuthPlain
		if s.TLS == config.SMTPNoTLS {
			auth = gomail.SMTPAuthPlainNoEnc
		}
		opts = append(opts, gomail.WithSMTPAuth(auth), gomail.WithUsername(s.Username), gomail.WithPassword(s.Password))
	}
	// Validate the options once at boot.
	if _, err := gomail.NewClient(s.Host, opts...); err != nil {
		return nil, fmt.Errorf("%w: %w", ErrSMTP, err)
	}
	return &SMTPMailer{from: from.String(), opts: opts, host: s.Host}, nil
}

// BuildMessage renders msg as a multipart/alternative message from from,
// with RFC 2369/8058 one-click unsubscribe headers when msg.ListUnsubscribe
// is set.
func BuildMessage(from string, msg Msg) (*gomail.Msg, error) {
	m := gomail.NewMsg()
	if err := m.From(from); err != nil {
		return nil, fmt.Errorf("%w: %w", ErrAddress, err)
	}
	if err := m.To(msg.To); err != nil {
		return nil, fmt.Errorf("%w: %w", ErrAddress, err)
	}
	m.Subject(msg.Subject)
	for name, value := range ListUnsubscribeHeaders(msg.ListUnsubscribe) {
		m.SetGenHeaderPreformatted(gomail.Header(name), value)
	}
	m.SetBodyString(gomail.TypeTextPlain, msg.Text)
	m.AddAlternativeString(gomail.TypeTextHTML, msg.HTML)
	return m, nil
}

// ListUnsubscribeHeaders are the RFC 2369 / RFC 8058 one-click headers for
// url (none without one). The URL is ASCII (the address is percent-encoded).
func ListUnsubscribeHeaders(url string) map[string]string {
	if url == "" {
		return nil
	}
	return map[string]string{
		"List-Unsubscribe":      "<" + url + ">",
		"List-Unsubscribe-Post": "List-Unsubscribe=One-Click",
	}
}

// Send implements Mailer. A fresh connection per message keeps the worker
// simple (batches are small and spaced by the poll interval).
func (s *SMTPMailer) Send(ctx context.Context, msg Msg) error {
	m, err := BuildMessage(s.from, msg)
	if err != nil {
		return err
	}
	client, err := gomail.NewClient(s.host, s.opts...)
	if err != nil {
		return fmt.Errorf("%w: %w", ErrSMTP, err)
	}
	if err := client.DialAndSendWithContext(ctx, m); err != nil {
		return fmt.Errorf("%w: %w", ErrSMTP, err)
	}
	return nil
}
