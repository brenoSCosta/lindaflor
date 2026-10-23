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
	var id uuid.UUID
	err := pool.QueryRow(ctx, `SELECT id FROM warehouses WHERE code = 'PRINCIPAL'`).Scan(&id)
	if err == nil {
		return id, nil
	}
	if !errors.Is(err, pgx.ErrNoRows) {
		return uuid.Nil, err
	}
	id = uuid.Must(uuid.NewV7())
	if _, err := pool.Exec(ctx, `INSERT INTO warehouses (id, code, name, is_default, active, created_at)
		VALUES ($1, 'PRINCIPAL', 'Depósito Principal', true, true, now())`, id); err != nil {
		return uuid.Nil, err
	}
	slog.InfoContext(ctx, "Warehouse created: PRINCIPAL (Depósito Principal)")
	return id, nil
}

const firstPurchaseCoupon = "BEMVINDA10"

func seedSettings(ctx context.Context, pool *pgxpool.Pool) error {
	tag, err := pool.Exec(ctx, `INSERT INTO store_settings (id, pix_key, pix_key_type, pix_merchant_name, pix_merchant_city, pix_discount_percent, announcement_text, first_purchase_coupon_code, whatsapp_number, whatsapp_message_template, created_at, updated_at)
		VALUES (true, $1, 'phone', 'Linda Flor Moda Praia', 'Aracaju', 5, NULL, $4, $2, $3, now(), now())
		ON CONFLICT (id) DO NOTHING`,
		envStr("SEED_PIX_KEY", "79998165115"),
		envStr("SEED_WHATSAPP_NUMBER", "5579998165115"),
		envStr("SEED_WHATSAPP_MESSAGE_TEMPLATE", "Olá! Fiz o pedido {{order_id}} no valor de {{total}} e quero confirmar o pagamento via PIX."),
		firstPurchaseCoupon)
	if err != nil {
		return err
	}
	if tag.RowsAffected() == 0 {
		slog.InfoContext(ctx, "Store settings already present, leaving unchanged")
	} else {
		slog.InfoContext(ctx, "Store settings created")
	}
	if _, err := pool.Exec(ctx, `INSERT INTO coupons (id, code, discount_type, discount_value, min_subtotal_cents, active, usage_type, per_user_limit, created_at, updated_at)
		VALUES ($1, $2, 'percent', 10, 0, true, 'unlimited', 1, now(), now())
		ON CONFLICT ((upper(code))) DO NOTHING`, uuid.Must(uuid.NewV7()), firstPurchaseCoupon); err != nil {
		return err
	}
	var banners int64
	if err := pool.QueryRow(ctx, `SELECT COUNT(*) FROM home_banners`).Scan(&banners); err != nil {
		return err
	}
	if banners == 0 {
		if _, err := pool.Exec(ctx, `INSERT INTO home_banners (id, title, subtitle, cta_label, link_url, image_url, sort_order, active, created_at)
			VALUES
			($1, 'Verão 2027', 'Coleção Nova', 'Explorar', '/produtos?sort=novos', 'https://images.unsplash.com/photo-1519046904884-53103b34b206?w=1200&h=600&fit=crop', 0, true, now()),
			($2, 'Liquida', '20% OFF', 'Ver Liquida', '/produtos?sale=true', 'https://images.unsplash.com/photo-1473496169904-658ba7c44d8a?w=1200&h=600&fit=crop', 1, true, now())`,
			uuid.Must(uuid.NewV7()), uuid.Must(uuid.NewV7())); err != nil {
			return err
		}
		slog.InfoContext(ctx, "Home banners created")
	}
	return nil
}

func compareUUID(a, b uuid.UUID) int { return bytes.Compare(a[:], b[:]) }

