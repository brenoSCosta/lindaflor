package authapi_test

import (
	"encoding/json"
	"net/http"
	"strings"
	"testing"
	"time"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/testutil"
)

// --- tests/auth_admin.rs ---

func adminAccount(t *testing.T, s *testutil.Server, prefix, name string) account {
	t.Helper()
	a := signUp(t, s, prefix, name)
	makeAdmin(t, s, a.id)
	return a
}

func sessionCount(t *testing.T, s *testutil.Server, id uuid.UUID) int64 {
	t.Helper()
	return scalar[int64](t, s, `SELECT count(*) FROM sessions WHERE user_id = $1`, id)
}

func auditCount(t *testing.T, s *testutil.Server, actor uuid.UUID, aggregateID, action string) int64 {
	t.Helper()
	return scalar[int64](t, s, `SELECT count(*) FROM audit_events WHERE actor_user_id = $1 AND aggregate_type = 'user'
		AND aggregate_id = $2 AND metadata->>'action' = $3`, actor, aggregateID, action)
}

type auditRow struct {
	aggType, aggID, entityType string
	entityID                   uuid.UUID
	actor                      *uuid.UUID
	actorName                  *string
	meta                       map[string]any
}

func lastAudit(t *testing.T, s *testutil.Server, actor uuid.UUID, aggregateID, action string) auditRow {
	t.Helper()
	var r auditRow
	var meta []byte
	err := s.Pool.QueryRow(t.Context(), `SELECT aggregate_type, aggregate_id, entity_type, entity_id, actor_user_id, actor_name, metadata
		FROM audit_events WHERE actor_user_id = $1 AND aggregate_type = 'user' AND aggregate_id = $2 AND metadata->>'action' = $3
		ORDER BY occurred_at DESC LIMIT 1`, actor, aggregateID, action).Scan(&r.aggType, &r.aggID, &r.entityType, &r.entityID, &r.actor, &r.actorName, &meta)
	if err != nil {
		t.Fatalf("audit row %s: %v", action, err)
	}
	if err := json.Unmarshal(meta, &r.meta); err != nil {
		t.Fatal(err)
	}
	return r
}

func TestAdminImpersonateAndStop(t *testing.T) {
	s := newServer(t)
	admin := adminAccount(t, s, "admin", "Admin User")
	target := signUp(t, s, "target", "Target User")
	imp := admin.c.PostJSON("/api/auth/admin/impersonate-user", map[string]any{"userId": target.id.String()})
	wantStatus(t, imp, http.StatusOK)
	j := imp.JSON(t)
	if field(j, "user", "email") != target.email || field(j, "session", "impersonatedBy") != admin.id.String() {
		t.Fatal(imp.Body)
	}
	sess := admin.c.Get("/api/auth/get-session").JSON(t)
	if field(sess, "user", "email") != target.email || field(sess, "session", "impersonatedBy") != admin.id.String() {
		t.Fatalf("session %v", sess)
	}
	stop := admin.c.PostJSON("/api/auth/admin/stop-impersonating", map[string]any{})
	wantStatus(t, stop, http.StatusOK)
	sj := stop.JSON(t)
	if field(sj, "user", "email") != admin.email || field(sj, "session", "impersonatedBy") != nil {
		t.Fatal(stop.Body)
	}
	// Not impersonating any more.
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/stop-impersonating", map[string]any{}), http.StatusBadRequest)
}

func TestNonAdminCannotImpersonate(t *testing.T) {
	s := newServer(t)
	user := signUp(t, s, "nonadmin", "Regular")
	other := signUp(t, s, "other", "Other")
	wantStatus(t, user.c.PostJSON("/api/auth/admin/impersonate-user", map[string]any{"userId": other.id.String()}), http.StatusForbidden)
	wantStatus(t, s.Client(t).PostJSON("/api/auth/admin/impersonate-user", map[string]any{"userId": other.id.String()}), http.StatusUnauthorized)
}

func TestAdminCannotImpersonateSelfOrBanned(t *testing.T) {
	s := newServer(t)
	admin := adminAccount(t, s, "imp-self", "Imp Self")
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/impersonate-user", map[string]any{"userId": admin.id.String()}), http.StatusBadRequest)
	banned := signUp(t, s, "imp-banned", "Banned")
	exec(t, s, `UPDATE users SET banned = true WHERE id = $1`, banned.id)
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/impersonate-user", map[string]any{"userId": banned.id.String()}), http.StatusBadRequest)
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/impersonate-user", map[string]any{"userId": "nope"}), http.StatusBadRequest)
}

