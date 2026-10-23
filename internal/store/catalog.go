package store

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/db"
)

// Storefront catalog queries (src/app/store/queries.rs). The faceted
// listing builds its SQL dynamically (filters/sorts), so it is hand-written
// pgx; every other statement is registered for TestStatementsPrepare.

// Catalog constants.
const (
	StorefrontPageSize = 24
	HomeFeaturedLimit  = 8
)

// ColorSwatch is a (name, optional hex) color.
type ColorSwatch struct {
	Name string
	Hex  *string
}

// ProductSummary is a product card.
type ProductSummary struct {
	ID                    uuid.UUID
	Name                  string
	Slug                  string
	PriceInCents          int64
	CompareAtPriceInCents *int64
	Category              string
	// ImageURL / SecondaryImageURL are storage keys or URLs (see
	// ResolveSummaryImages).
	ImageURL          *string
	SecondaryImageURL *string
	AvailableTotal    int32
	Colors            []ColorSwatch
}

// SoldOut reports no available stock.
func (p ProductSummary) SoldOut() bool { return p.AvailableTotal == 0 }

// CategoryLabel is the storefront category label.
func (p ProductSummary) CategoryLabel() string { return CategoryLabel(p.Category) }

// DiscountBadge is "-NN%" ("" without a compare-at price).
func (p ProductSummary) DiscountBadge() string {
	if pct, ok := DiscountPercent(p.CompareAtPriceInCents, p.PriceInCents); ok {
		return fmt.Sprintf("-%d%%", pct)
	}
	return ""
}

const productSummaryColumns = `p.id, p.name, p.slug, p.price_in_cents, p.compare_at_price_in_cents, p.category::text AS category,
        (SELECT pi.url FROM product_images pi WHERE pi.product_id = p.id ORDER BY pi.sort_order ASC LIMIT 1) AS image_url,
        (SELECT pi.url FROM product_images pi WHERE pi.product_id = p.id ORDER BY pi.sort_order ASC LIMIT 1 OFFSET 1) AS secondary_image_url,
        COALESCE((SELECT SUM(GREATEST(i.quantity - i.reserved, 0))::int
            FROM inventory i
            INNER JOIN warehouses w ON w.id = i.warehouse_id
            WHERE i.variant_id IN (SELECT pv.id FROM product_variants pv WHERE pv.product_id = p.id)
              AND w.active = true), 0)::int AS available_total,
        COALESCE(
          (SELECT json_agg(json_build_array(pv.color, pv.color_hex) ORDER BY pv.color ASC)
           FROM (SELECT DISTINCT ON (pv.color) pv.color, pv.color_hex
                 FROM product_variants pv
                 WHERE pv.product_id = p.id
                 ORDER BY pv.color ASC, pv.color_hex ASC) pv),
          '[]'::json
        )::text AS colors_json`

type productSummaryRow struct {
	ID                    uuid.UUID `db:"id"`
	Name                  string    `db:"name"`
	Slug                  string    `db:"slug"`
	PriceInCents          int64     `db:"price_in_cents"`
	CompareAtPriceInCents *int64    `db:"compare_at_price_in_cents"`
	Category              string    `db:"category"`
	ImageURL              *string   `db:"image_url"`
	SecondaryImageURL     *string   `db:"secondary_image_url"`
	AvailableTotal        *int32    `db:"available_total"`
	ColorsJSON            *string   `db:"colors_json"`
}

func parseColors(raw *string) []ColorSwatch {
	out := []ColorSwatch{}
	if raw == nil {
		return out
	}
	var pairs [][2]*string
	if json.Unmarshal([]byte(*raw), &pairs) != nil {
		return out
	}
	for _, p := range pairs {
		if p[0] == nil {
			continue
		}
		out = append(out, ColorSwatch{Name: *p[0], Hex: p[1]})
	}
	return out
}

func (r productSummaryRow) summary() ProductSummary {
	return ProductSummary{
		ID: r.ID, Name: r.Name, Slug: r.Slug, PriceInCents: r.PriceInCents,
		CompareAtPriceInCents: r.CompareAtPriceInCents, Category: r.Category,
		ImageURL: r.ImageURL, SecondaryImageURL: r.SecondaryImageURL,
		AvailableTotal: deref(r.AvailableTotal), Colors: parseColors(r.ColorsJSON),
	}
}

