package mail

import (
	"fmt"
	"strings"

	"github.com/google/uuid"

	"github.com/wellington-vell/lindaflor/internal/web/format"
)

// Brand is the name in every subject and the mail header.
const Brand = "Lindaflor"

// Email is a rendered message (subject + HTML and text alternatives).
type Email struct {
	Subject string
	HTML    string
	Text    string
}

// Msg turns the email into an outbox message.
func (e Email) Msg(kind, to, dedupeKey string) Msg {
	return Msg{Kind: kind, To: to, Subject: e.Subject, HTML: e.HTML, Text: e.Text, DedupeKey: dedupeKey}
}

// Escape escapes text for HTML element and attribute contexts
// (& < > " ').
func Escape(raw string) string {
	var b strings.Builder
	b.Grow(len(raw))
	for _, r := range raw {
		switch r {
		case '&':
			b.WriteString("&amp;")
		case '<':
			b.WriteString("&lt;")
		case '>':
			b.WriteString("&gt;")
		case '"':
			b.WriteString("&quot;")
		case '\'':
			b.WriteString("&#39;")
		default:
			b.WriteRune(r)
		}
	}
	return b.String()
}

// labeled is a (label, value) pair: an action button or a code block.
type labeled struct{ label, value string }

// body is one block of copy rendered to both HTML and text from the same
// data so the two alternatives never drift.
type body struct {
	heading    string
	paragraphs []string
	action     *labeled // call-to-action (label, url)
	code       *labeled // preformatted block (label, code)
	footnote   string
}

func render(subject string, b body) Email { return renderWithUnsubscribe(subject, b, "") }

// renderWithUnsubscribe adds a "Cancelar inscrição" footer for marketing mail.
func renderWithUnsubscribe(subject string, b body, unsubscribeURL string) Email {
	var h strings.Builder
	h.WriteString(`<!doctype html><html lang="pt-BR"><head><meta charset="utf-8">` +
		`<meta name="viewport" content="width=device-width,initial-scale=1">`)
	fmt.Fprintf(&h, "<title>%s</title></head>", Escape(subject))
	h.WriteString(`<body style="margin:0;padding:0;background:#f6f1ee;` +
		`font-family:Helvetica,Arial,sans-serif;color:#2b2b2b;">` +
		`<table role="presentation" width="100%" cellpadding="0" ` +
		`cellspacing="0" style="background:#f6f1ee;padding:24px 0;"><tr>` +
		`<td align="center"><table role="presentation" width="100%" ` +
		`cellpadding="0" cellspacing="0" style="max-width:560px;` +
		`background:#ffffff;border-radius:12px;padding:32px;"><tr><td>`)
	fmt.Fprintf(&h, `<p style="margin:0 0 24px;font-size:22px;font-weight:bold;color:#b4536b;">%s</p>`, Brand)
	fmt.Fprintf(&h, `<h1 style="margin:0 0 16px;font-size:20px;">%s</h1>`, Escape(b.heading))
	for _, p := range b.paragraphs {
		fmt.Fprintf(&h, `<p style="margin:0 0 16px;font-size:15px;line-height:1.5;">%s</p>`, Escape(p))
	}
	if c := b.code; c != nil {
		fmt.Fprintf(&h, `<p style="margin:0 0 8px;font-size:13px;color:#666;">%s</p>`+
			`<pre style="margin:0 0 16px;padding:12px;background:#f6f1ee;`+
			`border-radius:8px;font-size:12px;white-space:pre-wrap;`+
			`word-break:break-all;">%s</pre>`, Escape(c.label), Escape(c.value))
	}
	if a := b.action; a != nil {
		url, label := Escape(a.value), Escape(a.label)
		fmt.Fprintf(&h, `<p style="margin:24px 0;"><a href="%s" style="display:inline-block;`+
			`background:#b4536b;color:#ffffff;text-decoration:none;font-weight:bold;`+
			`padding:12px 24px;border-radius:8px;">%s</a></p>`+
			`<p style="margin:0 0 16px;font-size:12px;color:#666;">`+
			`Se o botão não funcionar, copie e cole este endereço no navegador:<br>`+
			`<a href="%s" style="color:#b4536b;word-break:break-all;">%s</a></p>`, url, label, url, url)
	}
	if b.footnote != "" {
		fmt.Fprintf(&h, `<p style="margin:16px 0 0;font-size:12px;color:#666;">%s</p>`, Escape(b.footnote))
	}
	fmt.Fprintf(&h, `</td></tr></table><p style="font-size:11px;color:#999;">%s · Esta é uma mensagem automática, não responda.</p>`, Brand)
	if unsubscribeURL != "" {
		fmt.Fprintf(&h, `<p style="font-size:11px;color:#999;">Não quer mais receber nossos `+
			`e-mails? <a href="%s" style="color:#999;">Cancelar inscrição</a></p>`, Escape(unsubscribeURL))
	}
	h.WriteString("</td></tr></table></body></html>")

	var t strings.Builder
	fmt.Fprintf(&t, "%s\n\n%s\n\n", Brand, b.heading)
	for _, p := range b.paragraphs {
		t.WriteString(p)
		t.WriteString("\n\n")
	}
	if c := b.code; c != nil {
		fmt.Fprintf(&t, "%s\n%s\n\n", c.label, c.value)
	}
	if a := b.action; a != nil {
		fmt.Fprintf(&t, "%s: %s\n\n", a.label, a.value)
	}
	if b.footnote != "" {
		t.WriteString(b.footnote)
		t.WriteString("\n\n")
	}
	fmt.Fprintf(&t, "-- \n%s · Esta é uma mensagem automática, não responda.\n", Brand)
	if unsubscribeURL != "" {
		fmt.Fprintf(&t, "Cancelar inscrição: %s\n", unsubscribeURL)
	}
	return Email{Subject: subject, HTML: h.String(), Text: t.String()}
}