func TestAdminListsAndSearchesUsers(t *testing.T) {
	s := newServer(t)
	admin := adminAccount(t, s, "list-admin", "List Admin")
	target := signUp(t, s, "list-target", "Alvo Busca")
	listed := admin.c.Get("/api/auth/admin/list-users")
	wantStatus(t, listed, http.StatusOK)
	j := listed.JSON(t)
	if !strings.Contains(listed.Body, target.email) || j["total"].(float64) < 2 {
		t.Fatal(listed.Body)
	}
	needle, _, _ := strings.Cut(target.email, "@")
	searched := admin.c.Get("/api/auth/admin/list-users?q=" + needle)
	wantStatus(t, searched, http.StatusOK)
	sj := searched.JSON(t)
	users := sj["users"].([]any)
	if sj["total"].(float64) != 1 || users[0].(map[string]any)["email"] != target.email || users[0].(map[string]any)["name"] != "Alvo Busca" {
		t.Fatal(searched.Body)
	}
	wantStatus(t, admin.c.Get("/api/auth/admin/list-users?limit=abc"), http.StatusBadRequest)
	if got := admin.c.Get("/api/auth/admin/list-users?limit=1&offset=-5").JSON(t)["users"].([]any); len(got) != 1 {
		t.Fatalf("limit=1 returned %d", len(got))
	}
	// LIKE wildcards are escaped.
	if admin.c.Get("/api/auth/admin/list-users?q=%25").JSON(t)["total"].(float64) != 0 {
		t.Fatal("% must be literal")
	}
}

func TestNonAdminCannotListOrBan(t *testing.T) {
	s := newServer(t)
	user := signUp(t, s, "list-user", "Regular")
	other := signUp(t, s, "list-other", "Other")
	wantStatus(t, user.c.Get("/api/auth/admin/list-users"), http.StatusForbidden)
	wantStatus(t, user.c.PostJSON("/api/auth/admin/ban-user", map[string]any{"userId": other.id.String(), "banReason": "nope"}), http.StatusForbidden)
}

func TestAdminBansAndUnbansUser(t *testing.T) {
	s := newServer(t)
	admin := adminAccount(t, s, "ban-admin", "Ban Admin")
	target := signUp(t, s, "ban-target", "Ban Target")
	if sessionCount(t, s, target.id) < 1 {
		t.Fatal("target has a session")
	}
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/ban-user", map[string]any{"userId": target.id.String(), "banReason": "abuso"}), http.StatusOK)
	var banned bool
	var reason *string
	var expires *time.Time
	if err := s.Pool.QueryRow(t.Context(), `SELECT banned, ban_reason, ban_expires FROM users WHERE id = $1`, target.id).Scan(&banned, &reason, &expires); err != nil {
		t.Fatal(err)
	}
	if !banned || reason == nil || *reason != "abuso" || expires != nil || sessionCount(t, s, target.id) != 0 {
		t.Fatal("ban state")
	}
	_, denied := signIn(t, s, target.email, strongPW)
	wantStatus(t, denied, http.StatusUnauthorized)
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/unban-user", map[string]any{"userId": target.id.String()}), http.StatusOK)
	if err := s.Pool.QueryRow(t.Context(), `SELECT banned, ban_reason, ban_expires FROM users WHERE id = $1`, target.id).Scan(&banned, &reason, &expires); err != nil {
		t.Fatal(err)
	}
	if banned || reason != nil || expires != nil || sessionCount(t, s, target.id) != 0 {
		t.Fatal("unban state")
	}
	_, ok := signIn(t, s, target.email, strongPW)
	wantStatus(t, ok, http.StatusOK)

	blank := signUp(t, s, "ban-blank", "Blank Reason")
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/ban-user", map[string]any{"userId": blank.id.String(), "banReason": "  "}), http.StatusOK)
	if r := scalar[string](t, s, `SELECT ban_reason FROM users WHERE id = $1`, blank.id); r != "Sem motivo" {
		t.Fatal(r)
	}
}

func TestAdminCannotBanSelf(t *testing.T) {
	s := newServer(t)
	admin := adminAccount(t, s, "self-ban", "Self Ban")
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/ban-user", map[string]any{"userId": admin.id.String()}), http.StatusBadRequest)
	if scalar[bool](t, s, `SELECT banned FROM users WHERE id = $1`, admin.id) {
		t.Fatal("admin banned itself")
	}
}

