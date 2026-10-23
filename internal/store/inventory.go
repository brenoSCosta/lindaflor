package store

import (
	"context"
	"errors"
	"log/slog"
	"math"
	"slices"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// Stock holds and movements on inventory rows (src/app/store/inventory.rs).
//
// Lock order invariant: every transaction that writes inventory takes all
// the row locks it needs in one statement, `… ORDER BY i.id FOR UPDATE`
// (lockInventoryIDs / lockVariantInventory), before the first write. Later
// locks in the same transaction only re-take rows it already holds. Since
// everyone acquires in ascending id order, pay, cancel, the sweeper,
// checkout and admin adjustments on overlapping rows queue instead of
// deadlocking (40P01). Keep it that way when adding a code path.

// LiveAvailableSQL is the sellable units of the inventory row aliased i:
// holds of pending orders past reservation_expires_at count as free (the
// sweeper releases them later). Same as the SQL function
// inventory_live_available(i.id, i.quantity, i.reserved).
const LiveAvailableSQL = `inventory_live_available(i.id, i.quantity, i.reserved)`

// ErrInsufficientStock is a hold or adjustment that would oversell.
var ErrInsufficientStock = errors.New("Estoque insuficiente para concluir o pedido.")

// ReserveLine is one variant quantity to hold, restock or convert.
type ReserveLine struct {
	VariantID uuid.UUID
	Quantity  int32
}

// inventoryLockRow is one locked inventory row (StoreLockInventoryIDs and
// StoreLockVariantInventory return the same columns).
type inventoryLockRow = sqlc.StoreLockVariantInventoryRow

func sortedUnique(ids []uuid.UUID) []uuid.UUID {
	out := slices.Clone(ids)
	slices.SortFunc(out, func(a, b uuid.UUID) int { return compareUUID(a, b) })
	return slices.Compact(out)
}

func compareUUID(a, b uuid.UUID) int {
	for i := range a {
		if a[i] != b[i] {
			if a[i] < b[i] {
				return -1
			}
			return 1
		}
	}
	return 0
}

// lockInventoryIDs locks the given inventory rows (any warehouse) in id order.
func lockInventoryIDs(ctx context.Context, tx DBTX, ids []uuid.UUID) ([]inventoryLockRow, error) {
	ids = sortedUnique(ids)
	if len(ids) == 0 {
		return nil, nil
	}
	rows, err := queries(tx).StoreLockInventoryIDs(ctx, ids)
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, func(r sqlc.StoreLockInventoryIDsRow) inventoryLockRow { return inventoryLockRow(r) }), nil
}

// lockVariantInventory locks every active-warehouse row of the variants.
func lockVariantInventory(ctx context.Context, tx DBTX, variantIDs []uuid.UUID) ([]inventoryLockRow, error) {
	variantIDs = sortedUnique(variantIDs)
	if len(variantIDs) == 0 {
		return nil, nil
	}
	return queries(tx).StoreLockVariantInventory(ctx, variantIDs)
}

// LockCartInventory locks every active-warehouse inventory row of the cart,
// plus every row held by an expired-but-unswept pending order that blocks
// one of them, in one id-ordered statement; then cancels those expired
// orders (skipping any another transaction is handling) so their holds are
// free for this checkout.
func LockCartInventory(ctx context.Context, tx DBTX, cartID uuid.UUID) error {
	q := queries(tx)
	cartRows, err := q.StoreCartInventoryRows(ctx, cartID)
	if err != nil {
		return err
	}
	if len(cartRows) == 0 {
		return nil
	}
	expired, err := q.StoreExpiredHolders(ctx, cartRows)
	if err != nil {
		return err
	}
	ids := slices.Clone(cartRows)
	orderIDs := make([]uuid.UUID, 0, len(expired))
	for _, h := range expired {
		ids = append(ids, h.InventoryID)
		orderIDs = append(orderIDs, h.OrderID)
	}
	if _, err := lockInventoryIDs(ctx, tx, ids); err != nil {
		return err
	}
	orderIDs = sortedUnique(orderIDs)
	if len(orderIDs) == 0 {
		return nil
	}
	// SKIP LOCKED: never wait on an order row while holding inventory locks
	// (Apply takes order → inventory).
	claimed, err := q.StoreClaimExpiredOrders(ctx, orderIDs)
	if err != nil {
		return err
	}
	for _, id := range claimed {
		if _, err := Apply(ctx, tx, id, EventCancel, ApplyOptions{}); err != nil {
			if !isApplyDomainError(err) {
				return err
			}
			slog.WarnContext(ctx, "expired hold reclaim skipped", "order_id", id, "error", err)
		}
	}
	return nil
}

