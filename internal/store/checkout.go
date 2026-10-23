package store

import (
	"context"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"strings"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/payments"
	"github.com/wellington-vell/lindaflor/internal/payments/pix"
)

// Checkout order service (src/app/store/checkout_service.rs): form
// validation and order creation (stock reservation, coupon redemption,
// PIX payload) behind the /checkout page.

// CheckoutInput is the posted checkout form.
type CheckoutInput struct {
	GuestEmail   string
	Name         string
	Phone        string
	Street       string
	Number       string
	Complement   string
	Neighborhood string
	City         string
	State        string
	ZipCode      string
	Notes        string
	CouponCode   string
	Intent       string
	// SaveAddress is the "Salvar este endereço" checkbox (signed-in only).
	SaveAddress bool
	// Website is the honeypot field (HoneypotField).
	Website string
}

// HoneypotField is the off-screen input humans never fill.
const HoneypotField = "website"

// HoneypotTripped: a filled honeypot means a form-filling bot; the page
// drops the order silently.
func HoneypotTripped(in CheckoutInput) bool { return !isBlank(in.Website) }

// MaxPendingOrders is how many unpaid orders one buyer (normalized email,
// cart or account) may hold at a time.
const MaxPendingOrders = 3

// ErrCouponRemoved: the stored/typed coupon vanished; checkout cleared it
// from the cart so the shopper can review the new total.
var ErrCouponRemoved = errors.New("Cupom removido, revise o total.")

// PaymentUnavailableMessage is shown while the PSP circuit is open.
const PaymentUnavailableMessage = payments.UnavailableMessage

const (
	maxEmailLen        = 254
	maxNameLen         = 100
	maxPhoneLen        = 20
	maxStreetLen       = 120
	maxNumberLen       = 20
	maxComplementLen   = 80
	maxNeighborhoodLen = 80
	maxCityLen         = 80
	maxNotesLen        = 500
)

// Field caps mirrored as maxlength on the inputs.
var CheckoutFieldMax = map[string]int{
	"guest_email": maxEmailLen, "name": maxNameLen, "phone": maxPhoneLen, "street": maxStreetLen,
	"number": maxNumberLen, "complement": maxComplementLen, "neighborhood": maxNeighborhoodLen,
	"city": maxCityLen, "notes": maxNotesLen,
}

func checkFieldLengths(in CheckoutInput) error {
	for _, f := range []struct {
		value string
		max   int
		what  string
	}{
		{in.GuestEmail, maxEmailLen, "O e-mail"},
		{in.Name, maxNameLen, "O nome"},
		{in.Phone, maxPhoneLen, "O WhatsApp"},
		{in.Street, maxStreetLen, "A rua"},
		{in.Number, maxNumberLen, "O número"},
		{in.Complement, maxComplementLen, "O complemento"},
		{in.Neighborhood, maxNeighborhoodLen, "O bairro"},
		{in.City, maxCityLen, "A cidade"},
		{in.Notes, maxNotesLen, "As observações"},
	} {
		if runeLen(strings.TrimSpace(f.value)) > f.max {
			verb := "deve"
			if strings.HasPrefix(f.what, "As ") {
				verb = "devem"
			}
			return userErr(fmt.Sprintf("%s %s ter no máximo %d caracteres.", f.what, verb, f.max))
		}
	}
	return nil
}

// PhoneDigitCount counts ASCII digits.
func PhoneDigitCount(v string) int { return len(digitsOnly(v)) }

