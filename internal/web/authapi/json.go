package authapi

import (
	"bytes"
	"encoding/json"
	"io"
	"log/slog"
	"net/http"
	"time"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/httpx"
)

// errorBody is the JSON shape of every error response.
type errorBody struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

func writeJSON(w http.ResponseWriter, status int, v any) {
	w.Header().Set("Content-Type", "application/json")
	w.Header().Set("Cache-Control", "no-store")
	w.WriteHeader(status)
	if err := json.NewEncoder(w).Encode(v); err != nil {
		slog.Error("authapi: encode response", "error", err)
	}
}

// writeError answers an auth error: client errors keep their status and
// message, anything else is a logged 500 with a generic body.
func writeError(w http.ResponseWriter, r *http.Request, err error) {
	if httpx.IsBodyTooLarge(err) {
		httpx.TooLarge(w)
		return
	}
	status := auth.StatusOf(err)
	if status == http.StatusInternalServerError {
		slog.ErrorContext(r.Context(), "auth api error", "error", err, "path", r.URL.Path)
		writeJSON(w, status, errorBody{Code: "INTERNAL_SERVER_ERROR", Message: "internal server error"})
		return
	}
	if status == http.StatusTooManyRequests {
		w.Header().Set("Retry-After", "60")
	}
	writeJSON(w, status, errorBody{Code: auth.CodeOf(err), Message: err.Error()})
}

var errInvalidBody = auth.BadRequest("invalid JSON body")

// decode reads a JSON body into dst (unknown fields ignored). A missing or
// malformed body is a 400.
func decode(r *http.Request, dst any) error {
	b, err := io.ReadAll(r.Body)
	if err != nil {
		if httpx.IsBodyTooLarge(err) {
			return err
		}
		return errInvalidBody
	}
	if len(bytes.TrimSpace(b)) == 0 {
		return errInvalidBody
	}
	if err := json.Unmarshal(b, dst); err != nil {
		return errInvalidBody
	}
	return nil
}

func formatTime(t time.Time) string { return t.UTC().Format(time.RFC3339Nano) }

// userJSON is the better-auth style user object.
type userJSON struct {
	ID               string  `json:"id"`
	Name             string  `json:"name"`
	Email            string  `json:"email"`
	EmailVerified    bool    `json:"emailVerified"`
	Image            *string `json:"image"`
	Role             *string `json:"role"`
	TwoFactorEnabled bool    `json:"twoFactorEnabled"`
	Banned           bool    `json:"banned"`
	BanReason        *string `json:"banReason"`
	BanExpires       *string `json:"banExpires"`
}

func toUserJSON(u *auth.User) userJSON {
	out := userJSON{
		ID: u.ID.String(), Name: u.Name, Email: u.Email, EmailVerified: u.EmailVerified, Image: u.Image,
		Role: u.Role, TwoFactorEnabled: u.TwoFactorEnabled, Banned: u.Banned, BanReason: u.BanReason,
	}
	if u.BanExpires != nil {
		s := formatTime(*u.BanExpires)
		out.BanExpires = &s
	}
	return out
}

// sessionJSON never carries the raw token.
type sessionJSON struct {
	ID             string  `json:"id"`
	ExpiresAt      string  `json:"expiresAt"`
	UserID         string  `json:"userId"`
	ImpersonatedBy *string `json:"impersonatedBy"`
}

// sessionPayload is the {user, session} body of sign-up/in/get-session.
type sessionPayload struct {
	User    userJSON    `json:"user"`
	Session sessionJSON `json:"session"`
}

func toPayload(u *auth.User, s *auth.Session) sessionPayload {
	sj := sessionJSON{ID: s.ID.String(), ExpiresAt: formatTime(s.ExpiresAt), UserID: u.ID.String()}
	if s.ImpersonatedBy != nil {
		v := s.ImpersonatedBy.String()
		sj.ImpersonatedBy = &v
	}
	return sessionPayload{User: toUserJSON(u), Session: sj}
}

type okStatus struct {
	Status bool `json:"status"`
}

var statusOK = okStatus{Status: true}

// socialUnavailable is the body of unsupported/unconfigured social sign-in.
type socialUnavailable struct {
	Code    string `json:"code"`
	Message string `json:"message"`
}

func parseUUID(raw, field string) (uuid.UUID, error) {
	id, err := uuid.Parse(trim(raw))
	if err != nil {
		return uuid.Nil, auth.BadRequest(field + " must be a valid uuid")
	}
	return id, nil
}
