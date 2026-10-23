package auth

import (
	"context"
	"errors"
	"fmt"
	"net/http"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/config"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/storage"
	"github.com/wellington-vell/lindaflor/internal/valkey"
)

// CredentialProviderID is accounts.provider_id of email/password logins.
const CredentialProviderID = "credential"

// Service implements every auth operation (port of src/auth/service.rs and
// the /api/auth route bodies). Handlers in authapi/authpages call it.
type Service struct {
	Cfg    *config.Config
	Pool   *pgxpool.Pool
	Q      *sqlc.Queries
	Valkey *valkey.Client
	Store  storage.Store
	// HTTP talks to Google's token/userinfo endpoints.
	HTTP *http.Client
}

// NewService builds the service. vk and store may be nil (fallback-only
// Valkey / unavailable storage).
func NewService(cfg *config.Config, pool *pgxpool.Pool, vk *valkey.Client, store storage.Store) *Service {
	if vk == nil {
		vk = valkey.NewFallbackOnly()
	}
	if store == nil {
		store = storage.Unavailable{}
	}
	return &Service{
		Cfg: cfg, Pool: pool, Q: sqlc.New(pool), Valkey: vk, Store: store,
		HTTP: &http.Client{Timeout: 10 * time.Second},
	}
}

func (s *Service) secure() bool { return s.Cfg.SecureCookies }

// TokenHash hashes a one-time token with the configured pepper.
func (s *Service) TokenHash(token string) string {
	return VerificationTokenHashWithPepper(token, s.Cfg.TokenPepper)
}

func newID() uuid.UUID {
	id, err := uuid.NewV7()
	if err != nil {
		panic("uuid v7: " + err.Error())
	}
	return id
}

func strPtr(s string) *string { return &s }

// LoadUser loads a user by id (400 "user not found" when missing).
func (s *Service) LoadUser(ctx context.Context, id uuid.UUID) (*User, error) {
	row, err := s.Q.GetUserByID(ctx, id)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, BadRequest("user not found")
	}
	if err != nil {
		return nil, fmt.Errorf("load user: %w", err)
	}
	return UserFromRow(row), nil
}

// StartSession inserts a session for user and writes the cookie (no login
// hooks: the JSON API, Google callback and impersonation paths).
func (s *Service) StartSession(w http.ResponseWriter, r *http.Request, user *User, impersonatedBy *uuid.UUID) (*Session, error) {
	return insertSession(r.Context(), w, r, s.Q, s.secure(), user.ID, impersonatedBy)
}

// StartSessionForUser is StartSession plus the login hooks (guest-cart
// merge) for non-impersonated sessions — Rust start_session_for_user.
func (s *Service) StartSessionForUser(w http.ResponseWriter, r *http.Request, user *User, impersonatedBy *uuid.UUID) (*Session, error) {
	sess, err := s.StartSession(w, r, user, impersonatedBy)
	if err != nil {
		return nil, err
	}
	if impersonatedBy == nil {
		if err := runLoginHooks(w, r, s.Pool, user.ID); err != nil {
			return nil, fmt.Errorf("login hook: %w", err)
		}
	}
	return sess, nil
}

// StopSession deletes the current session row and expires the cookie.
func (s *Service) StopSession(w http.ResponseWriter, r *http.Request) error {
	_, err := stopSession(r.Context(), w, r, s.Q, s.secure())
	return err
}

// SignOut stops the session and expires the cart cookie (API sign-out and
// HTML logout).
func (s *Service) SignOut(w http.ResponseWriter, r *http.Request) error {
	err := s.StopSession(w, r)
	ClearCartCookie(w, s.secure())
	return err
}

// ---------------------------------------------------------------------------
// Sign up / sign in
// ---------------------------------------------------------------------------

