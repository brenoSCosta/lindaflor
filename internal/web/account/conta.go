package account

import (
	"context"
	"log/slog"
	"net/http"
	"strconv"
	"strings"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/httpx/toast"
	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// GET /dashboard: shortcuts, first-purchase offer and the last 5 orders.
func (h *handlers) dashboard(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	u, _ := current(r)
	orders, err := store.LoadRecentOrders(ctx, h.d.Pool, u.ID)
	if err != nil {
		h.fail(w, r, "recent orders", err)
		return
	}
	offer, err := store.FirstPurchaseOffer(ctx, h.d.Pool, u.ID, u.Email)
	if err != nil {
		h.fail(w, r, "first purchase offer", err)
		return
	}
	seo.SetHead(ctx, seo.New("Painel").WithNoIndex())
	render.Page(w, r, h.d, dashboardPage(orders, offer)) //nolint:contextcheck // templ components get ctx from Render
}

func pageParam(r *http.Request) int64 {
	return store.ClampPage(store.ParsePage(r.URL.Query().Get("page")))
}

// GET /conta/pedidos?page=N: the order history.
func (h *handlers) myOrders(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	u, _ := current(r)
	page, err := store.LoadMyOrders(ctx, h.d.Pool, u.ID, pageParam(r))
	if err != nil {
		h.fail(w, r, "my orders", err)
		return
	}
	seo.SetHead(ctx, seo.New("Meus Pedidos").WithNoIndex())
	render.Page(w, r, h.d, myOrdersPage(page)) //nolint:contextcheck // templ components get ctx from Render
}

// GET /conta/pedidos/lista?page=N: the list fragment (htmx pager), the
// full page otherwise. Always scoped to the session user.
func (h *handlers) myOrdersList(w http.ResponseWriter, r *http.Request) {
	if !httpx.IsHTMX(r) {
		h.myOrders(w, r)
		return
	}
	u, _ := current(r)
	page, err := store.LoadMyOrders(r.Context(), h.d.Pool, u.ID, pageParam(r))
	if err != nil {
		h.fail(w, r, "my orders", err)
		return
	}
	render.Fragment(w, r, http.StatusOK, myOrdersList(page)) //nolint:contextcheck // templ components get ctx from Render
}

func ordersPageURL(path string, page int64) string {
	return path + "?page=" + strconv.FormatInt(page, 10)
}

// --- /conta/enderecos ---

const addressWriteFailed = "Não foi possível salvar o endereço. Tente novamente."

func addressForm(r *http.Request) store.AddressInput {
	f := r.PostForm
	return store.AddressInput{
		Label: f.Get("label"), RecipientName: f.Get("recipient_name"), ZipCode: f.Get("zip_code"),
		Street: f.Get("street"), Number: f.Get("number"), Complement: f.Get("complement"),
		Neighborhood: f.Get("neighborhood"), City: f.Get("city"), State: f.Get("state"),
		Phone: f.Get("phone"), IsDefault: f.Has("is_default"),
	}
}

func parseID(raw string) (uuid.UUID, bool) {
	id, err := uuid.Parse(strings.TrimSpace(raw))
	return id, err == nil
}

func addressMessage(r *http.Request, err error) string {
	if msg := store.AddressErrorMessage(err); msg != "" {
		return msg
	}
	slog.ErrorContext(r.Context(), "address write failed", "error", err)
	return addressWriteFailed
}

// addressesView is the /conta/enderecos template input.
type addressesView struct {
	Saved   []store.SavedAddress
	Editing *store.SavedAddress
}

func (v addressesView) atLimit() bool { return len(v.Saved) >= store.MaxAddressesPerUser }

func (v addressesView) showForm() bool { return v.Editing != nil || !v.atLimit() }

// GET /conta/enderecos?editar=<id>.
func (h *handlers) addresses(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	u, _ := current(r)
	saved, err := store.ListAddresses(ctx, h.d.Pool, u.ID)
	if err != nil {
		h.fail(w, r, "list addresses", err)
		return
	}
	v := addressesView{Saved: saved}
	// ?editar= only ever resolves among the user's own rows.
	if id, ok := parseID(r.URL.Query().Get("editar")); ok {
		for i := range saved {
			if saved[i].ID == id {
				v.Editing = &saved[i]
			}
		}
	}
	seo.SetHead(ctx, seo.New("Meus Endereços").WithNoIndex())
	render.Page(w, r, h.d, addressesPage(v)) //nolint:contextcheck // templ components get ctx from Render
}

// POST /conta/enderecos/salvar: create (no id) or update (own id).
func (h *handlers) saveAddress(w http.ResponseWriter, r *http.Request) {
	if !parseForm(w, r) {
		return
	}
	u, _ := current(r)
	in := addressForm(r)
	editing, isEdit := parseID(r.PostForm.Get("id"))
	var err error
	if isEdit {
		err = store.UpdateAddress(r.Context(), h.d.Pool, u.ID, editing, in)
	} else {
		_, err = store.CreateAddress(r.Context(), h.d.Pool, u.ID, in)
	}
	if err == nil {
		toast.Set(w, r, toast.Success("Endereço salvo."))
		toast.Redirect(w, r, "/conta/enderecos")
		return
	}
	toast.Set(w, r, toast.Error(addressMessage(r, err)))
	back := "/conta/enderecos"
	if isEdit && store.IsUserError(err) {
		back += "?editar=" + editing.String()
	}
	toast.Redirect(w, r, back)
}

// POST /conta/enderecos/remover.
func (h *handlers) removeAddress(w http.ResponseWriter, r *http.Request) {
	h.addressAction(w, r, "Endereço removido.", store.DeleteAddress)
}

// POST /conta/enderecos/padrao.
func (h *handlers) defaultAddress(w http.ResponseWriter, r *http.Request) {
	h.addressAction(w, r, "Endereço padrão atualizado.", store.SetDefaultAddress)
}

func (h *handlers) addressAction(w http.ResponseWriter, r *http.Request, okMsg string,
	act func(ctx context.Context, pool *pgxpool.Pool, userID, addressID uuid.UUID) error,
) {
	if !parseForm(w, r) {
		return
	}
	u, _ := current(r)
	err := store.ErrAddressNotFound
	if id, ok := parseID(r.PostForm.Get("id")); ok {
		err = act(r.Context(), h.d.Pool, u.ID, id)
	}
	if err == nil {
		toast.Set(w, r, toast.Success(okMsg))
	} else {
		toast.Set(w, r, toast.Error(addressMessage(r, err)))
	}
	toast.Redirect(w, r, "/conta/enderecos")
}
