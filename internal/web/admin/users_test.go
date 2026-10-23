package admin_test

import (
	"net/http"
	"testing"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/testutil"
)

func TestUsersDirectoryAndDialogs(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	u := signUp(t, s, "maria", "Maria Cliente")

	page := a.c.Get("/admin/usuarios?q=Maria")
	wantStatus(t, page, http.StatusOK)
	wantContains(t, page.Body, "Maria Cliente", u.email, "Usuário", "Definir papel", "Atuar como usuário",
		"/admin/usuarios?q=Maria&amp;papel="+u.id.String())

	// Each dialog renders from its query parameter.
	dialogs := map[string]string{
		"papel": "Definir papel", "editar": "Atualizar usuário", "banir": "Isso impedirá Maria Cliente de fazer login",
		"desbanir": "Isso permitirá que Maria Cliente faça login novamente.", "sessoes": "Sessões de Maria Cliente",
		"revogar_todas": "Isso encerrará a sessão de Maria Cliente", "remover": "Isso excluirá permanentemente Maria Cliente",
	}
	for key, want := range dialogs {
		res := a.c.Get("/admin/usuarios?" + key + "=" + u.id.String())
		wantStatus(t, res, http.StatusOK)
		wantContains(t, res.Body, want, "<dialog")
	}
	wantContains(t, a.c.Get("/admin/usuarios?sessoes="+u.id.String()).Body, "Revogar todas as sessões", "Criada ")
	wantContains(t, a.c.Get("/admin/usuarios?papel="+uuid.NewString()).Body, "Usuário não encontrado.")

	frag := a.c.HTMX().Get("/admin/usuarios/lista?q=Maria")
	wantStatus(t, frag, http.StatusOK)
	wantContains(t, frag.Body, `id="user-results"`, "Maria Cliente")
	wantNotContains(t, frag.Body, "<html", "Gerenciamento de usuários")
	if frag.Header.Get("HX-Replace-Url") != "/admin/usuarios?q=Maria" {
		t.Fatalf("HX-Replace-Url = %q", frag.Header.Get("HX-Replace-Url"))
	}
}

func TestUsersPagination(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	for i := range 11 {
		exec(t, s, `INSERT INTO users (id, name, email, email_verified) VALUES ($1, $2, $3, true)`,
			newID(), "Paginado", testutil.UniqueEmail("pag"+string(rune('a'+i))))
	}
	res := a.c.Get("/admin/usuarios?q=Paginado")
	wantContains(t, res.Body, "Página 1 de 2", "Próxima", `hx-get="/admin/usuarios/lista?q=Paginado&amp;page=2"`)
	// Out-of-range pages clamp to the last one.
	wantContains(t, a.c.Get("/admin/usuarios?q=Paginado&page=99").Body, "Página 2 de 2", "Anterior")
}

