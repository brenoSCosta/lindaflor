package store

import (
	"context"
	"errors"
	"fmt"
	"math"
	"slices"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/ratelimit"
)

// Cart (src/app/store/cart.rs): guest/user carts keyed by a bearer token
// cookie, loose lines and set (conjunto) member lines, stock clamping,
// login merge and the checkout quote.

// CartItem is one hydrated cart line.
type CartItem struct {
	VariantID      uuid.UUID
	ProductID      uuid.UUID
	ProductSlug    string
	ProductName    string
	VariantLabel   string
	ImageURL       *string // storage key or URL (resolve before rendering)
	UnitPriceCents int64
	Quantity       int32
	MaxQuantity    int32
	// SetID is the owning set for a fixed-composition bundle line.
	SetID   *uuid.UUID
	SetName *string
}

// LineTotalCents is unit × quantity.
func (i CartItem) LineTotalCents() int64 { return i.UnitPriceCents * int64(i.Quantity) }

// CartNotice tells the shopper what hydration changed.
type CartNotice struct {
	ProductName string
	// Quantity > 0: quantity adjusted to stock; 0: line removed.
	Quantity int32
	Removed  bool
}

// Message is the pt-BR notice.
func (n CartNotice) Message() string {
	if n.Removed {
		return n.ProductName + " foi removido do carrinho (indisponível)."
	}
	return fmt.Sprintf("%s: quantidade ajustada para %d (estoque).", n.ProductName, n.Quantity)
}

// HydratedCart is a cart with live prices and stock applied.
type HydratedCart struct {
	CartID  uuid.UUID
	Token   uuid.UUID
	Items   []CartItem
	Notices []CartNotice
}

// CartRef identifies a cart row.
type CartRef struct {
	ID    uuid.UUID
	Token uuid.UUID
}

// Cart errors.
var (
	ErrInvalidVariant = errors.New("Variação inválida.")
	ErrOutOfStock     = errors.New("Esta peça está esgotada.")
	ErrCartEmpty      = errors.New("Seu carrinho está vazio.")
)

type cartRow = sqlc.StoreCartByTokenRow

func fetchCartByToken(ctx context.Context, q DBTX, token uuid.UUID) (*cartRow, error) {
	c, found, err := noRows(queries(q).StoreCartByToken(ctx, token))
	if err != nil || !found {
		return nil, err
	}
	return &c, nil
}

func fetchCartByUser(ctx context.Context, q DBTX, userID uuid.UUID) (*cartRow, error) {
	c, found, err := noRows(queries(q).StoreCartByUser(ctx, userID))
	if err != nil || !found {
		return nil, err
	}
	return ptr(cartRow(c)), nil
}

func insertCart(ctx context.Context, q DBTX, userID *uuid.UUID) (*cartRow, error) {
	id := uuid.Must(uuid.NewV7())
	// The token is a bearer secret (cookie): random v4, not time-ordered.
	token := uuid.New()
	if err := queries(q).StoreInsertCart(ctx, sqlc.StoreInsertCartParams{ID: id, Token: token, UserID: userID}); err != nil {
		return nil, err
	}
	return &cartRow{ID: id, Token: token, UserID: userID}, nil
}

func belongsToOther(c *cartRow, userID *uuid.UUID) bool {
	return c.UserID != nil && (userID == nil || *userID != *c.UserID)
}

// FindCart is the read-only resolver for render paths (GET /checkout, the
// cart sheet): the cookie cart (unless it belongs to another account), else
// the signed-in user's cart; never inserts. writeCookie reports the cookie
// must be (re)set to ref.Token.
func FindCart(ctx context.Context, q DBTX, cookieToken, userID *uuid.UUID) (ref *CartRef, writeCookie bool, err error) {
	if cookieToken != nil {
		c, err := fetchCartByToken(ctx, q, *cookieToken)
		if err != nil {
			return nil, false, err
		}
		if c != nil && !belongsToOther(c, userID) {
			return &CartRef{ID: c.ID, Token: c.Token}, false, nil
		}
	}
	if userID == nil {
		return nil, false, nil
	}
	c, err := fetchCartByUser(ctx, q, *userID)
	if err != nil || c == nil {
		return nil, false, err
	}
	return &CartRef{ID: c.ID, Token: c.Token}, true, nil
}

