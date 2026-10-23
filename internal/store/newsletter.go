package store

import (
	"context"
	"errors"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/mail"
)

// Newsletter (src/app/newsletter.rs, src/mail/newsletter.rs and
// src/app/admin/newsletter.rs): subscribe with a welcome mail, signed
// one-click unsubscribe (also a suppression list for other marketing
// mail), admin list and CSV export.

// Newsletter limits.
const (
	NewsletterNameMax = 100
	// NewsletterPerMin is the per-IP subscribe budget (scope "newsletter").
	NewsletterPerMin = 5
	ScopeNewsletter  = "newsletter"
)

// NewsletterOutcome is the result of SubscribeNewsletter.
type NewsletterOutcome struct {
	Kind              NewsletterOutcomeKind
	FirstPurchaseCode string // Subscribed only
}

// NewsletterOutcomeKind classifies a subscribe.
type NewsletterOutcomeKind int

// Kinds.
const (
	NewsletterSubscribed NewsletterOutcomeKind = iota
	NewsletterResubscribed
	NewsletterAlreadySubscribed
)

// Toast is the pt-BR toast (ok=false for the error toast).
func (o NewsletterOutcome) Toast() (string, bool) {
	switch o.Kind {
	case NewsletterSubscribed:
		return SubscribedMessage(o.FirstPurchaseCode), true
	case NewsletterResubscribed:
		return "Inscrição reativada! Você voltará a receber nossas novidades.", true
	}
	return "E-mail já cadastrado na newsletter", false
}

// SubscribedMessage advertises the first-purchase coupon when configured.
func SubscribedMessage(firstPurchaseCode string) string {
	if code := strings.TrimSpace(firstPurchaseCode); code != "" {
		return "Inscrição realizada! Use o cupom " + code + " na sua primeira compra."
	}
	return "Inscrição realizada! Você vai receber nossas novidades por e-mail."
}

// ValidateNewsletterForm validates the subscribe form: msg is the pt-BR
// error toast ("" when valid, with the normalized email and capped name).
func ValidateNewsletterForm(rawEmail, rawName string) (email string, name *string, msg string) {
	if strings.TrimSpace(rawEmail) == "" {
		return "", nil, "E-mail é obrigatório"
	}
	email, ok := ValidateEmail(rawEmail)
	if !ok {
		return "", nil, "E-mail inválido"
	}
	if n := strings.TrimSpace(rawName); n != "" {
		n = truncateRunes(n, NewsletterNameMax)
		name = &n
	}
	return email, name, ""
}

var (
	sqlSubscribe = sqlText(`INSERT INTO newsletter_subscribers (id, email, name, created_at)
   VALUES (gen_random_uuid(), $1, $2, now())
   ON CONFLICT (lower(email)) DO UPDATE
     SET unsubscribed_at = NULL,
         name = COALESCE(EXCLUDED.name, newsletter_subscribers.name)
     WHERE newsletter_subscribers.unsubscribed_at IS NOT NULL
   RETURNING (xmax::text = '0') AS inserted`)
	sqlUnsubscribe = sqlText(`INSERT INTO newsletter_subscribers (id, email, created_at, unsubscribed_at)
   VALUES (gen_random_uuid(), $1, now(), now())
   ON CONFLICT (lower(email)) DO UPDATE
     SET unsubscribed_at = now()
     WHERE newsletter_subscribers.unsubscribed_at IS NULL
   RETURNING (xmax::text <> '0') AS cancelled`)
)

// SubscribeNewsletter inserts the subscriber and queues the welcome mail
// atomically; an unsubscribed address is reactivated (no second welcome).
// Only an active duplicate is AlreadySubscribed; other errors propagate.
func SubscribeNewsletter(ctx context.Context, pool *pgxpool.Pool, email string, name *string) (NewsletterOutcome, error) {
	var out NewsletterOutcome
	err := withTx(ctx, pool, func(tx pgx.Tx) error {
		var inserted bool
		err := tx.QueryRow(ctx, sqlSubscribe, email, name).Scan(&inserted)
		var pgErr *pgconn.PgError
		switch {
		case errors.Is(err, pgx.ErrNoRows):
			out.Kind = NewsletterAlreadySubscribed
			return errSkip
		case errors.As(err, &pgErr) && pgErr.Code == "23505":
			out.Kind = NewsletterAlreadySubscribed
			return errSkip
		case err != nil:
			return err
		case !inserted:
			out.Kind = NewsletterResubscribed
			return nil
		}
		code, err := FirstPurchaseCode(ctx, tx)
		if err != nil {
			return err
		}
		out = NewsletterOutcome{Kind: NewsletterSubscribed, FirstPurchaseCode: code}
		return mail.SendNewsletterWelcome(ctx, tx, MailLinks(), email, deref(name), code)
	})
	if errors.Is(err, errSkip) {
		err = nil
	}
	return out, err
}

