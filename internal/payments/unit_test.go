package payments_test

import (
	"errors"
	"fmt"
	"strconv"
	"strings"
	"testing"
	"time"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/payments"
)

func TestBreakerOpensAfterThresholdAndHalfOpensAfterCooldown(t *testing.T) {
	b := payments.NewCircuitBreaker(2, 30*time.Millisecond)
	if !b.Allow() {
		t.Fatal("closed")
	}
	b.OnFailure()
	if !b.Allow() {
		t.Fatal("one failure stays closed")
	}
	b.OnFailure()
	if !b.IsOpen() || b.Allow() {
		t.Fatal("open fails fast")
	}
	time.Sleep(40 * time.Millisecond)
	if !b.Allow() {
		t.Fatal("cooldown elapsed: probe allowed")
	}
	if b.Allow() {
		t.Fatal("only one probe at a time")
	}
	b.OnFailure()
	if b.Allow() {
		t.Fatal("failed probe re-opens")
	}
	time.Sleep(40 * time.Millisecond)
	if !b.Allow() {
		t.Fatal("probe")
	}
	b.OnSuccess()
	if b.IsOpen() || !b.Allow() {
		t.Fatal("success closes")
	}
}

func TestBreakerSuccessResetsFailureCount(t *testing.T) {
	b := payments.NewCircuitBreaker(2, time.Minute)
	b.OnFailure()
	b.OnSuccess()
	b.OnFailure()
	if !b.Allow() {
		t.Fatal("must stay closed")
	}
}

func TestBreakerNeutralReleasesProbe(t *testing.T) {
	b := payments.NewCircuitBreaker(1, 10*time.Millisecond)
	b.OnFailure()
	time.Sleep(15 * time.Millisecond)
	if !b.Allow() || b.Allow() {
		t.Fatal("one probe")
	}
	b.OnNeutral()
	if !b.Allow() {
		t.Fatal("neutral releases the probe slot")
	}
}

const (
	sigSecret = "test-webhook-secret"
	sigNow    = int64(1_790_000_000)
)

func headerFor(dataID, requestID string, ts int64) string {
	s := strconv.FormatInt(ts, 10)
	return fmt.Sprintf("ts=%s,v1=%s", s, payments.Sign(sigSecret, payments.Manifest(dataID, requestID, s)))
}

func TestManifestMatchesMercadoPagoTemplate(t *testing.T) {
	if got := payments.Manifest("123456", "req-1", "1704908010"); got != "id:123456;request-id:req-1;ts:1704908010;" {
		t.Fatal(got)
	}
	if got := payments.Manifest("ABC123", "", "1"); got != "id:abc123;ts:1;" {
		t.Fatal("alphanumeric ids are lowercased; missing parts omitted:", got)
	}
}

func TestValidSignaturePasses(t *testing.T) {
	if err := payments.VerifySignature(sigSecret, headerFor("987", "req-abc", sigNow), "req-abc", "987", sigNow+10); err != nil {
		t.Fatal(err)
	}
	ts := strconv.FormatInt(sigNow, 10)
	v1 := payments.Sign(sigSecret, payments.Manifest("987", "r", ts))
	if err := payments.VerifySignature(sigSecret, "v1="+v1+", ts="+ts, "r", "987", sigNow); err != nil {
		t.Fatal("spaced / reordered parts:", err)
	}
}

// Port of src/payments/signature.rs millisecond_ts_is_accepted: Mercado
// Pago may send ts in milliseconds; it is normalized before the skew check
// and signed as sent.
func TestMillisecondTSIsAccepted(t *testing.T) {
	if err := payments.VerifySignature(sigSecret, headerFor("5", "r", sigNow*1000), "r", "5", sigNow); err != nil {
		t.Fatal(err)
	}
	stale := (sigNow - payments.MaxSignatureSkew - 1) * 1000
	if err := payments.VerifySignature(sigSecret, headerFor("5", "r", stale), "r", "5", sigNow); !errors.Is(err, payments.ErrSignatureStale) {
		t.Fatalf("stale millisecond ts: %v", err)
	}
}

