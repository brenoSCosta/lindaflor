package store

import (
	"context"
	"fmt"
	"log/slog"
	"math"
	"net/url"
	"strconv"
	"strings"

	"github.com/jackc/pgx/v5/pgxpool"

	"github.com/wellington-vell/lindaflor/internal/db/sqlc"
	"github.com/wellington-vell/lindaflor/internal/payments/pix"
)

// DefaultFreeShippingThresholdCents is the schema default (R$ 299,00).
const DefaultFreeShippingThresholdCents int64 = 29900

// StoreSettings is the store_settings singleton.
type StoreSettings struct {
	WhatsappNumber             *string
	WhatsappMessageTemplate    *string
	PixKey                     *string
	PixKeyType                 *string
	PixMerchantName            *string
	PixMerchantCity            *string
	FreeShippingThresholdCents int64
	PixDiscountPercent         int
	AnnouncementText           *string
	FirstPurchaseCouponCode    *string
}

// DefaultStoreSettings is used when no settings row exists yet.
func DefaultStoreSettings() StoreSettings {
	return StoreSettings{FreeShippingThresholdCents: DefaultFreeShippingThresholdCents, PixDiscountPercent: 5}
}

// WhatsappNumberTrimmed is the configured number, "" when unset/blank.
func (s StoreSettings) WhatsappNumberTrimmed() string {
	return strings.TrimSpace(deref(s.WhatsappNumber))
}

// PixConfigured reports a non-blank PIX key and key type.
func (s StoreSettings) PixConfigured() bool {
	return !isBlank(deref(s.PixKey)) && !isBlank(deref(s.PixKeyType))
}

// GetStoreSettings reads the singleton (defaults when missing).
func GetStoreSettings(ctx context.Context, q DBTX) (StoreSettings, error) {
	r, found, err := noRows(queries(q).StoreGetSettings(ctx))
	if err != nil || !found {
		return DefaultStoreSettings(), err
	}
	return StoreSettings{
		WhatsappNumber: r.WhatsappNumber, WhatsappMessageTemplate: r.WhatsappMessageTemplate,
		PixKey: r.PixKey, PixKeyType: r.PixKeyType, PixMerchantName: r.PixMerchantName,
		PixMerchantCity: r.PixMerchantCity, FreeShippingThresholdCents: r.FreeShippingThresholdCents,
		PixDiscountPercent: int(r.PixDiscountPercent), AnnouncementText: r.AnnouncementText,
		FirstPurchaseCouponCode: r.FirstPurchaseCouponCode,
	}, nil
}

// StorefrontSettings is GetStoreSettings with a logged fallback to the
// defaults, for display (product cards, header, footer).
func StorefrontSettings(ctx context.Context, q DBTX) StoreSettings {
	s, err := GetStoreSettings(ctx, q)
	if err != nil {
		slog.WarnContext(ctx, "store settings unavailable, using defaults", "error", err)
		return DefaultStoreSettings()
	}
	return s
}

// RenderWhatsappTemplate replaces {{key}} placeholders; a blank template
// or any placeholder left over returns fallback.
func RenderWhatsappTemplate(template *string, vars [][2]string, fallback string) string {
	if template == nil || isBlank(*template) {
		return fallback
	}
	rendered := strings.TrimSpace(*template)
	for _, kv := range vars {
		rendered = strings.ReplaceAll(rendered, "{{"+kv[0]+"}}", kv[1])
	}
	if strings.Contains(rendered, "{{") {
		return fallback
	}
	return rendered
}

// WhatsappLink is https://wa.me/<number>?text=<text>.
func WhatsappLink(number, text string) string {
	u := url.URL{Scheme: "https", Host: "wa.me", Path: "/" + number}
	u.RawQuery = url.Values{"text": {text}}.Encode()
	return u.String()
}

// --- Admin: /admin/configuracoes ------------------------------------------

// PixKeyTypes are the pix_key_type enum values.
var PixKeyTypes = []string{"cpf", "cnpj", "email", "phone", "random"}

// SettingsForm is the raw admin settings form.
type SettingsForm struct {
	PixKey                  string
	PixKeyType              string
	PixMerchantName         string
	PixMerchantCity         string
	PixDiscountPercent      string
	AnnouncementText        string
	FirstPurchaseCouponCode string
	WhatsappNumber          string
	WhatsappMessageTemplate string
	FreeShippingThreshold   string
}

// SettingsFormFrom pre-fills the admin form from the stored settings.
func SettingsFormFrom(s StoreSettings) SettingsForm {
	keyType := deref(s.PixKeyType)
	if keyType == "" {
		keyType = "phone"
	}
	return SettingsForm{
		PixKey:                  deref(s.PixKey),
		PixKeyType:              keyType,
		PixMerchantName:         deref(s.PixMerchantName),
		PixMerchantCity:         deref(s.PixMerchantCity),
		PixDiscountPercent:      strconv.Itoa(s.PixDiscountPercent),
		AnnouncementText:        deref(s.AnnouncementText),
		FirstPurchaseCouponCode: deref(s.FirstPurchaseCouponCode),
		WhatsappNumber:          deref(s.WhatsappNumber),
		WhatsappMessageTemplate: deref(s.WhatsappMessageTemplate),
		FreeShippingThreshold:   FormatPriceInput(s.FreeShippingThresholdCents),
	}
}