// ValidatePayFields validates the checkout form and returns the shipping
// UF derived from the CEP (*UserError on failure).
func ValidatePayFields(in CheckoutInput) (string, error) {
	if err := checkFieldLengths(in); err != nil {
		return "", err
	}
	if _, ok := ValidateEmail(in.GuestEmail); !ok {
		return "", userErr("Informe um e-mail válido.")
	}
	if _, ok := ValidateName(in.Name); !ok {
		return "", userErr("Informe o nome completo.")
	}
	if d := PhoneDigitCount(in.Phone); d < 10 || d > 11 {
		return "", userErr("Informe um WhatsApp válido (DDD + número).")
	}
	switch {
	case isBlank(in.Street):
		return "", userErr("Informe a rua.")
	case isBlank(in.Number):
		return "", userErr("Informe o número.")
	case isBlank(in.Neighborhood):
		return "", userErr("Informe o bairro.")
	case isBlank(in.City):
		return "", userErr("Informe a cidade.")
	}
	return ResolveShippingUF(in.ZipCode, in.State)
}

// CheckoutDeps wires checkout to the payment side.
type CheckoutDeps struct {
	// Payments is the configured provider (nil = static PIX).
	Payments payments.Provider
	// Service creates the Mercado Pago charge after commit
	// (payments.Service.EnsureCharge); nil skips it (the order page
	// retries lazily).
	Service *payments.Service
}

// NewCheckoutDeps builds CheckoutDeps from the app wiring
// (d.Pool, d.Payments, d.Orders).
func NewCheckoutDeps(pool *pgxpool.Pool, provider payments.Provider, orders payments.OrderTransitioner) CheckoutDeps {
	return CheckoutDeps{Payments: provider, Service: &payments.Service{Pool: pool, Provider: provider, Orders: orders}}
}

// StaticPixCheckout is the CheckoutDeps of the static PIX provider.
var StaticPixCheckout = CheckoutDeps{Payments: payments.StaticPix{}}

func (d CheckoutDeps) provider() string {
	if d.Payments != nil && payments.IsMercadoPago(d.Payments) {
		return ProviderMercadoPago
	}
	return ProviderStaticPix
}

// PaymentAvailable: MP with its breaker closed, or a configured PIX key.
func PaymentAvailable(deps CheckoutDeps, s StoreSettings) bool {
	if deps.provider() == ProviderMercadoPago {
		return deps.Payments.AcceptingCharges()
	}
	return s.PixConfigured()
}

// CreatedOrder is a new order and its guest link token.
type CreatedOrder struct {
	ID          uuid.UUID
	AccessToken uuid.UUID
}

// CreateOrder validates the form and creates a pending PIX order from the
// cart in one transaction: pending-order cap (advisory locks), quote (with
// expired-hold reclaim), shipping by CEP UF, coupon (locked), PIX
// discount, order + items, stock hold, redemption, BR Code, cart cleared,
// "pedido recebido" mail. Mercado Pago charges are created after commit.
// Errors: *UserError (show it), ErrCouponRemoved (coupon cleared from the
// cart; show its message), anything else is internal.
func CreateOrder(ctx context.Context, pool *pgxpool.Pool, deps CheckoutDeps, cartID uuid.UUID, in CheckoutInput, userID *uuid.UUID) (CreatedOrder, error) {
	uf, err := ValidatePayFields(in)
	if err != nil {
		return CreatedOrder{}, err
	}
	settings, err := GetStoreSettings(ctx, pool)
	if err != nil {
		return CreatedOrder{}, err
	}
	provider := deps.provider()
	switch {
	case provider == ProviderStaticPix && !settings.PixConfigured():
		return CreatedOrder{}, userErr(pix.UnavailableMessage)
	case provider == ProviderMercadoPago && !deps.Payments.AcceptingCharges():
		// Circuit open: fail fast before holding stock for an unpayable order.
		return CreatedOrder{}, userErr(PaymentUnavailableMessage)
	}
	created := CreatedOrder{ID: uuid.Must(uuid.NewV7()), AccessToken: uuid.New()}
	err = withTx(ctx, pool, func(tx pgx.Tx) error {
		return createOrderInTx(ctx, tx, settings, createParams{
			order: created, cartID: cartID, in: in, userID: userID,
			zipDigits: CEPDigits(in.ZipCode), uf: uf, provider: provider,
		})
	})
	if err != nil {
		if errors.Is(err, ErrCouponRemoved) {
			if cerr := SetCartCoupon(ctx, pool, cartID, ""); cerr != nil {
				return CreatedOrder{}, cerr
			}
		}
		return CreatedOrder{}, err
	}
	// Charge after commit (no PSP call while holding row locks).
	if provider == ProviderMercadoPago && deps.Service != nil {
		if _, err := deps.Service.EnsureCharge(ctx, created.ID); err != nil {
			slog.WarnContext(ctx, "charge creation deferred", "order_id", created.ID, "error", err)
		}
	}
	return created, nil
}

