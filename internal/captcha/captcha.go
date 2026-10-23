// Package captcha verifies Cloudflare Turnstile tokens (siteverify).
//
// Policy (config.CaptchaConfig):
//   - Not configured (site key or secret missing): no-op, every request passes.
//   - Configured: the widget renders on the protected forms and every
//     presented token is checked with siteverify.
//   - TURNSTILE_ENFORCE on: a missing, rejected or unverifiable token
//     (Cloudflare unreachable, bad response) fails the request.
//   - TURNSTILE_ENFORCE off (report-only): failures are logged and the
//     request passes; an unreachable Cloudflare is a warning.
//
// Forms post the widget's hidden input cf-turnstile-response; JSON API
// bodies carry captchaToken; the x-captcha-response header (better-auth
// convention) works for both.
package captcha

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"io"
	"log/slog"
	"net/http"
	"net/url"
	"strings"
	"time"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/httpx"
)

const (
	// FormField is the widget's hidden input name.
	FormField = "cf-turnstile-response"
	// JSONField is the token key in JSON API bodies.
	JSONField = "captchaToken"
	// Header is the better-auth style token header.
	Header = "X-Captcha-Response"
	// ErrorCode is the JSON API error code of a failed check.
	ErrorCode = "CAPTCHA_FAILED"
	// Message is the pt-BR copy shown when the check fails.
	Message = "Confirme que você não é um robô e tente novamente."
	// Timeout bounds one siteverify call.
	Timeout = 5 * time.Second
	// maxTokenLen is Turnstile's documented token limit.
	maxTokenLen = 2048
)

// Errors describing why a token did not verify.
var (
	ErrMissingToken = errors.New("captcha: missing token")
	ErrInvalidToken = errors.New("captcha: malformed token")
	// ErrUnavailable wraps transport errors, non-2xx and undecodable
	// siteverify responses.
	ErrUnavailable = errors.New("captcha: siteverify unavailable")
)

// RejectedError is a siteverify answer with success=false.
type RejectedError struct{ Codes []string }

func (e *RejectedError) Error() string {
	return "captcha: rejected: " + strings.Join(e.Codes, ",")
}

// verifyResponse is the siteverify JSON body.
type verifyResponse struct {
	Success    bool     `json:"success"`
	ErrorCodes []string `json:"error-codes"`
	Hostname   string   `json:"hostname"`
	Action     string   `json:"action"`
}

// Verifier checks Turnstile tokens. A nil *Verifier is a no-op.
type Verifier struct {
	siteKey  string
	secret   string
	enforced bool
	url      string
	client   *http.Client
}

// New builds a verifier from the config. client may be nil (a private
// client with the siteverify timeout is used).
func New(cfg config.CaptchaConfig, client *http.Client) *Verifier {
	if client == nil {
		client = &http.Client{Timeout: Timeout}
	}
	u := cfg.VerifyURL
	if u == "" {
		u = config.TurnstileVerifyURL
	}
	v := &Verifier{siteKey: cfg.SiteKey, secret: cfg.Secret, enforced: cfg.Enforced, url: u, client: client}
	if cfg.Enforced && !cfg.Configured() {
		slog.Warn("TURNSTILE_ENFORCE is on but TURNSTILE_SITE_KEY/TURNSTILE_SECRET_KEY are not both set: CAPTCHA disabled")
	}
	return v
}

// Enabled reports whether tokens are verified (both keys configured).
func (v *Verifier) Enabled() bool { return v != nil && v.siteKey != "" && v.secret != "" }

// Enforced reports whether failures reject the request.
func (v *Verifier) Enforced() bool { return v.Enabled() && v.enforced }

// SiteKey is the widget site key, "" when disabled (no widget renders).
func (v *Verifier) SiteKey() string {
	if !v.Enabled() {
		return ""
	}
	return v.siteKey
}

// Verify checks token with siteverify. ok reports whether the request may
// proceed under the policy; err (when non-nil) says why the token did not
// verify, also when ok is true in report-only mode.
func (v *Verifier) Verify(ctx context.Context, token, remoteIP string) (ok bool, err error) {
	if !v.Enabled() {
		return true, nil
	}
	token = strings.TrimSpace(token)
	switch {
	case token == "":
		err = ErrMissingToken
	case len(token) > maxTokenLen:
		err = ErrInvalidToken
	default:
		err = v.siteverify(ctx, token, remoteIP)
	}
	if err == nil {
		return true, nil
	}
	if v.enforced {
		if errors.Is(err, ErrUnavailable) {
			slog.ErrorContext(ctx, "captcha: siteverify unavailable; rejecting (TURNSTILE_ENFORCE on)", "error", err)
		}
		return false, err
	}
	if errors.Is(err, ErrUnavailable) {
		slog.WarnContext(ctx, "captcha: siteverify unavailable; failing open (TURNSTILE_ENFORCE off)", "error", err)
	} else {
		slog.InfoContext(ctx, "captcha: token did not verify; accepted (TURNSTILE_ENFORCE off)", "error", err)
	}
	return true, err
}

// Check verifies the token of r (see Token) against the client IP and
// reports whether the request may proceed.
func (v *Verifier) Check(r *http.Request, token string) bool {
	if !v.Enabled() {
		return true
	}
	ok, _ := v.Verify(r.Context(), token, remoteIP(r))
	return ok
}

// CheckForm verifies the form token of r (FormField, else the Header).
// The form must already be parsed.
func (v *Verifier) CheckForm(r *http.Request) bool { return v.Check(r, Token(r, "")) }

// Token is the token presented by r: bodyToken (the JSON field or form
// value) when set, else the FormField form value, else the Header.
func Token(r *http.Request, bodyToken string) string {
	if t := strings.TrimSpace(bodyToken); t != "" {
		return t
	}
	if r.PostForm != nil {
		if t := strings.TrimSpace(r.PostForm.Get(FormField)); t != "" {
			return t
		}
	}
	return strings.TrimSpace(r.Header.Get(Header))
}

func remoteIP(r *http.Request) string {
	ip := httpx.ClientIP(r)
	if ip == "unknown" {
		return ""
	}
	return ip
}

func (v *Verifier) siteverify(ctx context.Context, token, ip string) error {
	ctx, cancel := context.WithTimeout(ctx, Timeout)
	defer cancel()
	form := url.Values{"secret": {v.secret}, "response": {token}}
	if ip != "" {
		form.Set("remoteip", ip)
	}
	req, err := http.NewRequestWithContext(ctx, http.MethodPost, v.url, strings.NewReader(form.Encode()))
	if err != nil {
		return fmt.Errorf("%w: %w", ErrUnavailable, err)
	}
	req.Header.Set("Content-Type", "application/x-www-form-urlencoded")
	res, err := v.client.Do(req)
	if err != nil {
		return fmt.Errorf("%w: %w", ErrUnavailable, err)
	}
	defer func() { _ = res.Body.Close() }()
	if res.StatusCode/100 != 2 {
		_, _ = io.Copy(io.Discard, io.LimitReader(res.Body, 64<<10))
		return fmt.Errorf("%w: status %d", ErrUnavailable, res.StatusCode)
	}
	var body verifyResponse
	if err := json.NewDecoder(io.LimitReader(res.Body, 64<<10)).Decode(&body); err != nil {
		return fmt.Errorf("%w: decode: %w", ErrUnavailable, err)
	}
	if !body.Success {
		return &RejectedError{Codes: body.ErrorCodes}
	}
	slog.DebugContext(ctx, "captcha: token verified", "hostname", body.Hostname, "action", body.Action)
	return nil
}
