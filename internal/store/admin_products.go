package store

import (
	"context"
	"errors"
	"math"
	"strconv"
	"strings"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgconn"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/db"
	"github.com/wellington-vell/lindaflor/internal/storage"
)

// Admin products (src/app/admin/produtos*.rs): directory, create, edit
// (variants by SKU, images, looks). The UI parses the multipart form into
// ProductForm and renders; every rule and message lives here.

const adminProductQueryMax = 80

// ClampAdminQuery trims and caps a directory search.
func ClampAdminQuery(raw string, maxChars int) string { return truncateRunes(strings.TrimSpace(raw), maxChars) }

// ClampProductActive is "true" | "false" | "all".
func ClampProductActive(raw string) string {
	switch v := strings.TrimSpace(raw); v {
	case "true", "false":
		return v
	}
	return "all"
}

// ProductActiveFilters are the directory filter buttons.
var ProductActiveFilters = [][2]string{{"all", "Todos"}, {"true", "Ativos"}, {"false", "Inativos"}}

// AdminProductRow is a directory row.
type AdminProductRow struct {
	ID             uuid.UUID `db:"id"`
	Name           string    `db:"name"`
	Slug           string    `db:"slug"`
	Category       string    `db:"category"`
	PriceInCents   int64     `db:"price_in_cents"`
	Active         bool      `db:"active"`
	VariantCount   int64     `db:"variant_count"`
	AvailableTotal int64     `db:"available_total"`
}

// CategoryLabel is the admin category label.
func (r AdminProductRow) CategoryLabel() string { return AdminCategoryLabel(r.Category) }

var (
	sqlAdminProductCount = sqlText(`SELECT COUNT(*) FROM products p WHERE ($1::text = '' OR p.name ILIKE '%' || $1::text || '%' ESCAPE '\') AND ($2::text = 'all' OR p.active = ($2::text = 'true'))`)
	sqlAdminProductList  = sqlText(`SELECT p.id, p.name, p.slug, p.category::text AS category, p.price_in_cents, p.active,
        COUNT(DISTINCT pv.id) AS variant_count,
        COALESCE(SUM(GREATEST(i.quantity - i.reserved, 0)) FILTER (WHERE w.active = true), 0)::bigint AS available_total
 FROM products p
 LEFT JOIN product_variants pv ON pv.product_id = p.id
 LEFT JOIN inventory i ON i.variant_id = pv.id
 LEFT JOIN warehouses w ON w.id = i.warehouse_id
 WHERE ($1::text = '' OR p.name ILIKE '%' || $1::text || '%' ESCAPE '\')
   AND ($2::text = 'all' OR p.active = ($2::text = 'true'))
 GROUP BY p.id
 ORDER BY p.name
 LIMIT $3 OFFSET $4`)
)

// AdminProductDirectory is /admin/produtos (and its /lista fragment).
type AdminProductDirectory struct {
	Query    string
	Active   string
	Products []AdminProductRow
	Pager    Pagination
}

// LoadAdminProducts searches products by name (literal match) and active
// flag, 20 per page.
func LoadAdminProducts(ctx context.Context, q DBTX, query, active string, page int64) (AdminProductDirectory, error) {
	out := AdminProductDirectory{Query: ClampAdminQuery(query, adminProductQueryMax), Active: ClampProductActive(active)}
	like := db.EscapeLike(out.Query)
	var total int64
	if err := q.QueryRow(ctx, sqlAdminProductCount, like, out.Active).Scan(&total); err != nil {
		return out, err
	}
	out.Pager = Paginate(page, 20, total)
	var err error
	out.Products, err = collect[AdminProductRow](q.Query(ctx, sqlAdminProductList, like, out.Active, out.Pager.PageSize, out.Pager.Offset()))
	return out, err
}

// --- Validation (shared by create/edit) ---

// ValidateProductName trims; 1..=200 chars.
func ValidateProductName(raw string) (string, error) {
	n := strings.TrimSpace(raw)
	if n == "" {
		return "", userErr("Informe o nome do produto.")
	}
	if runeLen(n) > 200 {
		return "", userErr("O nome deve ter no máximo 200 caracteres.")
	}
	return n, nil
}

