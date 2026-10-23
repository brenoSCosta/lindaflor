package checkout

import (
	"log/slog"
	"net/http"
	"net/url"
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/auth"
	"github.com/wellington-vell/lindaflor/internal/captcha"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
	"github.com/wellington-vell/lindaflor/internal/seo"
	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// checkoutForm reads the posted checkout form.
func checkoutForm(r *http.Request) store.CheckoutInput {
	f := r.PostForm
	return store.CheckoutInput{
		GuestEmail:   f.Get("guest_email"),
		Name:         f.Get("name"),
		Phone:        f.Get("phone"),
		Street:       f.Get("street"),
		Number:       f.Get("number"),
		Complement:   f.Get("complement"),
		Neighborhood: f.Get("neighborhood"),
		City:         f.Get("city"),
		State:        f.Get("state"),
		ZipCode:      f.Get("zip_code"),
		Notes:        f.Get("notes"),
		CouponCode:   f.Get("coupon_code"),
		Intent:       f.Get("intent"),
		SaveAddress:  f.Has("save_address"),
		Website:      f.Get(store.HoneypotField),
	}
}

func sessionUserID(r *http.Request) *uuid.UUID {
	if u, ok := auth.CurrentUser(r.Context()); ok {
		id := u.ID
		return &id
	}
	return nil
}

// orderURL is /pedido/{id}, with ?t= when a guest token is known.
func orderURL(id uuid.UUID, token string) string {
	return withToken("/pedido/"+id.String(), token)
}

// withToken appends ?t=token when token is set.
func withToken(path, token string) string {
	if token == "" {
		return path
	}
	return path + "?" + url.Values{"t": {token}}.Encode()
}

// GET/POST /checkout (intent=apply previews the coupon, intent=pay places
// the order).
func (h *handlers) checkoutPage(w http.ResponseWriter, r *http.Request) {
	ctx := r.Context()
	var posted *store.CheckoutInput
	if r.Method == http.MethodPost {
		if err := r.ParseForm(); err != nil {
			if httpx.IsBodyTooLarge(err) {
				httpx.TooLarge(w)
				return
			}
			http.Error(w, "Formulário inválido.", http.StatusBadRequest)
			return
		}
		in := checkoutForm(r)
		posted = &in
	}
	userID := sessionUserID(r)
	seo.SetHead(ctx, seo.New("Checkout").WithNoIndex())

	// Read-only lookup: no cart row for cookieless visitors.
	cart, err := store.FindCartHTTP(w, r, h.d.Pool, h.secure(), userID)
	if err != nil {
		h.fail(w, r, "find cart", err)
		return
	}
	deps := h.checkoutDeps()
	sum, err := store.LoadCheckoutSummary(ctx, h.d.Pool, store.CheckoutPageRequest{
		Cart: cart, Posted: posted, SessionUserID: userID,
		PickedAddress: r.URL.Query().Get("endereco"), Deps: deps,
	})
	if err != nil {
		if msg, ok := store.UserMessage(err); ok {
			http.Error(w, msg, http.StatusConflict)
			return
		}
		h.fail(w, r, "load checkout", err)
		return
	}
	if sum == nil {
		render.Page(w, r, h.d, emptyCartPage())
		return
	}
	store.ResolveCartImages(ctx, h.d.Storage, sum.Items)

	var checkoutError string
	if posted != nil && sum.CouponError == "" && posted.Intent == "pay" {
		if store.HoneypotTripped(*posted) {
			// Silent drop: back to the checkout as if nothing happened.
			slog.InfoContext(ctx, "checkout honeypot tripped; order dropped")
			httpx.Redirect(w, r, "/checkout")
			return
		}
		switch _, verr := store.ValidatePayFields(*posted); {
		case verr != nil:
			msg, ok := store.UserMessage(verr)
			if !ok {
				h.fail(w, r, "validate checkout", verr)
				return
			}
			checkoutError = msg
		case !h.d.Captcha.CheckForm(r):
			checkoutError = captcha.Message
		case !ratelimit.CheckWindow(ratelimit.ScopeCheckoutOrder, httpx.ClientIP(r),
			ratelimit.DefaultCheckoutOrdersPerWindow, ratelimit.CheckoutOrderWindow):
			checkoutError = store.CheckoutRateLimitedMessage
		default:
			created, cerr := store.CreateOrder(ctx, h.d.Pool, deps, cart.ID, *posted, userID)
			if cerr == nil {
				if userID != nil && posted.SaveAddress {
					store.SaveCheckoutAddress(ctx, h.d.Pool, *userID, *posted)
				}
				httpx.Redirect(w, r, orderURL(created.ID, created.AccessToken.String()))
				return
			}
			checkoutError = store.CheckoutErrorMessage(cerr)
			if checkoutError == "" {
				h.fail(w, r, "create order", cerr)
				return
			}
		}
	}

	data := newCheckoutView(sum, posted, checkoutError)
	if u, ok := auth.CurrentUser(ctx); ok && posted == nil {
		data.Email = u.Email // signed-in buyers start with their account e-mail
	}
	data.SiteKey = h.d.Captcha.SiteKey()
	status := http.StatusOK
	if checkoutError != "" {
		status = http.StatusUnprocessableEntity
	}
	render.Status(w, r, h.d, status, checkoutPage(data))
}

func (h *handlers) fail(w http.ResponseWriter, r *http.Request, what string, err error) {
	slog.ErrorContext(r.Context(), what, "error", err)
	http.Error(w, "Erro interno.", http.StatusInternalServerError)
}

// checkoutView is everything the checkout template renders.
type checkoutView struct {
	Sum           *store.CheckoutSummary
	Error         string
	Email         string
	Name          string
	Phone         string
	Street        string
	Number        string
	Complement    string
	Neighborhood  string
	City          string
	State         string
	Zip           string
	Notes         string
	SaveChecked   bool
	CouponApplied string // code of the applied coupon ("" when none)
	SiteKey       string // Turnstile on the pay button ("" = off)
}

func newCheckoutView(sum *store.CheckoutSummary, posted *store.CheckoutInput, checkoutError string) checkoutView {
	v := checkoutView{Sum: sum, Error: checkoutError, State: "SE"}
	fill := posted
	if fill == nil {
		fill = sum.Prefill
	}
	if fill != nil {
		v.Name, v.Phone, v.Street, v.Number = fill.Name, fill.Phone, fill.Street, fill.Number
		v.Complement, v.Neighborhood, v.City, v.Zip = fill.Complement, fill.Neighborhood, fill.City, fill.ZipCode
		v.State = fill.State
	}
	if posted != nil {
		v.Email, v.Notes, v.SaveChecked = posted.GuestEmail, posted.Notes, posted.SaveAddress
	}
	if code := strings.TrimSpace(sum.CouponCode); code != "" && sum.CouponError == "" {
		v.CouponApplied = code
	}
	return v
}
