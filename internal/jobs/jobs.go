// Package jobs runs the background loops (port of the src/main.rs
// spawns): the mail outbox worker and, every RESERVATION_SWEEP_SECS, the
// payment-aware reservation sweep, data hygiene, back-in-stock alerts,
// abandoned-cart reminders and rate-limiter pruning.
package jobs

import (
	"context"
	"log/slog"
	"sync"
	"time"

	"github.com/wellington-vell/lindaflor/internal/app"
	"github.com/wellington-vell/lindaflor/internal/mail"
	"github.com/wellington-vell/lindaflor/internal/maintenance"
	"github.com/wellington-vell/lindaflor/internal/payments"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/store"
)

// Start launches the background loops; they stop when ctx is cancelled.
// The returned WaitGroup completes once every loop has returned (server
// shutdown waits on it before closing the pool).
func Start(ctx context.Context, d *app.Deps) *sync.WaitGroup {
	var wg sync.WaitGroup
	if d == nil || d.Pool == nil {
		return &wg
	}
	if d.Mailer != nil {
		wg.Go(func() {
			mail.RunWorker(ctx, mail.WorkerDeps{
				Pool: d.Pool, Mailer: d.Mailer, Links: mail.LinksFromConfig(d.Cfg),
				PollInterval: d.Cfg.MailPollInterval, MaxAttempts: d.Cfg.MailMaxAttempts,
			})
		})
	}
	every := time.Minute
	if d.Cfg != nil && d.Cfg.ReservationSweep > 0 {
		every = d.Cfg.ReservationSweep
	}
	wg.Go(func() { runSweeper(ctx, d, every) })
	return &wg
}

func runSweeper(ctx context.Context, d *app.Deps, every time.Duration) {
	ticker := time.NewTicker(every)
	defer ticker.Stop()
	for {
		// Like tokio's interval, the first tick fires immediately.
		Sweep(ctx, d)
		select {
		case <-ctx.Done():
			return
		case <-ticker.C:
		}
	}
}

// Sweep runs one pass of every periodic step; failures are logged and the
// other steps still run.
func Sweep(ctx context.Context, d *app.Deps) {
	// Payment-aware: MP orders are re-checked with MP before cancelling.
	orders := d.Orders
	if orders == nil {
		orders = store.OrderFSM{Pool: d.Pool}
	}
	svc := &payments.Service{Pool: d.Pool, Provider: d.Payments, Orders: orders}
	if err := svc.ExpirePendingReservations(ctx); err != nil && ctx.Err() == nil {
		slog.ErrorContext(ctx, "reservation sweep failed", "error", err)
	}
	// Stale guest carts + expired verification rows (bounded batches).
	switch out, err := maintenance.SweepStaleRows(ctx, d.Pool); {
	case err != nil:
		if ctx.Err() == nil {
			slog.ErrorContext(ctx, "data hygiene sweep failed", "error", err)
		}
	case out.Carts > 0 || out.Verifications > 0:
		slog.InfoContext(ctx, "data hygiene sweep", "carts", out.Carts, "verifications", out.Verifications)
	}
	// Back-in-stock alerts whose variant is sellable again.
	switch sent, err := store.NotifyRestocked(ctx, d.Pool); {
	case err != nil:
		if ctx.Err() == nil {
			slog.ErrorContext(ctx, "stock alert sweep failed", "error", err)
		}
	case sent > 0:
		slog.InfoContext(ctx, "back-in-stock alerts queued", "sent", sent)
	}
	if d.Cfg != nil && d.Cfg.AbandonedCartOn {
		minutes := int64(d.Cfg.AbandonedCartAfter / time.Minute)
		switch sent, err := store.SendAbandonedCartReminders(ctx, d.Pool, minutes); {
		case err != nil:
			if ctx.Err() == nil {
				slog.ErrorContext(ctx, "abandoned-cart sweep failed", "error", err)
			}
		case sent > 0:
			slog.InfoContext(ctx, "abandoned-cart reminders queued", "sent", sent)
		}
	}
	// Drop replenished buckets so the per-IP maps don't grow forever.
	ratelimit.Prune()
}