func validateSlug(raw, emptyMsg string) (string, error) {
	s := strings.ToLower(strings.TrimSpace(raw))
	if s == "" {
		return "", userErr(emptyMsg)
	}
	if len(s) > 120 {
		return "", userErr("O slug deve ter no máximo 120 caracteres.")
	}
	valid := !strings.HasPrefix(s, "-") && !strings.HasSuffix(s, "-") && !strings.Contains(s, "--")
	for _, c := range s {
		if !(c >= 'a' && c <= 'z' || c >= '0' && c <= '9' || c == '-') {
			valid = false
		}
	}
	if !valid {
		return "", userErr("O slug deve conter apenas letras minúsculas, números e hífens.")
	}
	return s, nil
}

// ValidateProductSlug lowercases and checks [a-z0-9-], no edge/double dash.
func ValidateProductSlug(raw string) (string, error) { return validateSlug(raw, "Informe o slug do produto.") }

// ValidateCategory checks the product_category enum.
func ValidateCategory(raw string) (string, error) {
	c := strings.TrimSpace(raw)
	if contains(ValidCategories, c) {
		return c, nil
	}
	return "", userErr("Categoria inválida.")
}

// ValidateSize lowercases and checks the product_size enum.
func ValidateSize(raw string) (string, error) {
	s := strings.ToLower(strings.TrimSpace(raw))
	if contains(ValidSizes, s) {
		return s, nil
	}
	return "", userErr("Tamanho inválido.")
}

// ParsePriceCents parses "199.90", "199,90" or "199" to cents (> 0,
// ≤ 9.999.999). errMsg is the invalid-value message.
func parsePriceCents(raw, emptyMsg, errMsg string, allowZero bool) (int64, bool, error) {
	t := strings.TrimSpace(raw)
	if t == "" {
		if emptyMsg == "" {
			return 0, false, nil
		}
		return 0, false, userErr(emptyMsg)
	}
	v, ok := parseDecimal(t)
	if !ok || v > 9_999_999 || v < 0 || (!allowZero && v <= 0) {
		return 0, false, userErr(errMsg)
	}
	return int64(math.Round(v * 100)), true, nil
}

// ParseProductPrice is the required product price.
func ParseProductPrice(raw string) (int64, error) {
	c, _, err := parsePriceCents(raw, "Informe o preço do produto.", "Preço inválido. Use 199.90.", false)
	return c, err
}

// ParseCompareAtPrice is the optional "de" price (must exceed the price).
func ParseCompareAtPrice(raw string, priceCents int64) (*int64, error) {
	c, ok, err := parsePriceCents(raw, "", "Preço original inválido. Use 199.90.", false)
	if err != nil || !ok {
		return nil, err
	}
	if c <= priceCents {
		return nil, userErr("Preço original deve ser maior que o preço do produto.")
	}
	return &c, nil
}

// ValidateColorHex is "" or #RRGGBB (uppercased).
func ValidateColorHex(raw string) (*string, error) {
	t := strings.TrimSpace(raw)
	if t == "" {
		return nil, nil
	}
	if len(t) != 7 || t[0] != '#' {
		return nil, userErr("Cor (hex) inválida. Use o formato #RRGGBB.")
	}
	for i := 1; i < 7; i++ {
		if !isHex(t[i]) {
			return nil, userErr("Cor (hex) inválida. Use o formato #RRGGBB.")
		}
	}
	u := strings.ToUpper(t)
	return &u, nil
}

// ParseStockQuantity is 0..=1.000.000.
func ParseStockQuantity(raw string) (int32, error) {
	n, err := strconv.ParseInt(strings.TrimSpace(raw), 10, 32)
	if err != nil || n < 0 || n > 1_000_000 {
		return 0, userErr("Estoque inválido.")
	}
	return int32(n), nil
}

func optionalDescription(raw string) *string { return nonEmpty(strings.TrimSpace(raw)) }

var (
	sqlDefaultWarehouse = sqlText(`SELECT id FROM warehouses WHERE is_default = true ORDER BY created_at LIMIT 1`)
	sqlAnyWarehouse     = sqlText(`SELECT id FROM warehouses ORDER BY created_at LIMIT 1`)
	sqlCreateWarehouse  = sqlText(`INSERT INTO warehouses (id, code, name, is_default, active, created_at)
 VALUES ($1, 'PRINCIPAL', 'Depósito Principal', true, true, now())`)
)

