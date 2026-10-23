// Command fakemp is a tiny stand-in for the Mercado Pago Payments API used
// by the end-to-end suite (e2e/). It is its own Go module so it is never
// part of the main module's build, lint or tests.
//
// API (the subset lindaflor calls, see internal/payments/mercadopago.go):
//
//	POST /v1/payments                 create a pending PIX payment (idempotent per X-Idempotency-Key)
//	GET  /v1/payments/{id}            payment status
//	POST /v1/payments/{id}/refunds    full refund
//
// Control endpoints for the test driver:
//
//	GET  /_fake/payments                         every payment (JSON)
//	POST /_fake/approve?id=…|external_reference=… mark approved and send a signed webhook
//	POST /_fake/webhook?id=…                      resend the signed webhook only
//
// Webhooks are signed like Mercado Pago does (x-signature ts=…,v1=HMAC over
// "id:{data.id};request-id:{x-request-id};ts:{ts};") and posted to the
// payment's notification_url, or FAKEMP_WEBHOOK_URL when set.
package main

import (
	"bytes"
	"crypto/hmac"
	"crypto/sha256"
	"encoding/base64"
	"encoding/hex"
	"encoding/json"
	"flag"
	"fmt"
	"image"
	"image/color"
	"image/png"
	"io"
	"log"
	"net/http"
	"os"
	"strconv"
	"strings"
	"sync"
	"time"
)

type server struct {
	token, secret, webhookURL string

	mu       sync.Mutex
	payments map[int64]map[string]any
	notify   map[int64]string
	byKey    map[string]int64
	refunds  map[int64]int
	nextID   int64
	qrPNG    string
}

func main() {
	addr := flag.String("addr", envOr("FAKEMP_ADDR", "127.0.0.1:4290"), "listen address")
	flag.Parse()
	s := &server{
		token:      envOr("MP_ACCESS_TOKEN", "TEST-fake-token"),
		secret:     envOr("MP_WEBHOOK_SECRET", "fake-webhook-secret"),
		webhookURL: os.Getenv("FAKEMP_WEBHOOK_URL"),
		payments:   map[int64]map[string]any{},
		notify:     map[int64]string{},
		byKey:      map[string]int64{},
		refunds:    map[int64]int{},
		nextID:     time.Now().Unix() * 1000,
		qrPNG:      qrPlaceholder(),
	}
	log.Printf("fakemp listening on %s", *addr)
	srv := &http.Server{Addr: *addr, Handler: s, ReadHeaderTimeout: 5 * time.Second}
	log.Fatal(srv.ListenAndServe())
}

func envOr(k, def string) string {
	if v := os.Getenv(k); v != "" {
		return v
	}
	return def
}

// qrPlaceholder is a small checkerboard PNG standing in for MP's QR image.
func qrPlaceholder() string {
	img := image.NewGray(image.Rect(0, 0, 64, 64))
	for y := range 64 {
		for x := range 64 {
			if (x/8+y/8)%2 == 0 {
				img.Set(x, y, color.Black)
			} else {
				img.Set(x, y, color.White)
			}
		}
	}
	var buf bytes.Buffer
	_ = png.Encode(&buf, img)
	return base64.StdEncoding.EncodeToString(buf.Bytes())
}

func writeJSON(w http.ResponseWriter, code int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.WriteHeader(code)
	_ = json.NewEncoder(w).Encode(v)
}

func (s *server) ServeHTTP(w http.ResponseWriter, r *http.Request) {
	raw, _ := io.ReadAll(r.Body)
	log.Printf("%s %s", r.Method, r.URL.RequestURI())
	if strings.HasPrefix(r.URL.Path, "/_fake/") {
		s.control(w, r)
		return
	}
	if r.Header.Get("Authorization") != "Bearer "+s.token {
		writeJSON(w, http.StatusUnauthorized, map[string]any{"message": "invalid access token"})
		return
	}
	var body map[string]any
	_ = json.Unmarshal(raw, &body)
	path := r.URL.Path
	s.mu.Lock()
	defer s.mu.Unlock()
	switch {
	case r.Method == http.MethodPost && path == "/v1/payments":
		key := r.Header.Get("X-Idempotency-Key")
		if id, ok := s.byKey[key]; ok && key != "" {
			writeJSON(w, http.StatusOK, s.payments[id])
			return
		}
		s.nextID++
		id := s.nextID
		p := map[string]any{
			"id": id, "status": "pending", "status_detail": "pending_waiting_transfer",
			"external_reference": body["external_reference"], "transaction_amount": body["transaction_amount"],
			"description": body["description"], "date_of_expiration": body["date_of_expiration"],
			"point_of_interaction": map[string]any{"transaction_data": map[string]any{
				"qr_code":        fmt.Sprintf("00020101021226850014br.gov.bcb.pix2563qr.fakemp.test/%d5204000053039865802BR5909LINDAFLOR6007ARACAJU62070503***6304ABCD", id),
				"qr_code_base64": s.qrPNG,
				"ticket_url":     fmt.Sprintf("https://fakemp.test/ticket/%d", id),
			}},
		}
		s.payments[id] = p
		if u, ok := body["notification_url"].(string); ok {
			s.notify[id] = u
		}
		if key != "" {
			s.byKey[key] = id
		}
		writeJSON(w, http.StatusCreated, p)
	case r.Method == http.MethodPost && strings.HasSuffix(path, "/refunds"):
		id, _ := strconv.ParseInt(strings.TrimSuffix(strings.TrimPrefix(path, "/v1/payments/"), "/refunds"), 10, 64)
		p, ok := s.payments[id]
		if !ok {
			writeJSON(w, http.StatusNotFound, map[string]any{"message": "payment not found"})
			return
		}
		p["status"] = "refunded"
		p["status_detail"] = "refunded"
		s.refunds[id]++
		writeJSON(w, http.StatusCreated, map[string]any{"id": id*10 + int64(s.refunds[id]), "payment_id": id, "status": "approved"})
	case r.Method == http.MethodGet && strings.HasPrefix(path, "/v1/payments/"):
		id, _ := strconv.ParseInt(strings.TrimPrefix(path, "/v1/payments/"), 10, 64)
		p, ok := s.payments[id]
		if !ok {
			writeJSON(w, http.StatusNotFound, map[string]any{"message": "payment not found"})
			return
		}
		writeJSON(w, http.StatusOK, p)
	default:
		writeJSON(w, http.StatusNotFound, map[string]any{"message": "no route"})
	}
}

