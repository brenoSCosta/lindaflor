package store

import "math"

// PixPrice is the price after the PIX discount (discount rounded down).
func PixPrice(cents int64, discountPercent int) int64 {
	return cents - PixDiscountCents(cents, discountPercent)
}

// PixDiscountCents is the PIX discount, rounded down so the customer is
// never overcharged relative to the advertised percentage. Non-positive
// amounts get no discount.
func PixDiscountCents(cents int64, discountPercent int) int64 {
	if cents <= 0 {
		return 0
	}
	pct := int64(min(max(discountPercent, 0), 100))
	if cents > math.MaxInt64/100 {
		return cents / 100 * pct
	}
	return cents * pct / 100
}

// DiscountPercent is round((compareAt - price) / compareAt * 100), or
// ok=false without a compare-at price above price.
func DiscountPercent(compareAt *int64, price int64) (int, bool) {
	if compareAt == nil || *compareAt <= price {
		return 0, false
	}
	ca := *compareAt
	return int(math.Round(float64(ca-price) / float64(ca) * 100)), true
}