func seedLooks(ctx context.Context, pool *pgxpool.Pool) error {
	rows, err := pool.Query(ctx, `SELECT p.id, p.category::text, pv.color
		FROM products p
		INNER JOIN product_variants pv ON pv.product_id = p.id
		WHERE p.active = true
		  AND p.category IN ('biquini_top'::product_category, 'biquini_bottom'::product_category)`)
	if err != nil {
		return err
	}
	colors := map[uuid.UUID]map[string]bool{}
	tops, bottoms := map[uuid.UUID]bool{}, map[uuid.UUID]bool{}
	var id uuid.UUID
	var category, color string
	if _, err := pgx.ForEachRow(rows, []any{&id, &category, &color}, func() error {
		if colors[id] == nil {
			colors[id] = map[string]bool{}
		}
		colors[id][color] = true
		if category == "biquini_top" {
			tops[id] = true
		} else {
			bottoms[id] = true
		}
		return nil
	}); err != nil {
		return err
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
				if _, err := pool.Exec(ctx, `INSERT INTO product_looks (product_id, related_product_id, sort_order)
					VALUES ($1, $2, $3) ON CONFLICT DO NOTHING`, pair[0], pair[1], int32(sort)); err != nil { //nolint:gosec // ≤ 8
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
	var m setMember
	err := pool.QueryRow(ctx, `SELECT p.id, pv.id, p.price_in_cents
		FROM products p
		INNER JOIN LATERAL (
		  SELECT pv.id FROM product_variants pv
		  WHERE pv.product_id = p.id
		  ORDER BY pv.created_at ASC LIMIT 1
		) pv ON true
		WHERE p.slug = $1 AND p.active = true`, slug).Scan(&m.productID, &m.variantID, &m.price)
	if errors.Is(err, pgx.ErrNoRows) {
		return nil, nil
	}
	return &m, err
}

func seedSets(ctx context.Context, pool *pgxpool.Pool, c catalog) error {
	maxSets, err := envInt("SEED_SET_COUNT", 12)
	if err != nil {
		return err
	}
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
		var setID uuid.UUID
		err = pool.QueryRow(ctx, `SELECT id FROM sets WHERE slug = $1`, set.Slug).Scan(&setID)
		switch {
		case errors.Is(err, pgx.ErrNoRows):
			setID = uuid.Must(uuid.NewV7())
			if _, err := pool.Exec(ctx, `INSERT INTO sets (id, name, slug, description, active, price_in_cents, compare_at_price_in_cents, created_at, updated_at)
				VALUES ($1, $2, $3, $4, true, $5, $6, now(), now())`, setID, set.Name, set.Slug, set.Description, price, sum); err != nil {
				return err
			}
			slog.InfoContext(ctx, "Set created", "name", set.Name)
		case err != nil:
			return err
		}
		var images int64
		if err := pool.QueryRow(ctx, `SELECT COUNT(*) FROM set_images WHERE set_id = $1`, setID).Scan(&images); err != nil {
			return err
		}
		if images == 0 {
			rows, err := pool.Query(ctx, `SELECT url, alt FROM product_images
				WHERE product_id = ANY($1) ORDER BY sort_order ASC LIMIT 3`, []uuid.UUID{top.productID, bottom.productID})
			if err != nil {
				return err
			}
			type img struct {
				URL string  `db:"url"`
				Alt *string `db:"alt"`
			}
			imgs, err := pgx.CollectRows(rows, pgx.RowToStructByName[img])
			if err != nil {
				return err
			}
			for sort, im := range imgs {
				if _, err := pool.Exec(ctx, `INSERT INTO set_images (id, set_id, url, alt, sort_order, created_at)
					VALUES ($1, $2, $3, $4, $5, now())`, uuid.Must(uuid.NewV7()), setID, im.URL, im.Alt, int32(sort)); err != nil { //nolint:gosec // ≤ 3
					return err
				}
			}
		}
		for order, pid := range []uuid.UUID{top.productID, bottom.productID} {
			if _, err := pool.Exec(ctx, `INSERT INTO set_members (set_id, product_id, sort_order)
				VALUES ($1, $2, $3) ON CONFLICT DO NOTHING`, setID, pid, int32(order)); err != nil { //nolint:gosec // 2 members
				return err
			}
		}
		var slots int64
		if err := pool.QueryRow(ctx, `SELECT COUNT(*) FROM set_slots WHERE set_id = $1`, setID).Scan(&slots); err != nil {
			return err
		}
		if slots == 0 {
			for order, s := range []struct {
				label   string
				variant uuid.UUID
			}{{"Top", top.variantID}, {"Calcinha", bottom.variantID}} {
				slotID := uuid.Must(uuid.NewV7())
				if _, err := pool.Exec(ctx, `INSERT INTO set_slots (id, set_id, label, sort_order) VALUES ($1, $2, $3, $4)`,
					slotID, setID, s.label, int32(order)); err != nil { //nolint:gosec // 2 slots
					return err
				}
				if _, err := pool.Exec(ctx, `INSERT INTO set_slot_options (slot_id, variant_id, price_override_cents)
					VALUES ($1, $2, NULL) ON CONFLICT DO NOTHING`, slotID, s.variant); err != nil {
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
	for productIndex, p := range takeProducts(c.Products, int(max(count, 0))) {
		if !slices.Contains(validCategories, p.Category) {
			slog.WarnContext(ctx, "Skipping product with invalid category", "slug", p.Slug, "category", p.Category)
			continue
		}
		var existing uuid.UUID
		err := pool.QueryRow(ctx, `SELECT id FROM products WHERE slug = $1`, p.Slug).Scan(&existing)
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
		if _, err := pool.Exec(ctx, `INSERT INTO products (id, name, slug, description, price_in_cents, compare_at_price_in_cents, category, active, featured, created_at, updated_at)
			VALUES ($1, $2, $3, $4, $5, $6, $7::text::product_category, true, $8, now(), now())`,
			id, p.Name, p.Slug, p.Description, price, compare, p.Category, p.Featured); err != nil {
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
				if _, err := pool.Exec(ctx, `INSERT INTO product_variants (id, product_id, sku, size, color, color_hex, price_in_cents, low_stock_threshold, created_at, updated_at)
					VALUES ($1, $2, $3, $4::text::product_size, $5, $6, NULL, 5, now(), now())`,
					variantID, id, sku, size, color.Name, color.Hex); err != nil {
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
				if _, err := pool.Exec(ctx, `INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved, updated_at)
					VALUES ($1, $2, $3, $4, 0, now())`, uuid.Must(uuid.NewV7()), variantID, warehouseID, int32(quantity)); err != nil { //nolint:gosec // env-bounded
					return err
				}
			}
		}
		for sort, img := range p.Images[:min(len(p.Images), 8)] {
			if _, err := pool.Exec(ctx, `INSERT INTO product_images (id, product_id, url, alt, sort_order, created_at)
				VALUES ($1, $2, $3, $4, $5, now())`, uuid.Must(uuid.NewV7()), id, img.URL, img.Alt, int32(sort)); err != nil { //nolint:gosec // ≤ 8
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
