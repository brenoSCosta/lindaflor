package store

import (
	"context"
	"encoding/json"
	"fmt"
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/db"
	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
)

// Storefront catalog queries (src/app/store/queries.rs), all in
// db/queries/store_catalog.sql. The faceted listing is one static query:
// optional filters are sqlc.narg / flag / empty-array guards and the sort
// is a list of CASE keys selected by the sort name.

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

// productSummaryRow is one product card row (product_summaries view); the
// other card queries return the same columns.
type productSummaryRow = sqlc.StoreListProductsRow

func parseColors(raw string) []ColorSwatch {
	out := []ColorSwatch{}
	var pairs [][2]*string
	if json.Unmarshal([]byte(raw), &pairs) != nil {
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

func toSummary(r productSummaryRow) ProductSummary {
	return ProductSummary{
		ID: r.ID, Name: r.Name, Slug: r.Slug, PriceInCents: r.PriceInCents,
		CompareAtPriceInCents: r.CompareAtPriceInCents, Category: r.Category,
		ImageURL: r.ImageUrl, SecondaryImageURL: r.SecondaryImageUrl,
		AvailableTotal: r.AvailableTotal, Colors: parseColors(r.ColorsJson),
	}
}

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

// catalogFilters maps cq's filters onto the query parameters ("" category
// / search = off; nil slices become empty arrays).
func catalogFilters(cq CatalogQuery) sqlc.StoreCountProductsParams {
	p := sqlc.StoreCountProductsParams{
		Category:          nonEmpty(cq.Category),
		FeaturedOnly:      cq.FeaturedOnly,
		IncludeOutOfStock: cq.IncludeOutOfStock,
		PriceMin:          cq.Filters.PriceMinCents,
		PriceMax:          cq.Filters.PriceMaxCents,
		Colors:            append([]string{}, cq.Filters.Colors...),
		Sizes:             append([]string{}, cq.Filters.Sizes...),
		SaleOnly:          cq.Filters.SaleOnly,
	}
	if cq.Search != "" {
		p.Search = ptr(db.LikeContains(strings.TrimSpace(cq.Search)))
	}
	return p
}

// ListProducts returns one page of active products with filters and sort
// (relevantes, mais_vendidos, maior_desconto, menor_preco, maior_preco,
// novos, antigos; anything else sorts like relevantes).
func ListProducts(ctx context.Context, q DBTX, cq CatalogQuery) ([]ProductSummary, error) {
	f := catalogFilters(cq)
	rows, err := queries(q).StoreListProducts(ctx, sqlc.StoreListProductsParams{
		Category: f.Category, Search: f.Search, FeaturedOnly: f.FeaturedOnly,
		IncludeOutOfStock: f.IncludeOutOfStock, PriceMin: f.PriceMin, PriceMax: f.PriceMax,
		Colors: f.Colors, Sizes: f.Sizes, SaleOnly: f.SaleOnly,
		Sort: cq.Sort, PageSize: cq.Limit, PageOffset: cq.Offset,
	})
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, toSummary), nil
}

// CountProducts counts the products matching cq's filters.
func CountProducts(ctx context.Context, q DBTX, cq CatalogQuery) (int64, error) {
	return queries(q).StoreCountProducts(ctx, catalogFilters(cq))
}

// ListRelatedProducts lists 4 other active products of the category.
func ListRelatedProducts(ctx context.Context, q DBTX, productID uuid.UUID, category string) ([]ProductSummary, error) {
	rows, err := queries(q).StoreRelatedProducts(ctx, sqlc.StoreRelatedProductsParams{Category: category, ProductID: productID})
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, func(r sqlc.StoreRelatedProductsRow) ProductSummary { return toSummary(productSummaryRow(r)) }), nil
}

// ListProductLooks lists "complete the look" products.
func ListProductLooks(ctx context.Context, q DBTX, productID uuid.UUID) ([]ProductSummary, error) {
	rows, err := queries(q).StoreProductLooks(ctx, productID)
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, func(r sqlc.StoreProductLooksRow) ProductSummary { return toSummary(productSummaryRow(r)) }), nil
}

// ListSetMembers lists a set's active member products in order.
func ListSetMembers(ctx context.Context, q DBTX, setID uuid.UUID) ([]ProductSummary, error) {
	rows, err := queries(q).StoreSetMemberProducts(ctx, setID)
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, func(r sqlc.StoreSetMemberProductsRow) ProductSummary { return toSummary(productSummaryRow(r)) }), nil
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