// ReserveForOrder holds stock for a pending PIX order: reserved += qty
// across warehouses (default first) with one inventory_reservations row per
// take, and sets the 48h deadline.
func ReserveForOrder(ctx context.Context, tx DBTX, orderID uuid.UUID, lines []ReserveLine) error {
	var variantIDs []uuid.UUID
	for _, l := range lines {
		if l.Quantity > 0 {
			variantIDs = append(variantIDs, l.VariantID)
		}
	}
	rows, err := lockVariantInventory(ctx, tx, variantIDs)
	if err != nil {
		return err
	}
	q := queries(tx)
	// Default warehouse first, then id (locks were already taken in id order).
	slices.SortStableFunc(rows, func(a, b inventoryLockRow) int {
		if a.IsDefault != b.IsDefault {
			if a.IsDefault {
				return -1
			}
			return 1
		}
		return compareUUID(a.ID, b.ID)
	})
	for _, line := range lines {
		if line.Quantity <= 0 {
			continue
		}
		var available int32
		for _, r := range rows {
			if r.VariantID == line.VariantID {
				available += max(r.Quantity-r.Reserved, 0)
			}
		}
		if available < line.Quantity {
			return ErrInsufficientStock
		}
		remaining := line.Quantity
		for i := range rows {
			r := &rows[i]
			if r.VariantID != line.VariantID {
				continue
			}
			if remaining <= 0 {
				break
			}
			take := min(remaining, max(r.Quantity-r.Reserved, 0))
			if take <= 0 {
				continue
			}
			if err := q.StoreReserveInventory(ctx, sqlc.StoreReserveInventoryParams{ID: r.ID, Quantity: take}); err != nil {
				return err
			}
			if err := q.StoreInsertReservation(ctx, sqlc.StoreInsertReservationParams{
				ID: uuid.Must(uuid.NewV7()), OrderID: orderID, InventoryID: r.ID, Quantity: take,
			}); err != nil {
				return err
			}
			r.Reserved += take
			remaining -= take
		}
		if remaining > 0 {
			return ErrInsufficientStock
		}
	}
	return q.StoreSetReservationExpiry(ctx, orderID)
}

type reservationRow = sqlc.StoreReservationRowsRow

// convertOrRelease sells (convert) or frees quantity units of a hold.
func convertOrRelease(ctx context.Context, q *sqlc.Queries, inventoryID uuid.UUID, quantity int32, convert bool) error {
	if convert {
		return q.StoreConvertInventory(ctx, sqlc.StoreConvertInventoryParams{ID: inventoryID, Quantity: quantity})
	}
	return q.StoreReleaseInventory(ctx, sqlc.StoreReleaseInventoryParams{ID: inventoryID, Quantity: quantity})
}

func applyReservationRows(ctx context.Context, tx DBTX, orderID uuid.UUID, rows []reservationRow, convert bool) error {
	ids := make([]uuid.UUID, 0, len(rows))
	for _, r := range rows {
		ids = append(ids, r.InventoryID)
	}
	locked, err := lockInventoryIDs(ctx, tx, ids)
	if err != nil {
		return err
	}
	onHand := make(map[uuid.UUID]int32, len(locked))
	variantOf := make(map[uuid.UUID]uuid.UUID, len(locked))
	for _, r := range locked {
		onHand[r.ID] = r.Quantity
		variantOf[r.ID] = r.VariantID
	}
	for _, r := range rows {
		if convert {
			if q, ok := onHand[r.InventoryID]; ok {
				if q < r.Quantity {
					slog.WarnContext(ctx, "convert clamps stock at zero: oversell",
						"order_id", orderID, "variant_id", variantOf[r.InventoryID],
						"inventory_id", r.InventoryID, "on_hand", q, "sold", r.Quantity)
				}
				onHand[r.InventoryID] = max(q-r.Quantity, 0)
			}
		}
		if err := convertOrRelease(ctx, queries(tx), r.InventoryID, r.Quantity, convert); err != nil {
			return err
		}
	}
	return nil
}