// EnsureDefaultWarehouse returns the default warehouse, else the first
// one, creating PRINCIPAL when none exists.
func EnsureDefaultWarehouse(ctx context.Context, q DBTX) (uuid.UUID, error) {
	var id uuid.UUID
	for _, s := range []string{sqlDefaultWarehouse, sqlAnyWarehouse} {
		err := q.QueryRow(ctx, s).Scan(&id)
		if found, err := optional(err); err != nil {
			return uuid.Nil, err
		} else if found {
			return id, nil
		}
	}
	id = uuid.Must(uuid.NewV7())
	_, err := q.Exec(ctx, sqlCreateWarehouse, id)
	return id, err
}

// productDBError maps constraint violations to admin messages.
func productDBError(err error) error {
	var pg *pgconn.PgError
	if errors.As(err, &pg) {
		switch {
		case pg.ConstraintName == "product_variants_sku_uidx":
			return userErr("Este SKU já está em uso por outra variante.")
		case pg.ConstraintName == "products_slug_uidx":
			return userErr("Este slug já está em uso por outro produto.")
		case pg.Code == "23503":
			return userErr("Não é possível remover variante com pedidos vinculados.")
		}
	}
	return userErr("Não foi possível salvar. Tente novamente.")
}

// VariantForm is one variant row of the product form.
type VariantForm struct {
	SKU      string
	Size     string
	Color    string
	ColorHex string
	// Quantity is the initial stock (create form).
	Quantity string
	// Threshold is low_stock_threshold (edit form).
	Threshold string
}

// ProductForm is the create/edit multipart form.
type ProductForm struct {
	ProductID      string // edit only
	Name           string
	Slug           string
	Description    string
	Price          string
	CompareAtPrice string
	Category       string
	Featured       bool
	Active         bool // edit only (create is always active)
	Variants       []VariantForm
	DeleteImageIDs []string
	LookProductIDs []string
	Images         []Upload
}

// ZipVariantForms builds the variant rows from the repeated form fields
// variant_sku[], variant_size[], variant_color[], variant_color_hex[] and
// variant_quantity[] (create) or variant_threshold[] (edit). Mismatched
// lengths give the Rust messages.
func ZipVariantForms(skus, sizes, colors, hexes, quantities, thresholds []string, edit bool) ([]VariantForm, error) {
	counts := quantities
	msg := "Informe ao menos uma variante completa."
	if edit {
		counts = thresholds
		msg = "Variantes inconsistentes. Confira os campos."
	}
	if (!edit && len(skus) == 0) || len(skus) != len(sizes) || len(skus) != len(colors) || len(skus) != len(counts) {
		return nil, userErr(msg)
	}
	out := make([]VariantForm, len(skus))
	for i := range skus {
		out[i] = VariantForm{SKU: skus[i], Size: sizes[i], Color: colors[i]}
		if i < len(hexes) {
			out[i].ColorHex = hexes[i]
		}
		if edit {
			out[i].Threshold = thresholds[i]
		} else {
			out[i].Quantity = quantities[i]
		}
	}
	return out, nil
}

type validVariant struct {
	sku, size, color string
	hex              *string
	quantity         int32
	threshold        int32
}

type validProduct struct {
	name, slug, category string
	description          *string
	price                int64
	compare              *int64
}

func validateProductBase(f ProductForm) (validProduct, error) {
	var v validProduct
	var err error
	if v.name, err = ValidateProductName(f.Name); err != nil {
		return v, err
	}
	if v.slug, err = ValidateProductSlug(f.Slug); err != nil {
		return v, err
	}
	if v.price, err = ParseProductPrice(f.Price); err != nil {
		return v, err
	}
	if v.compare, err = ParseCompareAtPrice(f.CompareAtPrice, v.price); err != nil {
		return v, err
	}
	if v.category, err = ValidateCategory(f.Category); err != nil {
		return v, err
	}
	v.description = optionalDescription(f.Description)
	return v, nil
}

func validateUploads(images []Upload) error {
	for _, up := range images {
		if _, err := ValidateImage(up.Bytes, up.ContentType); err != nil {
			return err
		}
	}
	return nil
}