func greeting(name string) string {
	name = strings.TrimSpace(name)
	if name == "" {
		return "Olá!"
	}
	return "Olá, " + name + "!"
}

const ignoreNote = "Se você não fez esta solicitação, ignore este e-mail com segurança."

func welcomeCoupon(code string) (string, bool) {
	code = strings.TrimSpace(code)
	if code == "" {
		return "", false
	}
	return "Presente de boas-vindas: use o cupom " + code + " na sua primeira compra.", true
}

// --- Auth ---

// VerifyEmail is the account confirmation mail. firstPurchaseCode is the
// storefront's welcome coupon (store_settings.first_purchase_coupon_code),
// shown to new customers when set.
func VerifyEmail(name, url, firstPurchaseCode string) Email {
	paragraphs := []string{
		greeting(name),
		"Para ativar sua conta, confirme seu endereço de e-mail clicando no botão abaixo. O link vale por 24 horas.",
	}
	if p, ok := welcomeCoupon(firstPurchaseCode); ok {
		paragraphs = append(paragraphs, p)
	}
	return render("Confirme seu e-mail · "+Brand, body{
		heading:    "Confirme seu e-mail",
		paragraphs: paragraphs,
		action:     &labeled{"Confirmar e-mail", url},
		footnote:   ignoreNote,
	})
}

// PasswordReset is the password reset mail.
func PasswordReset(name, url string) Email {
	return render("Redefinição de senha · "+Brand, body{
		heading: "Redefina sua senha",
		paragraphs: []string{
			greeting(name),
			"Recebemos um pedido para redefinir a senha da sua conta. O link abaixo vale por 1 hora e só pode ser usado uma vez.",
		},
		action:   &labeled{"Redefinir senha", url},
		footnote: ignoreNote,
	})
}

// ChangeEmail confirms a new address (sent to the new address).
func ChangeEmail(name, newEmail, url string) Email {
	return render("Confirme seu novo e-mail · "+Brand, body{
		heading: "Confirme a troca de e-mail",
		paragraphs: []string{
			greeting(name),
			"Recebemos um pedido para alterar o e-mail da sua conta para " + newEmail + ". Confirme pelo botão abaixo; o link vale por 30 minutos.",
		},
		action:   &labeled{"Confirmar novo e-mail", url},
		footnote: ignoreNote,
	})
}

// DeleteAccount confirms an account deletion.
func DeleteAccount(name, url string) Email {
	return render("Confirme a exclusão da sua conta · "+Brand, body{
		heading: "Excluir sua conta",
		paragraphs: []string{
			greeting(name),
			"Recebemos um pedido para excluir sua conta. Essa ação é permanente: seus dados de acesso serão apagados. O link abaixo vale por 30 minutos.",
		},
		action:   &labeled{"Confirmar exclusão", url},
		footnote: "Se você não pediu a exclusão, ignore este e-mail e considere trocar sua senha.",
	})
}