// LoadOrCreateCart resolves the cookie cart, creating a guest/user cart if
// needed. writeCookie reports the cookie must be set to ref.Token.
func LoadOrCreateCart(ctx context.Context, q DBTX, cookieToken, userID *uuid.UUID) (ref CartRef, writeCookie bool, err error) {
	if cookieToken != nil {
		c, err := fetchCartByToken(ctx, q, *cookieToken)
		if err != nil {
			return CartRef{}, false, err
		}
		if c != nil && !belongsToOther(c, userID) {
			return CartRef{ID: c.ID, Token: c.Token}, false, nil
		}
	}
	var c *cartRow
	if userID != nil {
		if c, err = fetchCartByUser(ctx, q, *userID); err != nil {
			return CartRef{}, false, err
		}
	}
	if c == nil {
		if c, err = insertCart(ctx, q, userID); err != nil {
			return CartRef{}, false, err
		}
	}
	return CartRef{ID: c.ID, Token: c.Token}, true, nil
}

// variantAvailability returns (active, live available, found).
func variantAvailability(ctx context.Context, q DBTX, variantID uuid.UUID) (bool, int32, bool, error) {
	row, found, err := noRows(queries(q).StoreVariantAvailability(ctx, variantID))
	return row.Active, row.Available, found, err
}

// upsertSetLine adds quantity of a set member line, capped at maxQuantity.
func upsertSetLine(ctx context.Context, q DBTX, cartID, setID, variantID uuid.UUID, quantity, maxQuantity int32) error {
	return queries(q).StoreUpsertSetLine(ctx, sqlc.StoreUpsertSetLineParams{
		ID: uuid.Must(uuid.NewV7()), CartID: cartID, VariantID: variantID,
		Quantity: quantity, SetID: &setID, MaxQuantity: maxQuantity,
	})
}

// AddItem adds quantity (clamped to 1..available) of a variant as a loose
// line (never touching set-member rows).
func AddItem(ctx context.Context, q DBTX, cartID, variantID uuid.UUID, quantity int32) error {
	active, available, found, err := variantAvailability(ctx, q, variantID)
	if err != nil {
		return err
	}
	if !found || !active {
		return ErrInvalidVariant
	}
	if available <= 0 {
		return ErrOutOfStock
	}
	add := min(max(quantity, 1), available)
	if err := queries(q).StoreUpsertLooseLine(ctx, sqlc.StoreUpsertLooseLineParams{
		ID: uuid.Must(uuid.NewV7()), CartID: cartID, VariantID: variantID, Quantity: add, MaxQuantity: available,
	}); err != nil {
		return err
	}
	return queries(q).StoreTouchCart(ctx, cartID)
}

// AddSetToCart adds a whole fixed-composition set: one row per slot
// variant, all stamped with set_id and sharing one quantity clamped to
// max(1, min(qty, min available)). Missing/inactive/priceless/empty sets
// and inactive members are ErrInvalidVariant; a sold-out member is
// ErrOutOfStock.
func AddSetToCart(ctx context.Context, q DBTX, cartID, setID uuid.UUID, quantity int32) error {
	set, found, err := noRows(queries(q).StoreSetForCart(ctx, setID))
	if err != nil {
		return err
	} else if !found || !set.Active || set.PriceInCents == nil {
		return ErrInvalidVariant
	}
	variants, err := queries(q).StoreSetVariants(ctx, setID)
	if err != nil {
		return err
	}
	if len(variants) == 0 {
		return ErrInvalidVariant
	}
	minAvailable := int32(math.MaxInt32)
	for _, v := range variants {
		vActive, available, found, err := variantAvailability(ctx, q, v)
		if err != nil {
			return err
		}
		if !found || !vActive {
			return ErrInvalidVariant
		}
		minAvailable = min(minAvailable, available)
	}
	if minAvailable <= 0 {
		return ErrOutOfStock
	}
	add := min(max(quantity, 1), minAvailable)
	for _, v := range variants {
		if err := upsertSetLine(ctx, q, cartID, setID, v, add, minAvailable); err != nil {
			return err
		}
	}
	return queries(q).StoreTouchCart(ctx, cartID)
}