type createParams struct {
	order     CreatedOrder
	cartID    uuid.UUID
	in        CheckoutInput
	userID    *uuid.UUID
	zipDigits string
	uf        string
	provider  string
}

var (
	sqlInsertOrder = sqlText(`INSERT INTO orders (id, user_id, guest_email, status, subtotal_cents, shipping_cents, discount_cents, pix_discount_cents, total_cents, shipping_address, notes, coupon_id, access_token, email_norm, cart_id)
VALUES ($1, $2, $3, 'pending_payment', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)`)
	sqlInsertOrderItem = sqlText(`INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents, set_id, set_name)
VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)`)
	sqlSetPaymentMeta  = sqlText(`UPDATE orders SET payment_meta = $1, payment_provider = $3 WHERE id = $2`)
	sqlAdvisoryLock    = sqlText(`SELECT pg_advisory_xact_lock(hashtextextended($1, 0))`)
	sqlPendingForBuyer = sqlText(`SELECT id FROM orders
WHERE status = 'pending_payment'
  AND (reservation_expires_at IS NULL OR reservation_expires_at > now())
  AND (email_norm = $1::text OR cart_id = $2
       OR ($3::uuid IS NOT NULL AND user_id = $3::uuid))
ORDER BY created_at DESC, id DESC`)
)

var errInvalidCart = userErr("Carrinho inválido. Atualize a página.")