// SignUpEmail validates, creates the user + credential account and starts a
// session (runHooks: HTML path merges the guest cart).
func (s *Service) SignUpEmail(w http.ResponseWriter, r *http.Request, name, email, password string, runHooks bool) (*User, *Session, error) {
	ctx := r.Context()
	name, err := ValidateName(name)
	if err != nil {
		return nil, nil, err
	}
	email, err = ValidateEmail(email)
	if err != nil {
		return nil, nil, err
	}
	if err := ValidatePassword(password, email, name, "lindaflor"); err != nil {
		return nil, nil, err
	}
	// Hash before the existence check so a taken e-mail answers in the same
	// time as a new one (the 5/min sign-up budget bounds probing).
	hash, err := HashPassword(password)
	if err != nil {
		return nil, nil, err
	}
	taken, err := s.Q.AuthUserExistsByEmail(ctx, email)
	if err != nil {
		return nil, nil, fmt.Errorf("sign-up lookup: %w", err)
	}
	if taken {
		return nil, nil, BadRequest("unable to complete sign-up")
	}
	user := &User{ID: newID(), Name: name, Email: email, Role: strPtr("user")}
	if err := s.Q.AuthInsertUser(ctx, sqlc.AuthInsertUserParams{
		ID: user.ID, Name: name, Email: email, Role: user.Role,
	}); err != nil {
		return nil, nil, fmt.Errorf("insert user: %w", err)
	}
	if err := s.Q.AuthInsertAccount(ctx, sqlc.AuthInsertAccountParams{
		ID: newID(), AccountID: user.ID.String(), ProviderID: CredentialProviderID,
		UserID: user.ID, Password: &hash,
	}); err != nil {
		return nil, nil, fmt.Errorf("insert account: %w", err)
	}
	start := s.StartSession
	if runHooks {
		start = s.StartSessionForUser
	}
	sess, err := start(w, r, user, nil)
	if err != nil {
		return nil, nil, err
	}
	return user, sess, nil
}

// SignInResult is a completed sign-in or a pending second factor.
type SignInResult struct {
	User    *User
	Session *Session
	// PendingToken is set (and User/Session nil) when 2FA is required.
	PendingToken string
}

// ErrSignInRequired is the 400 of empty/invalid credentials input.
var ErrSignInRequired = BadRequest("email and password are required")

// SignInEmail checks the password with lockouts (per ip+email and per
// email), a dummy Argon2 for unknown accounts, the ban rule, and either
// starts a session or creates a pending-2FA token. strictEmail (API)
// rejects malformed emails with 400; the HTML form only normalizes.
func (s *Service) SignInEmail(w http.ResponseWriter, r *http.Request, email, password string, strictEmail, runHooks bool) (*SignInResult, error) {
	ctx := r.Context()
	if strictEmail {
		e, err := ValidateEmail(email)
		if err != nil {
			return nil, ErrSignInRequired
		}
		email = e
	} else {
		email = NormalizeEmail(email)
		if email == "" {
			return nil, ErrSignInRequired
		}
	}
	if password == "" {
		return nil, ErrSignInRequired
	}
	ip := httpx.ClientIP(r)
	if s.Valkey.IsAuthLockedOut(ctx, ip, email) || s.Valkey.IsAuthEmailLockedOut(ctx, email) {
		return nil, ErrTooManyRequests
	}
	fail := func() (*SignInResult, error) {
		s.Valkey.RecordAuthFail(ctx, ip, email)
		s.Valkey.RecordAuthEmailFail(ctx, email)
		return nil, ErrUnauthorized
	}
	row, err := s.Q.AuthCredentialLogin(ctx, email)
	if errors.Is(err, pgx.ErrNoRows) {
		DummyVerify(password)
		return fail()
	}
	if err != nil {
		return nil, fmt.Errorf("sign-in lookup: %w", err)
	}
	if row.Password == nil {
		DummyVerify(password)
		return fail()
	}
	ok, err := VerifyPassword(password, *row.Password)
	if err != nil {
		return nil, err
	}
	if !ok {
		return fail()
	}
	s.Valkey.ClearAuthFails(ctx, ip, email)
	s.Valkey.ClearAuthEmailFails(ctx, email)
	if IsCurrentlyBanned(row.Banned, row.BanExpires) {
		return nil, ErrUnauthorized
	}
	if row.TwoFactorEnabled {
		token, err := s.CreatePending2FA(r, row.ID)
		if err != nil {
			return nil, err
		}
		return &SignInResult{PendingToken: token}, nil
	}
	user := &User{
		ID: row.ID, Name: row.Name, Email: row.Email, EmailVerified: row.EmailVerified, Image: row.Image,
		TwoFactorEnabled: row.TwoFactorEnabled, Role: row.Role, Banned: row.Banned,
		BanReason: row.BanReason, BanExpires: row.BanExpires,
	}
	start := s.StartSession
	if runHooks {
		start = s.StartSessionForUser
	}
	sess, err := start(w, r, user, nil)
	if err != nil {
		return nil, err
	}
	return &SignInResult{User: user, Session: sess}, nil
}

