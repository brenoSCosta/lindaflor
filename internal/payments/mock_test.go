package payments_test

import (
	"encoding/json"
	"fmt"
	"io"
	"math/rand/v2"
	"net/http"
	"net/http/httptest"
	"strconv"
	"strings"
	"sync"
	"testing"
	"time"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/payments"
)

// mockSecret signs webhook fixtures for mock-backed providers.
const mockSecret = "mock-webhook-secret"

type recorded struct {
	Method, Path, IdempotencyKey string
	Body                         map[string]any
}

// mockMP is a fake Mercado Pago API: POST /v1/payments (idempotent per
// X-Idempotency-Key), GET /v1/payments/{id}, POST /v1/payments/{id}/refunds.
// It records every request and can force status codes on the next ones.
type mockMP struct {
	*httptest.Server
	mu       sync.Mutex
	payments map[int64]map[string]any
	byKey    map[string]int64
	forced   []int
	requests []recorded
	nextID   int64
}

func newMockMP(t *testing.T) *mockMP {
	t.Helper()
	m := &mockMP{
		payments: map[int64]map[string]any{},
		byKey:    map[string]int64{},
		// Unique ids per mock: orders.mp_payment_id is unique.
		nextID: 1_000_000_000 + rand.Int64N(8_000_000_000_000), //nolint:gosec // test ids
	}
	m.Server = httptest.NewServer(http.HandlerFunc(m.serve))
	t.Cleanup(m.Close)
	return m
}

func writeJSON(w http.ResponseWriter, code int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(code)
	_ = json.NewEncoder(w).Encode(v)
}

func (m *mockMP) serve(w http.ResponseWriter, r *http.Request) {
	raw, _ := io.ReadAll(r.Body)
	var body map[string]any
	_ = json.Unmarshal(raw, &body)
	m.mu.Lock()
	defer m.mu.Unlock()
	rec := recorded{Method: r.Method, Path: r.URL.Path, IdempotencyKey: r.Header.Get("X-Idempotency-Key"), Body: body}
	m.requests = append(m.requests, rec)
	if len(m.forced) > 0 {
		code := m.forced[0]
		m.forced = m.forced[1:]
		writeJSON(w, code, map[string]any{"message": "forced", "status": code})
		return
	}
	if r.Header.Get("Authorization") != "Bearer TEST-access-token" {
		writeJSON(w, http.StatusUnauthorized, map[string]any{"message": "bad token"})
		return
	}
	path := r.URL.Path
	switch {
	case r.Method == http.MethodPost && path == "/v1/payments":
		if id, ok := m.byKey[rec.IdempotencyKey]; ok && rec.IdempotencyKey != "" {
			writeJSON(w, http.StatusOK, m.payments[id])
			return
		}
		id := m.nextID
		m.nextID++
		p := map[string]any{
			"id": id, "status": "pending", "status_detail": "pending_waiting_transfer",
			"external_reference": body["external_reference"], "transaction_amount": body["transaction_amount"],
			"point_of_interaction": map[string]any{"transaction_data": map[string]any{
				"qr_code": fmt.Sprintf("00020126MOCKPIX%d", id), "qr_code_base64": "iVBORw0KGgo=",
				"ticket_url": fmt.Sprintf("https://mp.test/ticket/%d", id),
			}},
		}
		m.payments[id] = p
		if rec.IdempotencyKey != "" {
			m.byKey[rec.IdempotencyKey] = id
		}
		writeJSON(w, http.StatusCreated, p)
	case r.Method == http.MethodPost && strings.HasSuffix(path, "/refunds"):
		id, _ := strconv.ParseInt(strings.TrimSuffix(strings.TrimPrefix(path, "/v1/payments/"), "/refunds"), 10, 64)
		p, ok := m.payments[id]
		if !ok {
			writeJSON(w, http.StatusNotFound, map[string]any{"message": "not found"})
			return
		}
		p["status"] = "refunded"
		writeJSON(w, http.StatusCreated, map[string]any{"id": 1, "payment_id": id})
	case r.Method == http.MethodGet && strings.HasPrefix(path, "/v1/payments/"):
		id, _ := strconv.ParseInt(strings.TrimPrefix(path, "/v1/payments/"), 10, 64)
		p, ok := m.payments[id]
		if !ok {
			writeJSON(w, http.StatusNotFound, map[string]any{"message": "not found"})
			return
		}
		writeJSON(w, http.StatusOK, p)
	default:
		writeJSON(w, http.StatusNotFound, map[string]any{"message": "no route"})
	}
}

func (m *mockMP) force(codes ...int) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.forced = append(m.forced, codes...)
}

func (m *mockMP) setField(id int64, field string, value any) {
	m.mu.Lock()
	defer m.mu.Unlock()
	m.payments[id][field] = value
}

func (m *mockMP) setStatus(id int64, status string) { m.setField(id, "status", status) }

func (m *mockMP) recorded() []recorded {
	m.mu.Lock()
	defer m.mu.Unlock()
	return append([]recorded(nil), m.requests...)
}

func (m *mockMP) count(method, prefix string) int {
	n := 0
	for _, r := range m.recorded() {
		if r.Method == method && strings.HasPrefix(r.Path, prefix) {
			n++
		}
	}
	return n
}

func (m *mockMP) providerWith(b *payments.CircuitBreaker) *payments.MercadoPago {
	return payments.NewMercadoPagoWith(config.MercadoPagoConfig{
		AccessToken: "TEST-access-token", WebhookSecret: mockSecret, BaseURL: m.URL,
	}, payments.RetryPolicy{MaxAttempts: 3, BaseDelay: time.Millisecond, MaxJitter: time.Millisecond}, b)
}

func (m *mockMP) provider() *payments.MercadoPago {
	return m.providerWith(payments.NewCircuitBreaker(5, 30*time.Second))
}
