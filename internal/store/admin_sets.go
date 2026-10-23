package store

import (
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/storage"
)

// Admin conjuntos (src/app/admin/sets.rs, sets/id.rs): list, create,
// toggle, delete, edit (fields, members, fixed-composition slots, images).

// AdminSetRow is a list row.
type AdminSetRow struct {
	ID          uuid.UUID
	Name        string
	Slug        string
	Price       *int64
	Active      bool
	CoverURL    string // resolved ("" for placeholder)
	MemberCount int64
	ImageCount  int64
}

var sqlAdminSets = sqlText(`SELECT s.id, s.name, s.slug, s.price_in_cents, s.active,
        (SELECT si.url FROM set_images si WHERE si.set_id = s.id ORDER BY si.sort_order ASC LIMIT 1) AS cover_url,
        (SELECT COUNT(*) FROM set_members sm WHERE sm.set_id = s.id) AS member_count,
        (SELECT COUNT(*) FROM set_images si WHERE si.set_id = s.id) AS image_count
 FROM sets s ORDER BY s.name ASC`)

// ListAdminSets lists every set with its cover resolved.
func ListAdminSets(ctx context.Context, q DBTX, st storage.Store) ([]AdminSetRow, error) {
	type row struct {
		ID          uuid.UUID `db:"id"`
		Name        string    `db:"name"`
		Slug        string    `db:"slug"`
		Price       *int64    `db:"price_in_cents"`
		Active      bool      `db:"active"`
		Cover       *string   `db:"cover_url"`
		MemberCount int64     `db:"member_count"`
		ImageCount  int64     `db:"image_count"`
	}
	rows, err := collect[row](q.Query(ctx, sqlAdminSets))
	if err != nil {
		return nil, err
	}
	out := make([]AdminSetRow, 0, len(rows))
	for _, r := range rows {
		out = append(out, AdminSetRow{
			ID: r.ID, Name: r.Name, Slug: r.Slug, Price: r.Price, Active: r.Active,
			CoverURL: ResolveStorageURL(ctx, st, r.Cover), MemberCount: r.MemberCount, ImageCount: r.ImageCount,
		})
	}
	return out, nil
}

// ValidateSetName trims; 1..=200 chars.
func ValidateSetName(raw string) (string, error) {
	n := strings.TrimSpace(raw)
	if n == "" {
		return "", userErr("Informe o nome do conjunto.")
	}
	if runeLen(n) > 200 {
		return "", userErr("O nome deve ter no máximo 200 caracteres.")
	}
	return n, nil
}

// ValidateSetSlug is the set slug rule.
func ValidateSetSlug(raw string) (string, error) {
	return validateSlug(raw, "Informe o slug do conjunto.")
}

// ParseSetPrice is the optional bundle price.
func ParseSetPrice(raw string) (*int64, error) {
	c, ok, err := parsePriceCents(raw, "", "Preço inválido. Use 199.90.", false)
	if err != nil || !ok {
		return nil, err
	}
	return &c, nil
}

// ParseSetCompare is the optional "de" price (needs a price, must exceed it).
func ParseSetCompare(raw string, price *int64) (*int64, error) {
	if strings.TrimSpace(raw) == "" {
		return nil, nil
	}
	if price == nil {
		return nil, userErr("Informe o preço do conjunto para usar o preço original.")
	}
	c, _, err := parsePriceCents(raw, "", "Preço original inválido. Use 199.90.", false)
	if err != nil {
		return nil, err
	}
	if c <= *price {
		return nil, userErr("Preço original deve ser maior que o preço do conjunto.")
	}
	return &c, nil
}

// SetSlotForm is one slot row (label + variant id).
type SetSlotForm struct {
	Label     string
	VariantID string
}

// SetForm is the create/edit multipart form.
type SetForm struct {
	Name           string
	Slug           string
	Description    string
	Price          string
	CompareAtPrice string
	Active         bool
	MemberIDs      []string      // edit: member_product_ids[]
	Slots          []SetSlotForm // edit: slot_label[] + slot_variant[]
	DeleteImageIDs []string
	Images         []Upload
}

// ZipSlotForms pairs slot_label[] with slot_variant[] ("Etapas
// inconsistentes. Confira os campos." on mismatch).
func ZipSlotForms(labels, variants []string) ([]SetSlotForm, error) {
	if len(labels) != len(variants) {
		return nil, userErr("Etapas inconsistentes. Confira os campos.")
	}
	out := make([]SetSlotForm, len(labels))
	for i := range labels {
		out[i] = SetSlotForm{Label: labels[i], VariantID: variants[i]}
	}
	return out, nil
}

