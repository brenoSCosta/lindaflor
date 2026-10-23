package mail_test

import (
	"bytes"
	"context"
	"errors"
	"strings"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/mail"
)

func TestRedactAddressHidesLocalPart(t *testing.T) {
	if got := mail.RedactAddress("ana@example.com"); got != "a***@example.com" {
		t.Fatal(got)
	}
	if got := mail.RedactAddress("nope"); got != "***" {
		t.Fatal(got)
	}
}

func TestEncodeQueryValueEscapesReserved(t *testing.T) {
	if got := mail.EncodeQueryValue("abc-_.~"); got != "abc-_.~" {
		t.Fatal(got)
	}
	if got := mail.EncodeQueryValue("a b&c=d"); got != "a%20b%26c%3Dd" {
		t.Fatal(got)
	}
}

func TestLogMailerNeverFails(t *testing.T) {
	e := mail.PasswordReset("Ana", "http://x/reset?token=t")
	if err := (mail.LogMailer{}).Send(context.Background(), e.Msg(mail.KindPasswordReset, "ana@example.com", "")); err != nil {
		t.Fatal(err)
	}
}

func TestListUnsubscribeHeadersAreOneClick(t *testing.T) {
	if len(mail.ListUnsubscribeHeaders("")) != 0 {
		t.Fatal("no url → no headers")
	}
	url := "https://loja.example.com/newsletter/cancelar?email=a%40b.com&token=ff"
	m, err := mail.BuildMessage("Loja <loja@example.com>", mail.Msg{To: "ana@example.com", Subject: "x", Text: "oi", HTML: "<p>oi</p>", ListUnsubscribe: url})
	if err != nil {
		t.Fatal(err)
	}
	var buf bytes.Buffer
	if _, err := m.WriteTo(&buf); err != nil {
		t.Fatal(err)
	}
	raw := buf.String()
	mustContain(t, raw, "List-Unsubscribe: <"+url+">", "List-Unsubscribe-Post: List-Unsubscribe=One-Click",
		"multipart/alternative", "text/plain", "text/html")
	// Transactional mail carries no unsubscribe header.
	m, err = mail.BuildMessage("Loja <loja@example.com>", mail.Msg{To: "ana@example.com", Subject: "x", Text: "oi", HTML: "oi"})
	if err != nil {
		t.Fatal(err)
	}
	buf.Reset()
	if _, err := m.WriteTo(&buf); err != nil {
		t.Fatal(err)
	}
	if strings.Contains(buf.String(), "List-Unsubscribe") {
		t.Fatal(buf.String())
	}
}

func TestSMTPMailerBuildsFromConfig(t *testing.T) {
	cfg := config.MailConfig{
		Transport: config.MailTransportSMTP,
		SMTP:      &config.SMTPConfig{Host: "127.0.0.1", Port: 1025, TLS: config.SMTPNoTLS},
		From:      "Lindaflor <loja@example.com>",
	}
	m, err := mail.FromConfig(cfg, false)
	if err != nil || m.Kind() != "smtp" {
		t.Fatalf("%v %v", m, err)
	}
	for _, tls := range []config.SMTPTLS{config.SMTPStartTLS, config.SMTPTLSOnly} {
		c := cfg
		c.SMTP = &config.SMTPConfig{Host: "smtp.example.com", Port: 587, TLS: tls, Username: "u", Password: "p"}
		if _, err := mail.FromConfig(c, true); err != nil {
			t.Fatal(tls, err)
		}
	}
	bad := cfg
	bad.From = "not an address"
	if _, err := mail.FromConfig(bad, false); !errors.Is(err, mail.ErrAddress) {
		t.Fatalf("want address error, got %v", err)
	}
	logm, err := mail.FromConfig(config.MailConfig{Transport: config.MailTransportLog}, false)
	if err != nil || logm.Kind() != "log" {
		t.Fatal(logm, err)
	}
}
