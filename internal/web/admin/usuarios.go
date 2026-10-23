package admin

import (
	"net/http"
	"strings"
	"time"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/components"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// /admin/usuarios (src/app/admin/usuarios.rs): user directory (+ /lista
// fragment) and the role/name/ban/unban/sessions/remove dialogs, opened
// by query parameters so they work without JS.

const (
	usersPath     = "/admin/usuarios"
	usersListPath = "/admin/usuarios/lista"
	userQueryMax  = 80
)

func usersURL(path, q string, page int64, extraKey, extraVal string) string {
	return withQuery(path, "q", q, "page", pageParam(page), extraKey, extraVal)
}

func clampUserPage(p int64) int64 { return min(max(p, 1), 10_000) }

// adminErrorMessage maps auth service errors to the pt-BR toasts.
func adminErrorMessage(err error) string {
	msg := strings.ToLower(err.Error())
	switch {
	case strings.Contains(msg, "cannot ban yourself"):
		return "Você não pode banir a si mesmo."
	case strings.Contains(msg, "cannot unban yourself"):
		return "Você não pode desbanir a si mesmo."
	case strings.Contains(msg, "cannot remove yourself"):
		return "Você não pode remover a si mesmo."
	case strings.Contains(msg, "cannot change own role"):
		return "Você não pode alterar seu próprio papel."
	case strings.Contains(msg, "cannot revoke own session") && !strings.Contains(msg, "cannot revoke own sessions"):
		return "Você não pode revogar sua própria sessão."
	case strings.Contains(msg, "cannot revoke own sessions"):
		return "Você não pode revogar suas próprias sessões."
	case strings.Contains(msg, "does not belong"):
		return "Sessão não pertence ao usuário."
	case strings.Contains(msg, "cannot impersonate yourself"):
		return "Você não pode atuar como si mesmo."
	case strings.Contains(msg, "already impersonating"):
		return "Você já está atuando como outro usuário."
	case strings.Contains(msg, "banned user"):
		return "Não é possível atuar como um usuário banido."
	case strings.Contains(msg, "name"):
		return "Informe o nome."
	case strings.Contains(msg, "role"):
		return "Papel inválido."
	case strings.Contains(msg, "not found"):
		return "Não encontrado."
	case strings.Contains(msg, "forbidden"):
		return "Você não tem permissão para esta ação."
	}
	return msgGenericError
}

func roleLabel(role *string) string {
	switch {
	case role != nil && strings.EqualFold(*role, "admin"):
		return "Administrador"
	case role != nil && strings.EqualFold(*role, "moderator"):
		return "Moderador"
	}
	return "Usuário"
}

func roleVariant(role *string) components.BadgeVariant {
	switch {
	case role != nil && strings.EqualFold(*role, "admin"):
		return components.BadgePrimary
	case role != nil && strings.EqualFold(*role, "moderator"):
		return components.BadgeOutline
	}
	return components.BadgeSecondary
}

type userRow struct {
	ID, Q, Page                                      string
	Name, Email                                      string
	RoleLabel                                        string
	RoleVariant                                      components.BadgeVariant
	Banned                                           bool
	ShowBan, ShowUnban, ShowImpersonate, ShowRemove  bool
	HrefRole, HrefEdit, HrefBan, HrefUnban, HrefSess string
	HrefRevokeAll, HrefRemove                        string
}

type userDirectory struct {
	Q     string
	Page  int64
	Rows  []userRow
	Pager pagerData
}

func (h *handler) loadUserDirectory(r *http.Request, actor auth.Actor, rawQ string, rawPage int64) (userDirectory, error) {
	q := store.ClampAdminQuery(rawQ, userQueryMax)
	page := clampUserPage(rawPage)
	const size = auth.AdminUserPageSize
	list, err := h.auth.ListUsers(r.Context(), actor, q, size, (page-1)*size)
	if err != nil {
		return userDirectory{}, err
	}
	totalPages := (list.Total + size - 1) / size
	if totalPages > 0 && page > totalPages {
		page = totalPages
		if list, err = h.auth.ListUsers(r.Context(), actor, q, size, (page-1)*size); err != nil {
			return userDirectory{}, err
		}
	}
	d := userDirectory{Q: q, Page: page}
	pageStr := itoa(page)
	for _, u := range list.Users {
		id := u.ID.String()
		self := u.ID == actor.User.ID
		d.Rows = append(d.Rows, userRow{
			ID: id, Q: q, Page: pageStr, Name: u.Name, Email: u.Email,
			RoleLabel: roleLabel(u.Role), RoleVariant: roleVariant(u.Role), Banned: u.Banned,
			ShowBan: !self && !u.Banned, ShowUnban: !self && u.Banned,
			ShowImpersonate: !self && !u.IsAdmin(), ShowRemove: !self,
			HrefRole:      usersURL(usersPath, q, page, "papel", id),
			HrefEdit:      usersURL(usersPath, q, page, "editar", id),
			HrefBan:       usersURL(usersPath, q, page, "banir", id),
			HrefUnban:     usersURL(usersPath, q, page, "desbanir", id),
			HrefSess:      usersURL(usersPath, q, page, "sessoes", id),
			HrefRevokeAll: usersURL(usersPath, q, page, "revogar_todas", id),
			HrefRemove:    usersURL(usersPath, q, page, "remover", id),
		})
	}
	d.Pager = pagerFor(page, totalPages, func(p int64) dirLink {
		return dirLink{Page: usersURL(usersPath, q, p, "", ""), Frag: usersURL(usersListPath, q, p, "", "")}
	})
	return d, nil
}

// pagerFor builds the pager links (link builds Page/Frag for a page).
func pagerFor(page, totalPages int64, link func(int64) dirLink) pagerData {
	p := pagerData{Show: totalPages > 1, Label: "Página 1 de 1"}
	if totalPages > 0 {
		p.Label = sprintf("Página %d de %d", page, totalPages)
	}
	if page > 1 {
		l := link(page - 1)
		l.Label = "Anterior"
		p.Prev = &l
	}
	if totalPages > 0 && page < totalPages {
		l := link(page + 1)
		l.Label = "Próxima"
		p.Next = &l
	}
	return p
}

// GET /admin/usuarios/lista (htmx fragment; full page otherwise).
func (h *handler) usersList(w http.ResponseWriter, r *http.Request) {
	if !httpx.IsHTMX(r) {
		h.usersPage(w, r)
		return
	}
	actor, ok := h.actor(w, r)
	if !ok {
		return
	}
	q := r.URL.Query()
	d, err := h.loadUserDirectory(r, actor, q.Get("q"), store.ParsePage(q.Get("page")))
	if err != nil {
		serverError(w, r, err)
		return
	}
	fragment(w, r, usersURL(usersPath, d.Q, d.Page, "", ""), userResults(d))
}

type userSession struct {
	ID, ShortID, CreatedAt, ExpiresAt, IP, UA string
}

type usersPanel struct {
	Kind      string // papel | editar | banir | desbanir | sessoes | revogar_todas | remover
	UserID    string
	Name      string
	Role      string
	Sessions  []userSession
	Q         string
	Page      string
	CancelURL string
	RevokeURL string
}

type usersPageData struct {
	Error string
	Dir   userDirectory
	Panel *usersPanel
}

var panelKeys = []string{"papel", "editar", "banir", "desbanir", "sessoes", "revogar_todas", "remover"}

// GET /admin/usuarios.
func (h *handler) usersPage(w http.ResponseWriter, r *http.Request) {
	actor, ok := h.actor(w, r)
	if !ok {
		return
	}
	query := r.URL.Query()
	q := store.ClampAdminQuery(query.Get("q"), userQueryMax)
	page := clampUserPage(store.ParsePage(query.Get("page")))
	dir, err := h.loadUserDirectory(r, actor, q, page)
	if err != nil {
		serverError(w, r, err)
		return
	}
	data := usersPageData{Dir: dir}

	// The panel kind is the first present key (papel → editar → … →
	// remover); the user id is the first non-empty value among them.
	kind := ""
	for _, k := range panelKeys {
		if query.Has(k) {
			kind = k
			break
		}
	}
	raw := firstPanelValue(query)
	var target *auth.User
	if raw != "" {
		if id, ok := parseUUID(raw); ok {
			if u, err := h.auth.LoadUser(r.Context(), id); err == nil {
				target = u
			}
		}
		if target == nil {
			data.Error = "Usuário não encontrado."
		}
	}
	if target != nil && kind != "" {
		p := &usersPanel{
			Kind: kind, UserID: target.ID.String(), Name: target.Name, Role: "user",
			Q: q, Page: itoa(page), CancelURL: usersURL(usersPath, q, page, "", ""),
			RevokeURL: usersURL(usersPath, q, page, "revogar_todas", target.ID.String()),
		}
		if target.Role != nil {
			p.Role = strings.ToLower(*target.Role)
		}
		if kind == "sessoes" {
			sessions, err := h.auth.ListUserSessions(r.Context(), actor, target.ID)
			if err != nil {
				serverError(w, r, err)
				return
			}
			for _, s := range sessions {
				id := s.ID.String()
				p.Sessions = append(p.Sessions, userSession{
					ID: id, ShortID: id[:8] + "…",
					CreatedAt: s.CreatedAt.UTC().Format(time.RFC3339Nano),
					ExpiresAt: s.ExpiresAt.UTC().Format(time.RFC3339Nano),
					IP:        orDash(s.IPAddress), UA: orDash(s.UserAgent),
				})
			}
		}
		data.Panel = p
	}
	head(r, "Gerenciamento de usuários")
	render.Page(w, r, h.d, usersPage(data))
}

func firstPanelValue(query map[string][]string) string {
	for _, k := range panelKeys {
		if vs, ok := query[k]; ok && len(vs) > 0 {
			if v := strings.TrimSpace(vs[0]); v != "" {
				return v
			}
		}
	}
	return ""
}

func orDash(s *string) string {
	if s == nil {
		return "—"
	}
	return *s
}

// --- POST actions ---

type userForm struct {
	q    string
	page int64
	back string
}

func readUserForm(r *http.Request) userForm {
	f := userForm{q: r.PostFormValue("q"), page: formPage(r.PostFormValue("page"))}
	f.back = usersURL(usersPath, f.q, f.page, "", "")
	return f
}

// userAction runs fn for the posted user_id; success redirects back
// silently (like Rust), failures toast the mapped message.
func (h *handler) userAction(w http.ResponseWriter, r *http.Request, fn func(actor auth.Actor, id uuid.UUID) error) {
	actor, ok := h.actor(w, r)
	if !ok || !parseForm(w, r) {
		return
	}
	f := readUserForm(r)
	id, ok := parseUUID(r.PostFormValue("user_id"))
	if !ok {
		failTo(w, r, f.back, "Identificador inválido.")
		return
	}
	if err := fn(actor, id); err != nil {
		logUnexpected(r, err)
		failTo(w, r, f.back, adminErrorMessage(err))
		return
	}
	toast.Redirect(w, r, f.back)
}

func logUnexpected(r *http.Request, err error) {
	if err != nil && !auth.IsClientError(err) {
		serverLog(r, err)
	}
}

// POST /admin/usuarios/papel.
func (h *handler) userSetRole(w http.ResponseWriter, r *http.Request) {
	h.userAction(w, r, func(a auth.Actor, id uuid.UUID) error {
		_, err := h.auth.SetUserRole(r.Context(), a, id, r.PostFormValue("role"))
		return err
	})
}

// POST /admin/usuarios/nome.
func (h *handler) userRename(w http.ResponseWriter, r *http.Request) {
	h.userAction(w, r, func(a auth.Actor, id uuid.UUID) error {
		_, err := h.auth.AdminRenameUser(r.Context(), a, id, r.PostFormValue("name"))
		return err
	})
}

// POST /admin/usuarios/banir.
func (h *handler) userBan(w http.ResponseWriter, r *http.Request) {
	h.userAction(w, r, func(a auth.Actor, id uuid.UUID) error {
		_, err := h.auth.BanUser(r.Context(), a, id, r.PostFormValue("ban_reason"))
		return err
	})
}

// POST /admin/usuarios/desbanir.
func (h *handler) userUnban(w http.ResponseWriter, r *http.Request) {
	h.userAction(w, r, func(a auth.Actor, id uuid.UUID) error {
		_, err := h.auth.UnbanUser(r.Context(), a, id)
		return err
	})
}

// POST /admin/usuarios/sessoes/revogar-todas.
func (h *handler) userRevokeSessions(w http.ResponseWriter, r *http.Request) {
	h.userAction(w, r, func(a auth.Actor, id uuid.UUID) error {
		return h.auth.RevokeUserSessions(r.Context(), a, id)
	})
}

// POST /admin/usuarios/remover.
func (h *handler) userRemove(w http.ResponseWriter, r *http.Request) {
	h.userAction(w, r, func(a auth.Actor, id uuid.UUID) error {
		return h.auth.RemoveUser(r.Context(), a, id)
	})
}

// POST /admin/usuarios/sessoes/revogar: back to the sessions dialog.
func (h *handler) userRevokeSession(w http.ResponseWriter, r *http.Request) {
	actor, ok := h.actor(w, r)
	if !ok || !parseForm(w, r) {
		return
	}
	f := readUserForm(r)
	sessionID, ok1 := parseUUID(r.PostFormValue("session_id"))
	userID, ok2 := parseUUID(r.PostFormValue("user_id"))
	if !ok1 || !ok2 {
		failTo(w, r, f.back, "Identificador inválido.")
		return
	}
	back := usersURL(usersPath, f.q, f.page, "sessoes", strings.TrimSpace(r.PostFormValue("user_id")))
	if err := h.auth.RevokeUserSession(r.Context(), actor, sessionID, userID); err != nil {
		logUnexpected(r, err)
		failTo(w, r, back, adminErrorMessage(err))
		return
	}
	toast.Redirect(w, r, back)
}

// POST /admin/usuarios/atuar: start impersonating, then a full navigation
// to /dashboard (the session cookie changed).
func (h *handler) userImpersonate(w http.ResponseWriter, r *http.Request) {
	actor, ok := h.actor(w, r)
	if !ok || !parseForm(w, r) {
		return
	}
	f := readUserForm(r)
	id, ok := parseUUID(r.PostFormValue("user_id"))
	if !ok {
		failTo(w, r, f.back, "Identificador inválido.")
		return
	}
	if _, _, err := h.auth.Impersonate(w, r, actor, id); err != nil {
		logUnexpected(r, err)
		msg := adminErrorMessage(err)
		if err.Error() == "forbidden" {
			msg = "Não é possível atuar como um administrador."
		}
		failTo(w, r, f.back, msg)
		return
	}
	fullRedirect(w, r, "/dashboard")
}

// fullRedirect is a 303, or HX-Redirect (full page load) for htmx.
func fullRedirect(w http.ResponseWriter, r *http.Request, loc string) {
	if httpx.IsHTMX(r) {
		w.Header().Set(httpx.HXRedirect, loc)
		w.WriteHeader(http.StatusOK)
		return
	}
	http.Redirect(w, r, loc, http.StatusSeeOther)
}