func TestInvalidSignaturesAreRejected(t *testing.T) {
	h := headerFor("987", "req-abc", sigNow)
	for _, c := range []struct {
		secret, header, reqID, dataID string
		now                           int64
		want                          error
	}{
		{sigSecret, h, "req-abc", "988", sigNow, payments.ErrSignatureMismatch},
		{sigSecret, h, "other", "987", sigNow, payments.ErrSignatureMismatch},
		{"nope", h, "req-abc", "987", sigNow, payments.ErrSignatureMismatch},
		{sigSecret, "", "req-abc", "987", sigNow, payments.ErrSignatureMissing},
		{"", h, "req-abc", "987", sigNow, payments.ErrSignatureMissing},
		{sigSecret, "garbage", "", "987", sigNow, payments.ErrSignatureMalformed},
		{sigSecret, "ts=1,v1=zz", "", "987", 1, payments.ErrSignatureMismatch},
		{sigSecret, headerFor("987", "r", sigNow-payments.MaxSignatureSkew-1), "r", "987", sigNow, payments.ErrSignatureStale},
		{sigSecret, headerFor("987", "r", sigNow+payments.MaxSignatureSkew+1), "r", "987", sigNow, payments.ErrSignatureStale},
	} {
		if err := payments.VerifySignature(c.secret, c.header, c.reqID, c.dataID, c.now); !errors.Is(err, c.want) {
			t.Errorf("%+v: got %v", c, err)
		}
	}
}

func TestFormatsExpirationInBrasiliaTime(t *testing.T) {
	if got := payments.FormatMPDateTime(time.Unix(1_793_016_000, 0)); got != "2026-10-26T09:00:00.000-03:00" {
		t.Fatal(got)
	}
}

func TestAmountConversionsRoundTrip(t *testing.T) {
	if payments.CentsToAmount(12_345) != 123.45 || payments.CentsToAmount(10) != 0.1 {
		t.Fatal("cents → amount")
	}
	if payments.AmountToCents(123.45) != 12_345 || payments.AmountToCents(0.1+0.2) != 30 {
		t.Fatal("amount → cents")
	}
}

func TestMapsMercadoPagoStatuses(t *testing.T) {
	for raw, want := range map[string]payments.ChargeStatus{
		"pending": payments.StatusPending, "approved": payments.StatusApproved,
		"rejected": payments.StatusFailed, "cancelled": payments.StatusFailed, "expired": payments.StatusFailed,
		"refunded": payments.StatusRefunded, "charged_back": payments.StatusRefunded,
		"in_mediation": payments.StatusOther,
	} {
		if got := payments.StatusFromMercadoPago(raw); got != want {
			t.Errorf("%s → %v", raw, got)
		}
	}
}

func TestEventForIsRedeliverySafe(t *testing.T) {
	type c struct {
		from     sqlc.OrderStatus
		everPaid bool
		status   payments.ChargeStatus
		want     payments.OrderEvent
		ok       bool
	}
	for _, tc := range []c{
		{sqlc.OrderStatusPendingPayment, false, payments.StatusApproved, payments.EventPay, true},
		{sqlc.OrderStatusCancelled, false, payments.StatusApproved, payments.EventReopenPay, true},
		{sqlc.OrderStatusCancelled, true, payments.StatusApproved, 0, false},
		{sqlc.OrderStatusPaid, true, payments.StatusApproved, 0, false},
		{sqlc.OrderStatusPendingPayment, false, payments.StatusFailed, payments.EventCancel, true},
		{sqlc.OrderStatusShipped, true, payments.StatusRefunded, payments.EventRefund, true},
		{sqlc.OrderStatusRefunded, true, payments.StatusRefunded, 0, false},
		{sqlc.OrderStatusPendingPayment, false, payments.StatusPending, 0, false},
	} {
		ev, ok := payments.EventFor(tc.from, tc.everPaid, tc.status)
		if ev != tc.want || ok != tc.ok {
			t.Errorf("%+v: got %v %v", tc, ev, ok)
		}
	}
}

func TestProviderSelectionAndRedaction(t *testing.T) {
	if payments.FromConfig(config.PaymentConfig{Kind: config.ProviderStaticPix}).Kind() != config.ProviderStaticPix {
		t.Fatal("static")
	}
	mp := payments.FromConfig(config.PaymentConfig{Kind: config.ProviderMercadoPago, MercadoPago: &config.MercadoPagoConfig{
		AccessToken: "APP_USR-secret-token", WebhookSecret: "whsec-very-secret", BaseURL: config.MPDefaultBaseURL,
	}})
	if !payments.IsMercadoPago(mp) || !mp.AcceptingCharges() {
		t.Fatal("mercado pago")
	}
	rendered := fmt.Sprintf("%v %+v %#v", mp, mp, mp)
	if strings.Contains(rendered, "secret-token") || strings.Contains(rendered, "very-secret") || !strings.Contains(rendered, "api.mercadopago.com") {
		t.Fatal(rendered)
	}
	// Mercado Pago without credentials falls back to static PIX.
	if payments.IsMercadoPago(payments.FromConfig(config.PaymentConfig{Kind: config.ProviderMercadoPago})) {
		t.Fatal("no credentials → static")
	}
}