type validSet struct {
	name, slug     string
	description    *string
	price, compare *int64
}

func validateSetBase(f SetForm) (validSet, error) {
	var v validSet
	var err error
	if v.name, err = ValidateSetName(f.Name); err != nil {
		return v, err
	}
	if v.slug, err = ValidateSetSlug(f.Slug); err != nil {
		return v, err
	}
	if v.price, err = ParseSetPrice(f.Price); err != nil {
		return v, err
	}
	if v.compare, err = ParseSetCompare(f.CompareAtPrice, v.price); err != nil {
		return v, err
	}
	v.description = optionalDescription(f.Description)
	return v, nil
}

var (
	sqlSetIDBySlug  = sqlText(`SELECT id FROM sets WHERE slug = $1`)
	sqlSetSlugTaken = sqlText(`SELECT id FROM sets WHERE slug = $1 AND id != $2`)
	sqlInsertSet    = sqlText(`INSERT INTO sets (id, name, slug, description, price_in_cents, compare_at_price_in_cents, active, created_at, updated_at)
 VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now())`)
	sqlToggleSet    = sqlText(`UPDATE sets SET active = NOT active, updated_at = now() WHERE id = $1`)
	sqlSetImageKeys = sqlText(`SELECT url FROM set_images WHERE set_id = $1`)
	sqlDeleteSet    = sqlText(`DELETE FROM sets WHERE id = $1`)
)

// CreateSetResult mirrors CreateProductResult.
type CreateSetResult struct {
	ID          uuid.UUID
	ImageFailed bool
}

// Message is the toast text.
func (r CreateSetResult) Message() string {
	if r.ImageFailed {
		return "Conjunto criado, mas alguma imagem falhou. Tente adicioná-la na edição."
	}
	return "Conjunto criado com sucesso."
}

// CreateSet validates and inserts a set, then uploads its images.
func CreateSet(ctx context.Context, pool *pgxpool.Pool, st storage.Store, f SetForm) (CreateSetResult, error) {
	v, err := validateSetBase(f)
	if err != nil {
		return CreateSetResult{}, err
	}
	if len(f.Images) > MaxImagesPerSet {
		return CreateSetResult{}, userErr(MsgSetImageCount)
	}
	if err := validateUploads(f.Images); err != nil {
		return CreateSetResult{}, err
	}
	var existing uuid.UUID
	err = pool.QueryRow(ctx, sqlSetIDBySlug, v.slug).Scan(&existing)
	if found, err := optional(err); err != nil {
		return CreateSetResult{}, err
	} else if found {
		return CreateSetResult{}, userErr("Este slug já está em uso por outro conjunto.")
	}
	id := uuid.Must(uuid.NewV7())
	if _, err := pool.Exec(ctx, sqlInsertSet, id, v.name, v.slug, v.description, v.price, v.compare, f.Active); err != nil {
		return CreateSetResult{}, err
	}
	res := CreateSetResult{ID: id}
	for i, up := range f.Images {
		if _, err := SaveSetImage(ctx, pool, st, id, up, int32(i)); err != nil { //nolint:gosec // ≤ 8 images
			res.ImageFailed = true
			break
		}
	}
	return res, nil
}

// ToggleSet flips a set's active flag ("Conjunto atualizado.").
func ToggleSet(ctx context.Context, q DBTX, setID uuid.UUID) error {
	_, err := q.Exec(ctx, sqlToggleSet, setID)
	return err
}

// DeleteSet deletes a set and its stored images ("Conjunto excluído.").
func DeleteSet(ctx context.Context, q DBTX, st storage.Store, setID uuid.UUID) error {
	keys, err := scalars[string](q.Query(ctx, sqlSetImageKeys, setID))
	if err != nil {
		return err
	}
	if _, err := q.Exec(ctx, sqlDeleteSet, setID); err != nil {
		return err
	}
	for _, k := range keys {
		if IsStorageKey(k) && strings.HasPrefix(k, "sets/") && !storage.IsUnavailable(st) {
			_ = st.Delete(ctx, k)
		}
	}
	return nil
}

// --- Edit ---