var (
	sqlProductIDBySlug    = sqlText(`SELECT id FROM products WHERE slug = $1`)
	sqlProductSlugTaken   = sqlText(`SELECT id FROM products WHERE slug = $1 AND id != $2`)
	sqlInsertProduct      = sqlText(`INSERT INTO products (id, name, slug, description, price_in_cents, compare_at_price_in_cents, category, active, featured, created_at, updated_at)
 VALUES ($1, $2, $3, $4, $5, $6, $7::text::product_category, true, $8, now(), now())`)
	sqlInsertVariant = sqlText(`INSERT INTO product_variants (id, product_id, sku, size, color, color_hex, price_in_cents, low_stock_threshold, created_at, updated_at)
 VALUES ($1, $2, $3, $4::text::product_size, $5, $6, NULL, $7, now(), now())`)
	sqlInsertInventory = sqlText(`INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved, updated_at)
 VALUES ($1, $2, $3, $4, 0, now())`)
)

// CreateProductResult is the outcome of CreateProduct: on success the UI
// redirects to the edit page with Message ("Produto criado com sucesso."
// or the partial-image warning as an error toast when ImageFailed).
type CreateProductResult struct {
	ID          uuid.UUID
	ImageFailed bool
}

// Message is the toast text.
func (r CreateProductResult) Message() string {
	if r.ImageFailed {
		return "Produto criado, mas alguma imagem falhou. Tente adicioná-la na edição."
	}
	return "Produto criado com sucesso."
}

// CreateProduct validates the form and creates the product, its variants
// and their default-warehouse stock in one transaction, then uploads the
// images. *UserError for every validation/constraint failure.
func CreateProduct(ctx context.Context, pool *pgxpool.Pool, st storage.Store, f ProductForm) (CreateProductResult, error) {
	v, err := validateProductBase(f)
	if err != nil {
		return CreateProductResult{}, err
	}
	if len(f.Variants) == 0 {
		return CreateProductResult{}, userErr("Informe ao menos uma variante completa.")
	}
	variants := make([]validVariant, 0, len(f.Variants))
	for _, vf := range f.Variants {
		sku := strings.TrimSpace(vf.SKU)
		if sku == "" {
			return CreateProductResult{}, userErr("Toda variante precisa de um SKU.")
		}
		size, err := ValidateSize(vf.Size)
		if err != nil {
			return CreateProductResult{}, err
		}
		color := strings.TrimSpace(vf.Color)
		if color == "" {
			return CreateProductResult{}, userErr("Toda variante precisa de uma cor.")
		}
		hex, err := ValidateColorHex(vf.ColorHex)
		if err != nil {
			return CreateProductResult{}, err
		}
		qty, err := ParseStockQuantity(vf.Quantity)
		if err != nil {
			return CreateProductResult{}, err
		}
		variants = append(variants, validVariant{sku: sku, size: size, color: color, hex: hex, quantity: qty, threshold: 5})
	}
	if len(f.Images) > MaxImagesPerProduct {
		return CreateProductResult{}, userErr(MsgProductImageCount)
	}
	if err := validateUploads(f.Images); err != nil {
		return CreateProductResult{}, err
	}
	var existing uuid.UUID
	err = pool.QueryRow(ctx, sqlProductIDBySlug, v.slug).Scan(&existing)
	if found, err := optional(err); err != nil {
		return CreateProductResult{}, err
	} else if found {
		return CreateProductResult{}, userErr("Este slug já está em uso por outro produto.")
	}
	warehouse, err := EnsureDefaultWarehouse(ctx, pool)
	if err != nil {
		return CreateProductResult{}, userErr("Não foi possível salvar. Tente novamente.")
	}
	productID := uuid.Must(uuid.NewV7())
	err = withTx(ctx, pool, func(tx pgx.Tx) error {
		if _, err := tx.Exec(ctx, sqlInsertProduct, productID, v.name, v.slug, v.description, v.price, v.compare, v.category, f.Featured); err != nil {
			return productDBError(err)
		}
		for _, vv := range variants {
			variantID := uuid.Must(uuid.NewV7())
			if _, err := tx.Exec(ctx, sqlInsertVariant, variantID, productID, vv.sku, vv.size, vv.color, vv.hex, vv.threshold); err != nil {
				return productDBError(err)
			}
			if _, err := tx.Exec(ctx, sqlInsertInventory, uuid.Must(uuid.NewV7()), variantID, warehouse, vv.quantity); err != nil {
				return productDBError(err)
			}
		}
		return nil
	})
	if err != nil {
		return CreateProductResult{}, err
	}
	res := CreateProductResult{ID: productID}
	for i, up := range f.Images {
		if _, err := SaveProductImage(ctx, pool, st, productID, up, int32(i)); err != nil { //nolint:gosec // ≤ 8 images
			res.ImageFailed = true
			break
		}
	}
	return res, nil
}

