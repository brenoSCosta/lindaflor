package auth

import (
	"context"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/mail"
)

// Auth mail goes through the platform hooks (internal/mail/hooks.go):
// verify / reset / change-email are capped per recipient
// (mail.EnqueueThrottled, a dropped mail is invisible to the caller); the
// delete confirmation is a plain enqueue. Links are absolute
// (APP_ORIGIN, dev fallback) and carry the raw token, which only ever
// lives in the outbox body.

func (s *Service) links() mail.Links { return mail.LinksFromConfig(s.Cfg) }

func (s *Service) sendVerifyEmail(ctx context.Context, db sqlc.DBTX, to, name, token string) error {
	return mail.SendVerifyEmail(ctx, db, s.links(), to, name, token)
}

func (s *Service) sendPasswordReset(ctx context.Context, db sqlc.DBTX, to, name, token string) error {
	return mail.SendPasswordReset(ctx, db, s.links(), to, name, token)
}

func (s *Service) sendChangeEmail(ctx context.Context, db sqlc.DBTX, newEmail, name, token string) error {
	return mail.SendChangeEmail(ctx, db, s.links(), newEmail, name, token)
}

func (s *Service) sendDeleteAccount(ctx context.Context, db sqlc.DBTX, to, name, token string) error {
	return mail.SendDeleteAccount(ctx, db, s.links(), to, name, token)
}
