package auth

import (
	"context"
	"errors"
	"net/http"
	"strings"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"

	"github.com/wellington-vell/lindaflor/internal/ratelimit"
)

// MsgImpersonating is the pt-BR refusal for HTML callers of credential
// changes during an impersonation.
const MsgImpersonating = "Esta ação não está disponível enquanto você atua como outra pessoa."

// ForbidImpersonated refuses credential-changing actions from an admin
// impersonation session (403): nothing planted there may outlive "Encerrar".
func ForbidImpersonated(s *Session) error {
	if s != nil && s.ImpersonatedBy != nil {
		return ErrForbidden
	}
	return nil
}

// CheckStepUpRate consumes the per-IP step-up budget (auth:step-up,
// 10/min by default): every password re-check is a full Argon2 verify.
func CheckStepUpRate(r *http.Request) error {
	if !RateAllow(r, ratelimit.ScopeStepUp, ratelimit.DefaultStepUpPerMin) {
		return ErrTooManyRequests
	}
	return nil
}

func (s *Service) ensureTOTPNotLocked(ctx context.Context, userID uuid.UUID) error {
	if s.Valkey.IsTotpLockedOut(ctx, userID) {
		return ErrTooManyRequests
	}
	return nil
}

// CheckTOTPCode verifies code with the per-user failure cap and the replay
// guard: false for a wrong or already-used code, 429 while locked out. A
// valid code is reserved so it cannot be presented again in its window.
func (s *Service) CheckTOTPCode(ctx context.Context, userID uuid.UUID, secret, code string) (bool, error) {
	if err := s.ensureTOTPNotLocked(ctx, userID); err != nil {
		return false, err
	}
	ok, err := VerifyTOTPCode(secret, code)
	if err != nil {
		return false, BadRequest(err.Error())
	}
	if !ok {
		s.Valkey.RecordTotpFail(ctx, userID)
		return false, nil
	}
	return s.Valkey.TotpReserve(ctx, userID, code), nil
}

// StepUpFactors lists the re-authentication factors an account has.
type StepUpFactors struct{ Password, TOTP bool }

// None: OAuth-only accounts without 2FA re-check nothing beyond the session.
func (f StepUpFactors) None() bool { return !f.Password && !f.TOTP }

// StepUpFactorsFor loads the factors of userID.
func (s *Service) StepUpFactorsFor(ctx context.Context, userID uuid.UUID) (StepUpFactors, error) {
	var f StepUpFactors
	pw, err := s.Q.AuthCredentialPassword(ctx, userID)
	if err != nil && !errors.Is(err, pgx.ErrNoRows) {
		return f, err
	}
	f.Password = pw != nil && *pw != ""
	tf, err := s.Q.AuthTwoFactorByUser(ctx, userID)
	if err != nil && !errors.Is(err, pgx.ErrNoRows) {
		return f, err
	}
	f.TOTP = err == nil && tf.Verified
	return f, nil
}

// VerifyStepUp requires the current password (when the account has one)
// or a current TOTP code (when 2FA is verified); either works when both
// exist; accounts with neither pass on the session. 401 for a missing or
// wrong factor, 429 while rate limited / TOTP locked.
func (s *Service) VerifyStepUp(r *http.Request, userID uuid.UUID, password, code string) error {
	ctx := r.Context()
	f, err := s.StepUpFactorsFor(ctx, userID)
	if err != nil {
		return err
	}
	if f.None() {
		return nil
	}
	if err := CheckStepUpRate(r); err != nil {
		return err
	}
	if f.Password && password != "" {
		stored, err := s.Q.AuthCredentialPassword(ctx, userID)
		if err != nil && !errors.Is(err, pgx.ErrNoRows) {
			return err
		}
		if stored != nil {
			ok, err := VerifyPassword(password, *stored)
			if err != nil {
				return err
			}
			if ok {
				return nil
			}
		}
	}
	if code = strings.TrimSpace(code); f.TOTP && code != "" {
		tf, err := s.Q.AuthVerifiedTwoFactor(ctx, userID)
		if errors.Is(err, pgx.ErrNoRows) {
			return ErrUnauthorized
		}
		if err != nil {
			return err
		}
		secret, err := DecryptTOTPSecret(tf.SecretNonce, tf.SecretCiphertext, tf.SecretKeyVersion)
		if err != nil {
			return ErrUnauthorized
		}
		ok, err := s.CheckTOTPCode(ctx, userID, secret, code)
		if err != nil {
			return err
		}
		if ok {
			return nil
		}
	}
	return ErrUnauthorized
}
