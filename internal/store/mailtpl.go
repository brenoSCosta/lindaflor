package store

import (
	"fmt"
	"html"
	"strings"
)

// Customer mail rendered by the store (orders, returns, back-in-stock,
// abandoned cart, newsletter welcome). The data loading and the outbox
// enqueue live next to the state change (same transaction); rendering goes
// through MailRenderer so the wave-1 mail package can supply its own
// templates via SetMailRenderer. DefaultRenderer is a faithful port of the
// commerce part of src/mail/templates.rs.

const brand = "Lindaflor"

// Email is one rendered message.
type Email struct {
	Subject string
	HTML    string
	Text    string
}

// OrderMail is what every order email needs.
type OrderMail struct {
	Code         string // OrderCode
	Name         string
	TotalCents   int64
	URL          string // absolute /pedido/{id}?t=...
	PixCopyPaste *string
	PayUntil     *string // "dd/mm/aaaa hh:mm" (Brasília)
	MercadoPago  bool
	Carrier      string
	TrackingCode *string
	TrackingURL  *string
}

// ReturnMail is what every return-request email needs.
type ReturnMail struct {
	OrderCode string
	Name      string
	KindLabel string
	Items     []string // "1x Biquíni Flor (M / azul)"
	URL       string
}

// CartReminderLine is one line of an abandoned-cart reminder.
type CartReminderLine struct {
	ProductName    string
	VariantLabel   string
	Quantity       int32
	UnitPriceCents int64
}

// MailRenderer renders the store's customer mail.
type MailRenderer interface {
	OrderCreated(OrderMail) Email
	PaymentConfirmed(OrderMail) Email
	OrderShipped(OrderMail) Email
	OrderCancelled(o OrderMail, expired bool) Email
	OrderRefunded(OrderMail) Email
	BackInStock(productName, variantLabel, productURL, cancelURL string) Email
	AbandonedCart(name string, lines []CartReminderLine, checkoutURL, unsubscribeURL string) Email
	NewsletterWelcome(name, shopURL string, firstPurchaseCode *string, unsubscribeURL string) Email
	ReturnRequested(ReturnMail) Email
	ReturnApproved(m ReturnMail, instructions string) Email
	ReturnRejected(m ReturnMail, reason string) Email
	ReturnResolved(m ReturnMail, refundCents *int64) Email
}

var renderer MailRenderer = DefaultRenderer{}

// SetMailRenderer swaps the templates (call once at boot).
func SetMailRenderer(r MailRenderer) {
	if r != nil {
		renderer = r
	}
}

// DefaultRenderer ports src/mail/templates.rs.
type DefaultRenderer struct{}

type mailBody struct {
	heading    string
	paragraphs []string
	action     *[2]string // label, url
	code       *[2]string // label, code
	footnote   string
}

func esc(s string) string { return html.EscapeString(s) }

