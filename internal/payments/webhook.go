package payments

import (
	"context"
	"encoding/json"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"strconv"
	"strings"
	"time"
)

// WebhookPath is the Mercado Pago notification route (exempt from the
// origin check, see httpx.OriginExemptPaths; rate limited under
// ratelimit.ScopeMPWebhook).
const WebhookPath = "/api/webhooks/mercadopago"

// ApplyBudget is how long the webhook waits for fetch + apply before
// answering 200 and letting the work finish in the background (MP expects
// an answer within ~20s).
const ApplyBudget = 8 * time.Second

// backgroundLimit bounds a sync that outlives the response.
const backgroundLimit = 2 * time.Minute

func reply(w http.ResponseWriter, status int, label string) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(status)
	_ = json.NewEncoder(w).Encode(map[string]string{"status": label})
}

type syncResult struct {
	out SyncOutcome
	err error
}

// WebhookHandler is POST /api/webhooks/mercadopago. Cross-origin,
// cookie-less server-to-server POST authenticated by the x-signature HMAC.
// The body is never trusted: only the signed data.id is used and the
// payment is re-fetched from the MP API before the order FSM runs.
//
// Responses: 404 (via notFound) when the provider is not Mercado Pago; 401
// on a bad/stale signature; 200 {"status":"ok"} once applied,
// "accepted" when still applying after ApplyBudget, "ignored" for
// non-payment notifications and 4xx from MP; 503 {"status":"retry"} when
// MP or the database failed, so MP redelivers later.
func (s *Service) WebhookHandler(notFound http.HandlerFunc) http.HandlerFunc {
	return func(w http.ResponseWriter, r *http.Request) {
		verifier, ok := s.Provider.(WebhookVerifier)
		if !ok || !IsMercadoPago(s.Provider) {
			notFound(w, r)
			return
		}
		query := r.URL.Query()
		dataID := strings.TrimSpace(query.Get("data.id"))
		if err := VerifySignature(verifier.WebhookSecret(), r.Header.Get("X-Signature"), r.Header.Get("X-Request-Id"),
			dataID, time.Now().Unix()); err != nil {
			slog.WarnContext(r.Context(), "mercado pago webhook rejected", "error", err)
			http.Error(w, "unauthorized", http.StatusUnauthorized)
			return
		}
		var body struct {
			Type string `json:"type"`
		}
		raw, _ := io.ReadAll(r.Body)
		_ = json.Unmarshal(raw, &body)
		kind := query.Get("type")
		if !query.Has("type") {
			kind = body.Type
		}
		if kind != "payment" {
			reply(w, http.StatusOK, "ignored")
			return
		}
		paymentID, err := strconv.ParseInt(dataID, 10, 64)
		if err != nil {
			reply(w, http.StatusOK, "ignored")
			return
		}

		done := make(chan syncResult, 1)
		// The sync outlives the response when it takes longer than the budget.
		bg, cancel := context.WithTimeout(context.WithoutCancel(r.Context()), backgroundLimit)
		go func(ctx context.Context) {
			defer cancel()
			defer func() {
				if p := recover(); p != nil {
					done <- syncResult{err: fmt.Errorf("panic: %v", p)}
				}
			}()
			out, err := s.SyncPayment(ctx, paymentID, MercadoPagoActor)
			done <- syncResult{out: out, err: err}
		}(bg)

		timer := time.NewTimer(ApplyBudget)
		defer timer.Stop()
		select {
		case res := <-done:
			switch {
			case res.err == nil:
				slog.InfoContext(r.Context(), "mercado pago webhook", "mp_payment_id", paymentID, "outcome", int(res.out.Kind))
				reply(w, http.StatusOK, "ok")
			case IsTransient(res.err):
				slog.WarnContext(r.Context(), "mercado pago unreachable; asking for redelivery", "mp_payment_id", paymentID, "error", res.err)
				reply(w, http.StatusServiceUnavailable, "retry")
			case IsPaymentError(res.err):
				// 4xx from MP (e.g. the dashboard's test notification for a
				// payment that does not exist): redelivery cannot help.
				slog.WarnContext(r.Context(), "mercado pago webhook ignored", "mp_payment_id", paymentID, "error", res.err)
				reply(w, http.StatusOK, "ignored")
			default:
				slog.ErrorContext(r.Context(), "mercado pago webhook failed", "mp_payment_id", paymentID, "error", res.err)
				reply(w, http.StatusServiceUnavailable, "retry")
			}
		case <-timer.C:
			// Still running: it completes in the background; acknowledge now.
			reply(w, http.StatusOK, "accepted")
		}
	}
}
