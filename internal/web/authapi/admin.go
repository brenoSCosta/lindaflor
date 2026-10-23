package authapi

import (
	"net/http"
	"strconv"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/auth"
)

// adminActor resolves the signed-in admin (401 anonymous, 403 non-admin).
func adminActor(w http.ResponseWriter, r *http.Request) (auth.Actor, bool) {
	u, s, ok := requireSession(w, r)
	if !ok {
		return auth.Actor{}, false
	}
	if !u.IsAdmin() {
		writeError(w, r, auth.ErrForbidden)
		return auth.Actor{}, false
	}
	return auth.Actor{User: u, Session: s}, true
}

type userIDBody struct {
	UserID *string `json:"userId"`
}

// adminWithUser decodes {userId, …} into body and returns actor + target id.
func adminWithUser[T any](w http.ResponseWriter, r *http.Request, body *T, userID func(*T) *string) (auth.Actor, uuid.UUID, bool) {
	actor, ok := adminActor(w, r)
	if !ok {
		return actor, uuid.Nil, false
	}
	if err := decode(r, body); err != nil {
		writeError(w, r, err)
		return actor, uuid.Nil, false
	}
	raw := userID(body)
	if raw == nil {
		writeError(w, r, auth.BadRequest("userId is required"))
		return actor, uuid.Nil, false
	}
	id, err := parseUUID(*raw, "userId")
	if err != nil {
		writeError(w, r, err)
		return actor, uuid.Nil, false
	}
	return actor, id, true
}

func uidOf(b *userIDBody) *string { return b.UserID }

// POST /api/auth/admin/impersonate-user {userId} → {user, session}.
func (a *api) impersonateUser(w http.ResponseWriter, r *http.Request) {
	var body userIDBody
	actor, id, ok := adminWithUser(w, r, &body, uidOf)
	if !ok {
		return
	}
	u, s, err := a.svc.Impersonate(w, r, actor, id)
	if err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, toPayload(u, s))
}

// POST /api/auth/admin/stop-impersonating → {user, session} of the admin.
func (a *api) stopImpersonating(w http.ResponseWriter, r *http.Request) {
	u, s, ok := requireSession(w, r)
	if !ok {
		return
	}
	admin, sess, err := a.svc.StopImpersonating(w, r, auth.Actor{User: u, Session: s})
	if err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, toPayload(admin, sess))
}

func parseInt(raw string, def int64) (int64, error) {
	raw = trim(raw)
	if raw == "" {
		return def, nil
	}
	n, err := strconv.ParseInt(raw, 10, 64)
	if err != nil {
		return 0, auth.BadRequest("limit and offset must be integers")
	}
	return n, nil
}

// GET /api/auth/admin/list-users?q&limit&offset → {users, total}.
func (a *api) listUsers(w http.ResponseWriter, r *http.Request) {
	actor, ok := adminActor(w, r)
	if !ok {
		return
	}
	q := r.URL.Query()
	limit, err := parseInt(q.Get("limit"), auth.AdminUserPageSize)
	if err != nil {
		writeError(w, r, err)
		return
	}
	offset, err := parseInt(q.Get("offset"), 0)
	if err != nil {
		writeError(w, r, err)
		return
	}
	list, err := a.svc.ListUsers(r.Context(), actor, q.Get("q"), limit, offset)
	if err != nil {
		writeError(w, r, err)
		return
	}
	users := make([]userJSON, 0, len(list.Users))
	for _, u := range list.Users {
		users = append(users, toUserJSON(u))
	}
	writeJSON(w, http.StatusOK, map[string]any{"users": users, "total": list.Total})
}

type roleBody struct {
	UserID *string `json:"userId"`
	Role   *string `json:"role"`
}

// POST /api/auth/admin/set-role {userId, role} → {user}.
func (a *api) setRole(w http.ResponseWriter, r *http.Request) {
	var body roleBody
	actor, id, ok := adminWithUser(w, r, &body, func(b *roleBody) *string { return b.UserID })
	if !ok {
		return
	}
	if body.Role == nil {
		writeError(w, r, auth.BadRequest("role is required"))
		return
	}
	u, err := a.svc.SetUserRole(r.Context(), actor, id, *body.Role)
	if err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"user": toUserJSON(u)})
}

type nameBody struct {
	UserID *string `json:"userId"`
	Name   *string `json:"name"`
}

