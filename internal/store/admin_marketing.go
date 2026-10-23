package store

import (
	"context"
	"errors"
	"fmt"
	"net/url"
	"strconv"
	"strings"

	"github.com/google/uuid"
	"github.com/jackc/pgx/v5/pgconn"

	"github.com/wellington-vell/lindaflor/internal/db"
	"github.com/wellington-vell/lindaflor/internal/httpx"
	"github.com/wellington-vell/lindaflor/internal/storage"
)

// Home banners (src/app/store/hero.rs, src/app/admin/banners.rs) and
// admin coupons (src/app/admin/cupons.rs).

// MaxHeroBanners is how many active banners the home carousel shows.
const MaxHeroBanners = 5

// HeroSlide is one home carousel slide (image resolved).
type HeroSlide struct {
	ID       uuid.UUID
	Title    string
	Subtitle *string
	CTALabel string
	LinkURL  string
	ImageURL string
}

var sqlHeroBanners = sqlText(`SELECT id, title, subtitle, cta_label, link_url, image_url
   FROM home_banners WHERE active = true
   ORDER BY sort_order ASC, created_at DESC LIMIT $1`)

// LoadHeroSlides loads active banners; unresolvable images are skipped.
// Empty → the caller keeps its static hero.
func LoadHeroSlides(ctx context.Context, q DBTX, st storage.Store) ([]HeroSlide, error) {
	type row struct {
		ID       uuid.UUID `db:"id"`
		Title    string    `db:"title"`
		Subtitle *string   `db:"subtitle"`
		CTALabel *string   `db:"cta_label"`
		LinkURL  *string   `db:"link_url"`
		ImageURL string    `db:"image_url"`
	}
	rows, err := collect[row](q.Query(ctx, sqlHeroBanners, MaxHeroBanners))
	if err != nil {
		return nil, err
	}
	out := []HeroSlide{}
	for _, r := range rows {
		img := ResolveStorageURL(ctx, st, &r.ImageURL)
		if img == "" {
			continue
		}
		cta := orDefault(r.CTALabel, "Ver coleção")
		link := orDefault(r.LinkURL, "/produtos")
		out = append(out, HeroSlide{ID: r.ID, Title: r.Title, Subtitle: r.Subtitle, CTALabel: cta, LinkURL: link, ImageURL: img})
	}
	return out, nil
}

// AdminBanner is an admin list row.
type AdminBanner struct {
	ID        uuid.UUID
	Title     string
	Subtitle  string
	LinkURL   string
	SortOrder int32
	Active    bool
	ImageURL  string
}

var sqlAdminBanners = sqlText(`SELECT id, title, subtitle, link_url, image_url, sort_order, active
   FROM home_banners ORDER BY sort_order ASC, created_at DESC`)

// ListAdminBanners lists every banner with resolved images.
func ListAdminBanners(ctx context.Context, q DBTX, st storage.Store) ([]AdminBanner, error) {
	type row struct {
		ID        uuid.UUID `db:"id"`
		Title     string    `db:"title"`
		Subtitle  *string   `db:"subtitle"`
		LinkURL   *string   `db:"link_url"`
		ImageURL  string    `db:"image_url"`
		SortOrder int32     `db:"sort_order"`
		Active    bool      `db:"active"`
	}
	rows, err := collect[row](q.Query(ctx, sqlAdminBanners))
	if err != nil {
		return nil, err
	}
	out := make([]AdminBanner, 0, len(rows))
	for _, r := range rows {
		out = append(out, AdminBanner{
			ID: r.ID, Title: r.Title, Subtitle: deref(r.Subtitle), LinkURL: deref(r.LinkURL),
			SortOrder: r.SortOrder, Active: r.Active, ImageURL: ResolveStorageURL(ctx, st, &r.ImageURL),
		})
	}
	return out, nil
}

const (
	bannerTitleMax = 120
	bannerTextMax  = 200
)

func limited(raw string, maxChars int) *string {
	t := strings.TrimSpace(raw)
	if t == "" {
		return nil
	}
	t = truncateRunes(t, maxChars)
	return &t
}

// ValidateBannerLink allows only same-site paths.
func ValidateBannerLink(raw string) (*string, error) {
	l := limited(raw, bannerTextMax)
	if l == nil {
		return nil, nil
	}
	p, ok := httpx.SafeLocalRedirect(*l)
	if !ok {
		return nil, userErr("O link deve ser um caminho interno, começando com /.")
	}
	return &p, nil
}

