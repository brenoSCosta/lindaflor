// Package payments: PIX charges through Mercado Pago (with circuit breaker
// and retries) or the static PIX BR Code fallback, plus the MP webhook.
//
// Wave 0 defines the Provider contract and the static provider.
// TODO(wave1-B): Mercado Pago client, circuit breaker, signature manifest,
// webhook handler, static PIX BR Code generation.
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
	StatusOther
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
	ErrUnavailable  = errors.New(UnavailableMessage) // circuit open
	ErrNotSupported = errors.New("Operação indisponível para este meio de pagamento.")
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

// IsTransient reports errors worth retrying later.
func IsTransient(err error) bool {
	var te *TransientError
	return errors.Is(err, ErrUnavailable) || errors.As(err, &te)
}

// Provider is the payment strategy for new orders.
type Provider interface {
	Kind() config.ProviderKind
	// AcceptingCharges is false while the circuit breaker is open.
	AcceptingCharges() bool
	CreateCharge(ctx context.Context, c NewCharge) (Charge, error)
	FetchStatus(ctx context.Context, paymentID int64) (PaymentInfo, error)
	Refund(ctx context.Context, paymentID int64) error
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

// FromConfig builds the configured provider.
// TODO(wave1-B): return the Mercado Pago client for ProviderMercadoPago.
func FromConfig(_ config.PaymentConfig) Provider { return StaticPix{} }