func TestAdminSetsRoleAndName(t *testing.T) {
	s := newServer(t)
	admin := adminAccount(t, s, "role-admin", "Role Admin")
	target := signUp(t, s, "role-target", "Old Name")
	role := admin.c.PostJSON("/api/auth/admin/set-role", map[string]any{"userId": target.id.String(), "role": "moderator"})
	wantStatus(t, role, http.StatusOK)
	if field(role.JSON(t), "user", "role") != "moderator" {
		t.Fatal(role.Body)
	}
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/set-role", map[string]any{"userId": target.id.String(), "role": "owner"}), http.StatusBadRequest)
	renamed := admin.c.PostJSON("/api/auth/admin/update-user", map[string]any{"userId": target.id.String(), "name": "  Novo Nome  "})
	wantStatus(t, renamed, http.StatusOK)
	if field(renamed.JSON(t), "user", "name") != "Novo Nome" {
		t.Fatal(renamed.Body)
	}
}

func TestAdminRemovesUserButNotSelf(t *testing.T) {
	s := newServer(t)
	admin := adminAccount(t, s, "remove-admin", "Remove Admin")
	target := signUp(t, s, "remove-target", "Remove Target")
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/remove-user", map[string]any{"userId": target.id.String()}), http.StatusOK)
	if scalar[bool](t, s, `SELECT EXISTS (SELECT 1 FROM users WHERE id = $1)`, target.id) {
		t.Fatal("target still exists")
	}
	r := lastAudit(t, s, admin.id, target.id.String(), "remove_user")
	if r.aggType != "user" || r.aggID != target.id.String() || r.entityType != "user" || r.entityID != target.id ||
		r.actor == nil || *r.actor != admin.id || r.actorName == nil || *r.actorName != "Remove Admin" || r.meta["action"] != "remove_user" {
		t.Fatalf("audit %+v", r)
	}
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/remove-user", map[string]any{"userId": admin.id.String()}), http.StatusBadRequest)
	if !scalar[bool](t, s, `SELECT EXISTS (SELECT 1 FROM users WHERE id = $1)`, admin.id) {
		t.Fatal("admin removed itself")
	}
}

func TestAdminRevokesOneSessionAndAllSessions(t *testing.T) {
	s := newServer(t)
	admin := adminAccount(t, s, "sess-admin", "Session Admin")
	target := signUp(t, s, "sess-target", "Session Target")
	if sessionCount(t, s, target.id) != 1 {
		t.Fatal("one session")
	}
	listed := admin.c.PostJSON("/api/auth/admin/list-user-sessions", map[string]any{"userId": target.id.String()})
	wantStatus(t, listed, http.StatusOK)
	sid := listed.JSON(t)["sessions"].([]any)[0].(map[string]any)["id"].(string)
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/revoke-user-session", map[string]any{"sessionId": sid, "userId": target.id.String()}), http.StatusOK)
	if sessionCount(t, s, target.id) != 0 {
		t.Fatal("revoked")
	}
	for range 2 {
		_, res := signIn(t, s, target.email, strongPW)
		wantStatus(t, res, http.StatusOK)
	}
	if sessionCount(t, s, target.id) != 2 {
		t.Fatal("two sessions")
	}
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/revoke-user-sessions", map[string]any{"userId": target.id.String()}), http.StatusOK)
	if sessionCount(t, s, target.id) != 0 {
		t.Fatal("all revoked")
	}
	if auditCount(t, s, admin.id, target.id.String(), "revoke_all_sessions") != 1 {
		t.Fatal("audited once")
	}
	r := lastAudit(t, s, admin.id, target.id.String(), "revoke_all_sessions")
	if r.aggType != "user" || r.entityType != "user" || r.entityID != target.id || r.actor == nil || *r.actor != admin.id {
		t.Fatalf("audit %+v", r)
	}
}

func TestAdminCannotImpersonateAdmin(t *testing.T) {
	s := newServer(t)
	admin := adminAccount(t, s, "imp-admin", "Imp Admin")
	other := adminAccount(t, s, "imp-other", "Other Admin")
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/impersonate-user", map[string]any{"userId": other.id.String()}), http.StatusForbidden)
}

func TestAdminCannotSelfDemoteViaSetRole(t *testing.T) {
	s := newServer(t)
	admin := adminAccount(t, s, "self-role", "Self Role")
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/set-role", map[string]any{"userId": admin.id.String(), "role": "user"}), http.StatusBadRequest)
	if r := scalar[string](t, s, `SELECT role FROM users WHERE id = $1`, admin.id); r != "admin" {
		t.Fatal(r)
	}
	if auditCount(t, s, admin.id, admin.id.String(), "set_role") != 0 {
		t.Fatal("no audit for a refused change")
	}
}

