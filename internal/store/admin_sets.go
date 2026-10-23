package store

import (
	"context"
	"errors"
	"fmt"
	"strings"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5"
	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
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

// ListAdminSets lists every set with its cover resolved.
func ListAdminSets(ctx context.Context, q DBTX, st storage.Store) ([]AdminSetRow, error) {
	rows, err := queries(q).StoreAdminSets(ctx)
	if err != nil {
		return nil, err
	}
	return mapSlice(rows, func(r sqlc.StoreAdminSetsRow) AdminSetRow {
		return AdminSetRow{
			ID: r.ID, Name: r.Name, Slug: r.Slug, Price: r.PriceInCents, Active: r.Active,
			CoverURL: ResolveStorageURL(ctx, st, r.CoverUrl), MemberCount: r.MemberCount, ImageCount: r.ImageCount,
		}
	}), nil
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
	qs := queries(pool)
	_, found, err := noRows(qs.StoreSetIDBySlug(ctx, v.slug))
	if err != nil {
		return CreateSetResult{}, err
	} else if found {
		return CreateSetResult{}, userErr("Este slug já está em uso por outro conjunto.")
	}
	id := uuid.Must(uuid.NewV7())
	if err := qs.StoreInsertSet(ctx, sqlc.StoreInsertSetParams{
		ID: id, Name: v.name, Slug: v.slug, Description: v.description, PriceInCents: v.price,
		CompareAtPriceInCents: v.compare, Active: f.Active,
	}); err != nil {
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
	return queries(q).StoreToggleSet(ctx, setID)
}

// DeleteSet deletes a set and its stored images ("Conjunto excluído.").
func DeleteSet(ctx context.Context, q DBTX, st storage.Store, setID uuid.UUID) error {
	qs := queries(q)
	keys, err := qs.StoreSetImageKeys(ctx, setID)
	if err != nil {
		return err
	}
	if err := qs.StoreDeleteSet(ctx, setID); err != nil {
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

// LoadAdminSet loads the edit page (nil when missing).
func LoadAdminSet(ctx context.Context, q DBTX, st storage.Store, setID uuid.UUID) (*AdminSetEdit, error) {
	qs := queries(q)
	r, found, err := noRows(qs.StoreAdminSet(ctx, setID))
	if err != nil || !found {
		return nil, err
	}
	s := AdminSetEdit{
		ID: r.ID, Name: r.Name, Slug: r.Slug, Description: r.Description, PriceInCents: r.PriceInCents,
		CompareAtPriceInCents: r.CompareAtPriceInCents, Active: r.Active,
	}
	imgs, err := qs.StoreAdminSetImages(ctx, setID)
	if err != nil {
		return nil, err
	}
	for _, r := range imgs {
		s.Images = append(s.Images, AdminImage{ID: r.ID, URL: ResolveStorageURL(ctx, st, &r.Url)})
	}
	members, err := qs.StoreAdminSetMembers(ctx, setID)
	if err != nil {
		return nil, err
	}
	s.MemberIDs = map[uuid.UUID]bool{}
	for _, m := range members {
		s.MemberIDs[m.ProductID] = true
	}
	products, err := qs.StoreSetProductOptions(ctx)
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
	slots, err := qs.StoreAdminSetSlots(ctx, setID)
	if err != nil {
		return nil, err
	}
	ids := make([]uuid.UUID, 0, len(slots))
	for _, sl := range slots {
		ids = append(ids, sl.ID)
	}
	opts, err := qs.StoreAdminSlotOptions(ctx, ids)
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
	vars, err := qs.StoreSetVariantOptions(ctx)
	if err != nil {
		return nil, err
	}
	for _, v := range vars {
		s.Variants = append(s.Variants, NamedOption{v.ID, fmt.Sprintf("%s — %s (%s/%s)", v.ProductName, v.Sku, v.Size, v.Color)})
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

// WriteSet rewrites a set's fields, members and slots in one transaction
// (a failure keeps the previous set; checkout never sees a slotless set).
func WriteSet(ctx context.Context, pool *pgxpool.Pool, setID uuid.UUID, w SetWrite) error {
	return withTx(ctx, pool, func(tx pgx.Tx) error {
		q := queries(tx)
		if err := q.StoreUpdateSet(ctx, sqlc.StoreUpdateSetParams{
			ID: setID, Name: w.Name, Slug: w.Slug, Description: w.Description, PriceInCents: w.Price,
			CompareAtPriceInCents: w.Compare, Active: w.Active,
		}); err != nil {
			return err
		}
		if err := q.StoreDeleteSetMembers(ctx, setID); err != nil {
			return err
		}
		members := w.MemberIDs
		if members == nil {
			members = []uuid.UUID{}
		}
		if err := q.StoreInsertSetMembers(ctx, sqlc.StoreInsertSetMembersParams{SetID: setID, ProductIds: members}); err != nil {
			return err
		}
		if err := q.StoreDeleteSetSlots(ctx, setID); err != nil {
			return err
		}
		for i, sl := range w.Slots {
			slotID := uuid.Must(uuid.NewV7())
			if err := q.StoreInsertSetSlot(ctx, sqlc.StoreInsertSetSlotParams{
				ID: slotID, SetID: setID, Label: sl.Label, SortOrder: int32(i), //nolint:gosec // small
			}); err != nil {
				return err
			}
			if err := q.StoreInsertSlotOption(ctx, sqlc.StoreInsertSlotOptionParams{SlotID: slotID, VariantID: sl.VariantID}); err != nil {
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
	qs := queries(pool)
	_, found, err := noRows(qs.StoreSetExists(ctx, setID))
	if err != nil {
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
		found, err := qs.StoreCountVariants(ctx, ids)
		if err != nil {
			return err
		}
		if found != int64(len(ids)) {
			return userErr("Variante inválida em uma das etapas.")
		}
	}
	deleteIDs := parseUUIDs(f.DeleteImageIDs)
	current, err := qs.StoreSetImageCount(ctx, setID)
	if err != nil {
		return err
	}
	remaining := current - int64(len(deleteIDs)) + int64(len(f.Images))
	if remaining < 0 || remaining > MaxImagesPerSet {
		return userErr(MsgSetImageCount)
	}
	if err := validateUploads(f.Images); err != nil {
		return err
	}
	_, found, err = noRows(qs.StoreSetSlugTaken(ctx, sqlc.StoreSetSlugTakenParams{Slug: v.slug, SetID: setID}))
	if err != nil {
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
		owner, found, err := noRows(qs.StoreSetImageOwner(ctx, imageID))
		if err != nil {
			return err
		} else if found && owner == setID {
			if err := DeleteSetImage(ctx, pool, st, imageID); err != nil {
				return err
			}
		}
	}
	next, err := qs.StoreNextSetImageOrder(ctx, setID)
	if err != nil {
		return err
	}
	for i, up := range f.Images {
		if _, err := SaveSetImage(ctx, pool, st, setID, up, next+int32(i)); err != nil { //nolint:gosec // ≤ 8 images
			return userErr("Alterações salvas, mas alguma imagem falhou. Tente novamente.")
		}
	}
	return nil
}
