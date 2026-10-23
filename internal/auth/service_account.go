package auth

import (
	"context"
	"errors"
	"fmt"
	"log/slog"
	"net/http"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// ErrInvalidToken is the 400 of unknown/expired one-time tokens.
var ErrInvalidToken = BadRequest("invalid or expired token")

// ErrTokenRequired is the 400 of a blank token.
var ErrTokenRequired = BadRequest("token is required")

// ---------------------------------------------------------------------------
// Password reset
// ---------------------------------------------------------------------------

// RequestPasswordReset issues a 1h reset token and queues the mail when the
// address has an account. Always silent (anti-enumeration).
func (s *Service) RequestPasswordReset(ctx context.Context, email, redirectTo string) error {
	email = NormalizeEmail(email)
	if email == "" {
		return nil
	}
	user, err := s.Q.AuthUserLiteByEmail(ctx, email)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil
	}
	if err != nil {
		return fmt.Errorf("reset lookup: %w", err)
	}
	token, err := s.issueToken(ctx, s.Q, ResetIdentifierPrefix+email, ResetTokenTTL)
	if err != nil {
		return err
	}
	if err := s.sendPasswordReset(ctx, s.Pool, email, user.Name, token); err != nil {
		return err
	}
	slog.InfoContext(ctx, "password reset token created; mail queued", "user_id", user.ID,
		"redirect_to", redirectTo, "token_hash_prefix", VerificationTokenHashPrefix(s.TokenHash(token)))
	return nil
}

// ResetPassword consumes a reset token: validates the new password (with
// the account email as zxcvbn input), replaces the credential, burns every
// reset token of the address and revokes all sessions.
func (s *Service) ResetPassword(ctx context.Context, token, newPassword string) error {
	if strings.TrimSpace(token) == "" {
		return ErrTokenRequired
	}
	v, err := s.findToken(ctx, token, ResetIdentifierPrefix)
	if err != nil {
		return err
	}
	if v == nil {
		return ErrInvalidToken
	}
	email := NormalizeEmail(strings.TrimPrefix(v.Identifier, ResetIdentifierPrefix))
	user, err := s.Q.AuthUserLiteByEmail(ctx, email)
	if errors.Is(err, pgx.ErrNoRows) {
		return ErrInvalidToken
	}
	if err != nil {
		return err
	}
	if err := ValidatePassword(newPassword, email, "lindaflor"); err != nil {
		return err
	}
	hash, err := HashPassword(newPassword)
	if err != nil {
		return err
	}
	n, err := s.Q.AuthUpdateCredentialPassword(ctx, sqlc.AuthUpdateCredentialPasswordParams{UserID: user.ID, Password: &hash})
	if err != nil {
		return err
	}
	if n == 0 {
		return BadRequest("no credential account for user")
	}
	if err := s.Q.AuthDeleteVerificationsByIdentifier(ctx, v.Identifier); err != nil {
		return err
	}
	return s.Q.AuthDeleteSessionsForUser(ctx, user.ID)
}

// ---------------------------------------------------------------------------
// Email verification
// ---------------------------------------------------------------------------

// SendVerificationEmail issues a 24h verification token for an unverified
// address and queues the mail. Silent for unknown/verified addresses.
func (s *Service) SendVerificationEmail(ctx context.Context, email, callbackURL string) error {
	email = NormalizeEmail(email)
	if email == "" {
		return ErrEmailRequired
	}
	user, err := s.Q.AuthUserLiteByEmail(ctx, email)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil
	}
	if err != nil {
		return err
	}
	if user.EmailVerified {
		return nil
	}
	token, err := s.issueToken(ctx, s.Q, VerifyIdentifierPrefix+email, VerifyTokenTTL)
	if err != nil {
		return err
	}
	if err := s.sendVerifyEmail(ctx, s.Pool, email, user.Name, token); err != nil {
		return err
	}
	slog.InfoContext(ctx, "email verification token created; mail queued", "user_id", user.ID,
		"callback_url", callbackURL, "token_hash_prefix", VerificationTokenHashPrefix(s.TokenHash(token)))
	return nil
}