// POST /api/auth/admin/update-user {userId, name} → {user}. Audited.
func (a *api) adminUpdateUser(w http.ResponseWriter, r *http.Request) {
	var body nameBody
	actor, id, ok := adminWithUser(w, r, &body, func(b *nameBody) *string { return b.UserID })
	if !ok {
		return
	}
	if body.Name == nil {
		writeError(w, r, auth.BadRequest("name is required"))
		return
	}
	u, err := a.svc.AdminRenameUser(r.Context(), actor, id, *body.Name)
	if err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, map[string]any{"user": toUserJSON(u)})
}

type banBody struct {
	UserID    *string `json:"userId"`
	BanReason string  `json:"banReason"`
}

// POST /api/auth/admin/ban-user {userId, banReason?} → {status}.
func (a *api) banUser(w http.ResponseWriter, r *http.Request) {
	var body banBody
	actor, id, ok := adminWithUser(w, r, &body, func(b *banBody) *string { return b.UserID })
	if !ok {
		return
	}
	if _, err := a.svc.BanUser(r.Context(), actor, id, body.BanReason); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

// POST /api/auth/admin/unban-user {userId} → {status}.
func (a *api) unbanUser(w http.ResponseWriter, r *http.Request) {
	var body userIDBody
	actor, id, ok := adminWithUser(w, r, &body, uidOf)
	if !ok {
		return
	}
	if _, err := a.svc.UnbanUser(r.Context(), actor, id); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

// POST /api/auth/admin/remove-user {userId} → {status}.
func (a *api) removeUser(w http.ResponseWriter, r *http.Request) {
	var body userIDBody
	actor, id, ok := adminWithUser(w, r, &body, uidOf)
	if !ok {
		return
	}
	if err := a.svc.RemoveUser(r.Context(), actor, id); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

type adminSessionJSON struct {
	ID        string  `json:"id"`
	CreatedAt string  `json:"createdAt"`
	ExpiresAt string  `json:"expiresAt"`
	IPAddress *string `json:"ipAddress"`
	UserAgent *string `json:"userAgent"`
}

// POST /api/auth/admin/list-user-sessions {userId} → {sessions}.
func (a *api) listUserSessions(w http.ResponseWriter, r *http.Request) {
	var body userIDBody
	actor, id, ok := adminWithUser(w, r, &body, uidOf)
	if !ok {
		return
	}
	list, err := a.svc.ListUserSessions(r.Context(), actor, id)
	if err != nil {
		writeError(w, r, err)
		return
	}
	out := make([]adminSessionJSON, 0, len(list))
	for _, s := range list {
		out = append(out, adminSessionJSON{
			ID: s.ID.String(), CreatedAt: formatTime(s.CreatedAt), ExpiresAt: formatTime(s.ExpiresAt),
			IPAddress: s.IPAddress, UserAgent: s.UserAgent,
		})
	}
	writeJSON(w, http.StatusOK, map[string]any{"sessions": out})
}

type revokeSessionBody struct {
	SessionID *string `json:"sessionId"`
	UserID    *string `json:"userId"`
}

// POST /api/auth/admin/revoke-user-session {sessionId, userId} → {status}.
// userId is required and must own the session.
func (a *api) revokeUserSession(w http.ResponseWriter, r *http.Request) {
	var body revokeSessionBody
	actor, ok := adminActor(w, r)
	if !ok {
		return
	}
	if err := decode(r, &body); err != nil {
		writeError(w, r, err)
		return
	}
	if body.SessionID == nil || body.UserID == nil {
		writeError(w, r, auth.BadRequest("sessionId and userId are required"))
		return
	}
	sid, err := parseUUID(*body.SessionID, "sessionId")
	if err != nil {
		writeError(w, r, err)
		return
	}
	uid, err := parseUUID(*body.UserID, "userId")
	if err != nil {
		writeError(w, r, err)
		return
	}
	if err := a.svc.RevokeUserSession(r.Context(), actor, sid, uid); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}

// POST /api/auth/admin/revoke-user-sessions {userId} → {status}.
func (a *api) revokeUserSessions(w http.ResponseWriter, r *http.Request) {
	var body userIDBody
	actor, id, ok := adminWithUser(w, r, &body, uidOf)
	if !ok {
		return
	}
	if err := a.svc.RevokeUserSessions(r.Context(), actor, id); err != nil {
		writeError(w, r, err)
		return
	}
	writeJSON(w, http.StatusOK, statusOK)
}
