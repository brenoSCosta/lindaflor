package payments

import (
	"bytes"
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"math"
	"math/rand/v2"
	"net"
	"net/http"
	"time"

	"github.com/wellington-vell/lindaflor/internal/config"
)

// RetryPolicy: transient failures are retried with base·2^(n-1) + jitter.
type RetryPolicy struct {
	MaxAttempts uint32
	BaseDelay   time.Duration
	MaxJitter   time.Duration
}

// DefaultRetryPolicy is 3 attempts, 200ms base, up to 150ms jitter.
var DefaultRetryPolicy = RetryPolicy{MaxAttempts: 3, BaseDelay: 200 * time.Millisecond, MaxJitter: 150 * time.Millisecond}

// backoff is the delay before retry number attempt (1-based).
func (p RetryPolicy) backoff(attempt uint32) time.Duration {
	exp := p.BaseDelay << min(max(attempt, 1)-1, 8)
	if p.MaxJitter <= 0 {
		return exp
	}
	return exp + time.Duration(rand.Int64N(int64(p.MaxJitter)+1)) //nolint:gosec // jitter, not security
}

// Circuit breaker defaults: consecutive transient failures that open it
// and how long it stays open before a half-open probe.
const (
	BreakerThreshold = 5
	BreakerCooldown  = 30 * time.Second
	mpHTTPTimeout    = 10 * time.Second
)

// MercadoPago is the Mercado Pago Payments API adapter. Payment creation
// carries X-Idempotency-Key = order id, so a retry (or a second attempt
// from the order page) can never double-charge.
type MercadoPago struct {
	http    *http.Client
	cfg     config.MercadoPagoConfig
	retry   RetryPolicy
	breaker *CircuitBreaker
}

// NewMercadoPago builds the client with the default retry policy and breaker.
func NewMercadoPago(cfg config.MercadoPagoConfig) *MercadoPago {
	return NewMercadoPagoWith(cfg, DefaultRetryPolicy, NewCircuitBreaker(BreakerThreshold, BreakerCooldown))
}

// NewMercadoPagoWith builds a client with an explicit policy and breaker.
func NewMercadoPagoWith(cfg config.MercadoPagoConfig, retry RetryPolicy, breaker *CircuitBreaker) *MercadoPago {
	return &MercadoPago{http: &http.Client{Timeout: mpHTTPTimeout}, cfg: cfg, retry: retry, breaker: breaker}
}

// String never prints the secrets.
func (m *MercadoPago) String() string { return "MercadoPago{" + m.cfg.String() + "}" }

// GoString keeps %#v redacted too.
func (m *MercadoPago) GoString() string { return m.String() }

// Kind implements Provider.
func (*MercadoPago) Kind() config.ProviderKind { return config.ProviderMercadoPago }

// AcceptingCharges implements Provider.
func (m *MercadoPago) AcceptingCharges() bool { return !m.breaker.IsOpen() }

// WebhookSecret implements WebhookVerifier.
func (m *MercadoPago) WebhookSecret() string { return m.cfg.WebhookSecret }

type mpPayment struct {
	ID                 int64    `json:"id"`
	Status             string   `json:"status"`
	StatusDetail       *string  `json:"status_detail"`
	ExternalReference  *string  `json:"external_reference"`
	TransactionAmount  *float64 `json:"transaction_amount"`
	PointOfInteraction *struct {
		TransactionData *struct {
			QRCode       *string `json:"qr_code"`
			QRCodeBase64 *string `json:"qr_code_base64"`
			TicketURL    *string `json:"ticket_url"`
		} `json:"transaction_data"`
	} `json:"point_of_interaction"`
}

var brasilia = time.FixedZone("BRT", -3*60*60)

// FormatMPDateTime is MP's date_of_expiration format in Brasília time
// (no DST since 2019): 2026-10-24T12:00:00.000-03:00.
func FormatMPDateTime(t time.Time) string {
	return t.In(brasilia).Format("2006-01-02T15:04:05.000") + "-03:00"
}

// CentsToAmount is MP's decimal transaction_amount (12345 → 123.45).
func CentsToAmount(cents int64) float64 { return float64(cents) / 100 }

// AmountToCents rounds MP's decimal amount to cents.
func AmountToCents(amount float64) int64 { return int64(math.Round(amount * 100)) }

type request struct {
	method, path, idempotencyKey string
	body                         any
}

func retryableNetErr(err error) (string, bool) {
	var ne net.Error
	if errors.As(err, &ne) && ne.Timeout() {
		return "timeout", true
	}
	var oe *net.OpError
	if errors.As(err, &oe) && oe.Op == "dial" {
		return "connection failed", true
	}
	return "", false
}