// ConsumeEmailVerification marks the token's address verified.
func (s *Service) ConsumeEmailVerification(ctx context.Context, token string) error {
	if strings.TrimSpace(token) == "" {
		return ErrTokenRequired
	}
	v, err := s.findToken(ctx, token, VerifyIdentifierPrefix)
	if err != nil {
		return err
	}
	if v == nil {
		return ErrInvalidToken
	}
	email := NormalizeEmail(strings.TrimPrefix(v.Identifier, VerifyIdentifierPrefix))
	n, err := s.Q.AuthVerifyEmailByAddress(ctx, email)
	if err != nil {
		return err
	}
	if n == 0 {
		return ErrUnauthorized
	}
	return s.Q.AuthDeleteVerification(ctx, v.ID)
}

// ---------------------------------------------------------------------------
// Profile / password / email change
// ---------------------------------------------------------------------------

// UpdateUserName validates and stores a new display name.
func (s *Service) UpdateUserName(ctx context.Context, userID uuid.UUID, name string) (*User, error) {
	name, err := ValidateName(name)
	if err != nil {
		return nil, err
	}
	if err := s.Q.AuthUpdateUserName(ctx, sqlc.AuthUpdateUserNameParams{ID: userID, Name: name}); err != nil {
		return nil, err
	}
	return s.LoadUser(ctx, userID)
}

// ChangePassword verifies the current password, stores the new one and
// optionally revokes every other session (keepSessionID stays).
func (s *Service) ChangePassword(ctx context.Context, user *User, keepSessionID uuid.UUID, current, next string, revokeOthers bool) error {
	if current == "" {
		return BadRequest("currentPassword is required")
	}
	if err := ValidatePassword(next, user.Email, user.Name, "lindaflor"); err != nil {
		return err
	}
	stored, err := s.Q.AuthCredentialPassword(ctx, user.ID)
	if errors.Is(err, pgx.ErrNoRows) || (err == nil && stored == nil) {
		return ErrUnauthorized
	}
	if err != nil {
		return err
	}
	ok, err := VerifyPassword(current, *stored)
	if err != nil {
		return err
	}
	if !ok {
		return ErrUnauthorized
	}
	hash, err := HashPassword(next)
	if err != nil {
		return err
	}
	if _, err := s.Q.AuthUpdateCredentialPassword(ctx, sqlc.AuthUpdateCredentialPasswordParams{UserID: user.ID, Password: &hash}); err != nil {
		return err
	}
	if revokeOthers {
		return s.Q.AuthDeleteSessionsExceptID(ctx, sqlc.AuthDeleteSessionsExceptIDParams{UserID: user.ID, ID: keepSessionID})
	}
	return nil
}

// RequestChangeEmail issues a 30-minute token for newEmail and mails the
// new address. A taken address is a silent success. Callers run step-up
// first (VerifyStepUp) and decide about other sessions.
func (s *Service) RequestChangeEmail(ctx context.Context, user *User, newEmail, callbackURL string) error {
	newEmail, err := ValidateEmail(newEmail)
	if err != nil {
		return err
	}
	if newEmail == NormalizeEmail(user.Email) {
		return BadRequest("newEmail must differ from current email")
	}
	taken, err := s.Q.AuthEmailTakenByOther(ctx, sqlc.AuthEmailTakenByOtherParams{Email: newEmail, ID: user.ID})
	if err != nil {
		return err
	}
	if taken {
		slog.InfoContext(ctx, "change-email skipped; address in use", "user_id", user.ID)
		return nil
	}
	if err := s.Q.AuthDeleteVerificationsLike(ctx, ChangeEmailIdentifierPrefix+user.ID.String()+":%"); err != nil {
		return err
	}
	token, err := s.issueToken(ctx, s.Q, ChangeEmailIdentifierPrefix+user.ID.String()+":"+newEmail, ChangeEmailTokenTTL)
	if err != nil {
		return err
	}
	if err := s.sendChangeEmail(ctx, s.Pool, newEmail, user.Name, token); err != nil {
		return err
	}
	slog.InfoContext(ctx, "change-email confirmation token created; mail queued", "user_id", user.ID,
		"callback_url", callbackURL, "token_hash_prefix", VerificationTokenHashPrefix(s.TokenHash(token)))
	return nil
}

