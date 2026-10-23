package auth

import (
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// TwoFactorEnrollment is the one-time enable response: the only place the
// plaintext secret / otpauth URI / backup codes ever appear.
type TwoFactorEnrollment struct {
	TOTPURI     string   `json:"totp_uri"`
	Secret      string   `json:"secret"`
	BackupCodes []string `json:"backup_codes"`
}

// ErrTwoFactorAlreadyEnabled is the 400 of enabling twice.
var ErrTwoFactorAlreadyEnabled = BadRequest("two-factor authentication is already enabled")

// EnableTwoFactor starts (or rotates) an unverified enrollment: new secret
// encrypted at rest + 10 hashed backup codes. Callers step-up first.
func (s *Service) EnableTwoFactor(ctx context.Context, user *User) (*TwoFactorEnrollment, error) {
	if user.TwoFactorEnabled {
		return nil, ErrTwoFactorAlreadyEnabled
	}
	secret, uri, err := GenerateTOTPSecret(user.Email)
	if err != nil {
		return nil, BadRequest(err.Error())
	}
	codes := GenerateBackupCodes()
	stored, err := HashBackupCodes(codes)
	if err != nil {
		return nil, fmt.Errorf("hash backup codes: %w", err)
	}
	enc, err := EncryptTOTPSecret(secret)
	if err != nil {
		return nil, err
	}
	if err := s.Q.AuthDeleteTwoFactor(ctx, user.ID); err != nil {
		return nil, err
	}
	if err := s.Q.AuthInsertTwoFactor(ctx, sqlc.AuthInsertTwoFactorParams{
		ID: newID(), BackupCodes: stored, UserID: user.ID,
		SecretNonce: enc.Nonce, SecretCiphertext: enc.Ciphertext, SecretKeyVersion: enc.KeyVersion,
	}); err != nil {
		return nil, err
	}
	return &TwoFactorEnrollment{TOTPURI: uri, Secret: secret, BackupCodes: codes}, nil
}

// ConfirmEnableTwoFactor verifies the first code of a pending enrollment
// and turns 2FA on.
func (s *Service) ConfirmEnableTwoFactor(ctx context.Context, user *User, code string, missingMsg string) error {
	if user.TwoFactorEnabled {
		return ErrTwoFactorAlreadyEnabled
	}
	code = strings.TrimSpace(code)
	if code == "" {
		return BadRequest("code is required")
	}
	tf, err := s.Q.AuthTwoFactorByUser(ctx, user.ID)
	if errors.Is(err, pgx.ErrNoRows) {
		return BadRequest(missingMsg)
	}
	if err != nil {
		return err
	}
	if tf.Verified {
		return BadRequest("two-factor authentication is already verified")
	}
	secret, err := DecryptTOTPSecret(tf.SecretNonce, tf.SecretCiphertext, tf.SecretKeyVersion)
	if err != nil {
		return ErrUnauthorized
	}
	ok, err := s.CheckTOTPCode(ctx, user.ID, secret, code)
	if err != nil {
		return err
	}
	if !ok {
		return ErrUnauthorized
	}
	if err := s.Q.AuthMarkTwoFactorVerified(ctx, tf.ID); err != nil {
		return err
	}
	return s.Q.AuthSetTwoFactorEnabled(ctx, sqlc.AuthSetTwoFactorEnabledParams{ID: user.ID, TwoFactorEnabled: true})
}

// HasUnverifiedTwoFactor reports a pending (unconfirmed) enrollment. It
// never reads the secret: the enable response is its only exposure.
func (s *Service) HasUnverifiedTwoFactor(ctx context.Context, userID uuid.UUID) (bool, error) {
	return s.Q.AuthHasUnverifiedTwoFactor(ctx, userID)
}

// DisableTwoFactor removes TOTP after step-up (password or code).
func (s *Service) DisableTwoFactor(ctx context.Context, userID uuid.UUID) error {
	if err := s.Q.AuthDeleteTwoFactor(ctx, userID); err != nil {
		return err
	}
	return s.Q.AuthSetTwoFactorEnabled(ctx, sqlc.AuthSetTwoFactorEnabledParams{ID: userID, TwoFactorEnabled: false})
}

// ConsumeBackupCodeSignedIn burns one backup code of a signed-in user
// (per-user failure cap applies).
func (s *Service) ConsumeBackupCodeSignedIn(ctx context.Context, userID uuid.UUID, code string) error {
	tf, err := s.Q.AuthTwoFactorByUser(ctx, userID)
	if errors.Is(err, pgx.ErrNoRows) {
		return BadRequest("two-factor is not configured")
	}
	if err != nil {
		return err
	}
	if !tf.Verified {
		return BadRequest("two-factor is not verified")
	}
	if err := s.ensureTOTPNotLocked(ctx, userID); err != nil {
		return err
	}
	remaining, ok := ConsumeBackupCode(tf.BackupCodes, code)
	if !ok {
		s.Valkey.RecordTotpFail(ctx, userID)
		return ErrUnauthorized
	}
	return s.Q.AuthSetBackupCodes(ctx, sqlc.AuthSetBackupCodesParams{ID: tf.ID, BackupCodes: remaining})
}

// RegenerateBackupCodes replaces every backup code (callers step-up first).
func (s *Service) RegenerateBackupCodes(ctx context.Context, userID uuid.UUID) ([]string, error) {
	tf, err := s.Q.AuthVerifiedTwoFactor(ctx, userID)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, BadRequest("two-factor is not configured")
	}
	if err != nil {
		return nil, err
	}
	codes := GenerateBackupCodes()
	stored, err := HashBackupCodes(codes)
	if err != nil {
		return nil, fmt.Errorf("hash backup codes: %w", err)
	}
	if err := s.Q.AuthSetBackupCodes(ctx, sqlc.AuthSetBackupCodesParams{ID: tf.ID, BackupCodes: stored}); err != nil {
		return nil, err
	}
	return codes, nil
}