func listSummaries(ctx context.Context, q DBTX, sql string, args ...any) ([]ProductSummary, error) {
	rows, err := collect[productSummaryRow](q.Query(ctx, sql, args...))
	if err != nil {
		return nil, err
	}
	out := make([]ProductSummary, 0, len(rows))
	for _, r := range rows {
		out = append(out, r.summary())
	}
	return out, nil
}

var (
	sqlWishlistProducts = sqlText(`SELECT ` + productSummaryColumns + `
 FROM wishlist_items wl
 INNER JOIN products p ON p.id = wl.product_id
 WHERE wl.user_id = $1 AND p.active = true
 ORDER BY wl.created_at DESC`)
	sqlRelatedProducts = sqlText(`SELECT ` + productSummaryColumns + `
 FROM products p
 WHERE p.active = true
   AND p.category = $1::text::product_category
   AND p.id != $2
 ORDER BY p.featured DESC, p.created_at DESC
 LIMIT 4`)
	sqlProductLooks = sqlText(`SELECT ` + productSummaryColumns + `
 FROM product_looks pl
 INNER JOIN products p ON p.id = pl.related_product_id
 WHERE pl.product_id = $1 AND p.active = true
 ORDER BY pl.sort_order ASC`)
	sqlSetMembers = sqlText(`SELECT ` + productSummaryColumns + `
 FROM set_members sm
 INNER JOIN products p ON p.id = sm.product_id
 WHERE sm.set_id = $1 AND p.active = true
 ORDER BY sm.sort_order ASC`)
)

// CatalogFilters are the faceted filters.
type CatalogFilters struct {
	Sizes         []string
	Colors        []string
	PriceMinCents *int64
	PriceMaxCents *int64
	AvailableOnly bool
	SaleOnly      bool
}

// CatalogQuery is one page of storefront products.
type CatalogQuery struct {
	Category          string // "" = all
	Search            string // "" = none
	FeaturedOnly      bool
	IncludeOutOfStock bool
	Filters           CatalogFilters
	Sort              string
	Limit, Offset     int64
}

type sqlBuilder struct {
	sb   strings.Builder
	args []any
}

func (b *sqlBuilder) arg(v any) string {
	b.args = append(b.args, v)
	return fmt.Sprintf("$%d", len(b.args))
}

func (b *sqlBuilder) appendFilters(q CatalogQuery) {
	if q.Category != "" {
		fmt.Fprintf(&b.sb, " AND p.category = %s::text::product_category", b.arg(q.Category))
	}
	if q.Search != "" {
		term := db.LikeContains(strings.TrimSpace(q.Search))
		fmt.Fprintf(&b.sb, ` AND (p.name ILIKE %s ESCAPE '\' OR p.description ILIKE %s ESCAPE '\' OR EXISTS (SELECT 1 FROM product_variants pv WHERE pv.product_id = p.id AND pv.sku ILIKE %s ESCAPE '\'))`,
			b.arg(term), b.arg(term), b.arg(term))
	}
	if q.FeaturedOnly {
		b.sb.WriteString(" AND p.featured = true")
	}
	if !q.IncludeOutOfStock {
		b.sb.WriteString(" AND COALESCE((SELECT SUM(GREATEST(i.quantity - i.reserved, 0)) FROM inventory i INNER JOIN warehouses w ON w.id = i.warehouse_id WHERE i.variant_id IN (SELECT pv.id FROM product_variants pv WHERE pv.product_id = p.id) AND w.active = true), 0) > 0")
	}
	f := q.Filters
	if f.PriceMinCents != nil {
		fmt.Fprintf(&b.sb, " AND p.price_in_cents >= %s", b.arg(*f.PriceMinCents))
	}
	if f.PriceMaxCents != nil {
		fmt.Fprintf(&b.sb, " AND p.price_in_cents <= %s", b.arg(*f.PriceMaxCents))
	}
	if len(f.Colors) > 0 {
		fmt.Fprintf(&b.sb, " AND EXISTS (SELECT 1 FROM product_variants pv WHERE pv.product_id = p.id AND pv.color = ANY(%s::text[]))", b.arg(f.Colors))
	}
	if len(f.Sizes) > 0 {
		fmt.Fprintf(&b.sb, " AND EXISTS (SELECT 1 FROM product_variants pv WHERE pv.product_id = p.id AND pv.size::text = ANY(%s::text[]))", b.arg(f.Sizes))
	}
	if f.SaleOnly {
		b.sb.WriteString(" AND p.compare_at_price_in_cents IS NOT NULL")
	}
}