// setGroupQuantity sets every member line of one set group to quantity.
func setGroupQuantity(ctx context.Context, q DBTX, cartID, setID uuid.UUID, quantity int32) error {
	return queries(q).StoreSetGroupQuantity(ctx, sqlc.StoreSetGroupQuantityParams{CartID: cartID, SetID: setID, Quantity: quantity})
}

// SetQuantity sets a loose line's quantity (≤0 removes; clamped to stock;
// unavailable variants are removed and reported).
func SetQuantity(ctx context.Context, q DBTX, cartID, variantID uuid.UUID, quantity int32) error {
	if quantity <= 0 {
		return RemoveItem(ctx, q, cartID, variantID)
	}
	active, available, found, err := variantAvailability(ctx, q, variantID)
	if err != nil {
		return err
	}
	if !found {
		if err := RemoveItem(ctx, q, cartID, variantID); err != nil {
			return err
		}
		return ErrInvalidVariant
	}
	if !active || available <= 0 {
		if err := RemoveItem(ctx, q, cartID, variantID); err != nil {
			return err
		}
		return ErrOutOfStock
	}
	qty := min(quantity, available)
	n, err := queries(q).StoreSetLooseQuantity(ctx, sqlc.StoreSetLooseQuantityParams{CartID: cartID, VariantID: variantID, Quantity: qty})
	if err != nil {
		return err
	}
	if n == 0 {
		return AddItem(ctx, q, cartID, variantID, qty)
	}
	return nil
}

// SetSetQuantity sets a whole set group's quantity (every member moves
// together, clamped by the smallest member availability).
func SetSetQuantity(ctx context.Context, q DBTX, cartID, setID uuid.UUID, quantity int32) error {
	if quantity <= 0 {
		return RemoveSet(ctx, q, cartID, setID)
	}
	members, err := queries(q).StoreSetMemberVariants(ctx, sqlc.StoreSetMemberVariantsParams{CartID: cartID, SetID: setID})
	if err != nil || len(members) == 0 {
		return err
	}
	minAvailable := int32(math.MaxInt32)
	for _, v := range members {
		active, available, found, err := variantAvailability(ctx, q, v)
		if err != nil {
			return err
		}
		if !found {
			if err := RemoveSet(ctx, q, cartID, setID); err != nil {
				return err
			}
			return ErrInvalidVariant
		}
		if !active || available <= 0 {
			if err := RemoveSet(ctx, q, cartID, setID); err != nil {
				return err
			}
			return ErrOutOfStock
		}
		minAvailable = min(minAvailable, available)
	}
	return setGroupQuantity(ctx, q, cartID, setID, min(max(quantity, 1), minAvailable))
}

// RemoveItem removes a loose line (set-member rows survive).
func RemoveItem(ctx context.Context, q DBTX, cartID, variantID uuid.UUID) error {
	return queries(q).StoreRemoveLoose(ctx, sqlc.StoreRemoveLooseParams{CartID: cartID, VariantID: variantID})
}

// RemoveSet removes every member line of one set group.
func RemoveSet(ctx context.Context, q DBTX, cartID, setID uuid.UUID) error {
	return queries(q).StoreRemoveSet(ctx, sqlc.StoreRemoveSetParams{CartID: cartID, SetID: setID})
}

// ClearItems empties a cart.
func ClearItems(ctx context.Context, q DBTX, cartID uuid.UUID) error {
	return queries(q).StoreClearItems(ctx, cartID)
}

// CartItemCount is the header badge: sum of stored quantities for the
// cookie token (0 without one; never creates a cart).
func CartItemCount(ctx context.Context, q DBTX, token *uuid.UUID) (int, error) {
	if token == nil {
		return 0, nil
	}
	n, err := queries(q).StoreCartCountForToken(ctx, *token)
	if err != nil {
		return 0, err
	}
	return int(max(n, 0)), nil
}