func fallbackFromOrderItems(ctx context.Context, tx DBTX, orderID uuid.UUID, convert bool) error {
	q := queries(tx)
	items, err := q.StoreOrderItemQuantities(ctx, orderID)
	if err != nil {
		return err
	}
	variantIDs := make([]uuid.UUID, 0, len(items))
	for _, it := range items {
		variantIDs = append(variantIDs, it.VariantID)
	}
	rows, err := lockVariantInventory(ctx, tx, variantIDs)
	if err != nil {
		return err
	}
	for _, it := range items {
		remaining := it.Quantity
		for i := range rows {
			r := &rows[i]
			if r.VariantID != it.VariantID {
				continue
			}
			if remaining <= 0 {
				break
			}
			take := min(remaining, max(r.Reserved, 0))
			if convert {
				take = min(take, max(r.Quantity, 0))
			}
			if take <= 0 {
				continue
			}
			if err := convertOrRelease(ctx, q, r.ID, take, convert); err != nil {
				return err
			}
			if convert {
				r.Quantity -= take
			}
			r.Reserved -= take
			remaining -= take
		}
	}
	return nil
}

func settleReservation(ctx context.Context, tx DBTX, orderID uuid.UUID, convert bool) error {
	q := queries(tx)
	rows, err := q.StoreReservationRows(ctx, orderID)
	if err != nil {
		return err
	}
	if len(rows) == 0 {
		err = fallbackFromOrderItems(ctx, tx, orderID, convert)
	} else {
		err = applyReservationRows(ctx, tx, orderID, rows, convert)
		if err == nil {
			err = q.StoreClearReservations(ctx, orderID)
		}
	}
	if err != nil {
		return err
	}
	return q.StoreClearReservationAt(ctx, orderID)
}

// ConvertReservation turns a hold into a sale using the stored rows.
func ConvertReservation(ctx context.Context, tx DBTX, orderID uuid.UUID) error {
	return settleReservation(ctx, tx, orderID, true)
}

// ReleaseReservation releases a hold without selling.
func ReleaseReservation(ctx context.Context, tx DBTX, orderID uuid.UUID) error {
	return settleReservation(ctx, tx, orderID, false)
}

// RestockSoldItems puts the order's sold units back on the default (or
// first) active warehouse row.
func RestockSoldItems(ctx context.Context, tx DBTX, orderID uuid.UUID) error {
	items, err := queries(tx).StoreOrderItemQuantities(ctx, orderID)
	if err != nil {
		return err
	}
	lines := mapSlice(items, func(it sqlc.StoreOrderItemQuantitiesRow) ReserveLine { return ReserveLine(it) })
	return RestockLines(ctx, tx, orderID, lines)
}

// RestockLines restocks only lines of an order (partial returns).
func RestockLines(ctx context.Context, tx DBTX, orderID uuid.UUID, lines []ReserveLine) error {
	var variantIDs []uuid.UUID
	for _, l := range lines {
		if l.Quantity > 0 {
			variantIDs = append(variantIDs, l.VariantID)
		}
	}
	rows, err := lockVariantInventory(ctx, tx, variantIDs)
	if err != nil {
		return err
	}
	for _, l := range lines {
		if l.Quantity <= 0 {
			continue
		}
		var target *inventoryLockRow
		for i := range rows {
			if rows[i].VariantID == l.VariantID && rows[i].IsDefault {
				target = &rows[i]
				break
			}
		}
		if target == nil {
			for i := range rows {
				if rows[i].VariantID == l.VariantID {
					target = &rows[i]
					break
				}
			}
		}
		if target == nil {
			slog.WarnContext(ctx, "restock skipped: variant has no active-warehouse inventory row",
				"order_id", orderID, "variant_id", l.VariantID, "quantity", l.Quantity)
			continue
		}
		if err := queries(tx).StoreRestockInventory(ctx, sqlc.StoreRestockInventoryParams{ID: target.ID, Quantity: l.Quantity}); err != nil {
			return err
		}
	}
	return nil
}

// AdjustStock changes the on-hand quantity by delta (may be negative),
// refusing to go below zero or below the reserved units. Returns the
// warehouse id (audit metadata).
func AdjustStock(ctx context.Context, tx DBTX, inventoryID uuid.UUID, delta int32) (uuid.UUID, error) {
	q := queries(tx)
	row, found, err := noRows(q.StoreAdjustLock(ctx, inventoryID))
	if err != nil {
		return uuid.Nil, err
	} else if !found {
		return uuid.Nil, ErrInsufficientStock
	}
	next := int64(row.Quantity) + int64(delta)
	if next > math.MaxInt32 || next < 0 || int64(row.Reserved) > next {
		return uuid.Nil, ErrInsufficientStock
	}
	if err := q.StoreAdjustUpdate(ctx, sqlc.StoreAdjustUpdateParams{ID: inventoryID, Quantity: int32(next)}); err != nil {
		return uuid.Nil, err
	}
	return row.WarehouseID, nil
}
