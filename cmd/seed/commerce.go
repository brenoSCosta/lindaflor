package main

import (
	"bytes"
	"context"
	_ "embed"
	"encoding/json"
	"errors"
	"fmt"
	"log/slog"
	"math/rand/v2"
	"os"
	"slices"
	"strconv"
	"strings"
	"unicode"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

//go:embed data/catalog.json
var catalogJSON []byte

var (
	validCategories = []string{"biquini", "biquini_top", "biquini_bottom", "maio", "saida_praia", "acessorio"}
	validSizes      = []string{"pp", "p", "m", "g", "gg"}
)

type catalog struct {
	Products []catalogProduct `json:"products"`
	Sets     []catalogSet     `json:"sets"`
}

type catalogProduct struct {
	Name                  string         `json:"name"`
	Slug                  string         `json:"slug"`
	Description           string         `json:"description"`
	Category              string         `json:"category"`
	PriceInCents          int64          `json:"price_in_cents"`
	CompareAtPriceInCents *int64         `json:"compare_at_price_in_cents"`
	Featured              bool           `json:"featured"`
	Images                []catalogImage `json:"images"`
	Colors                []catalogColor `json:"colors"`
	Sizes                 []string       `json:"sizes"`
}

type catalogImage struct {
	URL string `json:"url"`
	Alt string `json:"alt"`
}

type catalogColor struct {
	Name string `json:"name"`
	Hex  string `json:"hex"`
}

type catalogSet struct {
	Name        string `json:"name"`
	Slug        string `json:"slug"`
	Description string `json:"description"`
	TopSlug     string `json:"top_slug"`
	BottomSlug  string `json:"bottom_slug"`
}

func loadCatalog() (catalog, error) {
	var c catalog
	dec := json.NewDecoder(bytes.NewReader(catalogJSON))
	if err := dec.Decode(&c); err != nil {
		return c, fmt.Errorf("catalog parse error: %w", err)
	}
	return c, nil
}

func envStr(key, def string) string {
	if v, ok := os.LookupEnv(key); ok {
		return v
	}
	return def
}

func envInt(key string, def int64) (int64, error) {
	v, ok := os.LookupEnv(key)
	if !ok {
		return def, nil
	}
	n, err := strconv.ParseInt(v, 10, 64)
	if err != nil {
		return 0, fmt.Errorf("invalid value for %s", key)
	}
	return n, nil
}

func foldAccent(r rune) rune {
	switch r {
	case 'á', 'à', 'â', 'ã', 'ä', 'Á', 'À', 'Â', 'Ã', 'Ä':
		return 'a'
	case 'é', 'ê', 'ë', 'É', 'Ê', 'Ë':
		return 'e'
	case 'í', 'ï', 'Í', 'Ï':
		return 'i'
	case 'ó', 'ô', 'õ', 'ö', 'Ó', 'Ô', 'Õ', 'Ö':
		return 'o'
	case 'ú', 'ü', 'Ú', 'Ü':
		return 'u'
	case 'ç', 'Ç':
		return 'c'
	case 'ñ', 'Ñ':
		return 'n'
	}
	return r
}

// slugify folds accents, keeps alphanumerics (lowercased) and collapses
// everything else into single dashes.
func slugify(input string) string {
	var b strings.Builder
	for _, r := range input {
		r = foldAccent(r)
		switch {
		case unicode.IsLetter(r) || unicode.IsDigit(r):
			b.WriteString(strings.ToLower(string(r)))
		case b.Len() > 0 && !strings.HasSuffix(b.String(), "-"):
			b.WriteByte('-')
		}
	}
	return strings.Trim(b.String(), "-")
}

func clampPrice(price, lo, hi int64) int64 { return min(max(price, lo), hi) }

// takeProducts round-robins across categories so a small
// SEED_PRODUCT_COUNT still fills every nav bucket.
func takeProducts(products []catalogProduct, count int) []catalogProduct {
	buckets := map[string][]catalogProduct{}
	var order []string
	for _, p := range products {
		if _, ok := buckets[p.Category]; !ok {
			order = append(order, p.Category)
		}
		buckets[p.Category] = append(buckets[p.Category], p)
	}
	idx := map[string]int{}
	var chosen []catalogProduct
	for len(chosen) < count {
		progressed := false
		for _, cat := range order {
			if len(chosen) >= count {
				break
			}
			if i := idx[cat]; i < len(buckets[cat]) {
				chosen = append(chosen, buckets[cat][i])
				idx[cat] = i + 1
				progressed = true
			}
		}
		if !progressed {
			break
		}
	}
	return chosen
}

func seedWarehouse(ctx context.Context, pool *pgxpool.Pool) (uuid.UUID, error) {
	q := sqlc.New(pool)
	id, err := q.SeedWarehouseID(ctx)
	if err == nil {
		return id, nil
	}
	if !errors.Is(err, pgx.ErrNoRows) {
		return uuid.Nil, err
	}
	id = uuid.Must(uuid.NewV7())
	if err := q.SeedInsertWarehouse(ctx, id); err != nil {
		return uuid.Nil, err
	}
	slog.InfoContext(ctx, "Warehouse created: PRINCIPAL (Depósito Principal)")
	return id, nil
}

const firstPurchaseCoupon = "BEMVINDA10"

func seedSettings(ctx context.Context, pool *pgxpool.Pool) error {
	q := sqlc.New(pool)
	inserted, err := q.SeedInsertSettings(ctx, sqlc.SeedInsertSettingsParams{
		PixKey:                  envStr("SEED_PIX_KEY", "79998165115"),
		WhatsappNumber:          envStr("SEED_WHATSAPP_NUMBER", "5579998165115"),
		WhatsappMessageTemplate: envStr("SEED_WHATSAPP_MESSAGE_TEMPLATE", "Olá! Fiz o pedido {{order_id}} no valor de {{total}} e quero confirmar o pagamento via PIX."),
		FirstPurchaseCouponCode: firstPurchaseCoupon,
	})
	if err != nil {
		return err
	}
	if inserted == 0 {
		slog.InfoContext(ctx, "Store settings already present, leaving unchanged")
	} else {
		slog.InfoContext(ctx, "Store settings created")
	}
	if err := q.SeedInsertFirstPurchaseCoupon(ctx, sqlc.SeedInsertFirstPurchaseCouponParams{
		ID: uuid.Must(uuid.NewV7()), Code: firstPurchaseCoupon,
	}); err != nil {
		return err
	}
	banners, err := q.SeedCountBanners(ctx)
	if err != nil {
		return err
	}
	if banners == 0 {
		if err := q.SeedInsertBanners(ctx, sqlc.SeedInsertBannersParams{
			FirstID: uuid.Must(uuid.NewV7()), SecondID: uuid.Must(uuid.NewV7()),
		}); err != nil {
			return err
		}
		slog.InfoContext(ctx, "Home banners created")
	}
	return nil
}

func compareUUID(a, b uuid.UUID) int { return bytes.Compare(a[:], b[:]) }

func seedLooks(ctx context.Context, pool *pgxpool.Pool) error {
	q := sqlc.New(pool)
	rows, err := q.SeedLookCandidates(ctx)
	if err != nil {
		return err
	}
	colors := map[uuid.UUID]map[string]bool{}
	tops, bottoms := map[uuid.UUID]bool{}, map[uuid.UUID]bool{}
	for _, r := range rows {
		if colors[r.ID] == nil {
			colors[r.ID] = map[string]bool{}
		}
		colors[r.ID][r.Color] = true
		if r.Category == "biquini_top" {
			tops[r.ID] = true
		} else {
			bottoms[r.ID] = true
		}
	}
	const maxLooks = 8
	for top := range tops {
		var sharing []uuid.UUID
		for bottom := range bottoms {
			for c := range colors[top] {
				if colors[bottom][c] {
					sharing = append(sharing, bottom)
					break
				}
			}
		}
		if len(sharing) == 0 {
			for bottom := range bottoms {
				sharing = append(sharing, bottom)
			}
		}
		slices.SortFunc(sharing, compareUUID)
		for sort, bottom := range sharing[:min(len(sharing), maxLooks)] {
			for _, pair := range [][2]uuid.UUID{{top, bottom}, {bottom, top}} {
				if err := q.SeedInsertLook(ctx, sqlc.SeedInsertLookParams{
					ProductID: pair[0], RelatedProductID: pair[1], SortOrder: int32(sort), //nolint:gosec // ≤ 8
				}); err != nil {
					return err
				}
			}
		}
	}
	return nil
}

type setMember struct {
	productID, variantID uuid.UUID
	price                int64
}

func setMemberBySlug(ctx context.Context, pool *pgxpool.Pool, slug string) (*setMember, error) {
	r, err := sqlc.New(pool).SeedSetMemberBySlug(ctx, slug)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, nil
	}
	return &setMember{productID: r.ProductID, variantID: r.VariantID, price: r.PriceInCents}, err
}