// ---------------------------------------------------------------------------
// One-time tokens
// ---------------------------------------------------------------------------

// issueToken replaces every token under identifier (only the newest stays
// valid) and stores a new hashed one; returns the raw token.
func (s *Service) issueToken(ctx context.Context, q *sqlc.Queries, identifier string, ttl time.Duration) (string, error) {
	if err := q.AuthDeleteVerificationsByIdentifier(ctx, identifier); err != nil {
		return "", fmt.Errorf("delete tokens: %w", err)
	}
	token := RandomToken()
	hash := s.TokenHash(token)
	if err := q.AuthInsertVerification(ctx, sqlc.AuthInsertVerificationParams{
		ID: newID(), Identifier: identifier, Value: "", ValueHash: &hash, ExpiresAt: time.Now().Add(ttl),
	}); err != nil {
		return "", fmt.Errorf("insert token: %w", err)
	}
	return token, nil
}

// findToken resolves an unexpired token under prefix (nil row when absent).
func (s *Service) findToken(ctx context.Context, token, prefix string) (*sqlc.AuthFindVerificationRow, error) {
	hash := s.TokenHash(strings.TrimSpace(token))
	row, err := s.Q.AuthFindVerification(ctx, sqlc.AuthFindVerificationParams{ValueHash: &hash, IdentifierLike: prefix + "%"})
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, nil
	}
	if err != nil {
		return nil, fmt.Errorf("find token: %w", err)
	}
	return &row, nil
}

// ---------------------------------------------------------------------------
// Pending second factor
// ---------------------------------------------------------------------------

// CreatePending2FA stores a 10-minute pending-2FA token for userID (any
// previous one is replaced) bound to this request's IP/UA.
func (s *Service) CreatePending2FA(r *http.Request, userID uuid.UUID) (string, error) {
	ctx := r.Context()
	token, err := s.issueToken(ctx, s.Q, Pending2FAPrefix+userID.String(), Pending2FATTL)
	if err != nil {
		return "", err
	}
	s.Valkey.PendingBindEager(ctx, token, userID, httpx.ClientIP(r), httpx.UserAgent(r))
	return token, nil
}

func (s *Service) resolvePending(ctx context.Context, token string) (verificationID, userID uuid.UUID, err error) {
	row, err := s.findToken(ctx, token, Pending2FAPrefix)
	if err != nil {
		return uuid.Nil, uuid.Nil, err
	}
	if row == nil {
		return uuid.Nil, uuid.Nil, ErrUnauthorized
	}
	uid, err := uuid.Parse(strings.TrimPrefix(row.Identifier, Pending2FAPrefix))
	if err != nil {
		return uuid.Nil, uuid.Nil, ErrUnauthorized
	}
	return row.ID, uid, nil
}