// ChangeEmail is the API flow: forbid impersonation, validate, step-up,
// then (unless the address is taken) issue the token, revoke every other
// session and mail the new address.
func (s *Service) ChangeEmail(r *http.Request, user *User, sess *Session, newEmail, password, code, callbackURL string) error {
	ctx := r.Context()
	if err := ForbidImpersonated(sess); err != nil {
		return err
	}
	email, err := ValidateEmail(newEmail)
	if err != nil {
		return err
	}
	if email == user.Email {
		return BadRequest("newEmail must differ from current email")
	}
	if err := s.VerifyStepUp(r, user.ID, password, code); err != nil {
		return err
	}
	taken, err := s.Q.AuthEmailTakenByOther(ctx, sqlc.AuthEmailTakenByOtherParams{Email: email, ID: user.ID})
	if err != nil {
		return err
	}
	if taken {
		return nil
	}
	if err := s.Q.AuthDeleteVerificationsLike(ctx, ChangeEmailIdentifierPrefix+user.ID.String()+":%"); err != nil {
		return err
	}
	token, err := s.issueToken(ctx, s.Q, ChangeEmailIdentifierPrefix+user.ID.String()+":"+email, ChangeEmailTokenTTL)
	if err != nil {
		return err
	}
	if err := s.Q.AuthDeleteSessionsExceptID(ctx, sqlc.AuthDeleteSessionsExceptIDParams{UserID: user.ID, ID: sess.ID}); err != nil {
		return err
	}
	if err := s.sendChangeEmail(ctx, s.Pool, email, user.Name, token); err != nil {
		return err
	}
	slog.InfoContext(ctx, "change-email confirmation token created; mail queued", "user_id", user.ID,
		"callback_url", callbackURL, "token_hash_prefix", VerificationTokenHashPrefix(s.TokenHash(token)))
	return nil
}

// CompleteChangeEmail consumes a change-email token (works logged out):
// swaps the address (verified), burns the token and revokes every session.
func (s *Service) CompleteChangeEmail(ctx context.Context, token string) error {
	if strings.TrimSpace(token) == "" {
		return ErrTokenRequired
	}
	v, err := s.findToken(ctx, token, ChangeEmailIdentifierPrefix)
	if err != nil {
		return err
	}
	if v == nil {
		return ErrInvalidToken
	}
	rest := strings.TrimPrefix(v.Identifier, ChangeEmailIdentifierPrefix)
	idStr, rawEmail, ok := strings.Cut(rest, ":")
	if !ok {
		return ErrInvalidToken
	}
	userID, err := uuid.Parse(idStr)
	if err != nil {
		return ErrInvalidToken
	}
	newEmail := NormalizeEmail(rawEmail)
	if newEmail == "" {
		return ErrInvalidToken
	}
	taken, err := s.Q.AuthEmailTakenByOther(ctx, sqlc.AuthEmailTakenByOtherParams{Email: newEmail, ID: userID})
	if err != nil {
		return err
	}
	if taken {
		if err := s.Q.AuthDeleteVerification(ctx, v.ID); err != nil {
			return err
		}
		return BadRequest("unable to complete email change")
	}
	n, err := s.Q.AuthChangeUserEmail(ctx, sqlc.AuthChangeUserEmailParams{Email: newEmail, ID: userID})
	if err != nil {
		return err
	}
	if n == 0 {
		return ErrInvalidToken
	}
	if err := s.Q.AuthDeleteVerification(ctx, v.ID); err != nil {
		return err
	}
	return s.Q.AuthDeleteSessionsForUser(ctx, userID)
}