func catalogOrder(sort string) string {
	switch sort {
	case "relevantes":
		return " ORDER BY p.featured DESC, p.created_at DESC"
	case "mais_vendidos":
		return ` ORDER BY COALESCE((SELECT SUM(oi.quantity)::int FROM order_items oi
              INNER JOIN orders o ON o.id = oi.order_id
              WHERE oi.variant_id IN (SELECT pv.id FROM product_variants pv WHERE pv.product_id = p.id)
              AND o.status IN ('paid', 'processing', 'shipped', 'delivered')), 0) DESC,
              p.featured DESC, p.created_at DESC`
	case "maior_desconto":
		return ` ORDER BY CASE WHEN p.compare_at_price_in_cents IS NOT NULL
              THEN CAST(p.compare_at_price_in_cents - p.price_in_cents AS FLOAT) / p.compare_at_price_in_cents
              ELSE 0 END DESC,
              p.featured DESC, p.created_at DESC`
	case "menor_preco":
		return " ORDER BY p.price_in_cents ASC, p.featured DESC, p.created_at DESC"
	case "maior_preco":
		return " ORDER BY p.price_in_cents DESC, p.featured DESC, p.created_at DESC"
	case "novos":
		return " ORDER BY p.created_at DESC, p.featured DESC"
	case "antigos":
		return " ORDER BY p.created_at ASC, p.featured DESC"
	}
	return " ORDER BY p.featured DESC, p.created_at DESC"
}

// ListProducts returns one page of active products with filters and sort.
func ListProducts(ctx context.Context, q DBTX, cq CatalogQuery) ([]ProductSummary, error) {
	var b sqlBuilder
	b.sb.WriteString("SELECT " + productSummaryColumns + " FROM products p WHERE p.active = true")
	b.appendFilters(cq)
	b.sb.WriteString(catalogOrder(cq.Sort))
	fmt.Fprintf(&b.sb, " LIMIT %s OFFSET %s", b.arg(cq.Limit), b.arg(cq.Offset))
	return listSummaries(ctx, q, b.sb.String(), b.args...)
}

// CountProducts counts the products matching cq's filters.
func CountProducts(ctx context.Context, q DBTX, cq CatalogQuery) (int64, error) {
	var b sqlBuilder
	b.sb.WriteString("SELECT COUNT(*) FROM products p WHERE p.active = true")
	b.appendFilters(cq)
	var n int64
	err := q.QueryRow(ctx, b.sb.String(), b.args...).Scan(&n)
	return n, err
}

// ListRelatedProducts lists 4 other active products of the category.
func ListRelatedProducts(ctx context.Context, q DBTX, productID uuid.UUID, category string) ([]ProductSummary, error) {
	return listSummaries(ctx, q, sqlRelatedProducts, category, productID)
}

// ListProductLooks lists "complete the look" products.
func ListProductLooks(ctx context.Context, q DBTX, productID uuid.UUID) ([]ProductSummary, error) {
	return listSummaries(ctx, q, sqlProductLooks, productID)
}

// ListSetMembers lists a set's active member products in order.
func ListSetMembers(ctx context.Context, q DBTX, setID uuid.UUID) ([]ProductSummary, error) {
	return listSummaries(ctx, q, sqlSetMembers, setID)
}

// --- Product detail ---

// ProductImage is a gallery image (URL is a storage key or URL).
type ProductImage struct {
	ID  uuid.UUID `db:"id"`
	URL string    `db:"url"`
	Alt *string   `db:"alt"`
}

// ProductVariant is a sellable variant with live availability.
type ProductVariant struct {
	ID           uuid.UUID `db:"id"`
	SKU          string    `db:"sku"`
	Size         string    `db:"size"`
	Color        string    `db:"color"`
	PriceInCents *int64    `db:"price_in_cents"`
	Available    int32     `db:"available"`
}

// Label is "M · Azul".
func (v ProductVariant) Label() string { return VariantLabel(v.Size, v.Color) }

// ProductDetail is the product page.
type ProductDetail struct {
	ID                    uuid.UUID
	Name                  string
	Slug                  string
	Description           *string
	PriceInCents          int64
	CompareAtPriceInCents *int64
	Category              string
	Images                []ProductImage
	Variants              []ProductVariant
	Colors                []ColorSwatch
}