// --- Newsletter ---

// NewsletterWelcome welcomes a new subscriber; repeats the first-purchase
// coupon when configured.
func NewsletterWelcome(name, shopURL, firstPurchaseCode, unsubscribeURL string) Email {
	paragraphs := []string{
		greeting(name),
		"Obrigada por se inscrever na newsletter! Você vai receber em primeira mão nossos lançamentos, coleções e promoções.",
	}
	if p, ok := welcomeCoupon(firstPurchaseCode); ok {
		paragraphs = append(paragraphs, p)
	}
	return renderWithUnsubscribe("Boas-vindas à newsletter · "+Brand, body{
		heading:    "Que bom ter você por aqui",
		paragraphs: paragraphs,
		action:     &labeled{"Visitar a loja", shopURL},
		footnote:   `Se você não se inscreveu, use o link "Cancelar inscrição" no fim deste e-mail.`,
	}, unsubscribeURL)
}

// --- Commerce ---

// OrderMail is what every order email needs (built by the order hooks).
type OrderMail struct {
	Code         string // OrderCode(id)
	Name         string
	TotalCents   int64
	URL          string // absolute /pedido/{id}?t=...
	PixCopyPaste string // "" when none
	PayUntil     string // "dd/mm/aaaa hh:mm" (Brasília), "" when unknown
	MercadoPago  bool   // QR Code lives on the order page
	Carrier      string
	TrackingCode string // "" when none
	TrackingURL  string // carrier's public tracking page, "" when unknown
}

// OrderCode is the short human reference: first 8 hex chars, uppercase.
func OrderCode(orderID uuid.UUID) string {
	return strings.ToUpper(strings.ReplaceAll(orderID.String(), "-", "")[:8])
}

// OrderCreated is "Pedido recebido" with the payment instructions.
func OrderCreated(o OrderMail) Email {
	paragraphs := []string{
		greeting(o.Name),
		fmt.Sprintf("Recebemos seu pedido #%s no valor de %s. Para concluir, pague via PIX.", o.Code, format.Price(o.TotalCents)),
	}
	if o.PayUntil != "" {
		paragraphs = append(paragraphs, "Reservamos seus itens até "+o.PayUntil+". Depois disso o pedido é cancelado automaticamente.")
	} else {
		paragraphs = append(paragraphs, "Seus itens ficam reservados por tempo limitado.")
	}
	if o.MercadoPago {
		paragraphs = append(paragraphs, "O pagamento é processado pelo Mercado Pago: abra a página do pedido para ver o QR Code e o código copia-e-cola. A confirmação é automática.")
	} else {
		paragraphs = append(paragraphs, "Na página do pedido você encontra o QR Code e o código copia-e-cola, e acompanha a confirmação do pagamento.")
	}
	actionLabel := "Ver instruções de pagamento"
	var code *labeled
	if o.MercadoPago {
		actionLabel = "Pagar com PIX"
	} else if o.PixCopyPaste != "" {
		code = &labeled{"PIX copia-e-cola:", o.PixCopyPaste}
	}
	return render(fmt.Sprintf("Pedido #%s recebido · %s", o.Code, Brand), body{
		heading:    "Pedido recebido — falta o pagamento",
		paragraphs: paragraphs,
		action:     &labeled{actionLabel, o.URL},
		code:       code,
		footnote:   "Guarde este e-mail: o link acima dá acesso ao seu pedido.",
	})
}

// PaymentConfirmed is sent when the order is paid.
func PaymentConfirmed(o OrderMail) Email {
	return render(fmt.Sprintf("Pagamento confirmado · pedido #%s · %s", o.Code, Brand), body{
		heading: "Pagamento confirmado!",
		paragraphs: []string{
			greeting(o.Name),
			fmt.Sprintf("Confirmamos o pagamento de %s do pedido #%s. Agora vamos separar e preparar seus itens com carinho.", format.Price(o.TotalCents), o.Code),
			"Avisaremos por e-mail assim que o pedido for enviado.",
		},
		action: &labeled{"Acompanhar pedido", o.URL},
	})
}