// CartSubtotalCents sums unit × quantity; ok=false on overflow.
func CartSubtotalCents(items []CartItem) (int64, bool) {
	var total int64
	for _, it := range items {
		q := int64(it.Quantity)
		if q != 0 && (it.UnitPriceCents > math.MaxInt64/q || it.UnitPriceCents < math.MinInt64/q) {
			return 0, false
		}
		line := it.UnitPriceCents * q
		if (line > 0 && total > math.MaxInt64-line) || (line < 0 && total < math.MinInt64-line) {
			return 0, false
		}
		total += line
	}
	return total, true
}

// hydrateRow is one cart line joined with live product/stock data
// (StoreHydrateCart; StoreQuoteCart returns the same columns).
type hydrateRow = sqlc.StoreHydrateCartRow

func setMemberLabel(base string, setName *string) string {
	if setName != nil {
		return base + " — parte do conjunto " + *setName
	}
	return base + " — parte do conjunto"
}

func displayItem(r hydrateRow) (CartItem, bool) {
	if r.ProductID == nil {
		return CartItem{}, false
	}
	name := "Peça"
	if r.ProductName != nil {
		name = *r.ProductName
	}
	label := VariantLabel(deref(r.Size), deref(r.Color))
	if r.SetID != nil {
		label = setMemberLabel(label, r.SetName)
	}
	price := deref(r.ProductPrice)
	if r.VariantPrice != nil {
		price = *r.VariantPrice
	}
	return CartItem{
		VariantID:      r.VariantID,
		ProductID:      *r.ProductID,
		ProductSlug:    deref(r.ProductSlug),
		ProductName:    name,
		VariantLabel:   label,
		ImageURL:       r.ImageUrl,
		UnitPriceCents: price,
		Quantity:       r.Quantity,
		MaxQuantity:    r.Available,
		SetID:          r.SetID,
		SetName:        r.SetName,
	}, true
}

// HydrateCart loads the cart with live prices and stock in its own
// transaction, deleting unavailable lines and clamping quantities.
func HydrateCart(ctx context.Context, pool *pgxpool.Pool, cartID uuid.UUID) (*HydratedCart, error) {
	var out *HydratedCart
	err := withTx(ctx, pool, func(tx pgx.Tx) error {
		var err error
		out, err = HydrateCartTx(ctx, tx, cartID)
		return err
	})
	return out, err
}

// HydrateCartTx is HydrateCart inside the caller's transaction.
func HydrateCartTx(ctx context.Context, tx DBTX, cartID uuid.UUID) (*HydratedCart, error) {
	q := queries(tx)
	token, err := q.StoreCartToken(ctx, cartID)
	if err != nil {
		return nil, err
	}
	rows, err := q.StoreHydrateCart(ctx, cartID)
	if err != nil {
		return nil, err
	}
	h := &HydratedCart{CartID: cartID, Token: token, Items: []CartItem{}}
	// Set groups that lost a member: the rest of the group is dropped after
	// the loop (a partial set can never be quoted).
	var broken []uuid.UUID
	for _, r := range rows {
		name := "Peça"
		if r.ProductName != nil {
			name = *r.ProductName
		}
		available := r.Available
		if r.ProductID == nil || !deref(r.ProductActive) || available <= 0 {
			if err := q.StoreDeleteCartItem(ctx, r.CartItemID); err != nil {
				return nil, err
			}
			if r.SetID != nil {
				broken = append(broken, *r.SetID)
			}
			h.Notices = append(h.Notices, CartNotice{ProductName: name, Removed: true})
			continue
		}
		if r.Quantity > available {
			r.Quantity = available
			if err := q.StoreUpdateCartItemQty(ctx, sqlc.StoreUpdateCartItemQtyParams{ID: r.CartItemID, Quantity: available}); err != nil {
				return nil, err
			}
			h.Notices = append(h.Notices, CartNotice{ProductName: name, Quantity: available})
		}
		item, _ := displayItem(r)
		h.Items = append(h.Items, item)
	}
	if err := normalizeSetGroups(ctx, tx, h, broken); err != nil {
		return nil, err
	}
	return h, nil
}

