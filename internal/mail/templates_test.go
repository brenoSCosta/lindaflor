package mail_test

import (
	"strings"
	"testing"

	"github.com/wellington-vell/lindaflor/internal/mail"
)

func sampleOrder() mail.OrderMail {
	return mail.OrderMail{
		Code:         "0192ABCD",
		Name:         "Ana <b>",
		TotalCents:   12_990,
		URL:          "https://loja.example.com/pedido/abc?t=tok&x=1",
		PixCopyPaste: "00020126BR.GOV.BCB.PIX",
		PayUntil:     "01/11/2026 10:00",
		Carrier:      "Correios",
	}
}

func mustContain(t *testing.T, haystack string, needles ...string) {
	t.Helper()
	for _, n := range needles {
		if !strings.Contains(haystack, n) {
			t.Fatalf("missing %q in:\n%s", n, haystack)
		}
	}
}

func mustNotContain(t *testing.T, haystack string, needles ...string) {
	t.Helper()
	for _, n := range needles {
		if strings.Contains(haystack, n) {
			t.Fatalf("unexpected %q in:\n%s", n, haystack)
		}
	}
}

func TestOrderCreatedForMercadoPagoLinksOrderPageWithoutStaticPix(t *testing.T) {
	o := sampleOrder()
	o.MercadoPago = true
	o.PixCopyPaste = "" // the hook never passes the static code for MP
	e := mail.OrderCreated(o)
	mustNotContain(t, e.Text, "00020126BR.GOV.BCB.PIX")
	mustNotContain(t, e.HTML, "00020126BR.GOV.BCB.PIX")
	mustContain(t, e.Text, "Mercado Pago", "Pagar com PIX: https://loja.example.com/pedido/abc?t=tok&x=1")

	// Even if a code slips through, MP mail never shows it.
	o.PixCopyPaste = "00020126BR.GOV.BCB.PIX"
	mustNotContain(t, mail.OrderCreated(o).Text, "00020126BR.GOV.BCB.PIX")
}

func TestOrderShippedCarriesCarrierCodeAndTrackingLink(t *testing.T) {
	link := "https://rastreamento.correios.com.br/app/index.php?objetos=AA123456789BR"
	o := sampleOrder()
	o.TrackingCode = "AA123456789BR"
	o.TrackingURL = link
	e := mail.OrderShipped(o)
	for _, part := range []string{e.Text, e.HTML} {
		mustContain(t, part, "Correios", "AA123456789BR", "rastreamento.correios.com.br", "/pedido/abc?t=tok")
	}
	mustContain(t, e.Text, "Rastrear entrega: "+link)

	other := sampleOrder()
	other.Carrier = "Jadlog"
	other.TrackingCode = "JD123"
	e = mail.OrderShipped(other)
	mustContain(t, e.Text, "Jadlog", "JD123", "Acompanhar pedido: https://loja.example.com/pedido/abc")
}

func TestEscapeCoversHTMLSpecials(t *testing.T) {
	if got := mail.Escape(`<a href="x">'&'</a>`); got != "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;" {
		t.Fatal(got)
	}
}

func TestAuthTemplatesCarryLinkInBothParts(t *testing.T) {
	url := "https://loja.example.com/reset-password?token=abc123"
	for _, e := range []mail.Email{
		mail.VerifyEmail("Ana", url, ""),
		mail.PasswordReset("Ana", url),
		mail.ChangeEmail("Ana", "nova@example.com", url),
		mail.DeleteAccount("Ana", url),
	} {
		mustContain(t, e.Subject, "Lindaflor")
		mustContain(t, e.Text, url, "Olá, Ana!")
		mustContain(t, e.HTML, url, `lang="pt-BR"`)
	}
}

func TestOrderCreatedIncludesPixAndEscapes(t *testing.T) {
	e := mail.OrderCreated(sampleOrder())
	mustContain(t, e.Subject, "#0192ABCD")
	mustContain(t, e.Text, "R$ 129,90", "00020126BR.GOV.BCB.PIX", "01/11/2026 10:00", "/pedido/abc?t=tok&x=1")
	mustContain(t, e.HTML, "Ana &lt;b&gt;", "/pedido/abc?t=tok&amp;x=1")
	mustNotContain(t, e.HTML, "Ana <b>")
}