func TestAdminCannotTargetAdminUsers(t *testing.T) {
	s := newServer(t)
	admin := adminAccount(t, s, "guard-admin", "Guard Admin")
	other := adminAccount(t, s, "guard-other", "Guard Other")
	id := other.id.String()
	for path, body := range map[string]map[string]any{
		"/api/auth/admin/set-role":             {"userId": id, "role": "user"},
		"/api/auth/admin/ban-user":             {"userId": id},
		"/api/auth/admin/unban-user":           {"userId": id},
		"/api/auth/admin/remove-user":          {"userId": id},
		"/api/auth/admin/revoke-user-sessions": {"userId": id},
	} {
		res := admin.c.PostJSON(path, body)
		if res.StatusCode != http.StatusForbidden {
			t.Errorf("%s: %d %s", path, res.StatusCode, res.Body)
		}
	}
	if r := scalar[string](t, s, `SELECT role FROM users WHERE id = $1`, other.id); r != "admin" {
		t.Fatal(r)
	}
}

func TestAdminRevokeSessionEnforcesOwnership(t *testing.T) {
	s := newServer(t)
	admin := adminAccount(t, s, "revoke-admin", "Revoke Admin")
	second := adminAccount(t, s, "revoke-admin2", "Second Admin")
	a := signUp(t, s, "revoke-a", "Target A")
	b := signUp(t, s, "revoke-b", "Target B")
	firstSession := func(id uuid.UUID) string {
		res := admin.c.PostJSON("/api/auth/admin/list-user-sessions", map[string]any{"userId": id.String()})
		wantStatus(t, res, http.StatusOK)
		return res.JSON(t)["sessions"].([]any)[0].(map[string]any)["id"].(string)
	}
	sid := firstSession(a.id)
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/revoke-user-session", map[string]any{"sessionId": sid, "userId": b.id.String()}), http.StatusForbidden)
	if sessionCount(t, s, a.id) != 1 {
		t.Fatal("session survives a mismatched claim")
	}
	own := firstSession(admin.id)
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/revoke-user-session", map[string]any{"sessionId": own, "userId": admin.id.String()}), http.StatusBadRequest)
	otherAdmin := firstSession(second.id)
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/revoke-user-session", map[string]any{"sessionId": otherAdmin, "userId": second.id.String()}), http.StatusForbidden)
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/revoke-user-session", map[string]any{"sessionId": sid, "userId": a.id.String()}), http.StatusOK)
	if sessionCount(t, s, a.id) != 0 || auditCount(t, s, admin.id, a.id.String(), "revoke_session") != 1 {
		t.Fatal("revoked + audited")
	}
	r := lastAudit(t, s, admin.id, a.id.String(), "revoke_session")
	if r.entityType != "session" || r.entityID.String() != sid || r.actorName == nil || *r.actorName != "Revoke Admin" || r.meta["session_id"] != sid {
		t.Fatalf("audit %+v", r)
	}
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/revoke-user-session", map[string]any{"sessionId": sid}), http.StatusBadRequest)
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/revoke-user-session", map[string]any{"sessionId": uuid.NewString(), "userId": a.id.String()}), http.StatusBadRequest)
}

func TestAdminMutationsWriteAuditLog(t *testing.T) {
	s := newServer(t)
	admin := adminAccount(t, s, "audit-admin", "Audit Admin")
	target := signUp(t, s, "audit-target", "Audit Target")
	tid := target.id.String()
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/set-role", map[string]any{"userId": tid, "role": "moderator"}), http.StatusOK)
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/ban-user", map[string]any{"userId": tid, "banReason": "audit"}), http.StatusOK)
	wantStatus(t, admin.c.PostJSON("/api/auth/admin/unban-user", map[string]any{"userId": tid}), http.StatusOK)
	for _, action := range []string{"set_role", "ban_user", "unban_user"} {
		if n := auditCount(t, s, admin.id, tid, action); n != 1 {
			t.Errorf("%s audited %d times", action, n)
		}
	}
	r := lastAudit(t, s, admin.id, tid, "ban_user")
	if r.aggType != "user" || r.entityID != target.id || r.actorName == nil || *r.actorName != "Audit Admin" || r.meta["reason"] != "audit" {
		t.Fatalf("audit %+v", r)
	}
	if role := lastAudit(t, s, admin.id, tid, "set_role"); role.meta["role"] != "moderator" {
		t.Fatalf("role audit %+v", role)
	}
}