// --- Edit ---

// AdminProductEdit is the edit page.
type AdminProductEdit struct {
	ID                    uuid.UUID
	Name                  string
	Slug                  string
	Description           *string
	PriceInCents          int64
	CompareAtPriceInCents *int64
	Category              string
	Featured              bool
	Active                bool
	Variants              []AdminVariantRow
	Images                []AdminImage
	OtherProducts         []LookOption
	LookIDs               map[uuid.UUID]bool
}

// PriceInput is "199.90".
func (p AdminProductEdit) PriceInput() string { return FormatPriceInput(p.PriceInCents) }

// CompareAtInput is "" or "249.90".
func (p AdminProductEdit) CompareAtInput() string {
	if p.CompareAtPriceInCents == nil {
		return ""
	}
	return FormatPriceInput(*p.CompareAtPriceInCents)
}

// AdminVariantRow is an editable variant.
type AdminVariantRow struct {
	ID                uuid.UUID `db:"id"`
	SKU               string    `db:"sku"`
	Size              string    `db:"size"`
	Color             string    `db:"color"`
	ColorHex          *string   `db:"color_hex"`
	PriceInCents      *int64    `db:"price_in_cents"`
	LowStockThreshold int32     `db:"low_stock_threshold"`
}

// AdminImage is a gallery image with its resolved URL ("" when it can't
// be loaded).
type AdminImage struct {
	ID  uuid.UUID
	URL string
}

// LookOption is a "complete the look" candidate.
type LookOption struct {
	ID       uuid.UUID `db:"id"`
	Name     string    `db:"name"`
	Category string    `db:"category"`
}

var (
	sqlAdminProduct = sqlText(`SELECT id, name, slug, description, price_in_cents, compare_at_price_in_cents, category::text, featured, active FROM products WHERE id = $1`)
	sqlAdminVariants = sqlText(`SELECT id, sku, size::text AS size, color, color_hex, price_in_cents, low_stock_threshold FROM product_variants WHERE product_id = $1 ORDER BY created_at`)
	sqlAdminImages   = sqlText(`SELECT id, url FROM product_images WHERE product_id = $1 ORDER BY sort_order ASC`)
	sqlLookOptions   = sqlText(`SELECT id, name, category::text AS category FROM products WHERE active = true AND id != $1 ORDER BY name`)
	sqlExistingLooks = sqlText(`SELECT related_product_id FROM product_looks WHERE product_id = $1`)
)

// LoadAdminProduct loads the edit page (nil when missing).
func LoadAdminProduct(ctx context.Context, q DBTX, st storage.Store, id uuid.UUID) (*AdminProductEdit, error) {
	var p AdminProductEdit
	err := q.QueryRow(ctx, sqlAdminProduct, id).Scan(&p.ID, &p.Name, &p.Slug, &p.Description, &p.PriceInCents,
		&p.CompareAtPriceInCents, &p.Category, &p.Featured, &p.Active)
	if found, err := optional(err); err != nil || !found {
		return nil, err
	}
	if p.Variants, err = collect[AdminVariantRow](q.Query(ctx, sqlAdminVariants, id)); err != nil {
		return nil, err
	}
	type imgRow struct {
		ID  uuid.UUID `db:"id"`
		URL string    `db:"url"`
	}
	imgs, err := collect[imgRow](q.Query(ctx, sqlAdminImages, id))
	if err != nil {
		return nil, err
	}
	for _, r := range imgs {
		p.Images = append(p.Images, AdminImage{ID: r.ID, URL: ResolveStorageURL(ctx, st, &r.URL)})
	}
	if p.OtherProducts, err = collect[LookOption](q.Query(ctx, sqlLookOptions, id)); err != nil {
		return nil, err
	}
	looks, err := scalars[uuid.UUID](q.Query(ctx, sqlExistingLooks, id))
	if err != nil {
		return nil, err
	}
	p.LookIDs = map[uuid.UUID]bool{}
	for _, l := range looks {
		p.LookIDs[l] = true
	}
	return &p, nil
}

