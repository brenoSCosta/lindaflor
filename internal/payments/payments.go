// Package payments: PIX charges through Mercado Pago (circuit breaker,
// retries, signed webhook) or the static PIX BR Code fallback (see
// internal/payments/pix). Port of src/payments/**.
//
// Checkout, the order page, the webhook, the admin order page and the
// expiry sweeper talk only to Provider and Service; order state changes
// go through the OrderTransitioner (agent C's order FSM).
package payments

import (
	"context"
	"errors"
	"fmt"
	"time"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/config"
)

// UnavailableMessage is shown when the PSP is down or the circuit is open.
const UnavailableMessage = "Pagamento temporariamente indisponível. Tente novamente em instantes."

// NewCharge is a PIX charge built from the persisted order (never from
// client input).
type NewCharge struct {
	OrderID     uuid.UUID
	AmountCents int64
	PayerEmail  string
	ExpiresAt   time.Time
	Description string
}

// ChargeStatus buckets PSP statuses for the order FSM.
type ChargeStatus int

// Charge statuses.
const (
	StatusPending ChargeStatus = iota
	StatusApproved
	StatusFailed   // rejected | cancelled | expired
	StatusRefunded // refunded | charged_back
	StatusOther    // in_process, in_mediation, authorized, …: no transition
)

// StatusFromMercadoPago maps a raw MP status.
func StatusFromMercadoPago(raw string) ChargeStatus {
	switch raw {
	case "pending":
		return StatusPending
	case "approved":
		return StatusApproved
	case "rejected", "cancelled", "expired":
		return StatusFailed
	case "refunded", "charged_back":
		return StatusRefunded
	}
	return StatusOther
}

// Charge is a created charge with its PIX instructions.
type Charge struct {
	PaymentID    int64
	RawStatus    string
	StatusDetail *string
	QRCode       *string
	QRCodeBase64 *string
	TicketURL    *string
}

// PaymentInfo is the PSP's view of a payment.
type PaymentInfo struct {
	PaymentID         int64
	Status            ChargeStatus
	RawStatus         string
	StatusDetail      *string
	ExternalReference *string
	AmountCents       *int64
}

// Errors.
var (
	// ErrUnavailable: circuit breaker open, failed fast without calling the PSP.
	ErrUnavailable  = errors.New(UnavailableMessage)
	ErrNotSupported = errors.New("Operação indisponível para este meio de pagamento.") //nolint:staticcheck // user-facing pt-BR copy
)

// TransientError is a 5xx/timeout/connection failure after retries.
type TransientError struct{ Msg string }

func (e *TransientError) Error() string { return UnavailableMessage }

// RejectedError is a 4xx from the PSP (never retried).
type RejectedError struct {
	Status  int
	Message string
}

func (e *RejectedError) Error() string {
	return fmt.Sprintf("O Mercado Pago recusou a operação (HTTP %d).", e.Status)
}

// DecodeError is a 2xx with a body we could not understand.
type DecodeError struct{ Msg string }

func (e *DecodeError) Error() string { return "Resposta inválida do Mercado Pago." }

// IsTransient reports errors worth retrying later.
func IsTransient(err error) bool {
	var te *TransientError
	return errors.Is(err, ErrUnavailable) || errors.As(err, &te)
}

// Provider is the payment strategy for new orders.
type Provider interface {
	Kind() config.ProviderKind
	// AcceptingCharges is false while the circuit breaker is open: checkout
	// fails fast with UnavailableMessage instead of creating an order that
	// cannot get a charge.
	AcceptingCharges() bool
	CreateCharge(ctx context.Context, c NewCharge) (Charge, error)
	FetchStatus(ctx context.Context, paymentID int64) (PaymentInfo, error)
	Refund(ctx context.Context, paymentID int64) error
}

// WebhookVerifier is implemented by providers that receive signed webhooks
// (Mercado Pago).
type WebhookVerifier interface {
	WebhookSecret() string
}

// StaticPix is the manual-confirmation provider: orders show a locally
// generated BR Code and an admin marks them paid.
type StaticPix struct{}

// Kind implements Provider.
func (StaticPix) Kind() config.ProviderKind { return config.ProviderStaticPix }

// AcceptingCharges implements Provider.
func (StaticPix) AcceptingCharges() bool { return true }

// CreateCharge implements Provider.
func (StaticPix) CreateCharge(context.Context, NewCharge) (Charge, error) {
	return Charge{}, ErrNotSupported
}

// FetchStatus implements Provider.
func (StaticPix) FetchStatus(context.Context, int64) (PaymentInfo, error) {
	return PaymentInfo{}, ErrNotSupported
}

// Refund implements Provider.
func (StaticPix) Refund(context.Context, int64) error { return ErrNotSupported }

// FromConfig builds the configured provider: the Mercado Pago client when
// selected and configured, else StaticPix.
func FromConfig(cfg config.PaymentConfig) Provider {
	if cfg.Kind == config.ProviderMercadoPago && cfg.MercadoPago != nil {
		return NewMercadoPago(*cfg.MercadoPago)
	}
	return StaticPix{}
}

// IsMercadoPago reports a Mercado Pago provider.
func IsMercadoPago(p Provider) bool {
	return p != nil && p.Kind() == config.ProviderMercadoPago
}
