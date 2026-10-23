package admin

import (
	"net/http"

	"github.com/wellington-vell/lindaflor/internal/store"
	"github.com/wellington-vell/lindaflor/internal/web/render"
)

// /admin/configuracoes (src/app/admin/configuracoes.rs).

const settingsPath = "/admin/configuracoes"

// GET /admin/configuracoes.
func (h *handler) settingsPage(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok {
		return
	}
	s, err := store.GetStoreSettings(r.Context(), h.d.Pool)
	if err != nil {
		serverError(w, r, err)
		return
	}
	head(r, "Configurações da loja")
	render.Page(w, r, h.d, settingsPage(store.SettingsFormFrom(s)))
}

// POST /admin/configuracoes.
func (h *handler) settingsSave(w http.ResponseWriter, r *http.Request) {
	if _, ok := h.actor(w, r); !ok || !parseForm(w, r) {
		return
	}
	f := store.SettingsForm{
		PixKey: r.PostFormValue("pix_key"), PixKeyType: r.PostFormValue("pix_key_type"),
		PixMerchantName: r.PostFormValue("pix_merchant_name"), PixMerchantCity: r.PostFormValue("pix_merchant_city"),
		PixDiscountPercent: r.PostFormValue("pix_discount_percent"), AnnouncementText: r.PostFormValue("announcement_text"),
		FirstPurchaseCouponCode: r.PostFormValue("first_purchase_coupon_code"), WhatsappNumber: r.PostFormValue("whatsapp_number"),
		WhatsappMessageTemplate: r.PostFormValue("whatsapp_message_template"), FreeShippingThreshold: r.PostFormValue("free_shipping_threshold"),
	}
	if err := store.SaveSettings(r.Context(), h.d.Pool, f); err != nil {
		if msg, ok := store.UserMessage(err); ok {
			failTo(w, r, settingsPath, msg)
			return
		}
		serverError(w, r, err)
		return
	}
	okTo(w, r, settingsPath, "Configurações salvas com sucesso.")
}