var (
	sqlProductExists  = sqlText(`SELECT id FROM products WHERE id = $1`)
	sqlProductImgCount = sqlText(`SELECT COUNT(*) FROM product_images WHERE product_id = $1`)
	sqlUpdateProduct  = sqlText(`UPDATE products SET name = $2, slug = $3, description = $4, price_in_cents = $5, compare_at_price_in_cents = $6,
 category = $7::text::product_category, featured = $8, active = $9, updated_at = now() WHERE id = $1`)
	sqlExistingVariantSKUs = sqlText(`SELECT id, sku FROM product_variants WHERE product_id = $1`)
	sqlUpdateVariant       = sqlText(`UPDATE product_variants SET size = $2::text::product_size, color = $3, color_hex = $4,
 low_stock_threshold = $5, updated_at = now() WHERE id = $1`)
	sqlInsertEmptyInventory = sqlText(`INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved, updated_at)
 VALUES ($1, $2, $3, 0, 0, now())
 ON CONFLICT (variant_id, warehouse_id) DO NOTHING`)
	sqlDeleteVariant   = sqlText(`DELETE FROM product_variants WHERE id = $1`)
	sqlDeleteLooks     = sqlText(`DELETE FROM product_looks WHERE product_id = $1`)
	sqlInsertLooks     = sqlText(`INSERT INTO product_looks (product_id, related_product_id, sort_order)
 SELECT $1, p.id, (ids.ord - 1)::int
 FROM unnest($2::uuid[]) WITH ORDINALITY AS ids(id, ord)
 INNER JOIN products p ON p.id = ids.id
 WHERE p.id <> $1
 ON CONFLICT DO NOTHING`)
	sqlImageOwner     = sqlText(`SELECT product_id FROM product_images WHERE id = $1`)
	sqlNextImageOrder = sqlText(`SELECT (COALESCE(MAX(sort_order), -1) + 1)::int FROM product_images WHERE product_id = $1`)
)

// ErrProductNotFound: the edited product does not exist (redirect to the list).
var ErrProductNotFound = errors.New("Produto não encontrado.")

func parseUUIDs(raw []string) []uuid.UUID {
	out := []uuid.UUID{}
	for _, r := range raw {
		if id, err := uuid.Parse(strings.TrimSpace(r)); err == nil {
			out = append(out, id)
		}
	}
	return out
}

