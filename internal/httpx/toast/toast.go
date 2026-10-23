// Package toast is the flash-notification system (port of
// src/components/toast.rs, adapted to htmx).
package toast

import (
	"context"
	"encoding/json"
	"net/http"
	"net/url"
	"strings"
	"sync"
	"unicode"
	"unicode/utf8"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/httpx"
)

// Transport:
//
//   - Full page requests: Set queues the toast in an HttpOnly cookie
//     (`toast`, or `__Host-toast` outside development); the root layout
//     calls Take on the next full render and the toaster renders them.
//   - htmx requests: Set puts the toast on the `X-Toast` response
//     header (percent-encoded JSON array) and no cookie is written. The
//     toaster element is hx-preserve'd, so it survives HX-Location / boosted
//     swaps, and web/static/js/toast.js shows header toasts as they arrive.
//   - Redirect is 303 for regular requests
//     and 200 + HX-Location for htmx requests.

// Toast cookie names and limits.
const (
	CookieDev     = "toast"
	CookieSecure  = "__Host-toast"
	Header        = "X-Toast"
	toastMaxChars = 180
	maxToasts     = 5
	cookieBudget  = 3500
)

// Type is the visual tone; "default" has no icon.
type Type string

// Toast tones.
const (
	Default     Type = "default"
	SuccessType Type = "success"
	InfoType    Type = "info"
	WarningType Type = "warning"
	ErrorType   Type = "error"
	LoadingType Type = "loading"
)

// Link is a link rendered at the end of a toast.
type Link struct {
	Label string `json:"label"`
	Href  string `json:"href"`
}

// Toast is one queued notification. Duration 0 stays until dismissed; nil
// uses the toaster default.
type Toast struct {
	Type        Type    `json:"type"`
	Title       string  `json:"title"`
	Description string  `json:"description,omitempty"`
	Duration    *uint32 `json:"duration,omitempty"`
	Dismissible bool    `json:"dismissible"`
	Action      *Link   `json:"action,omitempty"`
	Cancel      *Link   `json:"cancel,omitempty"`
	// Shown: already visible before this page loaded, so the enter
	// animation must not play again.
	Shown bool `json:"shown,omitempty"`
}

// UnmarshalJSON defaults Dismissible to true and Type to default.
func (t *Toast) UnmarshalJSON(b []byte) error {
	type raw Toast
	v := raw{Dismissible: true, Type: Default}
	if err := json.Unmarshal(b, &v); err != nil {
		return err
	}
	*t = Toast(v)
	return nil
}

func newToast(kind Type, title string) Toast {
	return Toast{Type: kind, Title: title, Dismissible: true}
}

// Plain is a toast without an icon.
func Plain(title string) Toast { return newToast(Default, title) }

// Success is a success toast.
func Success(title string) Toast { return newToast(SuccessType, title) }

// Info is an informational toast.
func Info(title string) Toast { return newToast(InfoType, title) }

// Warning is a warning toast.
func Warning(title string) Toast { return newToast(WarningType, title) }

// Error is an error toast.
func Error(title string) Toast { return newToast(ErrorType, title) }

// Loading is a loading toast that stays until replaced or dismissed.
func Loading(title string) Toast { return newToast(LoadingType, title).WithDuration(0) }

// WithDescription sets the secondary text.
func (t Toast) WithDescription(d string) Toast { t.Description = d; return t }

// WithDuration sets the display time in ms (0 = until dismissed).
func (t Toast) WithDuration(ms uint32) Toast { t.Duration = &ms; return t }

// WithDismissible toggles the close button / swipe.
func (t Toast) WithDismissible(v bool) Toast { t.Dismissible = v; return t }

// WithAction adds a primary link.
func (t Toast) WithAction(label, href string) Toast {
	t.Action = &Link{Label: label, Href: href}
	return t
}

// WithCancel adds a secondary link.
func (t Toast) WithCancel(label, href string) Toast {
	t.Cancel = &Link{Label: label, Href: href}
	return t
}

func clip(s string, maxChars int) string {
	s = strings.TrimFunc(s, unicode.IsSpace)
	if utf8.RuneCountInString(s) <= maxChars {
		return s
	}
	return string([]rune(s)[:maxChars])
}

func clipLink(l *Link) *Link {
	if l == nil {
		return nil
	}
	label, href := clip(l.Label, 80), clip(l.Href, 300)
	if label == "" || href == "" {
		return nil
	}
	return &Link{Label: label, Href: href}
}

func sanitizeToast(t Toast) (Toast, bool) {
	t.Title = clip(t.Title, toastMaxChars)
	if t.Title == "" {
		return t, false
	}
	t.Description = clip(t.Description, toastMaxChars)
	t.Action = clipLink(t.Action)
	t.Cancel = clipLink(t.Cancel)
	if t.Type == "" {
		t.Type = Default
	}
	return t, true
}

// encodeToastValue percent-encodes JSON for cookie values and the X-Toast
// header (JS decodes with decodeURIComponent).
func encodeToastValue(v any) string {
	b, err := json.Marshal(v)
	if err != nil {
		return ""
	}
	return url.PathEscape(string(b))
}

func parseQueueBody(v string) ([]Toast, bool) {
	if msg, ok := strings.CutPrefix(v, "ok:"); ok {
		if t, ok := sanitizeToast(newToast(SuccessType, msg)); ok {
			return []Toast{t}, true
		}
		return nil, true
	}
	if msg, ok := strings.CutPrefix(v, "err:"); ok {
		if t, ok := sanitizeToast(newToast(ErrorType, msg)); ok {
			return []Toast{t}, true
		}
		return nil, true
	}
	if strings.HasPrefix(v, "[") {
		var q []Toast
		if err := json.Unmarshal([]byte(v), &q); err != nil {
			return nil, false
		}
		return q, true
	}
	return nil, false
}

