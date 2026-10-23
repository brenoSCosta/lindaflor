package store

import (
	"context"
	"fmt"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/payments"
)

// OrderFSM adapts the order state machine to payments.OrderTransitioner
// (webhook, admin sync/refund, payment-aware sweep). Wire it as
// app.Deps.Orders = store.OrderFSM{Pool: pool}.
type OrderFSM struct{ Pool *pgxpool.Pool }

var _ payments.OrderTransitioner = OrderFSM{}

func paymentEvent(ev payments.OrderEvent) (OrderEvent, error) {
	switch ev {
	case payments.EventPay:
		return EventPay, nil
	case payments.EventReopenPay:
		return EventReopenPay, nil
	case payments.EventCancel:
		return EventCancel, nil
	case payments.EventRefund:
		return EventRefund, nil
	}
	return "", fmt.Errorf("store: unknown payment event %d", ev)
}

// Apply implements payments.OrderTransitioner: ApplyWithAudit attributed
// to actor (stock, coupon, paid_at, customer mail in tx). A ReopenPay
// without stock fails with payments.ErrInsufficientStock.
func (f OrderFSM) Apply(ctx context.Context, tx pgx.Tx, orderID uuid.UUID, ev payments.OrderEvent, actor payments.Actor) (sqlc.OrderStatus, error) {
	event, err := paymentEvent(ev)
	if err != nil {
		return "", err
	}
	a := Actor{ID: actor.ID}
	if actor.Name != "" {
		a.Name = ptr(actor.Name)
	}
	next, err := ApplyWithAudit(ctx, tx, orderID, event, ApplyOptions{}, a)
	if err != nil {
		return "", err
	}
	return sqlc.OrderStatus(next), nil
}

// ExpireOne implements payments.OrderTransitioner.
func (f OrderFSM) ExpireOne(ctx context.Context, orderID uuid.UUID) error {
	return ExpireOne(ctx, f.Pool, orderID)
}

// ExpirePendingReservations implements payments.OrderTransitioner.
func (f OrderFSM) ExpirePendingReservations(ctx context.Context) error {
	return ExpirePendingReservations(ctx, f.Pool)
}
