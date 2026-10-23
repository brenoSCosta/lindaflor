package admin

import (
	"errors"
	"net/http"

	"github.com/go-chi/chi/v5"
	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// /admin/sets (src/app/admin/sets.rs, sets/id.rs).

const setsPath = "/admin/sets"

// GET /admin/sets.
func (h *handler) setsPage(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	rows, err := store.ListAdminSets(r.Context(), h.d.Pool, h.d.Storage)
	if err != nil {
		serverError(w, r, err)
		return
	}
	head(r, "Conjuntos")
	render.Page(w, r, h.d, setsPage(rows))
}

func setForm(f multipartForm) store.SetForm {
	return store.SetForm{
		Name: f.first("name"), Slug: f.first("slug"), Description: f.first("description"),
		Price: f.first("price"), CompareAtPrice: f.first("compare_at_price"), Active: f.has("active"),
		MemberIDs: f.all("member_product_ids[]"), DeleteImageIDs: f.all("delete_image_id"), Images: f.files,
	}
}

// POST /admin/sets/create (multipart).
func (h *handler) setCreate(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	f, ok := parseMultipart(w, r)
	if !ok {
		return
	}
	res, err := store.CreateSet(r.Context(), h.d.Pool, h.d.Storage, setForm(f))
	if err != nil {
		if msg, ok := store.UserMessage(err); ok {
			failTo(w, r, setsPath, msg)
			return
		}
		serverError(w, r, err)
		return
	}
	edit := setsPath + "/" + res.ID.String()
	if res.ImageFailed {
		failTo(w, r, edit, res.Message())
		return
	}
	okTo(w, r, edit, res.Message())
}

// setIDAction parses set_id for toggle/delete.
func (h *handler) setIDAction(w http.ResponseWriter, r *http.Request, done string, fn func(id uuid.UUID) error) {
	if _, ok := h.actor(w, r); !ok || !parseForm(w, r) {
		return
	}
	id, ok := parseUUID(r.PostFormValue("set_id"))
	if !ok {
		failTo(w, r, setsPath, "Conjunto inválido.")
		return
	}
	if err := fn(id); err != nil {
		serverError(w, r, err)
		return
	}
	okTo(w, r, setsPath, done)
}

// POST /admin/sets/toggle.
func (h *handler) setToggle(w http.ResponseWriter, r *http.Request) {
	h.setIDAction(w, r, "Conjunto atualizado.", func(id uuid.UUID) error {
		return store.ToggleSet(r.Context(), h.d.Pool, id)
	})
}

// POST /admin/sets/delete.
func (h *handler) setDelete(w http.ResponseWriter, r *http.Request) {
	h.setIDAction(w, r, "Conjunto excluído.", func(id uuid.UUID) error {
		return store.DeleteSet(r.Context(), h.d.Pool, h.d.Storage, id)
	})
}

// GET /admin/sets/{id}.
func (h *handler) setEditPage(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	var s *store.AdminSetEdit
	if id, ok := parseUUID(chi.URLParam(r, "id")); ok {
		var err error
		if s, err = store.LoadAdminSet(r.Context(), h.d.Pool, h.d.Storage, id); err != nil {
			serverError(w, r, err)
			return
		}
	}
	if s == nil {
		head(r, "Conjunto não encontrado")
		render.Page(w, r, h.d, notFoundBody("Conjunto não encontrado", setsPath, "Voltar para conjuntos"))
		return
	}
	head(r, "Editar conjunto")
	render.Page(w, r, h.d, setEditPage(s))
}

// POST /admin/sets/{id}/update (multipart).
func (h *handler) setUpdate(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	id, ok := parseUUID(chi.URLParam(r, "id"))
	if !ok {
		failTo(w, r, setsPath, "Conjunto inválido.")
		return
	}
	f, ok := parseMultipart(w, r)
	if !ok {
		return
	}
	back := setsPath + "/" + id.String()
	sf := setForm(f)
	slots, zipErr := store.ZipSlotForms(f.all("slot_label[]"), f.all("slot_variant[]"))
	sf.Slots = slots
	if zipErr != nil {
		failTo(w, r, back, userMessage(zipErr, msgGenericError))
		return
	}
	err := store.UpdateSet(r.Context(), h.d.Pool, h.d.Storage, id, sf)
	switch {
	case err == nil:
		okTo(w, r, back, "Alterações salvas com sucesso.")
	case errors.Is(err, store.ErrSetNotFound):
		failTo(w, r, setsPath, "Conjunto não encontrado.")
	default:
		if msg, ok := store.UserMessage(err); ok {
			failTo(w, r, back, msg)
			return
		}
		serverError(w, r, err)
	}
}