func TestUserAdminActions(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	u := signUp(t, s, "alvo", "Alvo Teste")
	back := "/admin/usuarios?q=Alvo&page=2"
	post := func(path string, kv ...string) *testutil.Response {
		v := form(append([]string{"user_id", u.id.String(), "q", "Alvo", "page", "2"}, kv...)...)
		return a.c.PostForm(path, v)
	}

	wantRedirect(t, post("/admin/usuarios/papel", "role", "moderator"), back)
	if r := scalar[string](t, s, `SELECT role FROM users WHERE id = $1`, u.id); r != "moderator" {
		t.Fatalf("role = %q", r)
	}
	wantRedirect(t, post("/admin/usuarios/papel", "role", "chefe"), back)
	wantToast(t, a.c, toast.ErrorType, "Papel inválido.")

	wantRedirect(t, post("/admin/usuarios/nome", "name", "Alvo Renomeado"), back)
	if n := scalar[string](t, s, `SELECT name FROM users WHERE id = $1`, u.id); n != "Alvo Renomeado" {
		t.Fatalf("name = %q", n)
	}

	wantRedirect(t, post("/admin/usuarios/banir", "ban_reason", "spam"), back)
	if !scalar[bool](t, s, `SELECT banned FROM users WHERE id = $1`, u.id) {
		t.Fatal("not banned")
	}
	// Banned users lost their sessions.
	wantRedirect(t, u.c.Get("/admin"), "/login*")
	wantRedirect(t, post("/admin/usuarios/atuar"), back)
	wantToast(t, a.c, toast.ErrorType, "Não é possível atuar como um usuário banido.")
	wantRedirect(t, post("/admin/usuarios/desbanir"), back)
	if scalar[bool](t, s, `SELECT banned FROM users WHERE id = $1`, u.id) {
		t.Fatal("still banned")
	}

	// Self-protection messages.
	self := form("user_id", a.id.String(), "q", "", "page", "1")
	wantRedirect(t, a.c.PostForm("/admin/usuarios/banir", self), "/admin/usuarios")
	wantToast(t, a.c, toast.ErrorType, "Você não pode banir a si mesmo.")
	wantRedirect(t, a.c.PostForm("/admin/usuarios/remover", self), "/admin/usuarios")
	wantToast(t, a.c, toast.ErrorType, "Você não pode remover a si mesmo.")
	wantRedirect(t, a.c.PostForm("/admin/usuarios/papel", form("user_id", "x")), "/admin/usuarios")
	wantToast(t, a.c, toast.ErrorType, "Identificador inválido.")

	// Sessions: revoke one (back to the dialog), then all.
	u2 := s.Client(t)
	wantStatus(t, u2.SignIn(u.email, strongPW), http.StatusOK)
	sessionID := scalar[uuid.UUID](t, s, `SELECT id FROM sessions WHERE user_id = $1 LIMIT 1`, u.id)
	res := a.c.PostForm("/admin/usuarios/sessoes/revogar", form("session_id", sessionID.String(), "user_id", u.id.String(), "q", "Alvo", "page", "1"))
	wantRedirect(t, res, "/admin/usuarios?q=Alvo&sessoes="+u.id.String())
	res = a.c.PostForm("/admin/usuarios/sessoes/revogar", form("session_id", sessionID.String(), "user_id", a.id.String()))
	wantRedirect(t, res, "/admin/usuarios?sessoes="+a.id.String())
	if q := flash(t, a.c); q[len(q)-1].Type != toast.ErrorType {
		t.Fatal("revoking a foreign session must fail")
	}
	wantRedirect(t, post("/admin/usuarios/sessoes/revogar-todas"), back)
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM sessions WHERE user_id = $1`, u.id); n != 0 {
		t.Fatalf("sessions left = %d", n)
	}

	wantRedirect(t, post("/admin/usuarios/remover"), back)
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM users WHERE id = $1`, u.id); n != 0 {
		t.Fatal("user not removed")
	}
}

func TestImpersonation(t *testing.T) {
	s := testutil.NewServer(t, testutil.Options{})
	a := adminAccount(t, s)
	u := signUp(t, s, "imp", "Pessoa Atuada")
	other := adminAccount(t, s)

	res := a.c.PostForm("/admin/usuarios/atuar", form("user_id", other.id.String()))
	wantRedirect(t, res, "/admin/usuarios")
	wantToast(t, a.c, toast.ErrorType, "Não é possível atuar como um administrador.")
	wantRedirect(t, a.c.PostForm("/admin/usuarios/atuar", form("user_id", a.id.String())), "/admin/usuarios")
	wantToast(t, a.c, toast.ErrorType, "Você não pode atuar como si mesmo.")

	// htmx: full navigation (HX-Redirect) since the session changed.
	h := s.Client(t)
	h.HTTP.Jar = a.c.HTTP.Jar
	res = h.HTMX().PostForm("/admin/usuarios/atuar", form("user_id", u.id.String()))
	wantStatus(t, res, http.StatusOK)
	if res.Header.Get("HX-Redirect") != "/dashboard" {
		t.Fatalf("HX-Redirect = %q", res.Header.Get("HX-Redirect"))
	}
	// Now acting as the customer: the admin area is closed.
	wantRedirect(t, a.c.Get("/admin"), "/dashboard")
	if n := scalar[int64](t, s, `SELECT COUNT(*) FROM audit_events WHERE metadata->>'action' = 'impersonate_start' AND aggregate_id = $1`, u.id.String()); n != 1 {
		t.Fatal("impersonation not audited")
	}
}