func renderMail(subject string, body mailBody, unsubscribeURL string) Email {
	var h strings.Builder
	h.WriteString(`<!doctype html><html lang="pt-BR"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width,initial-scale=1">`)
	fmt.Fprintf(&h, "<title>%s</title></head>", esc(subject))
	h.WriteString(`<body style="margin:0;padding:0;background:#f6f1ee;font-family:Helvetica,Arial,sans-serif;color:#2b2b2b;"><table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="background:#f6f1ee;padding:24px 0;"><tr><td align="center"><table role="presentation" width="100%" cellpadding="0" cellspacing="0" style="max-width:560px;background:#ffffff;border-radius:12px;padding:32px;"><tr><td>`)
	fmt.Fprintf(&h, `<p style="margin:0 0 24px;font-size:22px;font-weight:bold;color:#b4536b;">%s</p>`, brand)
	fmt.Fprintf(&h, `<h1 style="margin:0 0 16px;font-size:20px;">%s</h1>`, esc(body.heading))
	for _, p := range body.paragraphs {
		fmt.Fprintf(&h, `<p style="margin:0 0 16px;font-size:15px;line-height:1.5;">%s</p>`, esc(p))
	}
	if body.code != nil {
		fmt.Fprintf(&h, `<p style="margin:0 0 8px;font-size:13px;color:#666;">%s</p><pre style="margin:0 0 16px;padding:12px;background:#f6f1ee;border-radius:8px;font-size:12px;white-space:pre-wrap;word-break:break-all;">%s</pre>`, esc(body.code[0]), esc(body.code[1]))
	}
	if body.action != nil {
		label, url := esc(body.action[0]), esc(body.action[1])
		fmt.Fprintf(&h, `<p style="margin:24px 0;"><a href="%[2]s" style="display:inline-block;background:#b4536b;color:#ffffff;text-decoration:none;font-weight:bold;padding:12px 24px;border-radius:8px;">%[1]s</a></p><p style="margin:0 0 16px;font-size:12px;color:#666;">Se o botão não funcionar, copie e cole este endereço no navegador:<br><a href="%[2]s" style="color:#b4536b;word-break:break-all;">%[2]s</a></p>`, label, url)
	}
	if body.footnote != "" {
		fmt.Fprintf(&h, `<p style="margin:16px 0 0;font-size:12px;color:#666;">%s</p>`, esc(body.footnote))
	}
	fmt.Fprintf(&h, `</td></tr></table><p style="font-size:11px;color:#999;">%s · Esta é uma mensagem automática, não responda.</p>`, brand)
	if unsubscribeURL != "" {
		fmt.Fprintf(&h, `<p style="font-size:11px;color:#999;">Não quer mais receber nossos e-mails? <a href="%s" style="color:#999;">Cancelar inscrição</a></p>`, esc(unsubscribeURL))
	}
	h.WriteString("</td></tr></table></body></html>")

	var t strings.Builder
	fmt.Fprintf(&t, "%s\n\n%s\n\n", brand, body.heading)
	for _, p := range body.paragraphs {
		t.WriteString(p)
		t.WriteString("\n\n")
	}
	if body.code != nil {
		fmt.Fprintf(&t, "%s\n%s\n\n", body.code[0], body.code[1])
	}
	if body.action != nil {
		fmt.Fprintf(&t, "%s: %s\n\n", body.action[0], body.action[1])
	}
	if body.footnote != "" {
		t.WriteString(body.footnote)
		t.WriteString("\n\n")
	}
	fmt.Fprintf(&t, "-- \n%s · Esta é uma mensagem automática, não responda.\n", brand)
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

func pair(a, b string) *[2]string { return &[2]string{a, b} }

// NewsletterWelcome implements MailRenderer.
func (DefaultRenderer) NewsletterWelcome(name, shopURL string, firstPurchaseCode *string, unsubscribeURL string) Email {
	paragraphs := []string{
		greeting(name),
		"Obrigada por se inscrever na newsletter! Você vai receber em primeira mão nossos lançamentos, coleções e promoções.",
	}
	if firstPurchaseCode != nil {
		if code := strings.TrimSpace(*firstPurchaseCode); code != "" {
			paragraphs = append(paragraphs, fmt.Sprintf("Presente de boas-vindas: use o cupom %s na sua primeira compra.", code))
		}
	}
	return renderMail("Boas-vindas à newsletter · "+brand, mailBody{
		heading:    "Que bom ter você por aqui",
		paragraphs: paragraphs,
		action:     pair("Visitar a loja", shopURL),
		footnote:   `Se você não se inscreveu, use o link "Cancelar inscrição" no fim deste e-mail.`,
	}, unsubscribeURL)
}

// OrderCreated implements MailRenderer.
func (DefaultRenderer) OrderCreated(o OrderMail) Email {
	paragraphs := []string{
		greeting(o.Name),
		fmt.Sprintf("Recebemos seu pedido #%s no valor de %s. Para concluir, pague via PIX.", o.Code, FormatPrice(o.TotalCents)),
	}
	if o.PayUntil != nil {
		paragraphs = append(paragraphs, fmt.Sprintf("Reservamos seus itens até %s. Depois disso o pedido é cancelado automaticamente.", *o.PayUntil))
	} else {
		paragraphs = append(paragraphs, "Seus itens ficam reservados por tempo limitado.")
	}
	if o.MercadoPago {
		paragraphs = append(paragraphs, "O pagamento é processado pelo Mercado Pago: abra a página do pedido para ver o QR Code e o código copia-e-cola. A confirmação é automática.")
	} else {
		paragraphs = append(paragraphs, "Na página do pedido você encontra o QR Code e o código copia-e-cola, e acompanha a confirmação do pagamento.")
	}
	label := "Ver instruções de pagamento"
	var code *[2]string
	if o.MercadoPago {
		label = "Pagar com PIX"
	} else if o.PixCopyPaste != nil {
		code = pair("PIX copia-e-cola:", *o.PixCopyPaste)
	}
	return renderMail(fmt.Sprintf("Pedido #%s recebido · %s", o.Code, brand), mailBody{
		heading:    "Pedido recebido — falta o pagamento",
		paragraphs: paragraphs,
		action:     pair(label, o.URL),
		code:       code,
		footnote:   "Guarde este e-mail: o link acima dá acesso ao seu pedido.",
	}, "")
}

// PaymentConfirmed implements MailRenderer.
func (DefaultRenderer) PaymentConfirmed(o OrderMail) Email {
	return renderMail(fmt.Sprintf("Pagamento confirmado · pedido #%s · %s", o.Code, brand), mailBody{
		heading: "Pagamento confirmado!",
		paragraphs: []string{
			greeting(o.Name),
			fmt.Sprintf("Confirmamos o pagamento de %s do pedido #%s. Agora vamos separar e preparar seus itens com carinho.", FormatPrice(o.TotalCents), o.Code),
			"Avisaremos por e-mail assim que o pedido for enviado.",
		},
		action: pair("Acompanhar pedido", o.URL),
	}, "")
}

// OrderShipped implements MailRenderer.
func (DefaultRenderer) OrderShipped(o OrderMail) Email {
	carrier := o.Carrier
	if carrier == "" {
		carrier = "a transportadora"
	}
	paragraphs := []string{greeting(o.Name)}
	if o.TrackingCode != nil {
		paragraphs = append(paragraphs, fmt.Sprintf("O pedido #%s foi enviado por %s. Use o código de rastreio abaixo para acompanhar a entrega.", o.Code, carrier))
	} else {
		paragraphs = append(paragraphs, fmt.Sprintf("O pedido #%s foi enviado por %s. Acompanhe o status pela página do pedido.", o.Code, carrier))
	}
	action := pair("Acompanhar pedido", o.URL)
	footnote := ""
	if o.TrackingURL != nil {
		action = pair("Rastrear entrega", *o.TrackingURL)
		footnote = "Detalhes do pedido: " + o.URL
	}
	var code *[2]string
	if o.TrackingCode != nil {
		code = pair("Código de rastreio:", *o.TrackingCode)
	}
	return renderMail(fmt.Sprintf("Pedido #%s enviado · %s", o.Code, brand), mailBody{
		heading:    "Seu pedido está a caminho",
		paragraphs: paragraphs,
		action:     action,
		code:       code,
		footnote:   footnote,
	}, "")
}

// OrderCancelled implements MailRenderer.
func (DefaultRenderer) OrderCancelled(o OrderMail, expired bool) Email {
	subject := fmt.Sprintf("Pedido #%s cancelado · %s", o.Code, brand)
	heading := "Pedido cancelado"
	reason := fmt.Sprintf("O pedido #%s foi cancelado.", o.Code)
	if expired {
		subject = fmt.Sprintf("Pedido #%s expirado · %s", o.Code, brand)
		heading = "Pedido cancelado por falta de pagamento"
		reason = fmt.Sprintf("Não identificamos o pagamento do pedido #%s dentro do prazo, então ele foi cancelado e os itens voltaram ao estoque.", o.Code)
	}
	return renderMail(subject, mailBody{
		heading: heading,
		paragraphs: []string{
			greeting(o.Name),
			reason,
			"Se ainda quiser os produtos, é só fazer um novo pedido na loja. Em caso de dúvida, fale com a gente.",
		},
		action: pair("Ver pedido", o.URL),
	}, "")
}

// OrderRefunded implements MailRenderer.
func (DefaultRenderer) OrderRefunded(o OrderMail) Email {
	return renderMail(fmt.Sprintf("Reembolso do pedido #%s · %s", o.Code, brand), mailBody{
		heading: "Reembolso realizado",
		paragraphs: []string{
			greeting(o.Name),
			fmt.Sprintf("O reembolso do pedido #%s (%s) foi processado. O valor é devolvido via PIX para a conta de origem do pagamento.", o.Code, FormatPrice(o.TotalCents)),
		},
		action: pair("Ver pedido", o.URL),
	}, "")
}

// BackInStock implements MailRenderer.
func (DefaultRenderer) BackInStock(productName, variantLabel, productURL, cancelURL string) Email {
	return renderMail(fmt.Sprintf("%s voltou ao estoque · %s", productName, brand), mailBody{
		heading: "A peça que você queria voltou!",
		paragraphs: []string{
			"Olá!",
			fmt.Sprintf("%s (%s) está disponível de novo. O estoque é limitado: garanta a sua antes que acabe.", productName, variantLabel),
		},
		action:   pair("Ver peça", productURL),
		footnote: "Você recebeu este e-mail porque pediu para ser avisada quando a peça voltasse. Este aviso é enviado uma única vez.",
	}, cancelURL)
}

// AbandonedCart implements MailRenderer.
func (DefaultRenderer) AbandonedCart(name string, lines []CartReminderLine, checkoutURL, unsubscribeURL string) Email {
	paragraphs := []string{
		greeting(name),
		"Você deixou algumas peças no carrinho. Separamos tudo para você finalizar a compra:",
	}
	for _, l := range lines {
		qty := ""
		if l.Quantity > 1 {
			qty = fmt.Sprintf(" (x%d)", l.Quantity)
		}
		paragraphs = append(paragraphs, fmt.Sprintf("• %s — %s%s — %s", l.ProductName, l.VariantLabel, qty, FormatPrice(l.UnitPriceCents)))
	}
	paragraphs = append(paragraphs, "Os itens não ficam reservados até o pagamento: o estoque pode acabar.")
	return renderMail("Suas peças estão esperando · "+brand, mailBody{
		heading:    "Esqueceu algo no carrinho?",
		paragraphs: paragraphs,
		action:     pair("Finalizar compra", checkoutURL),
		footnote:   "Abra o link no mesmo navegador em que você montou o carrinho.",
	}, unsubscribeURL)
}

func returnParagraphs(m ReturnMail, lead string) []string {
	p := []string{greeting(m.Name), lead}
	if len(m.Items) > 0 {
		p = append(p, fmt.Sprintf("Itens: %s.", strings.Join(m.Items, "; ")))
	}
	return p
}

// ReturnRequested implements MailRenderer.
func (DefaultRenderer) ReturnRequested(m ReturnMail) Email {
	return renderMail(fmt.Sprintf("Solicitação de troca/devolução · pedido #%s · %s", m.OrderCode, brand), mailBody{
		heading:    "Recebemos sua solicitação",
		paragraphs: returnParagraphs(m, fmt.Sprintf("Recebemos sua solicitação (%s) para o pedido #%s. Vamos analisar e responder por e-mail com as instruções de envio.", m.KindLabel, m.OrderCode)),
		action:     pair("Acompanhar solicitação", m.URL),
		footnote:   "Lembre-se: a peça deve estar sem uso, com etiquetas e na embalagem original.",
	}, "")
}

// ReturnApproved implements MailRenderer.
func (DefaultRenderer) ReturnApproved(m ReturnMail, instructions string) Email {
	return renderMail(fmt.Sprintf("Troca/devolução aprovada · pedido #%s · %s", m.OrderCode, brand), mailBody{
		heading:    "Solicitação aprovada — envie a peça",
		paragraphs: returnParagraphs(m, fmt.Sprintf("Sua solicitação (%s) do pedido #%s foi aprovada. Siga as instruções abaixo para enviar a peça de volta.", m.KindLabel, m.OrderCode)),
		action:     pair("Ver pedido", m.URL),
		code:       pair("Instruções de envio:", instructions),
		footnote:   "O frete de devolução por arrependimento é por conta do cliente, exceto em casos de defeito ou erro no envio.",
	}, "")
}

// ReturnRejected implements MailRenderer.
func (DefaultRenderer) ReturnRejected(m ReturnMail, reason string) Email {
	p := returnParagraphs(m, fmt.Sprintf("Analisamos sua solicitação (%s) do pedido #%s e infelizmente não pudemos aceitá-la.", m.KindLabel, m.OrderCode))
	p = append(p, "Motivo: "+reason, "Se tiver dúvidas, fale com a gente pelos canais de atendimento.")
	return renderMail(fmt.Sprintf("Solicitação de troca/devolução · pedido #%s · %s", m.OrderCode, brand), mailBody{
		heading:    "Não pudemos aceitar sua solicitação",
		paragraphs: p,
		action:     pair("Ver pedido", m.URL),
	}, "")
}

// ReturnResolved implements MailRenderer.
func (DefaultRenderer) ReturnResolved(m ReturnMail, refundCents *int64) Email {
	lead := fmt.Sprintf("Recebemos e conferimos a peça do pedido #%s. A troca foi concluída e a nova peça segue para você; avisaremos o código de rastreio.", m.OrderCode)
	heading := "Troca concluída"
	if refundCents != nil {
		lead = fmt.Sprintf("Recebemos e conferimos a peça do pedido #%s. O reembolso de %s foi processado e é devolvido via PIX para a conta de origem do pagamento.", m.OrderCode, FormatPrice(*refundCents))
		heading = "Reembolso concluído"
	}
	return renderMail(fmt.Sprintf("Troca/devolução concluída · pedido #%s · %s", m.OrderCode, brand), mailBody{
		heading:    heading,
		paragraphs: returnParagraphs(m, lead),
		action:     pair("Ver pedido", m.URL),
	}, "")
}