// guardPending2FA: attempt cap (row deleted past the cap) and IP/UA
// binding. Runs before any code verification.
func (s *Service) guardPending2FA(r *http.Request, token string, userID, verificationID uuid.UUID) error {
	ctx := r.Context()
	if s.Valkey.PendingAttempt(ctx, token) > valkey.PendingMaxAttempts {
		if err := s.Q.AuthDeleteVerification(ctx, verificationID); err != nil {
			return err
		}
		s.Valkey.PendingClear(ctx, token)
		return ErrUnauthorized
	}
	if s.Valkey.PendingCheckBinding(ctx, token, userID, httpx.ClientIP(r), httpx.UserAgent(r)) == valkey.BindingMismatch {
		return ErrUnauthorized
	}
	return nil
}

// Complete2FA finishes a pending sign-in with a TOTP code (backup=false) or
// a backup code. runHooks: the HTML flow merges the guest cart.
func (s *Service) Complete2FA(w http.ResponseWriter, r *http.Request, pendingToken, code string, backup, runHooks bool) (*User, *Session, error) {
	ctx := r.Context()
	code = strings.TrimSpace(code)
	if code == "" {
		return nil, nil, BadRequest("code is required")
	}
	pendingToken = strings.TrimSpace(pendingToken)
	verificationID, userID, err := s.resolvePending(ctx, pendingToken)
	if err != nil {
		return nil, nil, err
	}
	tf, err := s.Q.AuthTwoFactorByUser(ctx, userID)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, nil, ErrUnauthorized
	}
	if err != nil {
		return nil, nil, err
	}
	if !tf.Verified {
		return nil, nil, ErrUnauthorized
	}
	if err := s.guardPending2FA(r, pendingToken, userID, verificationID); err != nil {
		return nil, nil, err
	}
	if backup {
		if err := s.ensureTOTPNotLocked(ctx, userID); err != nil {
			return nil, nil, err
		}
		remaining, ok := ConsumeBackupCode(tf.BackupCodes, code)
		if !ok {
			s.Valkey.RecordTotpFail(ctx, userID)
			return nil, nil, ErrUnauthorized
		}
		if err := s.Q.AuthSetBackupCodes(ctx, sqlc.AuthSetBackupCodesParams{ID: tf.ID, BackupCodes: remaining}); err != nil {
			return nil, nil, err
		}
	} else {
		secret, err := DecryptTOTPSecret(tf.SecretNonce, tf.SecretCiphertext, tf.SecretKeyVersion)
		if err != nil {
			return nil, nil, ErrUnauthorized
		}
		ok, err := s.CheckTOTPCode(ctx, userID, secret, code)
		if err != nil {
			return nil, nil, err
		}
		if !ok {
			return nil, nil, ErrUnauthorized
		}
	}
	if err := s.Q.AuthDeleteVerification(ctx, verificationID); err != nil {
		return nil, nil, err
	}
	s.Valkey.PendingClear(ctx, pendingToken)
	user, err := s.LoadUser(ctx, userID)
	if err != nil {
		return nil, nil, err
	}
	if IsCurrentlyBanned(user.Banned, user.BanExpires) {
		return nil, nil, ErrUnauthorized
	}
	start := s.StartSession
	if runHooks {
		start = s.StartSessionForUser
	}
	sess, err := start(w, r, user, nil)
	if err != nil {
		return nil, nil, err
	}
	return user, sess, nil
}

// Pending-2FA cookie (HTML login flow, 10 minutes).

// SetPending2FACookie stores the pending token for the /two-factor page.
func (s *Service) SetPending2FACookie(w http.ResponseWriter, token string) {
	SetCookie(w, cookieName(s.secure(), Pending2FACookieDev, Pending2FACookieSecure), token, Pending2FATTL, s.secure())
}

// ClearPending2FACookie expires it.
func (s *Service) ClearPending2FACookie(w http.ResponseWriter) {
	ClearCookie(w, cookieName(s.secure(), Pending2FACookieDev, Pending2FACookieSecure), s.secure())
}

// ReadPending2FACookie returns the pending token ("" when absent).
func (s *Service) ReadPending2FACookie(r *http.Request) string {
	return readCookie(r, cookieName(s.secure(), Pending2FACookieDev, Pending2FACookieSecure))
}
