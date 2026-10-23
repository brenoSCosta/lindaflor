// Package jobs runs the background loops: mail outbox worker, reservation
// sweeper (payment-aware), data hygiene, back-in-stock alerts, abandoned
// carts, rate-limiter pruning.
//
// TODO(wave1-C / wave1-B / wave3): implement the loops from src/main.rs
// (every Cfg.ReservationSweep) and the outbox worker (Cfg.MailPollInterval).
package jobs

import (
	"context"
	"sync"

	"github.com/wellington-vell/lindaflor/internal/app"
)

// Start launches the background loops; they stop when ctx is cancelled.
// The returned WaitGroup completes once every loop has returned (server
// shutdown waits on it before closing the pool).
func Start(ctx context.Context, d *app.Deps) *sync.WaitGroup {
	_ = ctx
	_ = d
	return &sync.WaitGroup{}
}
