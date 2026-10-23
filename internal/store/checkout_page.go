package store

import (
	"context"
	"errors"
	"log/slog"
	"strings"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgxpool"
)

// The data side of GET/POST /checkout (src/app/checkout.rs): the cart, the
// coupon preview, shipping and totals for the summary, saved-address
// prefill and the "pay" intent.

// CheckoutRateLimitedMessage is shown when the per-IP order budget
// (ratelimit window scope checkout:order) is exhausted.
const CheckoutRateLimitedMessage = "Muitos pedidos em pouco tempo. Aguarde alguns minutos e tente novamente."

// CheckoutSummary is the order summary panel.
type CheckoutSummary struct {
	Items             []CartItem
	SubtotalCents     int64
	ShippingCents     int64
	FreeShipping      bool
	CouponCode        string // the code shown in the coupon input
	CouponDiscount    int64
	CouponError       string // "" when the coupon is fine / absent
	PixDiscountCents  int64
	TotalCents        int64
	PixAvailable      bool
	Settings          StoreSettings
	PreviewUF         string
	SavedAddresses    []SavedAddress
	SelectedAddressID *uuid.UUID
	Prefill           *CheckoutInput // saved-address prefill (GET only)
	CanSaveAddress    bool
}

// CheckoutPageRequest is everything the handler knows.
type CheckoutPageRequest struct {
	Cart *CartRef
	// Posted is the submitted form (nil on GET).
	Posted        *CheckoutInput
	SessionUserID *uuid.UUID
	// PickedAddress is ?endereco=.
	PickedAddress string
	Deps          CheckoutDeps
}

// LoadCheckoutSummary hydrates the cart and computes the summary. It
// returns (nil, nil) for an empty/missing cart (the "carrinho vazio"
// page). For a POST it also remembers the typed email for abandoned-cart
// reminders (unless the honeypot tripped).
func LoadCheckoutSummary(ctx context.Context, pool *pgxpool.Pool, req CheckoutPageRequest) (*CheckoutSummary, error) {
	if req.Cart == nil {
		return nil, nil
	}
	h, err := HydrateCart(ctx, pool, req.Cart.ID)
	if err != nil {
		return nil, err
	}
	if len(h.Items) == 0 {
		return nil, nil
	}
	sub, ok := CartSubtotalCents(h.Items)
	if !ok {
		return nil, userErr("Carrinho inválido. Atualize a página.")
	}
	s := &CheckoutSummary{Items: h.Items, SubtotalCents: sub, CanSaveAddress: req.SessionUserID != nil}
	stored, err := CartCouponCode(ctx, pool, req.Cart.ID)
	if err != nil {
		return nil, err
	}
	s.CouponCode = stored
	var previewEmail *string
	if p := req.Posted; p != nil {
		if !isBlank(p.CouponCode) {
			s.CouponCode = p.CouponCode
		}
		if !isBlank(p.GuestEmail) {
			previewEmail = &p.GuestEmail
		}
		if !HoneypotTripped(*p) {
			if email, ok := ValidateEmail(p.GuestEmail); ok {
				if err := RememberCheckoutEmail(ctx, pool, req.Cart.ID, email); err != nil {
					slog.WarnContext(ctx, "could not remember checkout email", "error", err)
				}
			}
		}
	}
	tx, err := pool.Begin(ctx)
	if err != nil {
		return nil, err
	}
	resolved, cerr := ResolveCoupon(ctx, tx, s.CouponCode, sub, req.SessionUserID, previewEmail, false)
	_ = tx.Rollback(ctx)
	switch {
	case cerr == nil:
	case errors.Is(cerr, ErrCouponGone):
		s.CouponError = CouponGoneMessage
	default:
		msg, ok := UserMessage(cerr)
		if !ok {
			return nil, cerr
		}
		s.CouponError = msg
	}
	if resolved != nil {
		s.CouponDiscount = resolved.DiscountCents
	}
	if req.SessionUserID != nil {
		if s.SavedAddresses, err = ListAddresses(ctx, pool, *req.SessionUserID); err != nil {
			return nil, err
		}
	}
	if req.Posted == nil {
		if a := pickSavedAddress(s.SavedAddresses, req.PickedAddress); a != nil {
			in := a.AsCheckoutInput()
			s.Prefill, s.SelectedAddressID = &in, &a.ID
		}
	}
	fill := req.Posted
	if fill == nil {
		fill = s.Prefill
	}
	zip, state := "", "SE"
	if fill != nil {
		zip, state = CEPDigits(fill.ZipCode), fill.State
	}
	if s.Settings, err = GetStoreSettings(ctx, pool); err != nil {
		return nil, err
	}
	s.PixAvailable = PaymentAvailable(req.Deps, s.Settings)
	uf, ok := UFFromCEP(zip)
	switch {
	case ok:
		s.PreviewUF = uf
	case IsBrazilianState(state):
		s.PreviewUF = state
	default:
		s.PreviewUF = "SE"
	}
	s.ShippingCents, s.FreeShipping = CalculateShipping(sub, s.PreviewUF, s.Settings.FreeShippingThresholdCents)
	after := sub - s.CouponDiscount
	s.PixDiscountCents = PixDiscountCents(after, s.Settings.PixDiscountPercent)
	s.TotalCents = after - s.PixDiscountCents + s.ShippingCents
	return s, nil
}

func pickSavedAddress(saved []SavedAddress, picked string) *SavedAddress {
	if id, err := uuid.Parse(strings.TrimSpace(picked)); err == nil {
		for i := range saved {
			if saved[i].ID == id {
				return &saved[i]
			}
		}
	}
	for i := range saved {
		if saved[i].IsDefault {
			return &saved[i]
		}
	}
	return nil
}

// SaveCheckoutAddress is "Salvar este endereço" after a successful order
// (failures are only logged; the order never depends on it).
func SaveCheckoutAddress(ctx context.Context, pool *pgxpool.Pool, userID uuid.UUID, in CheckoutInput) {
	_, err := SaveAddressFromCheckout(ctx, pool, userID, AddressInput{
		RecipientName: in.Name, ZipCode: in.ZipCode, Street: in.Street, Number: in.Number,
		Complement: in.Complement, Neighborhood: in.Neighborhood, City: in.City, State: in.State, Phone: in.Phone,
	})
	if err != nil {
		slog.WarnContext(ctx, "saving checkout address failed", "user_id", userID, "error", err)
	}
}

// CheckoutErrorMessage maps CreateOrder errors to the inline checkout
// error ("" = internal error).
func CheckoutErrorMessage(err error) string {
	if msg, ok := UserMessage(err); ok {
		return msg
	}
	if errors.Is(err, ErrCouponRemoved) {
		return err.Error()
	}
	return ""
}