// BannerForm is the create form.
type BannerForm struct {
	Title, Subtitle, CTALabel, LinkURL, SortOrder string
	Active                                        bool
	Image                                         *Upload
}

var sqlInsertBanner = sqlText(`INSERT INTO home_banners (id, title, subtitle, cta_label, link_url, image_url, sort_order, active, created_at)
 VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now())`)

// CreateBanner validates, stores the image under banners/ and inserts
// ("Banner criado.").
func CreateBanner(ctx context.Context, q DBTX, st storage.Store, f BannerForm) error {
	title := limited(f.Title, bannerTitleMax)
	if title == nil {
		return userErr("Informe o título do banner.")
	}
	link, err := ValidateBannerLink(f.LinkURL)
	if err != nil {
		return err
	}
	sort, perr := strconv.ParseInt(strings.TrimSpace(f.SortOrder), 10, 32)
	if perr != nil {
		sort = 0
	}
	sort = min(max(sort, 0), 999)
	if f.Image == nil {
		return userErr("Envie uma imagem para o banner.")
	}
	kind, err := ValidateImage(f.Image.Bytes, f.Image.ContentType)
	if err != nil {
		return err
	}
	if storage.IsUnavailable(st) {
		return userErr(MsgStorageUnavailable)
	}
	id := uuid.Must(uuid.NewV7())
	key := fmt.Sprintf("banners/%s.%s", id, kind.Extension)
	if err := st.Put(ctx, key, f.Image.Bytes, kind.ContentType); err != nil {
		return userErr(MsgStorageUnavailable)
	}
	_, err = q.Exec(ctx, sqlInsertBanner, id, *title, limited(f.Subtitle, bannerTextMax), limited(f.CTALabel, bannerTextMax),
		link, key, int32(sort), f.Active)
	return err
}

var (
	sqlToggleBanner = sqlText(`UPDATE home_banners SET active = NOT active WHERE id = $1`)
	sqlDeleteBanner = sqlText(`DELETE FROM home_banners WHERE id = $1 RETURNING image_url`)
)

// ToggleBanner flips active ("Banner atualizado.").
func ToggleBanner(ctx context.Context, q DBTX, id uuid.UUID) error {
	_, err := q.Exec(ctx, sqlToggleBanner, id)
	return err
}

// DeleteBanner deletes the row and its banners/ object ("Banner excluído.").
func DeleteBanner(ctx context.Context, q DBTX, st storage.Store, id uuid.UUID) error {
	var key string
	err := q.QueryRow(ctx, sqlDeleteBanner, id).Scan(&key)
	if found, err := optional(err); err != nil || !found {
		return err
	}
	if strings.HasPrefix(key, "banners/") && !storage.IsUnavailable(st) {
		_ = st.Delete(ctx, key)
	}
	return nil
}

// --- Admin coupons ---

const couponQueryMax = 64

// ClampCouponQuery trims and caps the coupon/user searches.
func ClampCouponQuery(raw string) string {
	return truncateRunes(strings.TrimSpace(raw), couponQueryMax)
}

// AdminCouponRow is a coupon table row.
type AdminCouponRow struct {
	ID            uuid.UUID
	Code          string
	DiscountLabel string
	DiscountTone  BadgeTone
	MinLabel      string
	MaxLabel      string
	Active        bool
	StatusLabel   string
	StatusTone    BadgeTone
	Validity      string
	UsageLabel    string
	UsesCount     int64
	AssigneeCount int64
}

func formatCents32(cents int64) string {
	c := cents % 100
	if c < 0 {
		c = -c
	}
	return fmt.Sprintf("R$ %d,%02d", cents/100, c)
}

func shortDT(raw string) string {
	return truncateRunes(strings.ReplaceAll(strings.TrimSpace(raw), "T", " "), 16)
}

var sqlAdminCoupons = sqlText(`SELECT c.id, c.code, c.discount_type, c.discount_value,
     c.min_subtotal_cents, c.max_discount_cents, c.active,
     c.starts_at::text AS starts_at, c.expires_at::text AS expires_at,
     c.usage_type, c.max_uses, c.per_user_limit,
     (SELECT COUNT(*) FROM coupon_redemptions r WHERE r.coupon_id = c.id) AS uses_count,
     (SELECT COUNT(*) FROM coupon_assignments a WHERE a.coupon_id = c.id) AS assignee_count
 FROM coupons c
 WHERE ($1::text = '' OR c.code ILIKE '%' || $1::text || '%' ESCAPE '\')
 ORDER BY c.created_at DESC`)

