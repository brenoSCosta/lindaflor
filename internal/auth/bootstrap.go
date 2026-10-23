package auth

import (
	"context"
	"errors"
	"fmt"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/audit"
	"github.com/wellington-vell/lindaflor/internal/db"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// BootstrapActor is audit_events.actor_name for create-admin writes.
const BootstrapActor = "create-admin"

// ErrPasswordRequired: creating a new admin needs a password.
var ErrPasswordRequired = errors.New("no user with this email: a password is required to create one")

// AdminBootstrapOutcome reports whether the admin was created or promoted.
type AdminBootstrapOutcome struct {
	UserID  uuid.UUID
	Created bool
}

// CreateOrPromoteAdmin creates email as a verified admin, or promotes the
// existing user (role admin, email verified). A password (validated like
// sign-up) becomes the credential password, replacing an existing one
// (admin password recovery); creating a new user requires it. name is
// only used for new users. Audited (actor_name "create-admin").
func CreateOrPromoteAdmin(ctx context.Context, pool *pgxpool.Pool, email, name string, password *string) (AdminBootstrapOutcome, error) {
	var out AdminBootstrapOutcome
	email, err := ValidateEmail(email)
	if err != nil {
		return out, fmt.Errorf("invalid email: %w", err)
	}
	name, err = ValidateName(name)
	if err != nil {
		return out, fmt.Errorf("invalid name: %w", err)
	}
	var hash *string
	if password != nil {
		if err := ValidatePassword(*password, email, name, "lindaflor"); err != nil {
			return out, err
		}
		h, err := HashPassword(*password)
		if err != nil {
			return out, err
		}
		hash = &h
	}
	err = db.WithTx(ctx, pool, func(tx pgx.Tx) error {
		q := sqlc.New(tx)
		existing, err := q.AuthUserIDByEmailForUpdate(ctx, email)
		switch {
		case err == nil:
			if err := q.AuthPromoteAdmin(ctx, existing); err != nil {
				return err
			}
			out = AdminBootstrapOutcome{UserID: existing}
		case errors.Is(err, pgx.ErrNoRows):
			if hash == nil {
				return ErrPasswordRequired
			}
			out = AdminBootstrapOutcome{UserID: newID(), Created: true}
			if err := q.AuthInsertUser(ctx, sqlc.AuthInsertUserParams{
				ID: out.UserID, Name: name, Email: email, EmailVerified: true, Role: strPtr("admin"),
			}); err != nil {
				return err
			}
		default:
			return err
		}
		if hash != nil {
			n, err := q.AuthUpdateCredentialPassword(ctx, sqlc.AuthUpdateCredentialPasswordParams{UserID: out.UserID, Password: hash})
			if err != nil {
				return err
			}
			if n == 0 {
				if err := q.AuthInsertAccount(ctx, sqlc.AuthInsertAccountParams{
					ID: newID(), AccountID: out.UserID.String(), ProviderID: CredentialProviderID,
					UserID: out.UserID, Password: hash,
				}); err != nil {
					return err
				}
			}
		}
		actor := BootstrapActor
		return audit.Record(ctx, tx, audit.Event{
			AggregateType: "user", AggregateID: out.UserID.String(), EntityType: "user", EntityID: out.UserID,
			ActorName: &actor,
			Metadata:  map[string]any{"action": "bootstrap_admin", "created": out.Created, "password_set": hash != nil},
		})
	})
	if err != nil {
		return AdminBootstrapOutcome{}, err
	}
	return out, nil
}