// FormatPriceInput renders cents as "199.90" for form inputs.
func FormatPriceInput(cents int64) string { return fmt.Sprintf("%.2f", float64(cents)/100) }

// SettingsWrite is a validated settings save.
type SettingsWrite struct {
	PixKey                     *string
	PixKeyType                 *string
	PixMerchantName            *string
	PixMerchantCity            *string
	PixDiscountPercent         int16
	AnnouncementText           *string
	FirstPurchaseCouponCode    *string
	WhatsappNumber             *string
	WhatsappMessageTemplate    *string
	FreeShippingThresholdCents int64
}

func optionalText(raw string) *string { return nonEmpty(strings.TrimSpace(raw)) }

// ValidateSettings checks the admin form (*UserError on failure).
func ValidateSettings(f SettingsForm) (SettingsWrite, error) {
	var w SettingsWrite
	keyType := strings.TrimSpace(f.PixKeyType)
	if keyType != "" && !contains(PixKeyTypes, keyType) {
		return w, userErr("Tipo de chave PIX inválido.")
	}
	key := optionalText(f.PixKey)
	switch {
	case key == nil && keyType == "":
	case key == nil || keyType == "":
		return w, userErr("Informe a chave e o tipo da chave PIX, ou deixe ambos em branco.")
	default:
		normalized, err := pix.NormalizeKey(keyType, *key)
		if err != nil {
			return w, userErr(err.Error())
		}
		w.PixKey, w.PixKeyType = &normalized, &keyType
	}
	w.PixMerchantName = optionalText(f.PixMerchantName)
	if w.PixMerchantName != nil && runeLen(*w.PixMerchantName) > 25 {
		return w, userErr("Nome do recebedor deve ter no máximo 25 caracteres.")
	}
	w.PixMerchantCity = optionalText(f.PixMerchantCity)
	if w.PixMerchantCity != nil && runeLen(*w.PixMerchantCity) > 15 {
		return w, userErr("Cidade deve ter no máximo 15 caracteres.")
	}
	pct := strings.TrimSpace(f.PixDiscountPercent)
	if pct == "" {
		w.PixDiscountPercent = 5
	} else {
		v, err := strconv.ParseUint(pct, 10, 32)
		if err != nil {
			return w, userErr("Desconto PIX inválido. Use um número entre 0 e 30.")
		}
		if v > 30 {
			return w, userErr("Desconto PIX deve estar entre 0 e 30%.")
		}
		w.PixDiscountPercent = int16(v) //nolint:gosec // bounded above
	}
	w.AnnouncementText = optionalText(f.AnnouncementText)
	w.FirstPurchaseCouponCode = optionalText(f.FirstPurchaseCouponCode)
	w.WhatsappNumber = optionalText(f.WhatsappNumber)
	w.WhatsappMessageTemplate = optionalText(f.WhatsappMessageTemplate)
	threshold, err := parseThresholdCents(f.FreeShippingThreshold)
	if err != nil {
		return w, err
	}
	w.FreeShippingThresholdCents = threshold
	return w, nil
}

func parseThresholdCents(raw string) (int64, error) {
	t := strings.TrimSpace(raw)
	if t == "" {
		return DefaultFreeShippingThresholdCents, nil
	}
	v, ok := parseDecimal(t)
	if !ok || v < 0 || v > 9_999_999 {
		return 0, userErr("Valor inválido. Use 299.00.")
	}
	return int64(math.Round(v * 100)), nil
}

// parseDecimal accepts "1.234,56", "1234,56", "1234.56" and "1234".
func parseDecimal(trimmed string) (float64, bool) {
	normalized := trimmed
	if strings.Contains(trimmed, ",") {
		normalized = strings.ReplaceAll(strings.ReplaceAll(trimmed, ".", ""), ",", ".")
	}
	v, err := strconv.ParseFloat(normalized, 64)
	if err != nil || math.IsInf(v, 0) || math.IsNaN(v) {
		return 0, false
	}
	// Rust's f64 parser rejects hex/underscore forms Go accepts.
	if strings.ContainsAny(normalized, "xXpP_") {
		return 0, false
	}
	return v, true
}

// UpsertStoreSettings writes the singleton (race-free single upsert).
func UpsertStoreSettings(ctx context.Context, q DBTX, w SettingsWrite) error {
	return queries(q).StoreUpsertSettings(ctx, sqlc.StoreUpsertSettingsParams(w))
}

// SaveSettings validates and saves the admin form. On success the UI shows
// "Configurações salvas com sucesso."; a *UserError carries the toast.
func SaveSettings(ctx context.Context, pool *pgxpool.Pool, f SettingsForm) error {
	w, err := ValidateSettings(f)
	if err != nil {
		return err
	}
	return UpsertStoreSettings(ctx, pool, w)
}
