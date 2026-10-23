package store

import (
	"context"
	"errors"
	"fmt"
	"math"
	"strings"
	"time"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// Coupons (src/app/store/coupons.go): validation, discount math,
// per-identity limits, first-purchase code, redemptions.

// Coupon mirrors the coupons table.
type Coupon struct {
	ID               uuid.UUID  `db:"id"`
	Code             string     `db:"code"`
	DiscountType     string     `db:"discount_type"`
	DiscountValue    int64      `db:"discount_value"`
	MinSubtotalCents int64      `db:"min_subtotal_cents"`
	MaxDiscountCents *int64     `db:"max_discount_cents"`
	Active           bool       `db:"active"`
	StartsAt         *time.Time `db:"starts_at"`
	ExpiresAt        *time.Time `db:"expires_at"`
	UsageType        string     `db:"usage_type"`
	MaxUses          *int32     `db:"max_uses"`
	PerUserLimit     int32      `db:"per_user_limit"`
}

// IsUnique reports a single-use coupon.
func (c *Coupon) IsUnique() bool { return c.UsageType == "unique" }

// EffectiveMaxUses: unique coupons are single-use even with max_uses NULL;
// unlimited coupons use max_uses (nil = no cap).
func (c *Coupon) EffectiveMaxUses() *int32 {
	if c.IsUnique() {
		return ptr(int32(1))
	}
	return c.MaxUses
}

// CalcCouponDiscount computes the discount for subtotal: 0 below the
// minimum; fixed = min(value, subtotal); percent = floor(subtotal*value/100)
// capped by max_discount and the subtotal; unknown types give 0.
func CalcCouponDiscount(c *Coupon, subtotalCents int64) int64 {
	if subtotalCents <= 0 || subtotalCents < c.MinSubtotalCents {
		return 0
	}
	switch c.DiscountType {
	case "fixed":
		return max(min(c.DiscountValue, subtotalCents), 0)
	case "percent":
		var d int64
		if c.DiscountValue != 0 && subtotalCents > math.MaxInt64/c.DiscountValue {
			d = math.MaxInt64 / 100
		} else {
			d = subtotalCents * c.DiscountValue / 100
		}
		if c.MaxDiscountCents != nil {
			d = min(d, *c.MaxDiscountCents)
		}
		return max(min(d, subtotalCents), 0)
	}
	return 0
}

// NormalizeEmail folds an email for per-person limits: trim + lowercase,
// drop a +tag, and fold Gmail aliases (dots, googlemail.com) onto one
// @gmail.com address. ok=false for blank input.
func NormalizeEmail(raw string) (string, bool) {
	lower := strings.ToLower(strings.TrimSpace(raw))
	if lower == "" {
		return "", false
	}
	at := strings.LastIndex(lower, "@")
	if at < 0 {
		return lower, true
	}
	local, domain := lower[:at], lower[at+1:]
	if base, _, ok := strings.Cut(local, "+"); ok {
		local = base
	}
	if local == "" || domain == "" {
		return lower, true
	}
	if domain == "gmail.com" || domain == "googlemail.com" {
		return strings.ReplaceAll(local, ".", "") + "@gmail.com", true
	}
	return local + "@" + domain, true
}

func normalizeEmailPtr(raw *string) *string {
	if raw == nil {
		return nil
	}
	if n, ok := NormalizeEmail(*raw); ok {
		return &n
	}
	return nil
}

// ResolvedCoupon is a coupon usable by this shopper.
type ResolvedCoupon struct {
	ID            uuid.UUID
	Code          string
	DiscountCents int64
	// EmailNorm is the identity the per-user limit was checked against.
	EmailNorm *string
}

// Coupon messages.
const (
	InvalidCouponMessage      = "Cupom inválido."
	CouponGoneMessage         = InvalidCouponMessage
	FirstPurchaseOnlyMessage  = "Este cupom é válido apenas para a primeira compra."
	couponLimitReachedMessage = "Este cupom atingiu o limite de utilizações."
	couponPerUserMessage      = "Você já utilizou este cupom o máximo de vezes permitido."
	couponIdentityMessage     = "Entre para usar este cupom, ou informe seu e-mail no checkout."
)

// ErrCouponGone: the code does not exist or was deactivated. Checkout drops
// it from the cart instead of blocking the order.
var ErrCouponGone = errors.New(CouponGoneMessage)

// couponByCode looks a coupon up case-insensitively (nil when missing),
// locking the row when forUpdate.
func couponByCode(ctx context.Context, q *sqlc.Queries, code string, forUpdate bool) (*Coupon, error) {
	var (
		row   sqlc.StoreCouponByCodeRow
		found bool
		err   error
	)
	if forUpdate {
		var locked sqlc.StoreCouponByCodeForUpdateRow
		locked, found, err = noRows(q.StoreCouponByCodeForUpdate(ctx, code))
		row = sqlc.StoreCouponByCodeRow(locked)
	} else {
		row, found, err = noRows(q.StoreCouponByCode(ctx, code))
	}
	if err != nil || !found {
		return nil, err
	}
	return ptr(Coupon(row)), nil
}

// ResolveCoupon validates a code against the DB and computes its discount.
//
// Checks, in order: lookup (case-insensitive), active + start/expiry
// window, assignment restriction (only assigned signed-in users) — all with
// the generic InvalidCouponMessage so codes can't be enumerated; then the
// minimum subtotal, the global cap (unique counts as 1) and the
// per-identity cap (user id or normalized email; active redemptions only).
// A per-user coupon without identity is rejected. The configured
// first-purchase code is refused once the identity has a paid order.
// Unknown or inactive codes return ErrCouponGone; other refusals a
// *UserError. A blank code returns (nil, nil). forUpdate locks the coupon
// row (inside the order transaction).
func ResolveCoupon(ctx context.Context, tx DBTX, rawCode string, subtotalCents int64, userID *uuid.UUID, guestEmail *string, forUpdate bool) (*ResolvedCoupon, error) {
	code := strings.TrimSpace(rawCode)
	if code == "" {
		return nil, nil
	}
	q := queries(tx)
	c, err := couponByCode(ctx, q, code, forUpdate)
	if err != nil {
		return nil, err
	}
	if c == nil || !c.Active {
		return nil, ErrCouponGone
	}
	now := time.Now()
	if (c.StartsAt != nil && c.StartsAt.After(now)) || (c.ExpiresAt != nil && !c.ExpiresAt.After(now)) {
		return nil, userErr(InvalidCouponMessage)
	}
	assignments, err := q.StoreCouponAssignmentCount(ctx, c.ID)
	if err != nil {
		return nil, err
	}
	if assignments > 0 {
		if userID == nil {
			return nil, userErr(InvalidCouponMessage)
		}
		member, err := q.StoreCouponIsAssigned(ctx, sqlc.StoreCouponIsAssignedParams{CouponID: c.ID, UserID: *userID})
		if err != nil {
			return nil, err
		}
		if !member {
			return nil, userErr(InvalidCouponMessage)
		}
	}
	if subtotalCents < c.MinSubtotalCents {
		return nil, userErr(fmt.Sprintf("Este cupom exige uma compra mínima de %s.", FormatPrice(c.MinSubtotalCents)))
	}
	if limit := c.EffectiveMaxUses(); limit != nil {
		uses, err := q.StoreCouponGlobalUses(ctx, c.ID)
		if err != nil {
			return nil, err
		}
		if uses >= int64(*limit) {
			return nil, userErr(couponLimitReachedMessage)
		}
	}
	var accountEmail *string
	if userID != nil {
		e, found, err := noRows(q.StoreUserEmail(ctx, *userID))
		if err != nil {
			return nil, err
		} else if found {
			accountEmail = &e
		}
	}
	emailNorm := normalizeEmailPtr(accountEmail)
	if emailNorm == nil {
		emailNorm = normalizeEmailPtr(guestEmail)
	}
	if userID == nil && emailNorm == nil {
		return nil, userErr(couponIdentityMessage)
	}
	first, err := FirstPurchaseCode(ctx, tx)
	if err != nil {
		return nil, err
	}
	if first != "" && strings.EqualFold(first, c.Code) {
		bought, err := HasPaidOrder(ctx, tx, userID, accountEmail, guestEmail)
		if err != nil {
			return nil, err
		}
		if bought {
			return nil, userErr(FirstPurchaseOnlyMessage)
		}
	}
	personal, err := q.StoreCouponPersonalUses(ctx, sqlc.StoreCouponPersonalUsesParams{CouponID: c.ID, UserID: userID, EmailNorm: emailNorm})
	if err != nil {
		return nil, err
	}
	if personal >= int64(c.PerUserLimit) {
		return nil, userErr(couponPerUserMessage)
	}
	return &ResolvedCoupon{
		ID:            c.ID,
		Code:          c.Code,
		DiscountCents: CalcCouponDiscount(c, subtotalCents),
		EmailNorm:     emailNorm,
	}, nil
}

// FirstPurchaseCode is store_settings.first_purchase_coupon_code trimmed,
// "" when unset.
func FirstPurchaseCode(ctx context.Context, q DBTX) (string, error) {
	code, found, err := noRows(queries(q).StoreFirstPurchaseCode(ctx))
	if err != nil || !found {
		return "", err
	}
	return strings.TrimSpace(deref(code)), nil
}

// FirstPurchaseOffer is the first-purchase code to advertise to a signed-in
// customer: "" when none is configured or the account already bought.
func FirstPurchaseOffer(ctx context.Context, q DBTX, userID uuid.UUID, accountEmail string) (string, error) {
	code, err := FirstPurchaseCode(ctx, q)
	if err != nil || code == "" {
		return "", err
	}
	bought, err := HasPaidOrder(ctx, q, &userID, &accountEmail, nil)
	if err != nil || bought {
		return "", err
	}
	return code, nil
}

// HasPaidOrder: the identity has an order with paid_at set (even if later
// refunded/cancelled), matched by user id or by order/account email.
func HasPaidOrder(ctx context.Context, q DBTX, userID *uuid.UUID, accountEmail, guestEmail *string) (bool, error) {
	emails := []string{}
	for _, e := range []*string{accountEmail, guestEmail} {
		if e == nil {
			continue
		}
		if v := strings.ToLower(strings.TrimSpace(*e)); v != "" {
			emails = append(emails, v)
		}
	}
	return queries(q).StoreHasPaidOrder(ctx, sqlc.StoreHasPaidOrderParams{UserID: userID, Emails: emails})
}

// RecordRedemption records that orderID consumed coupon. guestEmail is the
// raw checkout email, kept for guests only.
func RecordRedemption(ctx context.Context, tx DBTX, coupon *ResolvedCoupon, userID *uuid.UUID, orderID uuid.UUID, guestEmail *string) error {
	return queries(tx).StoreRecordRedemption(ctx, sqlc.StoreRecordRedemptionParams{
		ID: uuid.Must(uuid.NewV7()), CouponID: coupon.ID, UserID: userID, OrderID: orderID,
		GuestEmail: guestEmail, EmailNorm: coupon.EmailNorm,
	})
}

// RestoreRedemptionForOrder frees the coupon use of a cancelled order.
func RestoreRedemptionForOrder(ctx context.Context, tx DBTX, orderID uuid.UUID) error {
	return queries(tx).StoreRestoreRedemption(ctx, orderID)
}

// ReclaimRedemptionForOrder consumes the coupon again after a reopen.
func ReclaimRedemptionForOrder(ctx context.Context, tx DBTX, orderID uuid.UUID) error {
	return queries(tx).StoreReclaimRedemption(ctx, orderID)
}