// ---------------------------------------------------------------------------
// Sessions
// ---------------------------------------------------------------------------

// ListedSession is one active session of a user.
type ListedSession struct {
	ID        uuid.UUID
	UserAgent *string
	IPAddress *string
	CreatedAt time.Time
	ExpiresAt time.Time
	Current   bool
}

// ListSessions lists the user's unexpired sessions (newest first),
// flagging the one whose token hash is currentHash.
func (s *Service) ListSessions(ctx context.Context, userID uuid.UUID, currentHash string) ([]ListedSession, error) {
	rows, err := s.Q.AuthListActiveSessions(ctx, userID)
	if err != nil {
		return nil, err
	}
	out := make([]ListedSession, 0, len(rows))
	for _, r := range rows {
		out = append(out, ListedSession{
			ID: r.ID, UserAgent: r.UserAgent, IPAddress: r.IpAddress, CreatedAt: r.CreatedAt,
			ExpiresAt: r.ExpiresAt, Current: currentHash != "" && r.Token == currentHash,
		})
	}
	return out, nil
}

// RevokeSession deletes one of the user's sessions; revoking the current
// one also expires the cookie.
func (s *Service) RevokeSession(w http.ResponseWriter, r *http.Request, user *User, current *Session, sessionID uuid.UUID) error {
	n, err := s.Q.AuthDeleteOwnedSession(r.Context(), sqlc.AuthDeleteOwnedSessionParams{ID: sessionID, UserID: user.ID})
	if err != nil {
		return err
	}
	if n == 0 {
		return BadRequest("session not found")
	}
	if current != nil && sessionID == current.ID {
		ClearCookie(w, SessionCookieName(s.secure()), s.secure())
	}
	return nil
}

// RevokeOtherSessions deletes every session of the user but the one with
// keepHash.
func (s *Service) RevokeOtherSessions(ctx context.Context, userID uuid.UUID, keepHash string) error {
	return s.Q.AuthDeleteSessionsExceptToken(ctx, sqlc.AuthDeleteSessionsExceptTokenParams{UserID: userID, Token: keepHash})
}

// ---------------------------------------------------------------------------
// Linked accounts
// ---------------------------------------------------------------------------

// LinkedAccount is one login method.
type LinkedAccount struct {
	ID         uuid.UUID
	ProviderID string
	AccountID  string
	CreatedAt  time.Time
}

// ListLinkedAccounts lists the user's login methods (oldest first).
func (s *Service) ListLinkedAccounts(ctx context.Context, userID uuid.UUID) ([]LinkedAccount, error) {
	rows, err := s.Q.AuthListAccounts(ctx, userID)
	if err != nil {
		return nil, err
	}
	out := make([]LinkedAccount, 0, len(rows))
	for _, r := range rows {
		out = append(out, LinkedAccount{ID: r.ID, ProviderID: r.ProviderID, AccountID: r.AccountID, CreatedAt: r.CreatedAt})
	}
	return out, nil
}

// UnlinkAccount removes a login method unless it is the last usable one
// (a credential row counts only with a password). Callers step-up first.
func (s *Service) UnlinkAccount(ctx context.Context, userID uuid.UUID, providerID, accountID string) error {
	providerID, accountID = strings.TrimSpace(providerID), strings.TrimSpace(accountID)
	if providerID == "" || accountID == "" {
		return BadRequest("providerId and accountId are required")
	}
	target, err := s.Q.AuthFindAccount(ctx, sqlc.AuthFindAccountParams{UserID: userID, ProviderID: providerID, AccountID: accountID})
	if errors.Is(err, pgx.ErrNoRows) {
		return BadRequest("account not found")
	}
	if err != nil {
		return err
	}
	others, err := s.Q.AuthOtherAccounts(ctx, sqlc.AuthOtherAccountsParams{UserID: userID, ID: target.ID})
	if err != nil {
		return err
	}
	hasOther := false
	for _, a := range others {
		if a.ProviderID != CredentialProviderID || (a.Password != nil && *a.Password != "") {
			hasOther = true
			break
		}
	}
	if !hasOther {
		return BadRequest("cannot unlink the only remaining login method")
	}
	return s.Q.AuthDeleteAccount(ctx, target.ID)
}