func TestOrderStatusTemplatesRender(t *testing.T) {
	o := sampleOrder()
	mustContain(t, mail.PaymentConfirmed(o).Subject, "Pagamento")
	mustContain(t, mail.OrderShipped(o).Subject, "enviado")
	mustContain(t, mail.OrderCancelled(o, true).Subject, "expirado")
	mustContain(t, mail.OrderCancelled(o, false).Subject, "cancelado")
	mustContain(t, mail.OrderRefunded(o).Subject, "Reembolso")
}

func TestVerifyEmailMentionsFirstPurchaseCoupon(t *testing.T) {
	url := "https://x/verify-email?token=t"
	with := mail.VerifyEmail("Ana", url, " BEMVINDA10 ")
	mustContain(t, with.Text, "use o cupom BEMVINDA10 na sua primeira")
	mustContain(t, with.HTML, "BEMVINDA10")
	for _, code := range []string{"", "  "} {
		mustNotContain(t, mail.VerifyEmail("Ana", url, code).Text, "cupom")
	}
}

func TestNewsletterWelcomeMentionsCouponOnlyWhenConfigured(t *testing.T) {
	unsub := "https://loja.example.com/newsletter/cancelar?email=a&token=b"
	with := mail.NewsletterWelcome("Ana", "https://loja.example.com/", "BEMVINDA10", unsub)
	mustContain(t, with.Subject, "newsletter")
	mustContain(t, with.Text, "cupom BEMVINDA10")
	mustContain(t, with.HTML, "https://loja.example.com/")
	without := mail.NewsletterWelcome("", "https://loja.example.com/", "", unsub)
	mustNotContain(t, without.Text, "cupom")
	mustContain(t, without.Text, "Olá!")
}

func TestNewsletterWelcomeCarriesUnsubscribeLink(t *testing.T) {
	url := "https://loja.example.com/newsletter/cancelar?email=a&token=b"
	e := mail.NewsletterWelcome("Ana", "https://loja.example.com/", "", url)
	mustContain(t, e.Text, "Cancelar inscrição: "+url)
	mustContain(t, e.HTML, `href="https://loja.example.com/newsletter/cancelar?email=a&amp;token=b"`)
	mustNotContain(t, mail.PasswordReset("Ana", "https://x/reset?token=t").Text, "Cancelar inscrição")
}

func TestGreetingWithoutName(t *testing.T) {
	mustContain(t, mail.VerifyEmail("  ", "https://x", "").Text, "Olá!")
}

func TestReturnTemplatesRender(t *testing.T) {
	m := mail.ReturnMail{OrderCode: "0192ABCD", Name: "Ana", KindLabel: "Devolução", Items: []string{"1x Biquíni (M / azul)"}, URL: "https://x/pedido/1?t=2"}
	mustContain(t, mail.ReturnRequested(m).Text, "Recebemos sua solicitação (Devolução)", "Itens: 1x Biquíni (M / azul).")
	approved := mail.ReturnApproved(m, "Poste nos Correios")
	mustContain(t, approved.Text, "Instruções de envio:\nPoste nos Correios")
	mustContain(t, mail.ReturnRejected(m, "Fora do prazo").Text, "Motivo: Fora do prazo")
	cents := int64(5000)
	mustContain(t, mail.ReturnResolved(m, &cents).Text, "Reembolso concluído", "R$ 50,00")
	mustContain(t, mail.ReturnResolved(m, nil).Text, "Troca concluída")
}

func TestMarketingTemplatesCarryUnsubscribe(t *testing.T) {
	cancel := "https://x/avise-me/cancelar?email=a&token=b"
	e := mail.BackInStock("Biquíni Sol", "M · Azul", "https://x/produtos/sol", cancel)
	mustContain(t, e.Text, "Biquíni Sol (M · Azul) está disponível", "Cancelar inscrição: "+cancel)
	cart := mail.AbandonedCart("Ana", []mail.CartReminderLine{
		{ProductName: "Biquíni", VariantLabel: "M · Azul", Quantity: 2, UnitPriceCents: 8990},
		{ProductName: "Saída", VariantLabel: "U · Branco", Quantity: 1, UnitPriceCents: 5000},
	}, "https://x/checkout", "https://x/newsletter/cancelar?email=a&token=b")
	mustContain(t, cart.Text, "• Biquíni — M · Azul (x2) — R$ 89,90", "• Saída — U · Branco — R$ 50,00", "Finalizar compra: https://x/checkout")
}