// OrderShipped carries the carrier, tracking code and tracking link.
func OrderShipped(o OrderMail) Email {
	carrier := o.Carrier
	if carrier == "" {
		carrier = "a transportadora"
	}
	paragraphs := []string{greeting(o.Name)}
	if o.TrackingCode != "" {
		paragraphs = append(paragraphs, fmt.Sprintf("O pedido #%s foi enviado por %s. Use o código de rastreio abaixo para acompanhar a entrega.", o.Code, carrier))
	} else {
		paragraphs = append(paragraphs, fmt.Sprintf("O pedido #%s foi enviado por %s. Acompanhe o status pela página do pedido.", o.Code, carrier))
	}
	// The carrier page is the main action when we have one; the order page
	// stays reachable through the footnote.
	action := &labeled{"Acompanhar pedido", o.URL}
	footnote := ""
	if o.TrackingURL != "" {
		action = &labeled{"Rastrear entrega", o.TrackingURL}
		footnote = "Detalhes do pedido: " + o.URL
	}
	var code *labeled
	if o.TrackingCode != "" {
		code = &labeled{"Código de rastreio:", o.TrackingCode}
	}
	return render(fmt.Sprintf("Pedido #%s enviado · %s", o.Code, Brand), body{
		heading:    "Seu pedido está a caminho",
		paragraphs: paragraphs,
		action:     action,
		code:       code,
		footnote:   footnote,
	})
}

// OrderCancelled is the cancellation mail; expired marks a cancel by the
// reservation deadline.
func OrderCancelled(o OrderMail, expired bool) Email {
	subject := fmt.Sprintf("Pedido #%s cancelado · %s", o.Code, Brand)
	heading := "Pedido cancelado"
	reason := fmt.Sprintf("O pedido #%s foi cancelado.", o.Code)
	if expired {
		subject = fmt.Sprintf("Pedido #%s expirado · %s", o.Code, Brand)
		heading = "Pedido cancelado por falta de pagamento"
		reason = fmt.Sprintf("Não identificamos o pagamento do pedido #%s dentro do prazo, então ele foi cancelado e os itens voltaram ao estoque.", o.Code)
	}
	return render(subject, body{
		heading: heading,
		paragraphs: []string{
			greeting(o.Name),
			reason,
			"Se ainda quiser os produtos, é só fazer um novo pedido na loja. Em caso de dúvida, fale com a gente.",
		},
		action: &labeled{"Ver pedido", o.URL},
	})
}

// OrderRefunded is the refund mail.
func OrderRefunded(o OrderMail) Email {
	return render(fmt.Sprintf("Reembolso do pedido #%s · %s", o.Code, Brand), body{
		heading: "Reembolso realizado",
		paragraphs: []string{
			greeting(o.Name),
			fmt.Sprintf("O reembolso do pedido #%s (%s) foi processado. O valor é devolvido via PIX para a conta de origem do pagamento.", o.Code, format.Price(o.TotalCents)),
		},
		action: &labeled{"Ver pedido", o.URL},
	})
}

// --- Notifications ---

// BackInStock is the "Avise-me" alert. cancelURL stops every pending
// alert of the address (Links.StockAlertCancelURL).
func BackInStock(productName, variantLabel, productURL, cancelURL string) Email {
	return renderWithUnsubscribe(productName+" voltou ao estoque · "+Brand, body{
		heading: "A peça que você queria voltou!",
		paragraphs: []string{
			"Olá!",
			fmt.Sprintf("%s (%s) está disponível de novo. O estoque é limitado: garanta a sua antes que acabe.", productName, variantLabel),
		},
		action:   &labeled{"Ver peça", productURL},
		footnote: "Você recebeu este e-mail porque pediu para ser avisada quando a peça voltasse. Este aviso é enviado uma única vez.",
	}, cancelURL)
}

// CartReminderLine is one line of an abandoned-cart reminder.
type CartReminderLine struct {
	ProductName    string
	VariantLabel   string // e.g. "M · Azul"
	Quantity       int32
	UnitPriceCents int64
}

// AbandonedCart is the cart reminder (marketing: carries the newsletter
// unsubscribe link; the worker adds List-Unsubscribe).
func AbandonedCart(name string, lines []CartReminderLine, checkoutURL, unsubscribeURL string) Email {
	paragraphs := []string{
		greeting(name),
		"Você deixou algumas peças no carrinho. Separamos tudo para você finalizar a compra:",
	}
	for _, l := range lines {
		qty := ""
		if l.Quantity > 1 {
			qty = fmt.Sprintf(" (x%d)", l.Quantity)
		}
		paragraphs = append(paragraphs, fmt.Sprintf("• %s — %s%s — %s", l.ProductName, l.VariantLabel, qty, format.Price(l.UnitPriceCents)))
	}
	paragraphs = append(paragraphs, "Os itens não ficam reservados até o pagamento: o estoque pode acabar.")
	return renderWithUnsubscribe("Suas peças estão esperando · "+Brand, body{
		heading:    "Esqueceu algo no carrinho?",
		paragraphs: paragraphs,
		action:     &labeled{"Finalizar compra", checkoutURL},
		footnote:   "Abra o link no mesmo navegador em que você montou o carrinho.",
	}, unsubscribeURL)
}