// GetProductBySlug loads an active product's page data (nil when missing).
func GetProductBySlug(ctx context.Context, q DBTX, slug string) (*ProductDetail, error) {
	qs := queries(q)
	r, found, err := noRows(qs.StoreProductBySlug(ctx, slug))
	if err != nil || !found {
		return nil, err
	}
	p := ProductDetail{
		ID: r.ID, Name: r.Name, Slug: r.Slug, Description: r.Description, PriceInCents: r.PriceInCents,
		CompareAtPriceInCents: r.CompareAtPriceInCents, Category: r.Category,
	}
	images, err := qs.StoreProductImages(ctx, p.ID)
	if err != nil {
		return nil, err
	}
	p.Images = mapSlice(images, func(i sqlc.StoreProductImagesRow) ProductImage {
		return ProductImage{ID: i.ID, URL: i.Url, Alt: i.Alt}
	})
	variants, err := qs.StoreProductVariants(ctx, p.ID)
	if err != nil {
		return nil, err
	}
	p.Variants = mapSlice(variants, func(v sqlc.StoreProductVariantsRow) ProductVariant {
		return ProductVariant{ID: v.ID, SKU: v.Sku, Size: v.Size, Color: v.Color, PriceInCents: v.PriceInCents, Available: v.Available}
	})
	colors, err := qs.StoreProductColors(ctx, p.ID)
	if err != nil {
		return nil, err
	}
	p.Colors = parseColors(colors)
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

func toSetSummary(r sqlc.StoreListSetsRow) SetSummary {
	return SetSummary{
		ID: r.ID, Name: r.Name, Slug: r.Slug, Description: r.Description, Active: r.Active,
		CoverURL: r.CoverUrl, ProductCount: r.ProductCount, PriceInCents: r.PriceInCents,
		CompareAtPriceInCents: r.CompareAtPriceInCents,
	}
}

// ListSets lists active sets by name (header nav uses the first 6).
func ListSets(ctx context.Context, q DBTX) ([]SetSummary, error) {
	rows, err := queries(q).StoreListSets(ctx)
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, toSetSummary), nil
}

// GetSetBySlug loads an active set (nil when missing).
func GetSetBySlug(ctx context.Context, q DBTX, slug string) (*SetSummary, error) {
	r, found, err := noRows(queries(q).StoreSetBySlug(ctx, slug))
	if err != nil || !found {
		return nil, err
	}
	return ptr(toSetSummary(sqlc.StoreListSetsRow(r))), nil
}

// ListSetImages lists a set's images in display order.
func ListSetImages(ctx context.Context, q DBTX, setID uuid.UUID) ([]ProductImage, error) {
	rows, err := queries(q).StoreSetImages(ctx, setID)
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, func(i sqlc.StoreSetImagesRow) ProductImage {
		return ProductImage{ID: i.ID, URL: i.Url, Alt: i.Alt}
	}), nil
}

// CountSetMembers counts active member products.
func CountSetMembers(ctx context.Context, q DBTX, setID uuid.UUID) (int64, error) {
	return queries(q).StoreSetMemberCount(ctx, setID)
}

// SetAvailableQuantity is MIN over slots of the summed live availability
// of each slot's option variants (0 without slots).
func SetAvailableQuantity(ctx context.Context, q DBTX, setID uuid.UUID) (int64, error) {
	return queries(q).StoreSetAvailable(ctx, setID)
}

// SetPriceSummary is the bundle price snapshot (nil when the set is gone).
func SetPriceSummary(ctx context.Context, q DBTX, setID uuid.UUID) (price, compareAt *int64, found bool, err error) {
	r, found, err := noRows(queries(q).StoreSetPrice(ctx, setID))
	return r.PriceInCents, r.CompareAtPriceInCents, found, err
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

// ListFilterColors lists distinct colors of active products (filter
// panel), optionally narrowed to a category ("" = all).
func ListFilterColors(ctx context.Context, q DBTX, category string) ([]ColorSwatch, error) {
	rows, err := queries(q).StoreFilterColors(ctx, nonEmpty(category))
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, func(r sqlc.StoreFilterColorsRow) ColorSwatch { return ColorSwatch{Name: r.Color, Hex: r.ColorHex} }), nil
}

// --- Home ---

// HomeSections are the home page product rows.
type HomeSections struct {
	Lancamentos  []ProductSummary // 8 newest
	MaisVendidos []ProductSummary // 8 best sellers
	Liquida      []ProductSummary // 4 biggest discounts
	HasBanners   bool
}

// LoadHomeSections loads the home rows (resolve images before rendering).
func LoadHomeSections(ctx context.Context, q DBTX) (HomeSections, error) {
	var h HomeSections
	var err error
	if h.HasBanners, err = queries(q).StoreHasBanners(ctx); err != nil {
		return h, err
	}
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