func (s *server) control(w http.ResponseWriter, r *http.Request) {
	q := r.URL.Query()
	switch r.URL.Path {
	case "/_fake/payments":
		s.mu.Lock()
		out := make([]map[string]any, 0, len(s.payments))
		for id, p := range s.payments {
			c := map[string]any{"refunds": s.refunds[id]}
			for k, v := range p {
				c[k] = v
			}
			out = append(out, c)
		}
		s.mu.Unlock()
		writeJSON(w, http.StatusOK, out)
	case "/_fake/approve", "/_fake/webhook":
		id := s.find(q.Get("id"), q.Get("external_reference"))
		if id == 0 {
			writeJSON(w, http.StatusNotFound, map[string]any{"message": "payment not found"})
			return
		}
		if r.URL.Path == "/_fake/approve" {
			status := q.Get("status")
			if status == "" {
				status = "approved"
			}
			s.mu.Lock()
			s.payments[id]["status"] = status
			s.payments[id]["status_detail"] = "accredited"
			s.mu.Unlock()
		}
		code, resp, err := s.sendWebhook(id)
		if err != nil {
			writeJSON(w, http.StatusBadGateway, map[string]any{"message": err.Error()})
			return
		}
		writeJSON(w, http.StatusOK, map[string]any{"id": id, "webhook_status": code, "webhook_body": resp})
	default:
		writeJSON(w, http.StatusNotFound, map[string]any{"message": "no route"})
	}
}

func (s *server) find(id, ref string) int64 {
	s.mu.Lock()
	defer s.mu.Unlock()
	if n, err := strconv.ParseInt(id, 10, 64); err == nil {
		if _, ok := s.payments[n]; ok {
			return n
		}
	}
	for pid, p := range s.payments {
		if ref != "" && p["external_reference"] == ref {
			return pid
		}
	}
	return 0
}

func (s *server) sendWebhook(id int64) (int, string, error) {
	s.mu.Lock()
	target := s.notify[id]
	s.mu.Unlock()
	if s.webhookURL != "" {
		target = s.webhookURL
	}
	if target == "" {
		return 0, "", fmt.Errorf("no notification_url for payment %d (set FAKEMP_WEBHOOK_URL)", id)
	}
	dataID := strconv.FormatInt(id, 10)
	requestID := fmt.Sprintf("fakemp-%d-%d", id, time.Now().UnixNano())
	ts := strconv.FormatInt(time.Now().Unix(), 10)
	manifest := "id:" + dataID + ";request-id:" + requestID + ";ts:" + ts + ";"
	mac := hmac.New(sha256.New, []byte(s.secret))
	mac.Write([]byte(manifest))
	sig := "ts=" + ts + ",v1=" + hex.EncodeToString(mac.Sum(nil))

	body, _ := json.Marshal(map[string]any{
		"action": "payment.updated", "api_version": "v1", "type": "payment",
		"data": map[string]any{"id": dataID}, "live_mode": false,
	})
	sep := "?"
	if strings.Contains(target, "?") {
		sep = "&"
	}
	req, err := http.NewRequest(http.MethodPost, target+sep+"data.id="+dataID+"&type=payment", bytes.NewReader(body))
	if err != nil {
		return 0, "", err
	}
	req.Header.Set("Content-Type", "application/json")
	req.Header.Set("X-Signature", sig)
	req.Header.Set("X-Request-Id", requestID)
	res, err := (&http.Client{Timeout: 30 * time.Second}).Do(req)
	if err != nil {
		return 0, "", err
	}
	defer res.Body.Close()
	b, _ := io.ReadAll(res.Body)
	return res.StatusCode, strings.TrimSpace(string(b)), nil
}
