package store

import (
	"context"
	"errors"
	"fmt"
	"math"
	"strings"
	"time"

	"github.com/google/uuid"
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

var (
	sqlCouponByCode          = sqlText(`SELECT id, code, discount_type, discount_value, min_subtotal_cents, max_discount_cents, active, starts_at, expires_at, usage_type, max_uses, per_user_limit FROM coupons WHERE UPPER(code) = UPPER($1)`)
	sqlCouponByCodeForUpdate = sqlText(`SELECT id, code, discount_type, discount_value, min_subtotal_cents, max_discount_cents, active, starts_at, expires_at, usage_type, max_uses, per_user_limit FROM coupons WHERE UPPER(code) = UPPER($1) FOR UPDATE`)
	sqlCouponAssignmentCount = sqlText(`SELECT COUNT(*) FROM coupon_assignments WHERE coupon_id = $1`)
	sqlCouponIsAssigned      = sqlText(`SELECT EXISTS(SELECT 1 FROM coupon_assignments WHERE coupon_id = $1 AND user_id = $2)`)
	sqlCouponGlobalUses      = sqlText(`SELECT COUNT(*) FROM coupon_redemptions WHERE coupon_id = $1 AND refunded_at IS NULL`)
	sqlUserEmail             = sqlText(`SELECT email FROM users WHERE id = $1`)
	sqlCouponPersonalUses    = sqlText(`SELECT COUNT(*) FROM coupon_redemptions
 WHERE coupon_id = $1
   AND refunded_at IS NULL
   AND (user_id = $2::uuid OR email_norm = $3::text)`)
	sqlFirstPurchaseCode = sqlText(`SELECT first_purchase_coupon_code FROM store_settings WHERE id`)
	sqlHasPaidOrder      = sqlText(`SELECT EXISTS(
   SELECT 1 FROM orders o
   LEFT JOIN users u ON u.id = o.user_id
   WHERE o.paid_at IS NOT NULL
     AND (o.user_id = $1::uuid
          OR lower(btrim(o.guest_email)) = ANY($2::text[])
          OR lower(u.email) = ANY($2::text[]))
 )`)
	sqlRecordRedemption = sqlText(`INSERT INTO coupon_redemptions
   (id, coupon_id, user_id, order_id, guest_email, email_norm)
 VALUES ($1, $2, $3, $4, $5, $6)`)
	sqlRestoreRedemption = sqlText(`UPDATE coupon_redemptions SET refunded_at = now()
 WHERE order_id = $1 AND refunded_at IS NULL`)
	sqlReclaimRedemption = sqlText(`UPDATE coupon_redemptions SET refunded_at = NULL
 WHERE order_id = $1 AND refunded_at IS NOT NULL`)
)

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
	q := sqlCouponByCode
	if forUpdate {
		q = sqlCouponByCodeForUpdate
	}
	c, err := collectOne[Coupon](tx.Query(ctx, q, code))
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
	var assignments int64
	if err := tx.QueryRow(ctx, sqlCouponAssignmentCount, c.ID).Scan(&assignments); err != nil {
		return nil, err
	}
	if assignments > 0 {
		if userID == nil {
			return nil, userErr(InvalidCouponMessage)
		}
		var member bool
		if err := tx.QueryRow(ctx, sqlCouponIsAssigned, c.ID, *userID).Scan(&member); err != nil {
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
		var uses int64
		if err := tx.QueryRow(ctx, sqlCouponGlobalUses, c.ID).Scan(&uses); err != nil {
			return nil, err
		}
		if uses >= int64(*limit) {
			return nil, userErr(couponLimitReachedMessage)
		}
	}
	var accountEmail *string
	if userID != nil {
		var e string
		err := tx.QueryRow(ctx, sqlUserEmail, *userID).Scan(&e)
		if found, err := optional(err); err != nil {
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
	var personal int64
	if err := tx.QueryRow(ctx, sqlCouponPersonalUses, c.ID, userID, emailNorm).Scan(&personal); err != nil {
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
	var code *string
	err := q.QueryRow(ctx, sqlFirstPurchaseCode).Scan(&code)
	if found, err := optional(err); err != nil || !found {
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
	var exists bool
	err := q.QueryRow(ctx, sqlHasPaidOrder, userID, emails).Scan(&exists)
	return exists, err
}

// RecordRedemption records that orderID consumed coupon. guestEmail is the
// raw checkout email, kept for guests only.
func RecordRedemption(ctx context.Context, tx DBTX, coupon *ResolvedCoupon, userID *uuid.UUID, orderID uuid.UUID, guestEmail *string) error {
	_, err := tx.Exec(ctx, sqlRecordRedemption, uuid.Must(uuid.NewV7()), coupon.ID, userID, orderID, guestEmail, coupon.EmailNorm)
	return err
}

// RestoreRedemptionForOrder frees the coupon use of a cancelled order.
func RestoreRedemptionForOrder(ctx context.Context, tx DBTX, orderID uuid.UUID) error {
	_, err := tx.Exec(ctx, sqlRestoreRedemption, orderID)
	return err
}

// ReclaimRedemptionForOrder consumes the coupon again after a reopen.
func ReclaimRedemptionForOrder(ctx context.Context, tx DBTX, orderID uuid.UUID) error {
	_, err := tx.Exec(ctx, sqlReclaimRedemption, orderID)
	return err
}