// UnsubscribeNewsletter flags the address as unsubscribed (idempotent);
// true when an active subscription was cancelled. Unknown addresses get a
// suppression row.
func UnsubscribeNewsletter(ctx context.Context, q DBTX, email string) (bool, error) {
	var cancelled bool
	err := q.QueryRow(ctx, sqlUnsubscribe, strings.ToLower(strings.TrimSpace(email))).Scan(&cancelled)
	if found, err := optional(err); err != nil || !found {
		return false, err
	}
	return cancelled, nil
}

// UnsubscribeURL is the absolute signed unsubscribe link.
func UnsubscribeURL(email string) string { return MailLinks().UnsubscribeURL(email) }

// VerifyUnsubscribeToken checks the unsubscribe token.
func VerifyUnsubscribeToken(email, token string) bool {
	return MailLinks().VerifyUnsubscribeToken(email, token)
}

// NewsletterSubscriber is an admin list row.
type NewsletterSubscriber struct {
	ID             uuid.UUID  `db:"id"`
	Email          string     `db:"email"`
	Name           *string    `db:"name"`
	CreatedAt      time.Time  `db:"created_at"`
	UnsubscribedAt *time.Time `db:"unsubscribed_at"`
}

// StatusLabel is "Ativa" / "Cancelada em <ISO 8601>".
func (s NewsletterSubscriber) StatusLabel() string {
	if s.UnsubscribedAt == nil {
		return "Ativa"
	}
	return "Cancelada em " + iso8601(*s.UnsubscribedAt)
}

func iso8601(t time.Time) string { return t.UTC().Format("2006-01-02T15:04:05.000000000Z07:00") }

var (
	sqlAllSubscribers = sqlText(`SELECT id, email, name, created_at, unsubscribed_at
 FROM newsletter_subscribers ORDER BY created_at DESC`)
	sqlActiveSubscribers = sqlText(`SELECT id, email, name, created_at, unsubscribed_at
 FROM newsletter_subscribers WHERE unsubscribed_at IS NULL
 ORDER BY created_at DESC`)
)

// NewsletterAdmin is /admin/newsletter.
type NewsletterAdmin struct {
	Subscribers []NewsletterSubscriber
	Active      int
	Cancelled   int
}

// LoadNewsletterAdmin lists every subscriber with counts.
func LoadNewsletterAdmin(ctx context.Context, q DBTX) (NewsletterAdmin, error) {
	subs, err := collect[NewsletterSubscriber](q.Query(ctx, sqlAllSubscribers))
	if err != nil {
		return NewsletterAdmin{}, err
	}
	out := NewsletterAdmin{Subscribers: subs}
	for _, s := range subs {
		if s.UnsubscribedAt == nil {
			out.Active++
		}
	}
	out.Cancelled = len(subs) - out.Active
	return out, nil
}

// ActiveSubscribers lists active subscribers (CSV export).
func ActiveSubscribers(ctx context.Context, q DBTX) ([]NewsletterSubscriber, error) {
	return collect[NewsletterSubscriber](q.Query(ctx, sqlActiveSubscribers))
}

// newsletterCSVField quotes and neutralises formulas (= + - @).
func newsletterCSVField(raw string) string {
	v := raw
	if raw != "" && strings.ContainsRune("=+-@", rune(raw[0])) {
		v = "'" + raw
	}
	return `"` + strings.ReplaceAll(v, `"`, `""`) + `"`
}

// NewsletterCSV is the /admin/newsletter/export-csv body
// (text/csv; filename newsletter.csv).
func NewsletterCSV(ctx context.Context, q DBTX) (string, error) {
	subs, err := ActiveSubscribers(ctx, q)
	if err != nil {
		return "", err
	}
	var b strings.Builder
	b.WriteString("email,name,created_at\n")
	for _, s := range subs {
		b.WriteString(newsletterCSVField(s.Email))
		b.WriteByte(',')
		b.WriteString(newsletterCSVField(deref(s.Name)))
		b.WriteByte(',')
		b.WriteString(iso8601(s.CreatedAt))
		b.WriteByte('\n')
	}
	return b.String(), nil
}