// ListAdminCoupons searches coupons by code (literal match).
func ListAdminCoupons(ctx context.Context, q DBTX, query string) ([]AdminCouponRow, error) {
	type row struct {
		ID               uuid.UUID `db:"id"`
		Code             string    `db:"code"`
		DiscountType     string    `db:"discount_type"`
		DiscountValue    int64     `db:"discount_value"`
		MinSubtotalCents int64     `db:"min_subtotal_cents"`
		MaxDiscountCents *int64    `db:"max_discount_cents"`
		Active           bool      `db:"active"`
		StartsAt         *string   `db:"starts_at"`
		ExpiresAt        *string   `db:"expires_at"`
		UsageType        string    `db:"usage_type"`
		MaxUses          *int32    `db:"max_uses"`
		PerUserLimit     int32     `db:"per_user_limit"`
		UsesCount        int64     `db:"uses_count"`
		AssigneeCount    int64     `db:"assignee_count"`
	}
	rows, err := collect[row](q.Query(ctx, sqlAdminCoupons, db.EscapeLike(ClampCouponQuery(query))))
	if err != nil {
		return nil, err
	}
	out := make([]AdminCouponRow, 0, len(rows))
	for _, r := range rows {
		c := AdminCouponRow{
			ID: r.ID, Code: r.Code, Active: r.Active, UsesCount: r.UsesCount, AssigneeCount: r.AssigneeCount,
			MinLabel: formatCents32(r.MinSubtotalCents), MaxLabel: "—", StatusLabel: "Inativo", StatusTone: ToneOutline,
		}
		if strings.EqualFold(r.DiscountType, "percent") {
			c.DiscountLabel, c.DiscountTone = fmt.Sprintf("%d%%", r.DiscountValue), TonePrimary
		} else {
			c.DiscountLabel, c.DiscountTone = formatCents32(r.DiscountValue), ToneSecondary
		}
		if r.MaxDiscountCents != nil {
			c.MaxLabel = formatCents32(*r.MaxDiscountCents)
		}
		if r.Active {
			c.StatusLabel, c.StatusTone = "Ativo", TonePrimary
		}
		switch {
		case r.StartsAt != nil && r.ExpiresAt != nil:
			c.Validity = shortDT(*r.StartsAt) + " → " + shortDT(*r.ExpiresAt)
		case r.StartsAt != nil:
			c.Validity = "de " + shortDT(*r.StartsAt)
		case r.ExpiresAt != nil:
			c.Validity = "até " + shortDT(*r.ExpiresAt)
		default:
			c.Validity = "Sem validade"
		}
		switch {
		case strings.EqualFold(r.UsageType, "unique"):
			c.UsageLabel = "Único"
		case r.MaxUses != nil:
			c.UsageLabel = fmt.Sprintf("Até %d usos", *r.MaxUses)
		default:
			c.UsageLabel = "Ilimitado"
		}
		out = append(out, c)
	}
	return out, nil
}

// CouponDBErrorMessage maps coupon admin DB errors to toasts.
func CouponDBErrorMessage(err error) string {
	msg := strings.ToLower(err.Error())
	var pg *pgconn.PgError
	switch {
	case errors.As(err, &pg) && pg.Code == "23505", strings.Contains(msg, "duplicate"), strings.Contains(msg, "unique"):
		return "Este código já está em uso."
	case strings.Contains(msg, "does not exist"), strings.Contains(msg, "undefined"):
		return "Tabelas de cupons indisponíveis. Fale com o responsável pela migração."
	}
	return "Não foi possível concluir a operação. Tente novamente."
}

// AssignedUser is a coupon assignee.
type AssignedUser struct {
	ID    uuid.UUID `db:"id"`
	Name  string    `db:"name"`
	Email string    `db:"email"`
}

var (
	sqlCouponAssignees   = sqlText(`SELECT u.id, u.name, u.email FROM coupon_assignments a INNER JOIN users u ON u.id = a.user_id WHERE a.coupon_id = $1 ORDER BY u.name ASC`)
	sqlCouponAssigneeIDs = sqlText(`SELECT user_id FROM coupon_assignments WHERE coupon_id = $1`)
)

