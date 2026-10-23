package admin

import (
	"net/http"
	"strings"

	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// /admin/avaliacoes (src/app/admin/avaliacoes.rs): moderation queue,
// pending first, approve/reject audited.

const reviewsPath = "/admin/avaliacoes"

type reviewsData struct {
	Pending int64
	Reviews []store.AdminReview
}

// GET /admin/avaliacoes.
func (h *handler) reviewsPage(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	var d reviewsData
	var err error
	if d.Pending, err = store.CountPendingReviews(r.Context(), h.d.Pool); err != nil {
		serverError(w, r, err)
		return
	}
	if d.Reviews, err = store.ListReviewsForAdmin(r.Context(), h.d.Pool, store.AdminReviewsLimit); err != nil {
		serverError(w, r, err)
		return
	}
	head(r, "Avaliações")
	render.Page(w, r, h.d, reviewsPage(d))
}

// POST /admin/avaliacoes: review_id + action=approve|reject.
func (h *handler) reviewsModerate(w http.ResponseWriter, r *http.Request) {
	actor, ok := h.actor(w, r)
	if !ok || !parseForm(w, r) {
		return
	}
	var approve, valid bool
	switch r.PostFormValue("action") {
	case "approve":
		approve, valid = true, true
	case "reject":
		valid = true
	}
	id, idOK := parseUUID(strings.TrimSpace(r.PostFormValue("review_id")))
	if !valid || !idOK {
		failTo(w, r, reviewsPath, "Ação inválida.")
		return
	}
	found, err := store.ModerateReview(r.Context(), h.d.Pool, id, approve, actor.User.ID, actor.User.Name)
	if err != nil {
		serverError(w, r, err)
		return
	}
	msg, success := store.ModerationMessage(found, approve)
	if !success {
		failTo(w, r, reviewsPath, msg)
		return
	}
	okTo(w, r, reviewsPath, msg)
}