func seedSets(ctx context.Context, pool *pgxpool.Pool, c catalog) error {
	maxSets, err := envInt("SEED_SET_COUNT", 12)
	if err != nil {
		return err
	}
	q := sqlc.New(pool)
	for i, set := range c.Sets {
		if int64(i) >= maxSets {
			break
		}
		top, err := setMemberBySlug(ctx, pool, set.TopSlug)
		if err != nil {
			return err
		}
		bottom, err := setMemberBySlug(ctx, pool, set.BottomSlug)
		if err != nil {
			return err
		}
		if top == nil || bottom == nil {
			slog.InfoContext(ctx, "Skipping set seed: missing top or bottom product", "slug", set.Slug)
			continue
		}
		sum := top.price + bottom.price
		price := sum * 90 / 100
		setID, err := q.SeedSetIDBySlug(ctx, set.Slug)
		switch {
		case errors.Is(err, pgx.ErrNoRows):
			setID = uuid.Must(uuid.NewV7())
			if err := q.SeedInsertSet(ctx, sqlc.SeedInsertSetParams{
				ID: setID, Name: set.Name, Slug: set.Slug, Description: &set.Description,
				PriceInCents: &price, CompareAtPriceInCents: &sum,
			}); err != nil {
				return err
			}
			slog.InfoContext(ctx, "Set created", "name", set.Name)
		case err != nil:
			return err
		}
		images, err := q.SeedCountSetImages(ctx, setID)
		if err != nil {
			return err
		}
		if images == 0 {
			imgs, err := q.SeedMemberImages(ctx, []uuid.UUID{top.productID, bottom.productID})
			if err != nil {
				return err
			}
			for sort, im := range imgs {
				if err := q.SeedInsertSetImage(ctx, sqlc.SeedInsertSetImageParams{
					ID: uuid.Must(uuid.NewV7()), SetID: setID, Url: im.Url, Alt: im.Alt, SortOrder: int32(sort), //nolint:gosec // ≤ 3
				}); err != nil {
					return err
				}
			}
		}
		for order, pid := range []uuid.UUID{top.productID, bottom.productID} {
			if err := q.SeedInsertSetMember(ctx, sqlc.SeedInsertSetMemberParams{
				SetID: setID, ProductID: pid, SortOrder: int32(order), //nolint:gosec // 2 members
			}); err != nil {
				return err
			}
		}
		slots, err := q.SeedCountSetSlots(ctx, setID)
		if err != nil {
			return err
		}
		if slots == 0 {
			for order, s := range []struct {
				label   string
				variant uuid.UUID
			}{{"Top", top.variantID}, {"Calcinha", bottom.variantID}} {
				slotID := uuid.Must(uuid.NewV7())
				if err := q.SeedInsertSetSlot(ctx, sqlc.SeedInsertSetSlotParams{
					ID: slotID, SetID: setID, Label: &s.label, SortOrder: int32(order), //nolint:gosec // 2 slots
				}); err != nil {
					return err
				}
				if err := q.SeedInsertSlotOption(ctx, sqlc.SeedInsertSlotOptionParams{SlotID: slotID, VariantID: s.variant}); err != nil {
					return err
				}
			}
		}
	}
	return nil
}