// CouponAssignees lists a coupon's assigned users.
func CouponAssignees(ctx context.Context, q DBTX, couponID uuid.UUID) ([]AssignedUser, error) {
	return collect[AssignedUser](q.Query(ctx, sqlCouponAssignees, couponID))
}

// CouponAssigneeIDs is the assigned-user set (user directory checkboxes).
func CouponAssigneeIDs(ctx context.Context, q DBTX, couponID uuid.UUID) (map[uuid.UUID]bool, error) {
	ids, err := scalars[uuid.UUID](q.Query(ctx, sqlCouponAssigneeIDs, couponID))
	if err != nil {
		return nil, err
	}
	out := map[uuid.UUID]bool{}
	for _, id := range ids {
		out[id] = true
	}
	return out, nil
}

// CouponForm is the create form.
type CouponForm struct {
	Code, DiscountType, DiscountValue, MinSubtotalCents, MaxDiscountCents string
	UsageType, MaxUses, PerUserLimit, StartsAt, ExpiresAt                 string
}

type validCoupon struct {
	code, discountType, usageType string
	discountValue, minSubtotal    int32
	maxDiscount, maxUses          *int32
	perUser                       int32
	startsAt, expiresAt           string
}

func parseOptionalInt(raw, field string) (*int32, error) {
	t := strings.TrimSpace(raw)
	if t == "" {
		return nil, nil
	}
	n, err := strconv.ParseInt(t, 10, 32)
	if err != nil {
		return nil, userErr(fmt.Sprintf("Campo %s inválido.", field))
	}
	v := int32(n)
	return &v, nil
}

func validateCoupon(f CouponForm) (validCoupon, error) {
	var c validCoupon
	c.code = strings.ToUpper(strings.TrimSpace(f.Code))
	if c.code == "" {
		return c, userErr("Informe o código do cupom.")
	}
	if len(c.code) > 32 {
		return c, userErr("O código deve ter até 32 caracteres.")
	}
	switch strings.ToLower(strings.TrimSpace(f.DiscountType)) {
	case "percent":
		c.discountType = "percent"
	case "fixed":
		c.discountType = "fixed"
	default:
		return c, userErr("Tipo de desconto inválido.")
	}
	v, err := strconv.ParseInt(strings.TrimSpace(f.DiscountValue), 10, 32)
	if err != nil {
		return c, userErr("Valor do desconto inválido.")
	}
	c.discountValue = int32(v)
	if c.discountType == "percent" && (v < 1 || v > 50) {
		return c, userErr("Percentual deve ser entre 1 e 50.")
	}
	if c.discountType == "fixed" && (v < 1 || v > 50_000) {
		return c, userErr("Valor fixo deve ser entre 1 e 50000 centavos.")
	}
	minSub, err := parseOptionalInt(f.MinSubtotalCents, "subtotal mínimo")
	if err != nil {
		return c, err
	}
	if minSub != nil {
		c.minSubtotal = *minSub
	}
	if c.minSubtotal < 0 {
		return c, userErr("Subtotal mínimo inválido.")
	}
	if c.maxDiscount, err = parseOptionalInt(f.MaxDiscountCents, "desconto máximo"); err != nil {
		return c, err
	}
	if c.maxDiscount != nil && *c.maxDiscount <= 0 {
		return c, userErr("Desconto máximo inválido.")
	}
	switch strings.ToLower(strings.TrimSpace(f.UsageType)) {
	case "unique":
		c.usageType = "unique"
		c.maxUses = ptr(int32(1))
	case "unlimited":
		c.usageType = "unlimited"
		if c.maxUses, err = parseOptionalInt(f.MaxUses, "máx. usos"); err != nil {
			return c, err
		}
		if c.maxUses != nil && *c.maxUses < 1 {
			return c, userErr("Máx. de usos inválido.")
		}
	default:
		return c, userErr("Tipo de uso inválido.")
	}
	per, err := parseOptionalInt(f.PerUserLimit, "limite por usuário")
	if err != nil {
		return c, err
	}
	c.perUser = 1
	if per != nil {
		c.perUser = *per
	}
	if c.perUser < 1 {
		return c, userErr("Limite por usuário inválido.")
	}
	c.startsAt, c.expiresAt = strings.TrimSpace(f.StartsAt), strings.TrimSpace(f.ExpiresAt)
	if c.startsAt != "" && c.expiresAt != "" && c.startsAt > c.expiresAt {
		return c, userErr("Início deve ser anterior ao término.")
	}
	return c, nil
}