// AdminSetEdit is the set edit page.
type AdminSetEdit struct {
	ID                    uuid.UUID
	Name                  string
	Slug                  string
	Description           *string
	PriceInCents          *int64
	CompareAtPriceInCents *int64
	Active                bool
	Images                []AdminImage
	// Products are member candidates (active, first 100) plus current
	// members, inactive ones suffixed " (inativo)".
	Products  []NamedOption
	MemberIDs map[uuid.UUID]bool
	// Slots are the current slots plus one blank row.
	Slots    []SetSlotForm
	Variants []NamedOption // "Produto — SKU (size/color)", first 200
}

// PriceInput is "" or "199.90".
func (s AdminSetEdit) PriceInput() string {
	if s.PriceInCents == nil {
		return ""
	}
	return FormatPriceInput(*s.PriceInCents)
}

// CompareInput is "" or "249.90".
func (s AdminSetEdit) CompareInput() string {
	if s.CompareAtPriceInCents == nil {
		return ""
	}
	return FormatPriceInput(*s.CompareAtPriceInCents)
}

// NamedOption is a select option.
type NamedOption struct {
	ID    uuid.UUID
	Label string
}

var (
	sqlAdminSet        = sqlText(`SELECT id, name, slug, description, price_in_cents, compare_at_price_in_cents, active FROM sets WHERE id = $1`)
	sqlAdminSetImages  = sqlText(`SELECT id, url FROM set_images WHERE set_id = $1 ORDER BY sort_order ASC`)
	sqlAdminSetMembers = sqlText(`SELECT sm.product_id, p.name FROM set_members sm
  INNER JOIN products p ON p.id = sm.product_id
  WHERE sm.set_id = $1 ORDER BY sm.sort_order ASC`)
	sqlSetProductOptions = sqlText(`SELECT id, name FROM products WHERE active = true ORDER BY name ASC LIMIT 100`)
	sqlAdminSetSlots     = sqlText(`SELECT id, label FROM set_slots WHERE set_id = $1 ORDER BY sort_order ASC`)
	sqlAdminSlotOptions  = sqlText(`SELECT slot_id, variant_id FROM set_slot_options WHERE slot_id = ANY($1)`)
	sqlSetVariantOptions = sqlText(`SELECT pv.id, pv.sku, p.name AS product_name, pv.size::text AS size, pv.color
   FROM product_variants pv
   INNER JOIN products p ON p.id = pv.product_id
   WHERE p.active = true
   ORDER BY p.name ASC, pv.sku ASC LIMIT 200`)
)

// LoadAdminSet loads the edit page (nil when missing).
func LoadAdminSet(ctx context.Context, q DBTX, st storage.Store, setID uuid.UUID) (*AdminSetEdit, error) {
	var s AdminSetEdit
	err := q.QueryRow(ctx, sqlAdminSet, setID).Scan(&s.ID, &s.Name, &s.Slug, &s.Description, &s.PriceInCents, &s.CompareAtPriceInCents, &s.Active)
	if found, err := optional(err); err != nil || !found {
		return nil, err
	}
	type idURL struct {
		ID  uuid.UUID `db:"id"`
		URL string    `db:"url"`
	}
	imgs, err := collect[idURL](q.Query(ctx, sqlAdminSetImages, setID))
	if err != nil {
		return nil, err
	}
	for _, r := range imgs {
		s.Images = append(s.Images, AdminImage{ID: r.ID, URL: ResolveStorageURL(ctx, st, &r.URL)})
	}
	type member struct {
		ProductID uuid.UUID `db:"product_id"`
		Name      string    `db:"name"`
	}
	members, err := collect[member](q.Query(ctx, sqlAdminSetMembers, setID))
	if err != nil {
		return nil, err
	}
	s.MemberIDs = map[uuid.UUID]bool{}
	for _, m := range members {
		s.MemberIDs[m.ProductID] = true
	}
	type opt struct {
		ID   uuid.UUID `db:"id"`
		Name string    `db:"name"`
	}
	products, err := collect[opt](q.Query(ctx, sqlSetProductOptions))
	if err != nil {
		return nil, err
	}
	listed := map[uuid.UUID]bool{}
	for _, p := range products {
		s.Products = append(s.Products, NamedOption{p.ID, p.Name})
		listed[p.ID] = true
	}
	for _, m := range members {
		if !listed[m.ProductID] {
			s.Products = append(s.Products, NamedOption{m.ProductID, m.Name + " (inativo)"})
		}
	}
	type slot struct {
		ID    uuid.UUID `db:"id"`
		Label *string   `db:"label"`
	}
	slots, err := collect[slot](q.Query(ctx, sqlAdminSetSlots, setID))
	if err != nil {
		return nil, err
	}
	ids := make([]uuid.UUID, 0, len(slots))
	for _, sl := range slots {
		ids = append(ids, sl.ID)
	}
	type slotOpt struct {
		SlotID    uuid.UUID `db:"slot_id"`
		VariantID uuid.UUID `db:"variant_id"`
	}
	opts, err := collect[slotOpt](q.Query(ctx, sqlAdminSlotOptions, ids))
	if err != nil {
		return nil, err
	}
	bySlot := map[uuid.UUID]uuid.UUID{}
	for _, o := range opts {
		bySlot[o.SlotID] = o.VariantID
	}
	for _, sl := range slots {
		f := SetSlotForm{Label: deref(sl.Label)}
		if v, ok := bySlot[sl.ID]; ok {
			f.VariantID = v.String()
		}
		s.Slots = append(s.Slots, f)
	}
	s.Slots = append(s.Slots, SetSlotForm{})
	type variantOpt struct {
		ID          uuid.UUID `db:"id"`
		SKU         string    `db:"sku"`
		ProductName string    `db:"product_name"`
		Size        string    `db:"size"`
		Color       string    `db:"color"`
	}
	vars, err := collect[variantOpt](q.Query(ctx, sqlSetVariantOptions))
	if err != nil {
		return nil, err
	}
	for _, v := range vars {
		s.Variants = append(s.Variants, NamedOption{v.ID, fmt.Sprintf("%s — %s (%s/%s)", v.ProductName, v.SKU, v.Size, v.Color)})
	}
	return &s, nil
}

