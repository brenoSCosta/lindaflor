package admin

import (
	"net/http"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// /admin/banners (src/app/admin/banners.rs): home carousel banners.

const bannersPath = "/admin/banners"

// GET /admin/banners.
func (h *handler) bannersPage(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	rows, err := store.ListAdminBanners(r.Context(), h.d.Pool, h.d.Storage)
	if err != nil {
		serverError(w, r, err)
		return
	}
	head(r, "Banners da home")
	render.Page(w, r, h.d, bannersPage(rows))
}

// POST /admin/banners/create (multipart, one image).
func (h *handler) bannerCreate(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	f, ok := parseMultipart(w, r)
	if !ok {
		return
	}
	form := store.BannerForm{
		Title: f.first("title"), Subtitle: f.first("subtitle"), CTALabel: f.first("cta_label"),
		LinkURL: f.first("link_url"), SortOrder: f.first("sort_order"), Active: f.has("active"),
	}
	if len(f.files) > 0 {
		form.Image = &f.files[0]
	}
	if err := store.CreateBanner(r.Context(), h.d.Pool, h.d.Storage, form); err != nil {
		if msg, ok := store.UserMessage(err); ok {
			failTo(w, r, bannersPath, msg)
			return
		}
		serverError(w, r, err)
		return
	}
	okTo(w, r, bannersPath, "Banner criado.")
}

func (h *handler) bannerIDAction(w http.ResponseWriter, r *http.Request, done string, fn func(uuid.UUID) error) {
	if _, ok := h.actor(w, r); !ok || !parseForm(w, r) {
		return
	}
	id, ok := parseUUID(r.PostFormValue("banner_id"))
	if !ok {
		failTo(w, r, bannersPath, "Banner inválido.")
		return
	}
	if err := fn(id); err != nil {
		serverError(w, r, err)
		return
	}
	okTo(w, r, bannersPath, done)
}

// POST /admin/banners/toggle.
func (h *handler) bannerToggle(w http.ResponseWriter, r *http.Request) {
	h.bannerIDAction(w, r, "Banner atualizado.", func(id uuid.UUID) error {
		return store.ToggleBanner(r.Context(), h.d.Pool, id)
	})
}

// POST /admin/banners/delete.
func (h *handler) bannerDelete(w http.ResponseWriter, r *http.Request) {
	h.bannerIDAction(w, r, "Banner excluído.", func(id uuid.UUID) error {
		return store.DeleteBanner(r.Context(), h.d.Pool, h.d.Storage, id)
	})
}