var (
	sqlProductBySlug = sqlText(`SELECT id, name, slug, description, price_in_cents, compare_at_price_in_cents, category::text
 FROM products WHERE slug = $1 AND active = true`)
	sqlProductImages   = sqlText(`SELECT id, url, alt FROM product_images WHERE product_id = $1 ORDER BY sort_order ASC`)
	sqlProductVariants = sqlText(`SELECT id, sku, size::text AS size, color, price_in_cents,
    COALESCE((SELECT SUM(` + LiveAvailableSQL + `)::int
        FROM inventory i
        INNER JOIN warehouses w ON w.id = i.warehouse_id
        WHERE i.variant_id = product_variants.id AND w.active = true), 0)::int AS available
 FROM product_variants WHERE product_id = $1 ORDER BY size ASC, color ASC`)
	sqlProductColors = sqlText(`SELECT COALESCE(
      (SELECT json_agg(json_build_array(pv.color, pv.color_hex) ORDER BY pv.color ASC)
       FROM (SELECT DISTINCT ON (pv.color) pv.color, pv.color_hex
             FROM product_variants pv
             WHERE pv.product_id = $1
             ORDER BY pv.color ASC, pv.color_hex ASC) pv),
      '[]'::json
    )::text`)
)

// GetProductBySlug loads an active product's page data (nil when missing).
func GetProductBySlug(ctx context.Context, q DBTX, slug string) (*ProductDetail, error) {
	var p ProductDetail
	err := q.QueryRow(ctx, sqlProductBySlug, slug).Scan(&p.ID, &p.Name, &p.Slug, &p.Description, &p.PriceInCents, &p.CompareAtPriceInCents, &p.Category)
	if found, err := optional(err); err != nil || !found {
		return nil, err
	}
	if p.Images, err = collect[ProductImage](q.Query(ctx, sqlProductImages, p.ID)); err != nil {
		return nil, err
	}
	if p.Variants, err = collect[ProductVariant](q.Query(ctx, sqlProductVariants, p.ID)); err != nil {
		return nil, err
	}
	var colors string
	if err := q.QueryRow(ctx, sqlProductColors, p.ID).Scan(&colors); err != nil {
		return nil, err
	}
	p.Colors = parseColors(&colors)
	return &p, nil
}

// --- Sets ---

// SetSummary is a conjunto card / page header.
type SetSummary struct {
	ID                    uuid.UUID `db:"id"`
	Name                  string    `db:"name"`
	Slug                  string    `db:"slug"`
	Description           *string   `db:"description"`
	Active                bool      `db:"active"`
	CoverURL              *string   `db:"cover_url"`
	ProductCount          int64     `db:"product_count"`
	PriceInCents          *int64    `db:"price_in_cents"`
	CompareAtPriceInCents *int64    `db:"compare_at_price_in_cents"`
}

const setSummaryColumns = `s.id, s.name, s.slug, s.description, s.active,
        (SELECT si.url FROM set_images si WHERE si.set_id = s.id ORDER BY si.sort_order ASC LIMIT 1) AS cover_url,
        (SELECT COUNT(*) FROM set_members sm INNER JOIN products p ON p.id = sm.product_id WHERE sm.set_id = s.id AND p.active = true) AS product_count,
        s.price_in_cents, s.compare_at_price_in_cents`

var (
	sqlListSets     = sqlText(`SELECT ` + setSummaryColumns + ` FROM sets s WHERE s.active = true ORDER BY s.name ASC`)
	sqlSetBySlug    = sqlText(`SELECT ` + setSummaryColumns + ` FROM sets s WHERE s.slug = $1 AND s.active = true`)
	sqlSetImages    = sqlText(`SELECT id, url, alt FROM set_images WHERE set_id = $1 ORDER BY sort_order ASC`)
	sqlSetMemberCnt = sqlText(`SELECT COUNT(*) FROM set_members sm
  INNER JOIN products p ON p.id = sm.product_id
  WHERE sm.set_id = $1 AND p.active = true`)
	sqlSetAvailable = sqlText(`SELECT COALESCE(MIN(slot_qty), 0)::bigint FROM (
   SELECT COALESCE(SUM(COALESCE((
     SELECT SUM(` + LiveAvailableSQL + `)
     FROM inventory i
     INNER JOIN warehouses w ON w.id = i.warehouse_id
     WHERE i.variant_id = o.variant_id AND w.active = true
   ), 0)), 0) AS slot_qty
   FROM set_slots s
   LEFT JOIN set_slot_options o ON o.slot_id = s.id
   WHERE s.set_id = $1
   GROUP BY s.id
 ) t`)
	sqlSetPrice = sqlText(`SELECT price_in_cents, compare_at_price_in_cents FROM sets WHERE id = $1`)
)