// ---------------------------------------------------------------------------
// Account deletion
// ---------------------------------------------------------------------------

// RequestDeleteUser issues a 30-minute delete token and mails it. Callers
// run step-up first (VerifyStepUp) and forbid impersonation.
func (s *Service) RequestDeleteUser(ctx context.Context, user *User, callbackURL string) error {
	token, err := s.issueToken(ctx, s.Q, DeleteIdentifierPrefix+user.ID.String(), DeleteTokenTTL)
	if err != nil {
		return err
	}
	if err := s.sendDeleteAccount(ctx, s.Pool, user.Email, user.Name, token); err != nil {
		return err
	}
	slog.InfoContext(ctx, "delete-user verification token created; mail queued", "user_id", user.ID,
		"callback_url", callbackURL, "token_hash_prefix", VerificationTokenHashPrefix(s.TokenHash(token)))
	return nil
}

// CompleteDelete consumes a delete token: deletes the user (cascades
// sessions, accounts, addresses, wishlist, carts; orders are detached by
// the schema trigger) and stops the current session cookie.
func (s *Service) CompleteDelete(w http.ResponseWriter, r *http.Request, token string) error {
	ctx := r.Context()
	v, err := s.findToken(ctx, token, DeleteIdentifierPrefix)
	if err != nil {
		return err
	}
	if v == nil {
		return ErrInvalidToken
	}
	userID, err := uuid.Parse(strings.TrimPrefix(v.Identifier, DeleteIdentifierPrefix))
	if err != nil {
		return ErrInvalidToken
	}
	if err := s.Q.AuthDeleteVerification(ctx, v.ID); err != nil {
		return err
	}
	n, err := s.Q.AuthDeleteUser(ctx, userID)
	if err != nil {
		return err
	}
	if n == 0 {
		return ErrInvalidToken
	}
	_ = s.StopSession(w, r)
	return nil
}

// DeleteUserConfirmed deletes user immediately when confirmEmail matches
// (HTML settings shortcut; the caller must have stepped up already).
func (s *Service) DeleteUserConfirmed(w http.ResponseWriter, r *http.Request, user *User, confirmEmail string) error {
	if NormalizeEmail(confirmEmail) != NormalizeEmail(user.Email) {
		return BadRequest("email confirmation does not match")
	}
	if _, err := s.Q.AuthDeleteUser(r.Context(), user.ID); err != nil {
		return err
	}
	_ = s.StopSession(w, r)
	return nil
}

// ---------------------------------------------------------------------------
// Profile update (API)
// ---------------------------------------------------------------------------

// ErrImageClientSupplied refuses client-chosen avatar keys.
var ErrImageClientSupplied = BadRequest("image can only be cleared; upload avatars in settings")

// UpdateUser applies an optional name and an optional image clear.
// imageSet reports the "image" key was present; only null/"" is allowed.
func (s *Service) UpdateUser(ctx context.Context, userID uuid.UUID, name *string, imageSet bool, image *string) (*User, error) {
	if name == nil && !imageSet {
		return nil, BadRequest("name or image is required")
	}
	var validName string
	if name != nil {
		n, err := ValidateName(*name)
		if err != nil {
			return nil, err
		}
		validName = n
	}
	if imageSet && image != nil && strings.TrimSpace(*image) != "" {
		return nil, ErrImageClientSupplied
	}
	if name != nil {
		if err := s.Q.AuthUpdateUserName(ctx, sqlc.AuthUpdateUserNameParams{ID: userID, Name: validName}); err != nil {
			return nil, err
		}
	}
	if imageSet {
		if err := s.RemoveAvatar(ctx, userID); err != nil {
			return nil, err
		}
	}
	return s.LoadUser(ctx, userID)
}
