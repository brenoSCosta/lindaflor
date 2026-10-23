package mail

import (
	"context"
	"fmt"
	"log/slog"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// Outbox worker tuning (port of src/mail/outbox.rs).
const (
	// Batch is the number of rows claimed per drain pass.
	Batch = 20
	// LeaseSecs is how long a claimed row stays invisible to other workers.
	LeaseSecs        = 300
	backoffBaseSecs  = 30
	backoffMaxSecs   = 3600
	maxErrorLen      = 500
	defaultPollEvery = 5 * time.Second
)

// Per-recipient cap of the RecipientThrottledKinds.
const (
	RecipientMaxPerWindow = 5
	RecipientWindowSecs   = 3600
)

// EnqueueThrottled is Enqueue for the RecipientThrottledKinds: it silently
// drops the mail (returns false) once the recipient already has
// RecipientMaxPerWindow of them queued or sent in the last
// RecipientWindowSecs, so the endpoints cannot flood a mailbox one
// rotating IP at a time. Callers answer the client exactly as on success.
// The count and insert are one statement (soft cap under concurrency).
func EnqueueThrottled(ctx context.Context, db sqlc.DBTX, msg Msg) (bool, error) {
	id, err := uuid.NewV7()
	if err != nil {
		return false, err
	}
	to := strings.TrimSpace(msg.To)
	n, err := sqlc.New(db).MailEnqueueThrottled(ctx, sqlc.MailEnqueueThrottledParams{
		ID: id, Kind: msg.Kind, ToAddress: to, Subject: msg.Subject, HtmlBody: msg.HTML, TextBody: msg.Text,
		ThrottledKinds: RecipientThrottledKinds, WindowSecs: RecipientWindowSecs, MaxPerWindow: RecipientMaxPerWindow,
	})
	if err != nil {
		return false, fmt.Errorf("enqueue throttled mail: %w", err)
	}
	if n == 0 {
		slog.InfoContext(ctx, "auth mail dropped: per-recipient limit reached",
			"kind", msg.Kind, "to", RedactAddress(to), "max", RecipientMaxPerWindow, "window_secs", RecipientWindowSecs)
		return false, nil
	}
	slog.DebugContext(ctx, "mail enqueued", "kind", msg.Kind, "to", RedactAddress(to))
	return true, nil
}

// BackoffSecs is the delay before retry attempts+1: 30s, 60s, 120s, …
// capped at 1h.
func BackoffSecs(attempts int32) int64 {
	exp := min(max(attempts-1, 0), 16)
	return min(int64(backoffBaseSecs)<<exp, backoffMaxSecs)
}

// DrainReport counts one DrainOnce pass.
type DrainReport struct {
	Sent, Retried, Failed int
}

func truncateError(msg string) string {
	if len(msg) <= maxErrorLen {
		return msg
	}
	cut := maxErrorLen
	for cut > 0 && !utf8Start(msg[cut]) {
		cut--
	}
	return msg[:cut]
}

func utf8Start(b byte) bool { return b&0xC0 != 0x80 }

// DrainOnce claims and delivers one batch of due rows. Bodies are blanked
// once a row is terminal (sent or failed): they may embed one-time links.
func DrainOnce(ctx context.Context, pool *pgxpool.Pool, mailer Mailer, links Links, maxAttempts int) (DrainReport, error) {
	q := sqlc.New(pool)
	rows, err := q.MailClaimDue(ctx, sqlc.MailClaimDueParams{LeaseSecs: LeaseSecs, Batch: Batch})
	if err != nil {
		return DrainReport{}, fmt.Errorf("claim mail: %w", err)
	}
	var report DrainReport
	for _, row := range rows {
		msg := Msg{Kind: row.Kind, To: row.ToAddress, Subject: row.Subject, HTML: row.HtmlBody, Text: row.TextBody}
		if IsNewsletterKind(row.Kind) {
			msg.ListUnsubscribe = links.UnsubscribeURL(row.ToAddress)
		}
		sendErr := mailer.Send(ctx, msg)
		if sendErr == nil {
			if err := q.MailMarkSent(ctx, row.ID); err != nil {
				return report, err
			}
			report.Sent++
			continue
		}
		message := truncateError(sendErr.Error())
		if int(row.Attempts) >= maxAttempts {
			if err := q.MailMarkFailed(ctx, sqlc.MailMarkFailedParams{ID: row.ID, LastError: message}); err != nil {
				return report, err
			}
			slog.ErrorContext(ctx, "mail delivery failed permanently", "id", row.ID, "kind", row.Kind,
				"to", RedactAddress(row.ToAddress), "attempts", row.Attempts, "error", message)
			report.Failed++
			continue
		}
		if err := q.MailScheduleRetry(ctx, sqlc.MailScheduleRetryParams{
			ID: row.ID, LastError: message, DelaySecs: float64(BackoffSecs(row.Attempts)),
		}); err != nil {
			return report, err
		}
		slog.WarnContext(ctx, "mail delivery failed; will retry", "id", row.ID, "kind", row.Kind,
			"attempts", row.Attempts, "error", message)
		report.Retried++
	}
	return report, nil
}

// WorkerDeps configures RunWorker.
type WorkerDeps struct {
	Pool         *pgxpool.Pool
	Mailer       Mailer
	Links        Links
	PollInterval time.Duration // MAIL_POLL_SECS (default 5s)
	MaxAttempts  int           // MAIL_MAX_ATTEMPTS (default 8)
}

// RunWorker drains the outbox every PollInterval, back-to-back while full
// batches keep coming, until ctx is cancelled.
func RunWorker(ctx context.Context, d WorkerDeps) {
	poll := d.PollInterval
	if poll <= 0 {
		poll = defaultPollEvery
	}
	maxAttempts := max(d.MaxAttempts, 1)
	slog.InfoContext(ctx, "mail outbox worker started", "transport", d.Mailer.Kind())
	ticker := time.NewTicker(poll)
	defer ticker.Stop()
	for {
		for {
			report, err := DrainOnce(ctx, d.Pool, d.Mailer, d.Links, maxAttempts)
			if err != nil {
				if ctx.Err() == nil {
					slog.ErrorContext(ctx, "mail outbox drain failed", "error", err)
				}
				break
			}
			if report.Sent+report.Retried+report.Failed < Batch {
				break
			}
		}
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
	}
}
