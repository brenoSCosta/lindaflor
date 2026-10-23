package store

import (
	"context"
	"sync"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/mail"
)

// Customer mail goes through internal/mail's hooks and templates, enqueued
// inside the caller's transaction so it commits (or rolls back) with the
// state change. Absolute/signed links use the configured mail.Links.

var (
	linksMu  sync.RWMutex
	linksSet bool
	links    mail.Links
)

// SetLinks sets the origin/pepper of mailed links (call once at boot with
// mail.LinksFromConfig(cfg)). Until then APP_ORIGIN/TOKEN_PEPPER are read
// from the environment (zero values fall back to the dev defaults).
func SetLinks(l mail.Links) {
	linksMu.Lock()
	links, linksSet = l, true
	linksMu.Unlock()
}

// MailLinks returns the links in use.
func MailLinks() mail.Links {
	linksMu.RLock()
	defer linksMu.RUnlock()
	if linksSet {
		return links
	}
	return mail.Links{Origin: config.AppOrigin(), Pepper: config.TokenPepper()}
}

// EnqueueOrderCreated queues "Pedido recebido" (checkout transaction).
func EnqueueOrderCreated(ctx context.Context, tx DBTX, orderID uuid.UUID) error {
	return mail.EnqueueOrderCreated(ctx, tx, MailLinks(), orderID)
}

// EnqueueOrderTransition queues the customer mail of an order edge.
func EnqueueOrderTransition(ctx context.Context, tx DBTX, orderID uuid.UUID, from, next OrderStatus, expired bool) error {
	return mail.EnqueueOrderTransition(ctx, tx, MailLinks(), orderID, sqlc.OrderStatus(from), sqlc.OrderStatus(next), expired)
}

// ReturnMailStep selects the return-request mail.
type ReturnMailStep struct {
	Kind         string // requested | approved | rejected | resolved
	Instructions string
	Reason       string
	RefundCents  *int64 // resolved: refund amount, nil = exchanged
}

func enqueueReturnMail(ctx context.Context, tx DBTX, returnID uuid.UUID, step ReturnMailStep) error {
	var s mail.ReturnStep
	switch step.Kind {
	case "approved":
		s = mail.ReturnApprovedStep(step.Instructions)
	case "rejected":
		s = mail.ReturnRejectedStep(step.Reason)
	case "resolved":
		s = mail.ReturnResolvedStep(step.RefundCents)
	default:
		s = mail.ReturnRequestedStep
	}
	return mail.EnqueueReturn(ctx, tx, MailLinks(), returnID, s)
}
