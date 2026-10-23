package store

import (
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/web/format"
)

// FormatPrice renders cents as "R$ 1.234,56".
func FormatPrice(cents int64) string { return format.Price(cents) }

// OrderCode is the short human reference of an order: the first 8 hex
// characters of its id, uppercase.
func OrderCode(orderID uuid.UUID) string {
	return strings.ToUpper(strings.ReplaceAll(orderID.String(), "-", "")[:8])
}

// CategoryLabel is the storefront label of a product category.
func CategoryLabel(category string) string {
	switch category {
	case "biquini":
		return "Biquíni"
	case "biquini_top":
		return "Top de biquíni"
	case "biquini_bottom":
		return "Calcinha de biquíni"
	case "maio":
		return "Maiô"
	case "saida_praia":
		return "Saída de praia"
	case "acessorio":
		return "Acessório"
	}
	return "Peça"
}

// AdminCategoryLabel is the admin product list label of a category
// (unknown values pass through).
func AdminCategoryLabel(category string) string {
	switch category {
	case "biquini":
		return "Biquíni"
	case "biquini_top":
		return "Top de Biquíni"
	case "biquini_bottom":
		return "Calcinha de Biquíni"
	case "maio":
		return "Maiô"
	case "saida_praia":
		return "Saída de Praia"
	case "acessorio":
		return "Acessório"
	}
	return category
}

// SizeLabel is the display label of a product_size value.
func SizeLabel(size string) string {
	switch size {
	case "pp":
		return "PP"
	case "p":
		return "P"
	case "m":
		return "M"
	case "g":
		return "G"
	case "gg":
		return "GG"
	}
	return "Único"
}

// VariantLabel is "M · Azul".
func VariantLabel(size, color string) string { return SizeLabel(size) + " · " + color }

// OrderStatusLabel is the pt-BR label of an order status.
func OrderStatusLabel(status string) string {
	switch status {
	case "pending_payment":
		return "Aguardando pagamento"
	case "paid":
		return "Pago"
	case "processing":
		return "Em separação"
	case "shipped":
		return "Enviado"
	case "delivered":
		return "Entregue"
	case "cancelled":
		return "Cancelado"
	case "refunded":
		return "Estornado"
	}
	return "Desconhecido"
}

// BadgeTone is a display hint for status badges (maps to the UI kit's
// badge variants).
type BadgeTone string

// Badge tones.
const (
	TonePrimary     BadgeTone = "primary"
	ToneSecondary   BadgeTone = "secondary"
	ToneDestructive BadgeTone = "destructive"
	ToneOutline     BadgeTone = "outline"
)

// CustomerOrderStatusTone is the badge tone of an order status on the
// customer pages (dashboard, conta/pedidos).
func CustomerOrderStatusTone(status string) BadgeTone {
	switch status {
	case "paid", "delivered":
		return TonePrimary
	case "cancelled":
		return ToneDestructive
	}
	return ToneSecondary
}

// AdminOrderStatusTone is the badge tone of an order status in the admin.
func AdminOrderStatusTone(status string) BadgeTone {
	switch status {
	case "pending_payment":
		return ToneSecondary
	case "cancelled":
		return ToneDestructive
	}
	return TonePrimary
}

// ProviderLabel is the label of orders.payment_provider.
func ProviderLabel(provider string) string {
	switch provider {
	case ProviderMercadoPago:
		return "Mercado Pago"
	case ProviderStaticPix:
		return "PIX (chave da loja)"
	}
	return "Outro"
}

// Payment provider values of orders.payment_provider.
const (
	ProviderStaticPix   = "static_pix"
	ProviderMercadoPago = "mercadopago"
)

// Categories in storefront order with their filter labels.
var Categories = []struct{ Key, Label string }{
	{"all", "Todos"},
	{"biquini", "Biquínis"},
	{"biquini_top", "Tops"},
	{"biquini_bottom", "Cantinhas"},
	{"maio", "Maiôs"},
	{"saida_praia", "Saídas"},
	{"acessorio", "Acessórios"},
}

// ValidCategories are the product_category enum values.
var ValidCategories = []string{"biquini", "biquini_top", "biquini_bottom", "maio", "saida_praia", "acessorio"}

// ValidSizes are the product_size enum values, smallest first.
var ValidSizes = []string{"pp", "p", "m", "g", "gg"}

func contains(list []string, v string) bool {
	for _, s := range list {
		if s == v {
			return true
		}
	}
	return false
}
