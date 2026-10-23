// Package mail: transactional email through the email_outbox table and its
// background worker.
//
// Wave 0 implements the enqueue contract (same SQL as the Rust outbox);
// TODO(wave1-B): templates (17), enqueue_throttled, worker (backoff,
// MAIL_MAX_ATTEMPTS), SMTP/log transports, redact_address, absolute_url.
package mail

import (
	"context"
	"fmt"
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

// Msg is one rendered email.
type Msg struct {
	Kind    string
	To      string
	Subject string
	HTML    string
	Text    string
	// DedupeKey makes a second enqueue under the same key a silent no-op.
	DedupeKey string
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
	_, err = db.Exec(ctx, `INSERT INTO email_outbox (id, kind, to_address, subject, html_body, text_body, dedupe_key)
		VALUES ($1, $2, $3, $4, $5, $6, $7)
		ON CONFLICT (dedupe_key) WHERE dedupe_key IS NOT NULL DO NOTHING`,
		id, msg.Kind, strings.TrimSpace(msg.To), msg.Subject, msg.HTML, msg.Text, dedupe)
	if err != nil {
		return fmt.Errorf("enqueue mail: %w", err)
	}
	return nil
}

// Mailer sends one message (implemented by the SMTP and log transports).
type Mailer interface {
	Send(ctx context.Context, msg Msg) error
	Kind() string
}