// SetSlot is a validated slot.
type SetSlot struct {
	Label     *string
	VariantID uuid.UUID
}

// SetWrite is a validated set save.
type SetWrite struct {
	Name        string
	Slug        string
	Description *string
	Price       *int64
	Compare     *int64
	Active      bool
	MemberIDs   []uuid.UUID
	Slots       []SetSlot
}

var (
	sqlUpdateSet = sqlText(`UPDATE sets SET name = $2, slug = $3, description = $4, price_in_cents = $5,
  compare_at_price_in_cents = $6, active = $7, updated_at = now() WHERE id = $1`)
	sqlDeleteSetMembers = sqlText(`DELETE FROM set_members WHERE set_id = $1`)
	sqlInsertSetMembers = sqlText(`INSERT INTO set_members (set_id, product_id, sort_order)
 SELECT $1, p.id, (ids.ord - 1)::int
 FROM unnest($2::uuid[]) WITH ORDINALITY AS ids(id, ord)
 INNER JOIN products p ON p.id = ids.id
 ON CONFLICT DO NOTHING`)
	sqlDeleteSetSlots   = sqlText(`DELETE FROM set_slots WHERE set_id = $1`)
	sqlInsertSetSlot    = sqlText(`INSERT INTO set_slots (id, set_id, label, sort_order) VALUES ($1, $2, $3, $4)`)
	sqlInsertSlotOption = sqlText(`INSERT INTO set_slot_options (slot_id, variant_id) VALUES ($1, $2)`)
	sqlCountVariants    = sqlText(`SELECT COUNT(*) FROM product_variants WHERE id = ANY($1)`)
	sqlSetExists        = sqlText(`SELECT id FROM sets WHERE id = $1`)
	sqlSetImgCount      = sqlText(`SELECT COUNT(*) FROM set_images WHERE set_id = $1`)
	sqlSetImageOwner    = sqlText(`SELECT set_id FROM set_images WHERE id = $1`)
	sqlNextSetImgOrder  = sqlText(`SELECT (COALESCE(MAX(sort_order), -1) + 1)::int FROM set_images WHERE set_id = $1`)
)