// send runs one API call with retry + breaker; each attempt sends an
// identical request (same idempotency key).
func (m *MercadoPago) send(ctx context.Context, req request, out any) error {
	if !m.breaker.Allow() {
		return ErrUnavailable
	}
	var payload []byte
	if req.body != nil {
		var err error
		if payload, err = json.Marshal(req.body); err != nil {
			m.breaker.OnNeutral()
			return fmt.Errorf("encode mercado pago request: %w", err)
		}
	}
	attempts := max(m.retry.MaxAttempts, 1)
	lastErr := ""
	for attempt := range attempts {
		if attempt > 0 {
			select {
			case <-ctx.Done():
				m.breaker.OnNeutral()
				return &TransientError{Msg: ctx.Err().Error()}
			case <-time.After(m.retry.backoff(attempt)):
			}
		}
		var body io.Reader
		if payload != nil {
			body = bytes.NewReader(payload)
		}
		hr, err := http.NewRequestWithContext(ctx, req.method, m.cfg.BaseURL+req.path, body)
		if err != nil {
			m.breaker.OnNeutral()
			return err
		}
		hr.Header.Set("Authorization", "Bearer "+m.cfg.AccessToken)
		if payload != nil {
			hr.Header.Set("Content-Type", "application/json")
		}
		if req.idempotencyKey != "" {
			hr.Header.Set("X-Idempotency-Key", req.idempotencyKey)
		}
		res, err := m.http.Do(hr)
		if err != nil {
			msg, retry := retryableNetErr(err)
			if !retry {
				m.breaker.OnFailure()
				return &TransientError{Msg: err.Error()}
			}
			lastErr = msg
		} else {
			raw, readErr := io.ReadAll(io.LimitReader(res.Body, 4<<20))
			_ = res.Body.Close()
			switch {
			case res.StatusCode >= 500:
				lastErr = fmt.Sprintf("HTTP %d", res.StatusCode)
			case res.StatusCode >= 200 && res.StatusCode < 300:
				if readErr != nil {
					m.breaker.OnNeutral()
					return &DecodeError{Msg: readErr.Error()}
				}
				if err := json.Unmarshal(raw, out); err != nil {
					m.breaker.OnNeutral()
					return &DecodeError{Msg: err.Error()}
				}
				m.breaker.OnSuccess()
				return nil
			default:
				// 4xx: our request is wrong (bad token, invalid payer, …).
				// Retrying cannot help and it says nothing about MP's health.
				var eb struct {
					Message string `json:"message"`
				}
				_ = json.Unmarshal(raw, &eb)
				m.breaker.OnNeutral()
				return &RejectedError{Status: res.StatusCode, Message: eb.Message}
			}
		}
		slog.WarnContext(ctx, "mercado pago request failed", "attempt", attempt+1, "error", lastErr)
	}
	m.breaker.OnFailure()
	return &TransientError{Msg: lastErr}
}

// CreateCharge implements Provider: POST /v1/payments with
// payment_method_id = "pix".
func (m *MercadoPago) CreateCharge(ctx context.Context, c NewCharge) (Charge, error) {
	body := map[string]any{
		"transaction_amount": CentsToAmount(c.AmountCents),
		"description":        c.Description,
		"payment_method_id":  "pix",
		"payer":              map[string]any{"email": c.PayerEmail},
		"external_reference": c.OrderID.String(),
		"date_of_expiration": FormatMPDateTime(c.ExpiresAt),
	}
	if m.cfg.NotificationURL != "" {
		body["notification_url"] = m.cfg.NotificationURL
	}
	var p mpPayment
	if err := m.send(ctx, request{method: http.MethodPost, path: "/v1/payments", idempotencyKey: c.OrderID.String(), body: body}, &p); err != nil {
		return Charge{}, err
	}
	ch := Charge{PaymentID: p.ID, RawStatus: p.Status, StatusDetail: p.StatusDetail}
	if p.PointOfInteraction != nil && p.PointOfInteraction.TransactionData != nil {
		td := p.PointOfInteraction.TransactionData
		ch.QRCode, ch.QRCodeBase64, ch.TicketURL = td.QRCode, td.QRCodeBase64, td.TicketURL
	}
	return ch, nil
}

// FetchStatus implements Provider: GET /v1/payments/{id}, the source of
// truth for webhook handling.
func (m *MercadoPago) FetchStatus(ctx context.Context, paymentID int64) (PaymentInfo, error) {
	var p mpPayment
	if err := m.send(ctx, request{method: http.MethodGet, path: fmt.Sprintf("/v1/payments/%d", paymentID)}, &p); err != nil {
		return PaymentInfo{}, err
	}
	info := PaymentInfo{
		PaymentID: p.ID, Status: StatusFromMercadoPago(p.Status), RawStatus: p.Status,
		StatusDetail: p.StatusDetail, ExternalReference: p.ExternalReference,
	}
	if p.TransactionAmount != nil {
		cents := AmountToCents(*p.TransactionAmount)
		info.AmountCents = &cents
	}
	return info, nil
}

// Refund implements Provider: POST /v1/payments/{id}/refunds (full
// refund), idempotent per payment.
func (m *MercadoPago) Refund(ctx context.Context, paymentID int64) error {
	var out json.RawMessage
	return m.send(ctx, request{
		method: http.MethodPost, path: fmt.Sprintf("/v1/payments/%d/refunds", paymentID),
		idempotencyKey: fmt.Sprintf("refund-%d", paymentID), body: map[string]any{},
	}, &out)
}