// normalizeSetGroups keeps every set group whole and uniform after per-row
// clamping: a group that lost a member is removed, and a group clamped to
// different quantities is lowered to its smallest member quantity.
func normalizeSetGroups(ctx context.Context, tx DBTX, h *HydratedCart, broken []uuid.UUID) error {
	for _, setID := range sortedUnique(broken) {
		if err := RemoveSet(ctx, tx, h.CartID, setID); err != nil {
			return err
		}
		var setName *string
		kept := h.Items[:0]
		for _, it := range h.Items {
			if it.SetID != nil && *it.SetID == setID {
				if setName == nil {
					setName = it.SetName
				}
				continue
			}
			kept = append(kept, it)
		}
		h.Items = kept
		if setName != nil {
			h.Notices = append(h.Notices, CartNotice{ProductName: *setName, Removed: true})
		}
	}
	groupMin := map[uuid.UUID]int32{}
	var order []uuid.UUID
	for _, it := range h.Items {
		if it.SetID == nil {
			continue
		}
		if m, ok := groupMin[*it.SetID]; !ok {
			groupMin[*it.SetID] = it.Quantity
			order = append(order, *it.SetID)
		} else {
			groupMin[*it.SetID] = min(m, it.Quantity)
		}
	}
	// BTreeMap order in Rust: by set id string.
	slices.SortFunc(order, func(a, b uuid.UUID) int { return compareUUID(a, b) })
	for _, setID := range order {
		m := groupMin[setID]
		uneven := false
		for _, it := range h.Items {
			if it.SetID != nil && *it.SetID == setID && it.Quantity != m {
				uneven = true
			}
		}
		if !uneven {
			continue
		}
		if err := setGroupQuantity(ctx, tx, h.CartID, setID, m); err != nil {
			return err
		}
		var setName *string
		for i := range h.Items {
			if h.Items[i].SetID != nil && *h.Items[i].SetID == setID {
				h.Items[i].Quantity = m
				if setName == nil {
					setName = h.Items[i].SetName
				}
			}
		}
		name := "Conjunto"
		if setName != nil {
			name = *setName
		}
		h.Notices = append(h.Notices, CartNotice{ProductName: name, Quantity: m})
	}
	return nil
}

// QuoteCartForCheckout locks the cart's inventory (reclaiming expired
// holds) and lines, then prices them; never deletes or clamps. Fails with
// ErrCartEmpty or ErrInsufficientStock (unavailable, oversold across
// split lines, partial/uneven/priceless/inactive sets). Set member lines
// get the set price split per unit (the first member by variant id takes
// the remainder).
func QuoteCartForCheckout(ctx context.Context, tx DBTX, cartID uuid.UUID) ([]CartItem, error) {
	if err := LockCartInventory(ctx, tx, cartID); err != nil {
		return nil, err
	}
	q := queries(tx)
	quoted, err := q.StoreQuoteCart(ctx, cartID)
	if err != nil {
		return nil, err
	}
	rows := mapSlice(quoted, func(r sqlc.StoreQuoteCartRow) hydrateRow { return hydrateRow(r) })
	if len(rows) == 0 {
		return nil, ErrCartEmpty
	}
	totals := map[uuid.UUID]int32{}
	for _, r := range rows {
		totals[r.VariantID] += r.Quantity
	}
	items := make([]CartItem, 0, len(rows))
	groups := map[uuid.UUID][]int{}
	var groupOrder []uuid.UUID
	for _, r := range rows {
		if r.ProductID == nil || !deref(r.ProductActive) || r.Available < totals[r.VariantID] {
			return nil, ErrInsufficientStock
		}
		item, _ := displayItem(r)
		if r.SetID != nil {
			if _, ok := groups[*r.SetID]; !ok {
				groupOrder = append(groupOrder, *r.SetID)
			}
			groups[*r.SetID] = append(groups[*r.SetID], len(items))
		}
		items = append(items, item)
	}
	slices.SortFunc(groupOrder, func(a, b uuid.UUID) int { return compareUUID(a, b) })
	for _, setID := range groupOrder {
		idx := groups[setID]
		first := rows[idx[0]].Quantity
		for _, i := range idx {
			if rows[i].Quantity != first {
				return nil, ErrInsufficientStock
			}
		}
		set, found, err := noRows(q.StoreQuoteSet(ctx, setID))
		if err != nil {
			return nil, err
		} else if !found || set.PriceInCents == nil || !set.Active {
			return nil, ErrInsufficientStock
		}
		name, price := set.Name, set.PriceInCents
		expected, err := q.StoreQuoteSetVariants(ctx, setID)
		if err != nil {
			return nil, err
		}
		if len(expected) == 0 {
			return nil, ErrInsufficientStock
		}
		slices.SortFunc(expected, compareUUID)
		got := make([]uuid.UUID, 0, len(idx))
		for _, i := range idx {
			got = append(got, rows[i].VariantID)
		}
		slices.SortFunc(got, compareUUID)
		if !slices.Equal(got, expected) {
			return nil, ErrInsufficientStock
		}
		n := int64(len(idx))
		base := *price / n
		firstShare := *price - base*(n-1)
		ordered := slices.Clone(idx)
		slices.SortStableFunc(ordered, func(a, b int) int { return compareUUID(rows[a].VariantID, rows[b].VariantID) })
		for pos, i := range ordered {
			if pos == 0 {
				items[i].UnitPriceCents = firstShare
			} else {
				items[i].UnitPriceCents = base
			}
			items[i].SetName = ptr(name)
		}
	}
	return items, nil
}