func seedProducts(ctx context.Context, pool *pgxpool.Pool, warehouseID uuid.UUID, c catalog) error {
	count, err := envInt("SEED_PRODUCT_COUNT", 108)
	if err != nil {
		return err
	}
	priceMin, err := envInt("SEED_PRICE_MIN", 3990)
	if err != nil {
		return err
	}
	priceMax, err := envInt("SEED_PRICE_MAX", 29990)
	if err != nil {
		return err
	}
	stockMin, err := envInt("SEED_STOCK_MIN", 5)
	if err != nil {
		return err
	}
	stockMax, err := envInt("SEED_STOCK_MAX", 30)
	if err != nil {
		return err
	}
	q := sqlc.New(pool)
	for productIndex, p := range takeProducts(c.Products, int(max(count, 0))) {
		if !slices.Contains(validCategories, p.Category) {
			slog.WarnContext(ctx, "Skipping product with invalid category", "slug", p.Slug, "category", p.Category)
			continue
		}
		_, err := q.SeedProductIDBySlug(ctx, p.Slug)
		if err == nil {
			continue
		}
		if !errors.Is(err, pgx.ErrNoRows) {
			return err
		}
		id := uuid.Must(uuid.NewV7())
		price := clampPrice(p.PriceInCents, priceMin, priceMax)
		var compare *int64
		if p.CompareAtPriceInCents != nil {
			v := clampPrice(*p.CompareAtPriceInCents, priceMin, max(priceMax, *p.CompareAtPriceInCents))
			if v > price {
				compare = &v
			}
		}
		if err := q.SeedInsertProduct(ctx, sqlc.SeedInsertProductParams{
			ID: id, Name: p.Name, Slug: p.Slug, Description: &p.Description, PriceInCents: price,
			CompareAtPriceInCents: compare, Category: p.Category, Featured: p.Featured,
		}); err != nil {
			return err
		}
		var sizes []string
		for _, s := range p.Sizes {
			if slices.Contains(validSizes, s) {
				sizes = append(sizes, s)
			}
		}
		if len(sizes) == 0 {
			sizes = []string{"m"}
		}
		variantIndex := 0
		for _, size := range sizes {
			for _, color := range p.Colors {
				variantID := uuid.Must(uuid.NewV7())
				sku := fmt.Sprintf("%s-%s-%s", p.Slug, size, slugify(color.Name))
				if err := q.SeedInsertVariant(ctx, sqlc.SeedInsertVariantParams{
					ID: variantID, ProductID: id, Sku: sku, Size: size, Color: color.Name, ColorHex: &color.Hex,
				}); err != nil {
					return err
				}
				quantity := stockMin
				switch {
				case (productIndex+variantIndex)%23 == 0:
					quantity = 0
				case stockMax > stockMin:
					quantity = stockMin + rand.Int64N(stockMax-stockMin+1) //nolint:gosec // seed data, not security
				}
				variantIndex++
				if err := q.SeedInsertInventory(ctx, sqlc.SeedInsertInventoryParams{
					ID: uuid.Must(uuid.NewV7()), VariantID: variantID, WarehouseID: warehouseID, Quantity: int32(quantity), //nolint:gosec // env-bounded
				}); err != nil {
					return err
				}
			}
		}
		for sort, img := range p.Images[:min(len(p.Images), 8)] {
			if err := q.SeedInsertProductImage(ctx, sqlc.SeedInsertProductImageParams{
				ID: uuid.Must(uuid.NewV7()), ProductID: id, Url: img.URL, Alt: &img.Alt, SortOrder: int32(sort), //nolint:gosec // ≤ 8
			}); err != nil {
				return err
			}
		}
		slog.InfoContext(ctx, "Product created", "name", p.Name, "slug", p.Slug)
	}
	return seedLooks(ctx, pool)
}

func seedCommerce(ctx context.Context, pool *pgxpool.Pool) error {
	c, err := loadCatalog()
	if err != nil {
		return err
	}
	warehouseID, err := seedWarehouse(ctx, pool)
	if err != nil {
		return err
	}
	if err := seedSettings(ctx, pool); err != nil {
		return err
	}
	if err := seedProducts(ctx, pool, warehouseID, c); err != nil {
		return err
	}
	return seedSets(ctx, pool, c)
}