// UpdateProduct saves the edit form: fields, variants reconciled by SKU
// (new SKUs get default-warehouse stock rows, missing SKUs are deleted),
// looks, then image deletions and uploads. On success the toast is
// "Alterações salvas com sucesso."; a *UserError (including the
// post-commit image failure "Alterações salvas, mas alguma imagem falhou.
// Tente novamente.") is shown as an error toast.
func UpdateProduct(ctx context.Context, pool *pgxpool.Pool, st storage.Store, productID uuid.UUID, f ProductForm) error {
	var exists uuid.UUID
	err := pool.QueryRow(ctx, sqlProductExists, productID).Scan(&exists)
	if found, err := optional(err); err != nil {
		return err
	} else if !found {
		return ErrProductNotFound
	}
	v, err := validateProductBase(f)
	if err != nil {
		return err
	}
	var variants []validVariant
	for _, vf := range f.Variants {
		sku := strings.TrimSpace(vf.SKU)
		if sku == "" {
			continue
		}
		size, err := ValidateSize(vf.Size)
		if err != nil {
			return err
		}
		color := strings.TrimSpace(vf.Color)
		if color == "" {
			return userErr("Toda variante precisa de uma cor.")
		}
		hex, err := ValidateColorHex(vf.ColorHex)
		if err != nil {
			return err
		}
		t, perr := strconv.ParseInt(strings.TrimSpace(vf.Threshold), 10, 32)
		if perr != nil || t < 0 || t > 1_000_000 {
			return userErr("Limite de estoque baixo inválido.")
		}
		variants = append(variants, validVariant{sku: sku, size: size, color: color, hex: hex, threshold: int32(t)})
	}
	deleteIDs := parseUUIDs(f.DeleteImageIDs)
	var current int64
	if err := pool.QueryRow(ctx, sqlProductImgCount, productID).Scan(&current); err != nil {
		return err
	}
	remaining := current - int64(len(deleteIDs)) + int64(len(f.Images))
	if remaining < 0 || remaining > MaxImagesPerProduct {
		return userErr(MsgProductImageCount)
	}
	if err := validateUploads(f.Images); err != nil {
		return err
	}
	var taken uuid.UUID
	err = pool.QueryRow(ctx, sqlProductSlugTaken, v.slug, productID).Scan(&taken)
	if found, err := optional(err); err != nil {
		return err
	} else if found {
		return userErr("Este slug já está em uso por outro produto.")
	}
	warehouse, err := EnsureDefaultWarehouse(ctx, pool)
	if err != nil {
		return userErr("Não foi possível salvar as variantes.")
	}
	err = withTx(ctx, pool, func(tx pgx.Tx) error {
		if _, err := tx.Exec(ctx, sqlUpdateProduct, productID, v.name, v.slug, v.description, v.price, v.compare, v.category, f.Featured, f.Active); err != nil {
			return productDBError(err)
		}
		if err := syncVariants(ctx, tx, productID, warehouse, variants); err != nil {
			return err
		}
		if _, err := tx.Exec(ctx, sqlDeleteLooks, productID); err != nil {
			return err
		}
		_, err := tx.Exec(ctx, sqlInsertLooks, productID, parseUUIDs(f.LookProductIDs))
		return err
	})
	if err != nil {
		return err
	}
	for _, imageID := range deleteIDs {
		var owner uuid.UUID
		err := pool.QueryRow(ctx, sqlImageOwner, imageID).Scan(&owner)
		if found, err := optional(err); err != nil {
			return err
		} else if found && owner == productID {
			if err := DeleteProductImage(ctx, pool, st, imageID); err != nil {
				return err
			}
		}
	}
	var next int32
	if err := pool.QueryRow(ctx, sqlNextImageOrder, productID).Scan(&next); err != nil {
		return err
	}
	for i, up := range f.Images {
		if _, err := SaveProductImage(ctx, pool, st, productID, up, next+int32(i)); err != nil { //nolint:gosec // ≤ 8 images
			return userErr("Alterações salvas, mas alguma imagem falhou. Tente novamente.")
		}
	}
	return nil
}

// syncVariants reconciles submitted variants (by SKU) with stored rows.
func syncVariants(ctx context.Context, tx DBTX, productID, warehouse uuid.UUID, variants []validVariant) error {
	type existingRow struct {
		ID  uuid.UUID `db:"id"`
		SKU string    `db:"sku"`
	}
	existing, err := collect[existingRow](tx.Query(ctx, sqlExistingVariantSKUs, productID))
	if err != nil {
		return userErr("Não foi possível salvar as variantes.")
	}
	bySKU := map[string]uuid.UUID{}
	for _, e := range existing {
		bySKU[e.SKU] = e.ID
	}
	submitted := map[string]bool{}
	for _, v := range variants {
		submitted[v.sku] = true
		if id, ok := bySKU[v.sku]; ok {
			if _, err := tx.Exec(ctx, sqlUpdateVariant, id, v.size, v.color, v.hex, v.threshold); err != nil {
				return productDBError(err)
			}
			continue
		}
		id := uuid.Must(uuid.NewV7())
		if _, err := tx.Exec(ctx, sqlInsertVariant, id, productID, v.sku, v.size, v.color, v.hex, v.threshold); err != nil {
			return productDBError(err)
		}
		if _, err := tx.Exec(ctx, sqlInsertEmptyInventory, uuid.Must(uuid.NewV7()), id, warehouse); err != nil {
			return productDBError(err)
		}
	}
	for _, e := range existing {
		if !submitted[e.SKU] {
			if _, err := tx.Exec(ctx, sqlDeleteVariant, e.ID); err != nil {
				return productDBError(err)
			}
		}
	}
	return nil
}