// CartCouponCode is the coupon stored on the cart ("" when none).
func CartCouponCode(ctx context.Context, q DBTX, cartID uuid.UUID) (string, error) {
	code, found, err := noRows(queries(q).StoreCartCouponCode(ctx, cartID))
	if err != nil || !found {
		return "", err
	}
	if code == nil || isBlank(*code) {
		return "", nil
	}
	return *code, nil
}

// SetCartCoupon stores (or clears with "") the cart's coupon code.
func SetCartCoupon(ctx context.Context, q DBTX, cartID uuid.UUID, code string) error {
	return queries(q).StoreSetCartCoupon(ctx, sqlc.StoreSetCartCouponParams{ID: cartID, CouponCode: nonEmpty(code)})
}

// CouponRateLimitedMessage is returned when the per-cart coupon budget is
// exhausted.
const CouponRateLimitedMessage = "Muitas tentativas. Aguarde um momento e tente novamente."

// CheckCouponCartRate consumes one cell of the per-cart coupon budget
// (ratelimit scope coupon:cart keyed by cart id), on top of the per-IP
// coupon:apply layer: slows code guessing spread across IPs reusing a cart.
func CheckCouponCartRate(cartID uuid.UUID) bool {
	return ratelimit.Check(ratelimit.ScopeCouponCart, cartID.String(), ratelimit.DefaultCouponCartPerMin)
}

// ApplyCartCoupon is the cart-sheet coupon form (/carrinho/cupom): an
// empty code clears the coupon; otherwise the per-cart budget is consumed
// and the code validated against the hydrated subtotal. Returns "" when
// applied/cleared, else the inline message to show.
func ApplyCartCoupon(ctx context.Context, pool *pgxpool.Pool, cartID uuid.UUID, userID *uuid.UUID, rawCode string) (string, error) {
	code := truncateRunes(trimSpace(rawCode), 64)
	if code == "" {
		return "", SetCartCoupon(ctx, pool, cartID, "")
	}
	if !CheckCouponCartRate(cartID) {
		return CouponRateLimitedMessage, nil
	}
	h, err := HydrateCart(ctx, pool, cartID)
	if err != nil {
		return "", err
	}
	subtotal, ok := CartSubtotalCents(h.Items)
	if !ok {
		return "", userErr("Carrinho inválido. Atualize a página.")
	}
	var resolved *ResolvedCoupon
	var rerr error
	// Validation only: the transaction is rolled back.
	tx, err := pool.Begin(ctx)
	if err != nil {
		return "", err
	}
	resolved, rerr = ResolveCoupon(ctx, tx, code, subtotal, userID, nil, false)
	_ = tx.Rollback(ctx)
	switch {
	case rerr == nil && resolved != nil:
		return "", SetCartCoupon(ctx, pool, cartID, resolved.Code)
	case rerr == nil:
		return "", SetCartCoupon(ctx, pool, cartID, "")
	case errors.Is(rerr, ErrCouponGone):
		return CouponGoneMessage, SetCartCoupon(ctx, pool, cartID, "")
	}
	if msg, ok := UserMessage(rerr); ok {
		return msg, SetCartCoupon(ctx, pool, cartID, "")
	}
	return "", rerr
}

