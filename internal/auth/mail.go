package auth

import (
	"context"
	"fmt"
	"html"
	"log/slog"
	"net/url"
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/mail"
)

// Per-recipient cap of the auth mails anyone can trigger for an arbitrary
// address (verify, reset, change-email): at most this many per window.
const (
	RecipientMaxPerWindow   = 5
	RecipientWindowSeconds  = 3600
	devOriginForMailLinks   = "http://localhost:4200"
	brand                   = "Lindaflor"
	ignoreNote              = "Se você não fez esta solicitação, ignore este e-mail com segurança."
	deleteAccountIgnoreNote = "Se você não pediu a exclusão, ignore este e-mail e considere trocar sua senha."
)

// RecipientThrottledKinds are capped per recipient (mail.Kind* constants).
var RecipientThrottledKinds = []string{mail.KindVerifyEmail, mail.KindPasswordReset, mail.KindChangeEmail}

// MailRenderer renders the four auth mails. The default (DefaultMailRenderer)
// is a port of the auth part of src/mail/templates.rs; the platform package
// may install its shared renderer instead (auth.Mail = …) as long as the
// copy, subjects and links stay the same.
type MailRenderer interface {
	VerifyEmail(name, link, firstPurchaseCode string) mail.Msg
	PasswordReset(name, link string) mail.Msg
	ChangeEmail(name, newEmail, link string) mail.Msg
	DeleteAccount(name, link string) mail.Msg
}

// Mail is the renderer used by the auth flows.
var Mail MailRenderer = DefaultMailRenderer{}

// TokenLink is the absolute link to path?token=<token> on origin (dev
// fallback http://localhost:4200).
func TokenLink(origin, path, token string) string {
	if origin == "" {
		origin = devOriginForMailLinks
	}
	return origin + path + "?token=" + url.QueryEscape(token)
}

func enqueueThrottled(ctx context.Context, db sqlc.DBTX, kind, to string, m mail.Msg) error {
	id, err := uuid.NewV7()
	if err != nil {
		return err
	}
	to = strings.TrimSpace(to)
	n, err := sqlc.New(db).AuthEnqueueMailThrottled(ctx, sqlc.AuthEnqueueMailThrottledParams{
		ID: id, Kind: kind, ToAddress: to, Subject: m.Subject, HtmlBody: m.HTML, TextBody: m.Text,
		Kinds: RecipientThrottledKinds, WindowSecs: RecipientWindowSeconds, MaxPerWindow: RecipientMaxPerWindow,
	})
	if err != nil {
		return fmt.Errorf("enqueue %s mail: %w", kind, err)
	}
	if n == 0 {
		slog.InfoContext(ctx, "auth mail dropped: per-recipient limit reached", "kind", kind,
			"max", RecipientMaxPerWindow, "window_secs", RecipientWindowSeconds)
	}
	return nil
}

func (s *Service) sendVerifyEmail(ctx context.Context, db sqlc.DBTX, to, name, token string) error {
	code := ""
	if c, err := sqlc.New(db).AuthFirstPurchaseCoupon(ctx); err == nil && c != nil {
		code = strings.TrimSpace(*c)
	}
	m := Mail.VerifyEmail(name, TokenLink(s.Cfg.AppOrigin, VerifyEmailPath, token), code)
	return enqueueThrottled(ctx, db, mail.KindVerifyEmail, to, m)
}

func (s *Service) sendPasswordReset(ctx context.Context, db sqlc.DBTX, to, name, token string) error {
	m := Mail.PasswordReset(name, TokenLink(s.Cfg.AppOrigin, ResetPasswordPath, token))
	return enqueueThrottled(ctx, db, mail.KindPasswordReset, to, m)
}

func (s *Service) sendChangeEmail(ctx context.Context, db sqlc.DBTX, newEmail, name, token string) error {
	m := Mail.ChangeEmail(name, newEmail, TokenLink(s.Cfg.AppOrigin, ChangeEmailPath, token))
	return enqueueThrottled(ctx, db, mail.KindChangeEmail, newEmail, m)
}

func (s *Service) sendDeleteAccount(ctx context.Context, db sqlc.DBTX, to, name, token string) error {
	m := Mail.DeleteAccount(name, TokenLink(s.Cfg.AppOrigin, DeleteAccountPath, token))
	m.Kind, m.To = mail.KindDeleteAccount, to
	return mail.Enqueue(ctx, db, m)
}

// DefaultMailRenderer renders the auth mails (minimal inline-styled HTML +
// plain text from the same data; every value escaped).
type DefaultMailRenderer struct{}

type mailBody struct {
	heading     string
	paragraphs  []string
	actionLabel string
	actionURL   string
	footnote    string
}

func greeting(name string) string {
	if n := strings.TrimSpace(name); n != "" {
		return "Olá, " + n + "!"
	}
	return "Olá!"
}

