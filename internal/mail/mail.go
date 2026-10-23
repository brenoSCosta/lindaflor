// Package mail: transactional email through the email_outbox table and its
// background worker (port of src/mail*).
//
// Callers never talk to SMTP on the request path: they render an Email
// from the templates and Enqueue it — inside their own transaction when
// they have one — and the worker (RunWorker) delivers it through the
// configured Mailer with retries and exponential backoff.
package mail

import (
	"context"
	"fmt"
	"log/slog"
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// Outbox kinds (email_outbox.kind).
const (
	KindVerifyEmail       = "verify_email"
	KindPasswordReset     = "password_reset"
	KindChangeEmail       = "change_email"
	KindDeleteAccount     = "delete_account"
	KindOrderCreated      = "order_created"
	KindPaymentConfirmed  = "payment_confirmed"
	KindOrderShipped      = "order_shipped"
	KindOrderCancelled    = "order_cancelled"
	KindOrderExpired      = "order_expired"
	KindOrderRefunded     = "order_refunded"
	KindNewsletterWelcome = "newsletter_welcome"
	KindBackInStock       = "back_in_stock"
	KindAbandonedCart     = "abandoned_cart"
	KindReturnRequested   = "return_requested"
	KindReturnApproved    = "return_approved"
	KindReturnRejected    = "return_rejected"
	KindReturnResolved    = "return_resolved"
)

// NewsletterKinds are marketing mail: delivered with List-Unsubscribe /
// List-Unsubscribe-Post headers (RFC 8058) pointing at the signed
// unsubscribe link of the recipient.
var NewsletterKinds = []string{KindNewsletterWelcome, KindAbandonedCart}

// RecipientThrottledKinds are auth mails anyone can trigger for an
// arbitrary address; EnqueueThrottled caps them per recipient.
var RecipientThrottledKinds = []string{KindVerifyEmail, KindPasswordReset, KindChangeEmail}

// IsNewsletterKind reports marketing kinds (List-Unsubscribe headers).
func IsNewsletterKind(kind string) bool {
	for _, k := range NewsletterKinds {
		if k == kind {
			return true
		}
	}
	return false
}

// Msg is one rendered email.
type Msg struct {
	Kind    string
	To      string
	Subject string
	HTML    string
	Text    string
	// DedupeKey makes a second enqueue under the same key a silent no-op.
	DedupeKey string
	// ListUnsubscribe is the one-click unsubscribe URL (marketing mail;
	// set by the worker, not by callers).
	ListUnsubscribe string
}

// Enqueue queues msg using db (pool or the caller's transaction, so the
// mail commits with the state change that triggered it).
func Enqueue(ctx context.Context, db sqlc.DBTX, msg Msg) error {
	id, err := uuid.NewV7()
	if err != nil {
		return err
	}
	var dedupe *string
	if msg.DedupeKey != "" {
		dedupe = &msg.DedupeKey
	}
	err = sqlc.New(db).MailEnqueue(ctx, sqlc.MailEnqueueParams{
		ID: id, Kind: msg.Kind, ToAddress: strings.TrimSpace(msg.To), Subject: msg.Subject,
		HtmlBody: msg.HTML, TextBody: msg.Text, DedupeKey: dedupe,
	})
	if err != nil {
		return fmt.Errorf("enqueue mail: %w", err)
	}
	slog.DebugContext(ctx, "mail enqueued", "kind", msg.Kind, "to", RedactAddress(msg.To))
	return nil
}

// Mailer sends one message (implemented by the SMTP and log transports).
type Mailer interface {
	Send(ctx context.Context, msg Msg) error
	Kind() string
}

// RedactAddress turns "ana@example.com" into "a***@example.com" for logs.
func RedactAddress(address string) string {
	local, domain, ok := strings.Cut(address, "@")
	if !ok {
		return "***"
	}
	first := "*"
	for _, r := range local {
		first = string(r)
		break
	}
	return first + "***@" + domain
}

// EncodeQueryValue percent-encodes a query value: unreserved bytes
// (A-Z a-z 0-9 - _ . ~) pass, everything else is %XX.
func EncodeQueryValue(value string) string {
	var b strings.Builder
	for i := 0; i < len(value); i++ {
		c := value[i]
		if isUnreserved(c) {
			b.WriteByte(c)
		} else {
			fmt.Fprintf(&b, "%%%02X", c)
		}
	}
	return b.String()
}

func isUnreserved(c byte) bool {
	return ('a' <= c && c <= 'z') || ('A' <= c && c <= 'Z') || ('0' <= c && c <= '9') ||
		c == '-' || c == '_' || c == '.' || c == '~'
}
