package auth

import (
	"context"
	"errors"
	"net/http"
	"strings"
	"time"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"

	"github.com/wellington-vell/lindaflor/internal/audit"
	"github.com/wellington-vell/lindaflor/internal/db"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// Admin defaults.
const (
	DefaultBanReason  = "Sem motivo"
	AdminUserPageSize = 10
)

// Actor is the signed-in principal performing an admin action.
type Actor struct {
	User    *User
	Session *Session
}

func (a Actor) auditEvent(targetID uuid.UUID, entityType string, entityID uuid.UUID, meta map[string]any) audit.Event {
	name := a.User.Name
	id := a.User.ID
	return audit.Event{
		AggregateType: "user", AggregateID: targetID.String(), EntityType: entityType, EntityID: entityID,
		ActorID: &id, ActorName: &name, Metadata: meta,
	}
}

func ensureAdmin(a Actor) error {
	if !a.User.IsAdmin() {
		return ErrForbidden
	}
	return nil
}

func ensureNotSelf(a Actor, target uuid.UUID, msg string) error {
	if target == a.User.ID {
		return BadRequest(msg)
	}
	return nil
}

func ensureNotAdminTarget(u *User) error {
	if u.IsAdmin() {
		return ErrForbidden
	}
	return nil
}

func canonicalRole(role string) (string, error) {
	switch strings.ToLower(strings.TrimSpace(role)) {
	case "admin":
		return "admin", nil
	case "moderator":
		return "moderator", nil
	case "user":
		return "user", nil
	}
	return "", BadRequest("invalid role")
}

// ---------------------------------------------------------------------------
// Impersonation
// ---------------------------------------------------------------------------

// Impersonate starts a session as target (capped at 1h, no login hooks)
// for an admin who is not already impersonating. Admins and banned users
// cannot be impersonated. Audited.
func (s *Service) Impersonate(w http.ResponseWriter, r *http.Request, admin Actor, targetID uuid.UUID) (*User, *Session, error) {
	ctx := r.Context()
	if !admin.User.IsAdmin() {
		return nil, nil, ErrForbidden
	}
	if admin.Session != nil && admin.Session.ImpersonatedBy != nil {
		return nil, nil, BadRequest("already impersonating a user")
	}
	if targetID == admin.User.ID {
		return nil, nil, BadRequest("cannot impersonate yourself")
	}
	target, err := s.LoadUser(ctx, targetID)
	if err != nil {
		return nil, nil, err
	}
	if target.IsAdmin() {
		return nil, nil, ErrForbidden
	}
	if IsCurrentlyBanned(target.Banned, target.BanExpires) {
		return nil, nil, BadRequest("cannot impersonate a banned user")
	}
	adminID := admin.User.ID
	sess, err := s.StartSessionForUser(w, r, target, &adminID)
	if err != nil {
		return nil, nil, err
	}
	err = audit.Record(ctx, s.Pool, admin.auditEvent(targetID, "user", targetID,
		map[string]any{"action": "impersonate_start", "session_id": sess.ID}))
	if err != nil {
		return nil, nil, err
	}
	return target, sess, nil
}

// StopImpersonating ends the impersonation session and signs the admin
// back in (with login hooks). Audited.
func (s *Service) StopImpersonating(w http.ResponseWriter, r *http.Request, current Actor) (*User, *Session, error) {
	ctx := r.Context()
	if current.Session == nil || current.Session.ImpersonatedBy == nil {
		return nil, nil, BadRequest("not currently impersonating")
	}
	admin, err := s.LoadUser(ctx, *current.Session.ImpersonatedBy)
	if err != nil {
		return nil, nil, err
	}
	if IsCurrentlyBanned(admin.Banned, admin.BanExpires) {
		return nil, nil, ErrUnauthorized
	}
	if err := s.StopSession(w, r); err != nil {
		return nil, nil, err
	}
	err = audit.Record(ctx, s.Pool, Actor{User: admin}.auditEvent(current.User.ID, "user", current.User.ID,
		map[string]any{"action": "impersonate_stop", "session_id": current.Session.ID}))
	if err != nil {
		return nil, nil, err
	}
	sess, err := s.StartSessionForUser(w, r, admin, nil)
	if err != nil {
		return nil, nil, err
	}
	return admin, sess, nil
}

// ---------------------------------------------------------------------------
// User management
// ---------------------------------------------------------------------------

// AdminUserList is one page of users plus the filtered total.
type AdminUserList struct {
	Users []*User
	Total int64
}

// ListUsers searches name/email (ILIKE, escaped) newest first; limit is
// clamped to 1..=100 and offset to ≥ 0.
func (s *Service) ListUsers(ctx context.Context, actor Actor, query string, limit, offset int64) (*AdminUserList, error) {
	if err := ensureAdmin(actor); err != nil {
		return nil, err
	}
	limit = min(max(limit, 1), 100)
	offset = max(offset, 0)
	offset = min(offset, 1<<31-1)
	pattern := ""
	if q := strings.TrimSpace(query); q != "" {
		pattern = db.LikeContains(q)
	}
	total, err := s.Q.AuthAdminCountUsers(ctx, pattern)
	if err != nil {
		return nil, err
	}
	rows, err := s.Q.AuthAdminListUsers(ctx, sqlc.AuthAdminListUsersParams{Pattern: pattern, Lim: int32(limit), Off: int32(offset)}) //nolint:gosec // clamped above
	if err != nil {
		return nil, err
	}
	out := &AdminUserList{Total: total, Users: make([]*User, 0, len(rows))}
	for _, r := range rows {
		out.Users = append(out.Users, &User{
			ID: r.ID, Name: r.Name, Email: r.Email, EmailVerified: r.EmailVerified, Image: r.Image,
			TwoFactorEnabled: r.TwoFactorEnabled, Role: r.Role, Banned: r.Banned,
			BanReason: r.BanReason, BanExpires: r.BanExpires,
		})
	}
	return out, nil
}

func (s *Service) adminTarget(ctx context.Context, actor Actor, userID uuid.UUID, selfMsg string) (*User, error) {
	if err := ensureAdmin(actor); err != nil {
		return nil, err
	}
	if selfMsg != "" {
		if err := ensureNotSelf(actor, userID, selfMsg); err != nil {
			return nil, err
		}
	}
	target, err := s.LoadUser(ctx, userID)
	if err != nil {
		return nil, err
	}
	if err := ensureNotAdminTarget(target); err != nil {
		return nil, err
	}
	return target, nil
}

// SetUserRole changes a non-admin user's role (not one's own). Audited.
func (s *Service) SetUserRole(ctx context.Context, actor Actor, userID uuid.UUID, role string) (*User, error) {
	if err := ensureAdmin(actor); err != nil {
		return nil, err
	}
	if err := ensureNotSelf(actor, userID, "cannot change own role"); err != nil {
		return nil, err
	}
	canon, err := canonicalRole(role)
	if err != nil {
		return nil, err
	}
	if _, err := s.adminTarget(ctx, actor, userID, ""); err != nil {
		return nil, err
	}
	err = db.WithTx(ctx, s.Pool, func(tx pgx.Tx) error {
		if err := sqlc.New(tx).AuthSetUserRole(ctx, sqlc.AuthSetUserRoleParams{ID: userID, Role: &canon}); err != nil {
			return err
		}
		return audit.Record(ctx, tx, actor.auditEvent(userID, "user", userID, map[string]any{"action": "set_role", "role": canon}))
	})
	if err != nil {
		return nil, err
	}
	return s.LoadUser(ctx, userID)
}

// AdminRenameUser renames any user. Audited (from/to).
func (s *Service) AdminRenameUser(ctx context.Context, actor Actor, userID uuid.UUID, name string) (*User, error) {
	if err := ensureAdmin(actor); err != nil {
		return nil, err
	}
	before, err := s.LoadUser(ctx, userID)
	if err != nil {
		return nil, err
	}
	updated, err := s.UpdateUserName(ctx, userID, name)
	if err != nil {
		return nil, err
	}
	err = audit.Record(ctx, s.Pool, actor.auditEvent(userID, "user", userID,
		map[string]any{"action": "rename_user", "from": before.Name, "to": updated.Name}))
	if err != nil {
		return nil, err
	}
	return updated, nil
}

// BanUser bans permanently (blank reason → "Sem motivo") and revokes every
// session. Audited.
func (s *Service) BanUser(ctx context.Context, actor Actor, userID uuid.UUID, reason string) (*User, error) {
	if _, err := s.adminTarget(ctx, actor, userID, "cannot ban yourself"); err != nil {
		return nil, err
	}
	reason = strings.TrimSpace(reason)
	if reason == "" {
		reason = DefaultBanReason
	}
	err := db.WithTx(ctx, s.Pool, func(tx pgx.Tx) error {
		q := sqlc.New(tx)
		if err := q.AuthBanUser(ctx, sqlc.AuthBanUserParams{ID: userID, BanReason: &reason}); err != nil {
			return err
		}
		if err := q.AuthDeleteSessionsForUser(ctx, userID); err != nil {
			return err
		}
		return audit.Record(ctx, tx, actor.auditEvent(userID, "user", userID, map[string]any{"action": "ban_user", "reason": reason}))
	})
	if err != nil {
		return nil, err
	}
	return s.LoadUser(ctx, userID)
}

// UnbanUser lifts a ban. Audited.
func (s *Service) UnbanUser(ctx context.Context, actor Actor, userID uuid.UUID) (*User, error) {
	if _, err := s.adminTarget(ctx, actor, userID, "cannot unban yourself"); err != nil {
		return nil, err
	}
	err := db.WithTx(ctx, s.Pool, func(tx pgx.Tx) error {
		if err := sqlc.New(tx).AuthUnbanUser(ctx, userID); err != nil {
			return err
		}
		return audit.Record(ctx, tx, actor.auditEvent(userID, "user", userID, map[string]any{"action": "unban_user"}))
	})
	if err != nil {
		return nil, err
	}
	return s.LoadUser(ctx, userID)
}

// RemoveUser deletes a non-admin user (audit row written after the delete
// in the same transaction; entity_id has no FK).
func (s *Service) RemoveUser(ctx context.Context, actor Actor, userID uuid.UUID) error {
	if _, err := s.adminTarget(ctx, actor, userID, "cannot remove yourself"); err != nil {
		return err
	}
	return db.WithTx(ctx, s.Pool, func(tx pgx.Tx) error {
		if _, err := sqlc.New(tx).AuthDeleteUser(ctx, userID); err != nil {
			return err
		}
		return audit.Record(ctx, tx, actor.auditEvent(userID, "user", userID, map[string]any{"action": "remove_user"}))
	})
}

// AdminSession is one active session shown to admins.
type AdminSession struct {
	ID        uuid.UUID
	CreatedAt time.Time
	ExpiresAt time.Time
	IPAddress *string
	UserAgent *string
}

// ListUserSessions lists a user's active sessions.
func (s *Service) ListUserSessions(ctx context.Context, actor Actor, userID uuid.UUID) ([]AdminSession, error) {
	if err := ensureAdmin(actor); err != nil {
		return nil, err
	}
	if _, err := s.LoadUser(ctx, userID); err != nil {
		return nil, err
	}
	rows, err := s.Q.AuthListActiveSessions(ctx, userID)
	if err != nil {
		return nil, err
	}
	out := make([]AdminSession, 0, len(rows))
	for _, r := range rows {
		out = append(out, AdminSession{ID: r.ID, CreatedAt: r.CreatedAt, ExpiresAt: r.ExpiresAt, IPAddress: r.IpAddress, UserAgent: r.UserAgent})
	}
	return out, nil
}

// RevokeUserSession revokes one session after checking it belongs to
// expectedUserID (else 403), is not the admin's own (400) and not another
// admin's (403). Audited with entity_type "session".
func (s *Service) RevokeUserSession(ctx context.Context, actor Actor, sessionID, expectedUserID uuid.UUID) error {
	if err := ensureAdmin(actor); err != nil {
		return err
	}
	owner, err := s.Q.AuthSessionOwner(ctx, sessionID)
	if errors.Is(err, pgx.ErrNoRows) {
		return BadRequest("session not found")
	}
	if err != nil {
		return err
	}
	if owner != expectedUserID {
		return ErrForbidden
	}
	if _, err := s.adminTarget(ctx, actor, owner, "cannot revoke own session"); err != nil {
		return err
	}
	return db.WithTx(ctx, s.Pool, func(tx pgx.Tx) error {
		n, err := sqlc.New(tx).AuthDeleteOwnedSession(ctx, sqlc.AuthDeleteOwnedSessionParams{ID: sessionID, UserID: owner})
		if err != nil {
			return err
		}
		if n == 0 {
			return BadRequest("session not found")
		}
		return audit.Record(ctx, tx, actor.auditEvent(owner, "session", sessionID,
			map[string]any{"action": "revoke_session", "session_id": sessionID}))
	})
}

// RevokeUserSessions revokes every session of a non-admin user. Audited.
func (s *Service) RevokeUserSessions(ctx context.Context, actor Actor, userID uuid.UUID) error {
	if _, err := s.adminTarget(ctx, actor, userID, "cannot revoke own sessions"); err != nil {
		return err
	}
	return db.WithTx(ctx, s.Pool, func(tx pgx.Tx) error {
		if err := sqlc.New(tx).AuthDeleteSessionsForUser(ctx, userID); err != nil {
			return err
		}
		return audit.Record(ctx, tx, actor.auditEvent(userID, "user", userID, map[string]any{"action": "revoke_all_sessions"}))
	})
}