// DecodeQueue reads a cookie value: raw JSON, percent-encoded JSON,
// or a legacy `ok:` / `err:` prefix.
func DecodeQueue(value string) []Toast {
	value = strings.TrimSpace(value)
	if q, ok := parseQueueBody(value); ok {
		return q
	}
	if dec, err := url.PathUnescape(value); err == nil {
		if q, ok := parseQueueBody(dec); ok {
			return q
		}
	}
	return nil
}

// enqueueToast appends, keeps the newest five, then drops the oldest until
// the Set-Cookie line fits the budget.
func enqueueToast(queue []Toast, t Toast, secure bool) []Toast {
	queue = append(queue, t)
	if len(queue) > maxToasts {
		queue = queue[len(queue)-maxToasts:]
	}
	for len(queue) > 1 {
		if len(toastCookie(encodeToastValue(queue), secure).String()) <= cookieBudget {
			break
		}
		queue = queue[1:]
	}
	return queue
}

// CookieName is the env-appropriate cookie name.
func CookieName(secure bool) string {
	if secure {
		return CookieSecure
	}
	return CookieDev
}

func toastCookie(value string, secure bool) *http.Cookie {
	// Secure is off only in development (local HTTP); HttpOnly + Lax always.
	return &http.Cookie{ //nolint:gosec // G124: Secure is env-dependent by design
		Name: CookieName(secure), Value: value, Path: "/", MaxAge: 60,
		HttpOnly: true, SameSite: http.SameSiteLaxMode, Secure: secure,
	}
}

type ctxKey struct{}

var stateKey ctxKey

// toastState is the per-request queue (installed by the Toasts middleware).
type toastState struct {
	mu     sync.Mutex
	secure bool
	queue  []Toast // cookie queue as written in this response (nil = untouched)
	wrote  bool
	header []Toast // toasts sent on X-Toast
}

// Middleware installs per-request toast state. secure selects the __Host-
// cookie (everything except APP_ENV development/dev).
func Middleware(secure bool) func(http.Handler) http.Handler {
	return func(next http.Handler) http.Handler {
		return http.HandlerFunc(func(w http.ResponseWriter, r *http.Request) {
			st := &toastState{secure: secure}
			next.ServeHTTP(w, r.WithContext(context.WithValue(r.Context(), stateKey, st)))
		})
	}
}

func stateFor(r *http.Request) *toastState {
	if st, ok := r.Context().Value(stateKey).(*toastState); ok {
		return st
	}
	return &toastState{secure: config.SecureCookies()}
}

// Set queues a toast (see the package comment for the transport).
func Set(w http.ResponseWriter, r *http.Request, t Toast) {
	t, ok := sanitizeToast(t)
	if !ok {
		return
	}
	st := stateFor(r)
	st.mu.Lock()
	defer st.mu.Unlock()
	if httpx.IsHTMX(r) {
		st.header = append(st.header, t)
		if len(st.header) > maxToasts {
			st.header = st.header[len(st.header)-maxToasts:]
		}
		w.Header().Set(Header, encodeToastValue(st.header))
		return
	}
	if !st.wrote {
		if c, err := r.Cookie(CookieName(st.secure)); err == nil {
			st.queue = DecodeQueue(c.Value)
		}
	}
	st.queue = enqueueToast(st.queue, t, st.secure)
	st.wrote = true
	setCookieReplacing(w, toastCookie(encodeToastValue(st.queue), st.secure))
}

// EmitHeader forwards already-taken toasts (e.g. cookie toasts found
// while rendering a page for an htmx request) on the X-Toast header.
func EmitHeader(w http.ResponseWriter, r *http.Request, toasts []Toast) {
	if len(toasts) == 0 {
		return
	}
	st := stateFor(r)
	st.mu.Lock()
	defer st.mu.Unlock()
	for _, t := range toasts {
		if t, ok := sanitizeToast(t); ok {
			st.header = append(st.header, t)
		}
	}
	if len(st.header) > maxToasts {
		st.header = st.header[len(st.header)-maxToasts:]
	}
	w.Header().Set(Header, encodeToastValue(st.header))
}

// Take reads and clears the toast cookie (root layout only). The
// clearing Set-Cookie mirrors the write attributes so it overwrites the
// cookie in every env.
func Take(w http.ResponseWriter, r *http.Request) []Toast {
	st := stateFor(r)
	st.mu.Lock()
	defer st.mu.Unlock()
	var q []Toast
	switch {
	case st.wrote:
		q = st.queue
	default:
		c, err := r.Cookie(CookieName(st.secure))
		if err != nil {
			return nil
		}
		q = DecodeQueue(c.Value)
	}
	expired := toastCookie("", st.secure) //nolint:gosec // G124: see toastCookie
	expired.MaxAge = -1
	setCookieReplacing(w, expired)
	st.queue, st.wrote = nil, true
	return q
}

// Redirect redirects after Set (303, or 200 + HX-Location for htmx).
func Redirect(w http.ResponseWriter, r *http.Request, location string) {
	httpx.Redirect(w, r, location)
}

// setCookieReplacing adds c, dropping an earlier Set-Cookie for the same
// name in this response.
func setCookieReplacing(w http.ResponseWriter, c *http.Cookie) {
	h := w.Header()
	prefix := c.Name + "="
	kept := h.Values("Set-Cookie")[:0:0]
	for _, v := range h.Values("Set-Cookie") {
		if !strings.HasPrefix(v, prefix) {
			kept = append(kept, v)
		}
	}
	h.Del("Set-Cookie")
	for _, v := range kept {
		h.Add("Set-Cookie", v)
	}
	if v := c.String(); v != "" {
		h.Add("Set-Cookie", v)
	}
}
