package payments

import (
	"crypto/hmac"
	"crypto/sha256"
	"encoding/hex"
	"errors"
	"strconv"
	"strings"
)

// Mercado Pago webhook x-signature verification.
//
// Header: "x-signature: ts=<unix>,v1=<hex hmac>". The signed manifest is
// "id:{data.id};request-id:{x-request-id};ts:{ts};" where data.id comes
// from the notification URL's query string (lowercased when alphanumeric)
// and any missing part is omitted. v1 = hex(HMAC_SHA256(secret, manifest)).

// MaxSignatureSkew is how far ts may drift from our clock (replay window).
const MaxSignatureSkew = 300

// Signature errors.
var (
	ErrSignatureMissing   = errors.New("signature missing")
	ErrSignatureMalformed = errors.New("signature malformed")
	ErrSignatureStale     = errors.New("signature stale")
	ErrSignatureMismatch  = errors.New("signature mismatch")
)

func isASCIIAlnum(s string) bool {
	for i := 0; i < len(s); i++ {
		c := s[i]
		if (c < 'a' || c > 'z') && (c < 'A' || c > 'Z') && (c < '0' || c > '9') {
			return false
		}
	}
	return true
}

// Manifest builds the string MP signs.
func Manifest(dataID, requestID, ts string) string {
	var b strings.Builder
	if dataID != "" {
		if isASCIIAlnum(dataID) {
			dataID = strings.ToLower(dataID)
		}
		b.WriteString("id:" + dataID + ";")
	}
	if requestID != "" {
		b.WriteString("request-id:" + requestID + ";")
	}
	b.WriteString("ts:" + ts + ";")
	return b.String()
}

// Sign is the hex HMAC-SHA256 of manifest (also signs test fixtures).
func Sign(secret, manifest string) string {
	m := hmac.New(sha256.New, []byte(secret))
	m.Write([]byte(manifest))
	return hex.EncodeToString(m.Sum(nil))
}

// VerifySignature checks an x-signature header ("" = absent) at nowSecs.
// ts is accepted in seconds or milliseconds.
func VerifySignature(secret, header, requestID, dataID string, nowSecs int64) error {
	if secret == "" || header == "" {
		return ErrSignatureMissing
	}
	var ts, v1 string
	for part := range strings.SplitSeq(header, ",") {
		k, v, ok := strings.Cut(part, "=")
		if !ok {
			continue
		}
		switch strings.TrimSpace(k) {
		case "ts":
			ts = strings.TrimSpace(v)
		case "v1":
			v1 = strings.TrimSpace(v)
		}
	}
	if ts == "" || v1 == "" {
		return ErrSignatureMalformed
	}
	n, err := strconv.ParseInt(ts, 10, 64)
	if err != nil {
		return ErrSignatureMalformed
	}
	if n > 100_000_000_000 {
		n /= 1000
	}
	if d := nowSecs - n; d > MaxSignatureSkew || d < -MaxSignatureSkew {
		return ErrSignatureStale
	}
	expected, err := hex.DecodeString(v1)
	if err != nil {
		return ErrSignatureMismatch
	}
	m := hmac.New(sha256.New, []byte(secret))
	m.Write([]byte(Manifest(dataID, requestID, ts)))
	if !hmac.Equal(m.Sum(nil), expected) {
		return ErrSignatureMismatch
	}
	return nil
}