// --- Returns / exchanges ---

// ReturnMail is what every return-request email needs.
type ReturnMail struct {
	OrderCode string
	Name      string
	KindLabel string   // "Troca de tamanho" / "Devolução" / "Defeito"
	Items     []string // e.g. "1x Biquíni Flor (M / azul)"
	URL       string   // absolute /pedido/{id}?t=...
}

func returnParagraphs(m ReturnMail, lead string) []string {
	p := []string{greeting(m.Name), lead}
	if len(m.Items) > 0 {
		p = append(p, "Itens: "+strings.Join(m.Items, "; ")+".")
	}
	return p
}

// ReturnRequested acknowledges a new return request.
func ReturnRequested(m ReturnMail) Email {
	return render(fmt.Sprintf("Solicitação de troca/devolução · pedido #%s · %s", m.OrderCode, Brand), body{
		heading:    "Recebemos sua solicitação",
		paragraphs: returnParagraphs(m, fmt.Sprintf("Recebemos sua solicitação (%s) para o pedido #%s. Vamos analisar e responder por e-mail com as instruções de envio.", m.KindLabel, m.OrderCode)),
		action:     &labeled{"Acompanhar solicitação", m.URL},
		footnote:   "Lembre-se: a peça deve estar sem uso, com etiquetas e na embalagem original.",
	})
}

// ReturnApproved carries the reverse-logistics instructions.
func ReturnApproved(m ReturnMail, instructions string) Email {
	return render(fmt.Sprintf("Troca/devolução aprovada · pedido #%s · %s", m.OrderCode, Brand), body{
		heading:    "Solicitação aprovada — envie a peça",
		paragraphs: returnParagraphs(m, fmt.Sprintf("Sua solicitação (%s) do pedido #%s foi aprovada. Siga as instruções abaixo para enviar a peça de volta.", m.KindLabel, m.OrderCode)),
		action:     &labeled{"Ver pedido", m.URL},
		code:       &labeled{"Instruções de envio:", instructions},
		footnote:   "O frete de devolução por arrependimento é por conta do cliente, exceto em casos de defeito ou erro no envio.",
	})
}

// ReturnRejected carries the reason given to the customer.
func ReturnRejected(m ReturnMail, reason string) Email {
	p := returnParagraphs(m, fmt.Sprintf("Analisamos sua solicitação (%s) do pedido #%s e infelizmente não pudemos aceitá-la.", m.KindLabel, m.OrderCode))
	p = append(p, "Motivo: "+reason, "Se tiver dúvidas, fale com a gente pelos canais de atendimento.")
	return render(fmt.Sprintf("Solicitação de troca/devolução · pedido #%s · %s", m.OrderCode, Brand), body{
		heading:    "Não pudemos aceitar sua solicitação",
		paragraphs: p,
		action:     &labeled{"Ver pedido", m.URL},
	})
}

// ReturnResolved: refundCents non-nil for a refund, nil for a completed
// exchange.
func ReturnResolved(m ReturnMail, refundCents *int64) Email {
	lead := fmt.Sprintf("Recebemos e conferimos a peça do pedido #%s. A troca foi concluída e a nova peça segue para você; avisaremos o código de rastreio.", m.OrderCode)
	heading := "Troca concluída"
	if refundCents != nil {
		lead = fmt.Sprintf("Recebemos e conferimos a peça do pedido #%s. O reembolso de %s foi processado e é devolvido via PIX para a conta de origem do pagamento.", m.OrderCode, format.Price(*refundCents))
		heading = "Reembolso concluído"
	}
	return render(fmt.Sprintf("Troca/devolução concluída · pedido #%s · %s", m.OrderCode, Brand), body{
		heading:    heading,
		paragraphs: returnParagraphs(m, lead),
		action:     &labeled{"Ver pedido", m.URL},
	})
}