func renderMail(subject string, b mailBody) mail.Msg {
	e := html.EscapeString
	var h strings.Builder
	h.WriteString(`<!doctype html><html lang="pt-BR"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">`)
	fmt.Fprintf(&h, "<title>%s</title></head>", e(subject))
	h.WriteString(`<body style="margin:0;padding:0;background:#f6f1ee;font-family:Helvetica,Arial,sans-serif;color:#2b2b2b;"><table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#f6f1ee;padding:24px 0;"><tr><td align="center"><table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="max-width:560px;background:#ffffff;border-radius:12px;padding:32px;"><tr><td>`)
	fmt.Fprintf(&h, `<p style="margin:0 0 24px;font-size:22px;font-weight:bold;color:#b4536b;">%s</p>`, brand)
	fmt.Fprintf(&h, `<h1 style="margin:0 0 16px;font-size:20px;">%s</h1>`, e(b.heading))
	for _, p := range b.paragraphs {
		fmt.Fprintf(&h, `<p style="margin:0 0 16px;font-size:15px;line-height:1.5;">%s</p>`, e(p))
	}
	if b.actionURL != "" {
		u, l := e(b.actionURL), e(b.actionLabel)
		fmt.Fprintf(&h, `<p style="margin:24px 0;"><a href="%s" style="display:inline-block;background:#b4536b;color:#ffffff;text-decoration:none;font-weight:bold;padding:12px 24px;border-radius:8px;">%s</a></p><p style="margin:0 0 16px;font-size:12px;color:#666;">Se o botão não funcionar, copie e cole este endereço no navegador:<br><a href="%s" style="color:#b4536b;word-break:break-all;">%s</a></p>`, u, l, u, u)
	}
	if b.footnote != "" {
		fmt.Fprintf(&h, `<p style="margin:16px 0 0;font-size:12px;color:#666;">%s</p>`, e(b.footnote))
	}
	fmt.Fprintf(&h, `</td></tr></table><p style="font-size:11px;color:#999;">%s · Esta é uma mensagem automática, não responda.</p></td></tr></table></body></html>`, brand)

	var t strings.Builder
	fmt.Fprintf(&t, "%s\n\n%s\n\n", brand, b.heading)
	for _, p := range b.paragraphs {
		t.WriteString(p + "\n\n")
	}
	if b.actionURL != "" {
		fmt.Fprintf(&t, "%s: %s\n\n", b.actionLabel, b.actionURL)
	}
	if b.footnote != "" {
		t.WriteString(b.footnote + "\n\n")
	}
	fmt.Fprintf(&t, "-- \n%s · Esta é uma mensagem automática, não responda.\n", brand)
	return mail.Msg{Subject: subject, HTML: h.String(), Text: t.String()}
}

// VerifyEmail also advertises the first-purchase coupon when configured.
func (DefaultMailRenderer) VerifyEmail(name, link, firstPurchaseCode string) mail.Msg {
	ps := []string{greeting(name), "Para ativar sua conta, confirme seu endereço de e-mail clicando no botão abaixo. O link vale por 24 horas."}
	if c := strings.TrimSpace(firstPurchaseCode); c != "" {
		ps = append(ps, "Presente de boas-vindas: use o cupom "+c+" na sua primeira compra.")
	}
	return renderMail("Confirme seu e-mail · "+brand, mailBody{
		heading: "Confirme seu e-mail", paragraphs: ps,
		actionLabel: "Confirmar e-mail", actionURL: link, footnote: ignoreNote,
	})
}

// PasswordReset is the reset link mail (1h, single use).
func (DefaultMailRenderer) PasswordReset(name, link string) mail.Msg {
	return renderMail("Redefinição de senha · "+brand, mailBody{
		heading:     "Redefina sua senha",
		paragraphs:  []string{greeting(name), "Recebemos um pedido para redefinir a senha da sua conta. O link abaixo vale por 1 hora e só pode ser usado uma vez."},
		actionLabel: "Redefinir senha", actionURL: link, footnote: ignoreNote,
	})
}

// ChangeEmail goes to the new address.
func (DefaultMailRenderer) ChangeEmail(name, newEmail, link string) mail.Msg {
	return renderMail("Confirme seu novo e-mail · "+brand, mailBody{
		heading:     "Confirme a troca de e-mail",
		paragraphs:  []string{greeting(name), "Recebemos um pedido para alterar o e-mail da sua conta para " + newEmail + ". Confirme pelo botão abaixo; o link vale por 30 minutos."},
		actionLabel: "Confirmar novo e-mail", actionURL: link, footnote: ignoreNote,
	})
}

// DeleteAccount confirms an account deletion request.
func (DefaultMailRenderer) DeleteAccount(name, link string) mail.Msg {
	return renderMail("Confirme a exclusão da sua conta · "+brand, mailBody{
		heading:     "Excluir sua conta",
		paragraphs:  []string{greeting(name), "Recebemos um pedido para excluir sua conta. Essa ação é permanente: seus dados de acesso serão apagados. O link abaixo vale por 30 minutos."},
		actionLabel: "Confirmar exclusão", actionURL: link, footnote: deleteAccountIgnoreNote,
	})
}