// WriteSet rewrites a set's fields, members and slots in one transaction
// (a failure keeps the previous set; checkout never sees a slotless set).
func WriteSet(ctx context.Context, pool *pgxpool.Pool, setID uuid.UUID, w SetWrite) error {
	return withTx(ctx, pool, func(tx pgx.Tx) error {
		if _, err := tx.Exec(ctx, sqlUpdateSet, setID, w.Name, w.Slug, w.Description, w.Price, w.Compare, w.Active); err != nil {
			return err
		}
		if _, err := tx.Exec(ctx, sqlDeleteSetMembers, setID); err != nil {
			return err
		}
		members := w.MemberIDs
		if members == nil {
			members = []uuid.UUID{}
		}
		if _, err := tx.Exec(ctx, sqlInsertSetMembers, setID, members); err != nil {
			return err
		}
		if _, err := tx.Exec(ctx, sqlDeleteSetSlots, setID); err != nil {
			return err
		}
		for i, sl := range w.Slots {
			slotID := uuid.Must(uuid.NewV7())
			if _, err := tx.Exec(ctx, sqlInsertSetSlot, slotID, setID, sl.Label, int32(i)); err != nil { //nolint:gosec // small
				return err
			}
			if _, err := tx.Exec(ctx, sqlInsertSlotOption, slotID, sl.VariantID); err != nil {
				return err
			}
		}
		return nil
	})
}

// ErrSetNotFound: the edited set does not exist.
var ErrSetNotFound = errors.New("Conjunto não encontrado.")

// UpdateSet validates and saves the edit form, then handles images. Toasts
// as UpdateProduct ("Alterações salvas com sucesso.").
func UpdateSet(ctx context.Context, pool *pgxpool.Pool, st storage.Store, setID uuid.UUID, f SetForm) error {
	var exists uuid.UUID
	err := pool.QueryRow(ctx, sqlSetExists, setID).Scan(&exists)
	if found, err := optional(err); err != nil {
		return err
	} else if !found {
		return ErrSetNotFound
	}
	v, err := validateSetBase(f)
	if err != nil {
		return err
	}
	var slots []SetSlot
	for _, sf := range f.Slots {
		label := strings.TrimSpace(sf.Label)
		raw := strings.TrimSpace(sf.VariantID)
		if label == "" && raw == "" {
			continue
		}
		id, perr := uuid.Parse(raw)
		if perr != nil {
			return userErr("Variante inválida em uma das etapas.")
		}
		slots = append(slots, SetSlot{Label: nonEmpty(label), VariantID: id})
	}
	if len(slots) > 0 {
		ids := make([]uuid.UUID, 0, len(slots))
		for _, s := range slots {
			ids = append(ids, s.VariantID)
		}
		var found int64
		if err := pool.QueryRow(ctx, sqlCountVariants, ids).Scan(&found); err != nil {
			return err
		}
		if found != int64(len(ids)) {
			return userErr("Variante inválida em uma das etapas.")
		}
	}
	deleteIDs := parseUUIDs(f.DeleteImageIDs)
	var current int64
	if err := pool.QueryRow(ctx, sqlSetImgCount, setID).Scan(&current); err != nil {
		return err
	}
	remaining := current - int64(len(deleteIDs)) + int64(len(f.Images))
	if remaining < 0 || remaining > MaxImagesPerSet {
		return userErr(MsgSetImageCount)
	}
	if err := validateUploads(f.Images); err != nil {
		return err
	}
	var taken uuid.UUID
	err = pool.QueryRow(ctx, sqlSetSlugTaken, v.slug, setID).Scan(&taken)
	if found, err := optional(err); err != nil {
		return err
	} else if found {
		return userErr("Este slug já está em uso por outro conjunto.")
	}
	if err := WriteSet(ctx, pool, setID, SetWrite{
		Name: v.name, Slug: v.slug, Description: v.description, Price: v.price, Compare: v.compare,
		Active: f.Active, MemberIDs: parseUUIDs(f.MemberIDs), Slots: slots,
	}); err != nil {
		return err
	}
	for _, imageID := range deleteIDs {
		var owner uuid.UUID
		err := pool.QueryRow(ctx, sqlSetImageOwner, imageID).Scan(&owner)
		if found, err := optional(err); err != nil {
			return err
		} else if found && owner == setID {
			if err := DeleteSetImage(ctx, pool, st, imageID); err != nil {
				return err
			}
		}
	}
	var next int32
	if err := pool.QueryRow(ctx, sqlNextSetImgOrder, setID).Scan(&next); err != nil {
		return err
	}
	for i, up := range f.Images {
		if _, err := SaveSetImage(ctx, pool, st, setID, up, next+int32(i)); err != nil { //nolint:gosec // ≤ 8 images
			return userErr("Alterações salvas, mas alguma imagem falhou. Tente novamente.")
		}
	}
	return nil
}