// mergeSetMemberLine merges one guest set-member line into the same set's
// line of the user's cart, capped at live stock.
func mergeSetMemberLine(ctx context.Context, q DBTX, cartID, setID, variantID uuid.UUID, quantity int32) error {
	active, available, found, err := variantAvailability(ctx, q, variantID)
	if err != nil {
		return err
	}
	if !found || !active {
		return ErrInvalidVariant
	}
	if available <= 0 {
		return ErrOutOfStock
	}
	add := min(max(quantity, 1), available)
	if err := upsertSetLine(ctx, q, cartID, setID, variantID, add, available); err != nil {
		return err
	}
	return queries(q).StoreTouchCart(ctx, cartID)
}

// MergeGuestCart merges the guest cart of the cookie token (if any) into
// the signed-in user's cart and returns the surviving token, which the
// caller writes back to the cart cookie. Called by auth on login.
func MergeGuestCart(ctx context.Context, q DBTX, guestToken *uuid.UUID, userID uuid.UUID) (uuid.UUID, error) {
	var guestID *uuid.UUID
	if guestToken != nil {
		c, err := fetchCartByToken(ctx, q, *guestToken)
		if err != nil {
			return uuid.Nil, err
		}
		if c != nil {
			guestID = &c.ID
		}
	}
	return MergeCartsForUser(ctx, q, guestID, userID)
}

// MergeCartsForUser sums guest lines into the user's cart (capped to live
// stock) and deletes the guest cart; without a guest cart it ensures a
// user cart exists. Returns the surviving token.
func MergeCartsForUser(ctx context.Context, q DBTX, guestCartID *uuid.UUID, userID uuid.UUID) (uuid.UUID, error) {
	qs := queries(q)
	userCart, err := fetchCartByUser(ctx, q, userID)
	if err != nil {
		return uuid.Nil, err
	}
	switch {
	case guestCartID == nil && userCart != nil:
		return userCart.Token, nil
	case guestCartID == nil:
		c, err := insertCart(ctx, q, &userID)
		if err != nil {
			return uuid.Nil, err
		}
		return c.Token, nil
	case userCart == nil:
		if err := qs.StoreAttachCart(ctx, sqlc.StoreAttachCartParams{ID: *guestCartID, UserID: userID}); err != nil {
			return uuid.Nil, err
		}
		token, err := qs.StoreCartToken(ctx, *guestCartID)
		if err != nil {
			return uuid.Nil, err
		}
		if _, err := HydrateCartTx(ctx, q, *guestCartID); err != nil {
			return uuid.Nil, err
		}
		return token, nil
	}
	if *guestCartID == userCart.ID {
		return userCart.Token, nil
	}
	owner, err := qs.StoreCartOwner(ctx, *guestCartID)
	if err != nil {
		return uuid.Nil, err
	}
	if owner != nil && *owner != userID {
		return userCart.Token, nil
	}
	lines, err := qs.StoreGuestLines(ctx, *guestCartID)
	if err != nil {
		return uuid.Nil, err
	}
	for _, l := range lines {
		var err error
		if l.SetID == nil {
			err = AddItem(ctx, q, userCart.ID, l.VariantID, l.Quantity)
		} else {
			err = mergeSetMemberLine(ctx, q, userCart.ID, *l.SetID, l.VariantID, l.Quantity)
		}
		if err != nil && !errors.Is(err, ErrInvalidVariant) && !errors.Is(err, ErrOutOfStock) {
			return uuid.Nil, err
		}
	}
	if err := qs.StoreDeleteCart(ctx, *guestCartID); err != nil {
		return uuid.Nil, err
	}
	if _, err := HydrateCartTx(ctx, q, userCart.ID); err != nil {
		return uuid.Nil, err
	}
	return userCart.Token, nil
}