var sqlInsertCoupon = sqlText(`INSERT INTO coupons (id, code, discount_type, discount_value,
 min_subtotal_cents, max_discount_cents, active, starts_at, expires_at,
 usage_type, max_uses, per_user_limit)
 VALUES ($1, $2, $3, $4, $5, $6, true,
 NULLIF($7::text, '')::timestamptz, NULLIF($8::text, '')::timestamptz, $9, $10, $11)
 ON CONFLICT ((upper(code))) DO NOTHING`)

// CreateCoupon validates and inserts a coupon (*UserError on refusal,
// including "Este código já está em uso.").
func CreateCoupon(ctx context.Context, q DBTX, f CouponForm) error {
	c, err := validateCoupon(f)
	if err != nil {
		return err
	}
	tag, err := q.Exec(ctx, sqlInsertCoupon, uuid.Must(uuid.NewV7()), c.code, c.discountType, c.discountValue,
		c.minSubtotal, c.maxDiscount, c.startsAt, c.expiresAt, c.usageType, c.maxUses, c.perUser)
	if err != nil {
		return userErr(CouponDBErrorMessage(err))
	}
	if tag.RowsAffected() != 1 {
		return userErr("Este código já está em uso.")
	}
	return nil
}

var (
	sqlToggleCoupon   = sqlText(`UPDATE coupons SET active = NOT active WHERE id = $1`)
	sqlDeleteCoupon   = sqlText(`DELETE FROM coupons WHERE id = $1`)
	sqlAssignCoupon   = sqlText(`INSERT INTO coupon_assignments (coupon_id, user_id) SELECT $1, unnest($2::uuid[]) ON CONFLICT DO NOTHING`)
	sqlUnassignCoupon = sqlText(`DELETE FROM coupon_assignments WHERE coupon_id = $1 AND user_id = $2`)
)

// ToggleCoupon flips active; *UserError "Cupom não encontrado." when absent.
func ToggleCoupon(ctx context.Context, q DBTX, id uuid.UUID) error {
	tag, err := q.Exec(ctx, sqlToggleCoupon, id)
	if err != nil {
		return userErr(CouponDBErrorMessage(err))
	}
	if tag.RowsAffected() != 1 {
		return userErr("Cupom não encontrado.")
	}
	return nil
}

// DeleteCoupon deletes a coupon; *UserError when absent.
func DeleteCoupon(ctx context.Context, q DBTX, id uuid.UUID) error {
	tag, err := q.Exec(ctx, sqlDeleteCoupon, id)
	if err != nil {
		return userErr(CouponDBErrorMessage(err))
	}
	if tag.RowsAffected() != 1 {
		return userErr("Cupom não encontrado.")
	}
	return nil
}

// AssignCoupon restricts a coupon to users (at least one).
func AssignCoupon(ctx context.Context, q DBTX, couponID uuid.UUID, rawUserIDs []string) error {
	ids := parseUUIDs(rawUserIDs)
	if len(ids) == 0 {
		return userErr("Selecione ao menos um usuário.")
	}
	if _, err := q.Exec(ctx, sqlAssignCoupon, couponID, ids); err != nil {
		return userErr(CouponDBErrorMessage(err))
	}
	return nil
}

// UnassignCoupon removes one assignment.
func UnassignCoupon(ctx context.Context, q DBTX, couponID, userID uuid.UUID) error {
	if _, err := q.Exec(ctx, sqlUnassignCoupon, couponID, userID); err != nil {
		return userErr(CouponDBErrorMessage(err))
	}
	return nil
}

// CouponsURL rebuilds /admin/cupons?q=&cupom=&uq= (uq only with cupom).
func CouponsURL(q, cupom, uq string) string {
	var parts []string
	if q != "" {
		parts = append(parts, "q="+urlQueryEscape(q))
	}
	if cupom != "" {
		parts = append(parts, "cupom="+urlQueryEscape(cupom))
		if uq != "" {
			parts = append(parts, "uq="+urlQueryEscape(uq))
		}
	}
	if len(parts) == 0 {
		return "/admin/cupons"
	}
	return "/admin/cupons?" + strings.Join(parts, "&")
}

func urlQueryEscape(s string) string { return url.QueryEscape(s) }
