package admin

import (
	"net/http"
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// /admin/cupons (src/app/admin/cupons.rs): create, search (+ /lista),
// toggle (+ htmx toggle-proc), delete and per-user assignment (+ the
// /usuarios user search fragment).

const (
	couponsPath      = "/admin/cupons"
	couponsListPath  = "/admin/cupons/lista"
	couponsUsersPath = "/admin/cupons/usuarios"
)

type couponList struct {
	Q     string
	Rows  []store.AdminCouponRow
	Error string
}

func (h *handler) loadCoupons(r *http.Request, rawQ string) couponList {
	q := store.ClampCouponQuery(rawQ)
	rows, err := store.ListAdminCoupons(r.Context(), h.d.Pool, q)
	if err != nil {
		serverLog(r, err)
		return couponList{Q: q, Error: store.CouponDBErrorMessage(err)}
	}
	return couponList{Q: q, Rows: rows}
}

type assignableUser struct {
	ID, Name, Email string
	Assigned        bool
}

type couponUsers struct {
	CouponID string
	UQ       string
	Users    []assignableUser
	Total    int64
}

func (h *handler) loadCouponUsers(r *http.Request, actor auth.Actor, couponID, rawUQ string) (couponUsers, error) {
	out := couponUsers{CouponID: couponID, UQ: store.ClampCouponQuery(rawUQ)}
	assigned := map[uuid.UUID]bool{}
	if id, ok := parseUUID(couponID); ok {
		var err error
		if assigned, err = store.CouponAssigneeIDs(r.Context(), h.d.Pool, id); err != nil {
			serverLog(r, err)
			assigned = map[uuid.UUID]bool{}
		}
	}
	list, err := h.auth.ListUsers(r.Context(), actor, out.UQ, 50, 0)
	if err != nil {
		return out, err
	}
	out.Total = list.Total
	for _, u := range list.Users {
		out.Users = append(out.Users, assignableUser{ID: u.ID.String(), Name: u.Name, Email: u.Email, Assigned: assigned[u.ID]})
	}
	return out, nil
}

type couponsPageData struct {
	List         couponList
	SelectedID   string
	SelectedCode string
	Assigned     []store.AssignedUser
	Users        couponUsers
}

// GET /admin/cupons?q=&cupom=&uq=.
func (h *handler) couponsPage(w http.ResponseWriter, r *http.Request) {
	actor, ok := h.actor(w, r)
	if !ok {
		return
	}
	query := r.URL.Query()
	d := couponsPageData{List: h.loadCoupons(r, query.Get("q"))}
	selected := strings.TrimSpace(query.Get("cupom"))
	for _, c := range d.List.Rows {
		if c.ID.String() == selected {
			d.SelectedID, d.SelectedCode = selected, c.Code
		}
	}
	if d.SelectedID != "" {
		id, _ := parseUUID(d.SelectedID)
		assigned, err := store.CouponAssignees(r.Context(), h.d.Pool, id)
		if err != nil {
			serverLog(r, err)
		}
		d.Assigned = assigned
		if d.Users, err = h.loadCouponUsers(r, actor, d.SelectedID, query.Get("uq")); err != nil {
			serverError(w, r, err)
			return
		}
	}
	head(r, "Cupons")
	render.Page(w, r, h.d, couponsPage(d))
}

// GET /admin/cupons/lista (htmx fragment).
func (h *handler) couponsList(w http.ResponseWriter, r *http.Request) {
	if !httpx.IsHTMX(r) {
		h.couponsPage(w, r)
		return
	}
	if _, ok := h.actor(w, r); !ok {
		return
	}
	l := h.loadCoupons(r, r.URL.Query().Get("q"))
	fragment(w, r, store.CouponsURL(l.Q, "", ""), couponResults(l))
}

// GET /admin/cupons/usuarios?cupom=&uq= (htmx fragment of the assign card).
func (h *handler) couponUsersList(w http.ResponseWriter, r *http.Request) {
	if !httpx.IsHTMX(r) {
		h.couponsPage(w, r)
		return
	}
	actor, ok := h.actor(w, r)
	if !ok {
		return
	}
	q := r.URL.Query()
	u, err := h.loadCouponUsers(r, actor, strings.TrimSpace(q.Get("cupom")), q.Get("uq"))
	if err != nil {
		serverError(w, r, err)
		return
	}
	render.Fragment(w, r, http.StatusOK, couponUserResults(u))
}

type couponForm struct{ q, cupom, uq, back string }

func readCouponForm(r *http.Request) couponForm {
	f := couponForm{
		q:     r.PostFormValue("q"),
		cupom: strings.TrimSpace(r.PostFormValue("cupom")),
		uq:    strings.TrimSpace(r.PostFormValue("uq")),
	}
	f.back = store.CouponsURL(f.q, f.cupom, f.uq)
	return f
}

func couponFail(w http.ResponseWriter, r *http.Request, back string, err error) {
	if _, ok := store.UserMessage(err); !ok {
		serverLog(r, err)
	}
	failTo(w, r, back, userMessage(err, msgGenericError))
}

// POST /admin/cupons/create.
func (h *handler) couponCreate(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok || !parseForm(w, r) {
		return
	}
	back := store.CouponsURL(r.PostFormValue("q"), "", "")
	err := store.CreateCoupon(r.Context(), h.d.Pool, store.CouponForm{
		Code: r.PostFormValue("code"), DiscountType: r.PostFormValue("discount_type"),
		DiscountValue: r.PostFormValue("discount_value"), MinSubtotalCents: r.PostFormValue("min_subtotal_cents"),
		MaxDiscountCents: r.PostFormValue("max_discount_cents"), UsageType: r.PostFormValue("usage_type"),
		MaxUses: r.PostFormValue("max_uses"), PerUserLimit: r.PostFormValue("per_user_limit"),
		StartsAt: r.PostFormValue("starts_at"), ExpiresAt: r.PostFormValue("expires_at"),
	})
	if err != nil {
		couponFail(w, r, back, err)
		return
	}
	toast.Redirect(w, r, back)
}

// couponIDAction runs fn with the posted coupon_id.
func (h *handler) couponIDAction(w http.ResponseWriter, r *http.Request, fn func(id uuid.UUID, f couponForm) (string, error)) {
	if _, ok := h.actor(w, r); !ok || !parseForm(w, r) {
		return
	}
	f := readCouponForm(r)
	id, ok := parseUUID(r.PostFormValue("coupon_id"))
	if !ok {
		failTo(w, r, f.back, "Cupom inválido.")
		return
	}
	back, err := fn(id, f)
	if err != nil {
		couponFail(w, r, f.back, err)
		return
	}
	toast.Redirect(w, r, back)
}

// POST /admin/cupons/toggle (no-JS fallback of toggle-proc).
func (h *handler) couponToggle(w http.ResponseWriter, r *http.Request) {
	h.couponIDAction(w, r, func(id uuid.UUID, f couponForm) (string, error) {
		return f.back, store.ToggleCoupon(r.Context(), h.d.Pool, id)
	})
}

// POST /admin/cupons/toggle-proc: htmx fast path; toggles and answers the
// re-rendered coupon list fragment for the posted q.
func (h *handler) couponToggleProc(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok || !parseForm(w, r) {
		return
	}
	if !httpx.IsHTMX(r) {
		h.couponToggle(w, r)
		return
	}
	if id, ok := parseUUID(r.PostFormValue("coupon_id")); ok {
		if err := store.ToggleCoupon(r.Context(), h.d.Pool, id); err != nil {
			toast.Set(w, r, toast.Error(userMessage(err, msgGenericError)))
		}
	}
	render.Fragment(w, r, http.StatusOK, couponResults(h.loadCoupons(r, r.PostFormValue("q"))))
}

// POST /admin/cupons/delete.
func (h *handler) couponDelete(w http.ResponseWriter, r *http.Request) {
	h.couponIDAction(w, r, func(id uuid.UUID, f couponForm) (string, error) {
		if err := store.DeleteCoupon(r.Context(), h.d.Pool, id); err != nil {
			return "", err
		}
		keep := f.cupom
		if keep == strings.TrimSpace(r.PostFormValue("coupon_id")) {
			keep = ""
		}
		return store.CouponsURL(f.q, keep, f.uq), nil
	})
}

// POST /admin/cupons/assign.
func (h *handler) couponAssign(w http.ResponseWriter, r *http.Request) {
	h.couponIDAction(w, r, func(id uuid.UUID, f couponForm) (string, error) {
		return f.back, store.AssignCoupon(r.Context(), h.d.Pool, id, r.PostForm["user_ids"])
	})
}

// POST /admin/cupons/unassign.
func (h *handler) couponUnassign(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok || !parseForm(w, r) {
		return
	}
	f := readCouponForm(r)
	couponID, ok1 := parseUUID(r.PostFormValue("coupon_id"))
	userID, ok2 := parseUUID(r.PostFormValue("user_id"))
	if !ok1 || !ok2 {
		failTo(w, r, f.back, "Identificador inválido.")
		return
	}
	if err := store.UnassignCoupon(r.Context(), h.d.Pool, couponID, userID); err != nil {
		couponFail(w, r, f.back, err)
		return
	}
	toast.Redirect(w, r, f.back)
}