func createOrderInTx(ctx context.Context, tx pgx.Tx, settings StoreSettings, p createParams) error {
	var emailNorm *string
	if n, ok := NormalizeEmail(p.in.GuestEmail); ok {
		emailNorm = &n
	}
	if err := enforcePendingLimit(ctx, tx, emailNorm, p.cartID, p.userID); err != nil {
		return err
	}
	items, err := QuoteCartForCheckout(ctx, tx, p.cartID)
	switch {
	case errors.Is(err, ErrCartEmpty):
		return userErr("Seu carrinho está vazio.")
	case errors.Is(err, ErrInsufficientStock):
		return userErr("Estoque insuficiente para concluir o pedido.")
	case err != nil:
		return err
	}
	subtotal, ok := CartSubtotalCents(items)
	if !ok {
		return errInvalidCart
	}
	shipping, _ := CalculateShipping(subtotal, p.uf, settings.FreeShippingThresholdCents)

	couponCode := p.in.CouponCode
	if isBlank(couponCode) {
		var stored *string
		if err := tx.QueryRow(ctx, sqlCartCouponCode, p.cartID).Scan(&stored); err != nil {
			return err
		}
		couponCode = deref(stored)
	}
	guestEmail := p.in.GuestEmail
	resolved, err := ResolveCoupon(ctx, tx, couponCode, subtotal, p.userID, &guestEmail, true)
	switch {
	case errors.Is(err, ErrCouponGone):
		return ErrCouponRemoved
	case err != nil:
		return err
	}
	var discount int64
	var couponID *uuid.UUID
	var appliedCode *string
	if resolved != nil {
		discount, couponID, appliedCode = resolved.DiscountCents, &resolved.ID, &resolved.Code
	}
	afterCoupon := subtotal - discount
	pixDiscount := PixDiscountCents(afterCoupon, settings.PixDiscountPercent)
	total := afterCoupon - pixDiscount + shipping

	address, err := json.Marshal(map[string]string{
		"name": p.in.Name, "street": p.in.Street, "number": p.in.Number,
		"complement": p.in.Complement, "neighborhood": p.in.Neighborhood,
		"city": p.in.City, "state": p.uf, "zip_code": p.zipDigits, "phone": p.in.Phone,
	})
	if err != nil {
		return err
	}
	notes := nonEmpty(strings.TrimSpace(p.in.Notes))
	if _, err := tx.Exec(ctx, sqlInsertOrder, p.order.ID, p.userID, p.in.GuestEmail, subtotal, shipping,
		discount, pixDiscount, total, address, notes, couponID, p.order.AccessToken, emailNorm, p.cartID); err != nil {
		return err
	}
	lines := make([]ReserveLine, 0, len(items))
	for _, it := range items {
		if _, err := tx.Exec(ctx, sqlInsertOrderItem, uuid.Must(uuid.NewV7()), p.order.ID, it.VariantID,
			it.ProductName, it.VariantLabel, it.Quantity, it.UnitPriceCents, it.SetID, it.SetName); err != nil {
			return err
		}
		lines = append(lines, ReserveLine{VariantID: it.VariantID, Quantity: it.Quantity})
	}
	if err := ReserveForOrder(ctx, tx, p.order.ID, lines); err != nil {
		if errors.Is(err, ErrInsufficientStock) {
			return userErr("Estoque insuficiente para concluir o pedido.")
		}
		return err
	}
	if resolved != nil {
		var guest *string
		if p.userID == nil {
			guest = &guestEmail
		}
		if err := RecordRedemption(ctx, tx, resolved, p.userID, p.order.ID, guest); err != nil {
			return err
		}
	}
	// Mercado Pago orders get their PIX code from the PSP after commit.
	var pixCode *string
	if p.provider == ProviderStaticPix {
		code, err := pix.Payload(deref(settings.PixKey), orDefault(settings.PixKeyType, "phone"),
			orDefault(settings.PixMerchantName, "Linda Flor"), orDefault(settings.PixMerchantCity, "Aracaju"),
			total, p.order.ID.String())
		if err != nil {
			return userErr(err.Error())
		}
		pixCode = &code
	}
	meta, err := json.Marshal(map[string]any{
		"pix_copy_paste": pixCode,
		"total_cents":    total,
		"coupon_code":    appliedCode,
		"coupon_id":      couponID,
		"discount_cents": discount,
	})
	if err != nil {
		return err
	}
	if _, err := tx.Exec(ctx, sqlSetPaymentMeta, meta, p.order.ID, p.provider); err != nil {
		return err
	}
	if err := ClearItems(ctx, tx, p.cartID); err != nil {
		return err
	}
	return EnqueueOrderCreated(ctx, tx, p.order.ID)
}

func orDefault(p *string, def string) string {
	if p == nil {
		return def
	}
	return *p
}

// enforcePendingLimit refuses a new order while the buyer already holds
// MaxPendingOrders unpaid, unexpired ones (by normalized email, cart or
// account). Transaction-scoped advisory locks on the email then the cart
// serialize concurrent checkouts of the same buyer.
func enforcePendingLimit(ctx context.Context, tx DBTX, emailNorm *string, cartID uuid.UUID, userID *uuid.UUID) error {
	if emailNorm != nil {
		if _, err := tx.Exec(ctx, sqlAdvisoryLock, "checkout:email:"+*emailNorm); err != nil {
			return err
		}
	}
	if _, err := tx.Exec(ctx, sqlAdvisoryLock, "checkout:cart:"+cartID.String()); err != nil {
		return err
	}
	pending, err := scalars[uuid.UUID](tx.Query(ctx, sqlPendingForBuyer, emailNorm, cartID, userID))
	if err != nil {
		return err
	}
	if len(pending) >= MaxPendingOrders {
		return userErr(fmt.Sprintf("Você já tem %d pedidos aguardando pagamento (o mais recente é o #%s). Pague pelo link que enviamos para o seu e-mail ou aguarde a reserva expirar antes de fazer um novo pedido.",
			MaxPendingOrders, OrderCode(pending[0])))
	}
	return nil
}