// ListSets lists active sets by name (header nav uses the first 6).
func ListSets(ctx context.Context, q DBTX) ([]SetSummary, error) {
	return collect[SetSummary](q.Query(ctx, sqlListSets))
}

// GetSetBySlug loads an active set (nil when missing).
func GetSetBySlug(ctx context.Context, q DBTX, slug string) (*SetSummary, error) {
	return collectOne[SetSummary](q.Query(ctx, sqlSetBySlug, slug))
}

// ListSetImages lists a set's images in display order.
func ListSetImages(ctx context.Context, q DBTX, setID uuid.UUID) ([]ProductImage, error) {
	return collect[ProductImage](q.Query(ctx, sqlSetImages, setID))
}

// CountSetMembers counts active member products.
func CountSetMembers(ctx context.Context, q DBTX, setID uuid.UUID) (int64, error) {
	var n int64
	err := q.QueryRow(ctx, sqlSetMemberCnt, setID).Scan(&n)
	return n, err
}

// SetAvailableQuantity is MIN over slots of the summed live availability
// of each slot's option variants (0 without slots).
func SetAvailableQuantity(ctx context.Context, q DBTX, setID uuid.UUID) (int64, error) {
	var n int64
	err := q.QueryRow(ctx, sqlSetAvailable, setID).Scan(&n)
	return n, err
}

// SetPriceSummary is the bundle price snapshot (nil when the set is gone).
func SetPriceSummary(ctx context.Context, q DBTX, setID uuid.UUID) (price, compareAt *int64, found bool, err error) {
	err = q.QueryRow(ctx, sqlSetPrice, setID).Scan(&price, &compareAt)
	found, err = optional(err)
	return price, compareAt, found, err
}

// NavSets are the first 6 active sets for the header "Conjuntos" menu.
func NavSets(ctx context.Context, q DBTX) ([]SetSummary, error) {
	sets, err := ListSets(ctx, q)
	if err != nil {
		return nil, err
	}
	if len(sets) > 6 {
		sets = sets[:6]
	}
	return sets, nil
}

var sqlFilterColors = sqlText(`SELECT DISTINCT ON (pv.color) pv.color, pv.color_hex
 FROM product_variants pv
 INNER JOIN products p ON p.id = pv.product_id
 WHERE p.active = true
   AND ($1::text IS NULL OR p.category::text = $1::text)
 ORDER BY pv.color ASC, pv.color_hex ASC NULLS LAST
 LIMIT 30`)

// ListFilterColors lists distinct colors of active products (filter
// panel), optionally narrowed to a category ("" = all).
func ListFilterColors(ctx context.Context, q DBTX, category string) ([]ColorSwatch, error) {
	rows, err := q.Query(ctx, sqlFilterColors, nonEmpty(category))
	if err != nil {
		return nil, err
	}
	defer rows.Close()
	out := []ColorSwatch{}
	for rows.Next() {
		var c ColorSwatch
		if err := rows.Scan(&c.Name, &c.Hex); err != nil {
			return nil, err
		}
		out = append(out, c)
	}
	return out, rows.Err()
}

// --- Home ---

// HomeSections are the home page product rows.
type HomeSections struct {
	Lancamentos  []ProductSummary // 8 newest
	MaisVendidos []ProductSummary // 8 best sellers
	Liquida      []ProductSummary // 4 biggest discounts
	HasBanners   bool
}

var sqlHasBanners = sqlText(`SELECT EXISTS(SELECT 1 FROM home_banners WHERE active = true)`)

// LoadHomeSections loads the home rows (resolve images before rendering).
func LoadHomeSections(ctx context.Context, q DBTX) (HomeSections, error) {
	var h HomeSections
	if err := q.QueryRow(ctx, sqlHasBanners).Scan(&h.HasBanners); err != nil {
		return h, err
	}
	var err error
	base := CatalogQuery{IncludeOutOfStock: true, Limit: 8}
	nq := base
	nq.Sort = "novos"
	if h.Lancamentos, err = ListProducts(ctx, q, nq); err != nil {
		return h, err
	}
	bq := base
	bq.Sort = "mais_vendidos"
	if h.MaisVendidos, err = ListProducts(ctx, q, bq); err != nil {
		return h, err
	}
	sq := base
	sq.Sort, sq.Limit, sq.Filters.SaleOnly = "maior_desconto", 4, true
	h.Liquida, err = ListProducts(ctx, q, sq)
	return h, err
}
