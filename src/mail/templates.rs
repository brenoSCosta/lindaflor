//! pt-BR email templates: a minimal inline-styled HTML layout plus a
//! plain-text alternative for every message. Every interpolated value is
//! HTML-escaped; links are absolute (see [`super::absolute_url`]).

use crate::app::store::queries::format_price;

const BRAND: &str = "Lindaflor";

/// A rendered message, ready for the outbox.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Email {
  pub subject: String,
  pub html: String,
  pub text: String,
}

/// Escape text for HTML element and attribute contexts.
pub fn escape(raw: &str) -> String {
  let mut out = String::with_capacity(raw.len());
  for ch in raw.chars() {
    match ch {
      '&' => out.push_str("&amp;"),
      '<' => out.push_str("&lt;"),
      '>' => out.push_str("&gt;"),
      '"' => out.push_str("&quot;"),
      '\'' => out.push_str("&#39;"),
      _ => out.push(ch),
    }
  }
  out
}

/// One block of copy: paragraphs, an optional call-to-action, and an
/// optional footnote. Rendered to both HTML and text from the same data so
/// the two alternatives never drift.
struct Body<'a> {
  heading: &'a str,
  paragraphs: Vec<String>,
  action: Option<(&'a str, &'a str)>,
  /// Preformatted block (e.g. the PIX copia-e-cola code).
  code: Option<(&'a str, &'a str)>,
  footnote: Option<&'a str>,
}

fn render(subject: String, body: Body<'_>) -> Email {
  render_with_unsubscribe(subject, body, None)
}

/// [`render`] plus, for marketing mail, a "Cancelar inscrição" footer link.
fn render_with_unsubscribe(
  subject: String,
  body: Body<'_>,
  unsubscribe_url: Option<&str>,
) -> Email {
  let mut html = String::new();
  html.push_str(
    "<!doctype html><html lang=\"pt-BR\"><head><meta charset=\"utf-8\">\
     <meta name=\"viewport\" content=\"width=device-width,initial-scale=1\">",
  );
  html.push_str(&format!("<title>{}</title></head>", escape(&subject)));
  html.push_str(
    "<body style=\"margin:0;padding:0;background:#f6f1ee;\
     font-family:Helvetica,Arial,sans-serif;color:#2b2b2b;\">\
     <table role=\"presentation\" width=\"100%\" cellpadding=\"0\" \
     cellspacing=\"0\" style=\"background:#f6f1ee;padding:24px 0;\"><tr>\
     <td align=\"center\"><table role=\"presentation\" width=\"100%\" \
     cellpadding=\"0\" cellspacing=\"0\" style=\"max-width:560px;\
     background:#ffffff;border-radius:12px;padding:32px;\"><tr><td>",
  );
  html.push_str(&format!(
    "<p style=\"margin:0 0 24px;font-size:22px;font-weight:bold;\
     color:#b4536b;\">{BRAND}</p>"
  ));
  html.push_str(&format!(
    "<h1 style=\"margin:0 0 16px;font-size:20px;\">{}</h1>",
    escape(body.heading)
  ));
  for paragraph in &body.paragraphs {
    html.push_str(&format!(
      "<p style=\"margin:0 0 16px;font-size:15px;line-height:1.5;\">{}</p>",
      escape(paragraph)
    ));
  }
  if let Some((label, code)) = body.code {
    html.push_str(&format!(
      "<p style=\"margin:0 0 8px;font-size:13px;color:#666;\">{}</p>\
       <pre style=\"margin:0 0 16px;padding:12px;background:#f6f1ee;\
       border-radius:8px;font-size:12px;white-space:pre-wrap;\
       word-break:break-all;\">{}</pre>",
      escape(label),
      escape(code)
    ));
  }
  if let Some((label, url)) = body.action {
    html.push_str(&format!(
      "<p style=\"margin:24px 0;\"><a href=\"{url}\" style=\"display:inline-block;\
       background:#b4536b;color:#ffffff;text-decoration:none;font-weight:bold;\
       padding:12px 24px;border-radius:8px;\">{label}</a></p>\
       <p style=\"margin:0 0 16px;font-size:12px;color:#666;\">\
       Se o botão não funcionar, copie e cole este endereço no navegador:<br>\
       <a href=\"{url}\" style=\"color:#b4536b;word-break:break-all;\">{url}</a></p>",
      url = escape(url),
      label = escape(label),
    ));
  }
  if let Some(note) = body.footnote {
    html.push_str(&format!(
      "<p style=\"margin:16px 0 0;font-size:12px;color:#666;\">{}</p>",
      escape(note)
    ));
  }
  html.push_str(&format!(
    "</td></tr></table><p style=\"font-size:11px;color:#999;\">\
     {BRAND} · Esta é uma mensagem automática, não responda.</p>"
  ));
  if let Some(url) = unsubscribe_url {
    html.push_str(&format!(
      "<p style=\"font-size:11px;color:#999;\">Não quer mais receber nossos \
       e-mails? <a href=\"{url}\" style=\"color:#999;\">Cancelar inscrição</a></p>",
      url = escape(url),
    ));
  }
  html.push_str("</td></tr></table></body></html>");

  let mut text = format!("{BRAND}\n\n{}\n\n", body.heading);
  for paragraph in &body.paragraphs {
    text.push_str(paragraph);
    text.push_str("\n\n");
  }
  if let Some((label, code)) = body.code {
    text.push_str(&format!("{label}\n{code}\n\n"));
  }
  if let Some((label, url)) = body.action {
    text.push_str(&format!("{label}: {url}\n\n"));
  }
  if let Some(note) = body.footnote {
    text.push_str(note);
    text.push_str("\n\n");
  }
  text.push_str(&format!(
    "-- \n{BRAND} · Esta é uma mensagem automática, não responda.\n"
  ));
  if let Some(url) = unsubscribe_url {
    text.push_str(&format!("Cancelar inscrição: {url}\n"));
  }

  Email {
    subject,
    html,
    text,
  }
}

fn greeting(name: &str) -> String {
  let name = name.trim();
  if name.is_empty() {
    "Olá!".to_string()
  } else {
    format!("Olá, {name}!")
  }
}

const IGNORE_NOTE: &str =
  "Se você não fez esta solicitação, ignore este e-mail com segurança.";

// --- Auth ---

/// `first_purchase_code` is the storefront's welcome coupon
/// (`store_settings.first_purchase_coupon_code`), shown to new customers.
pub fn verify_email(
  name: &str,
  url: &str,
  first_purchase_code: Option<&str>,
) -> Email {
  let mut paragraphs = vec![
    greeting(name),
    "Para ativar sua conta, confirme seu endereço de e-mail clicando no botão abaixo. O link vale por 24 horas.".to_string(),
  ];
  if let Some(code) = first_purchase_code
    .map(str::trim)
    .filter(|code| !code.is_empty())
  {
    paragraphs.push(format!(
      "Presente de boas-vindas: use o cupom {code} na sua primeira compra."
    ));
  }
  render(
    format!("Confirme seu e-mail · {BRAND}"),
    Body {
      heading: "Confirme seu e-mail",
      paragraphs,
      action: Some(("Confirmar e-mail", url)),
      code: None,
      footnote: Some(IGNORE_NOTE),
    },
  )
}

pub fn password_reset(name: &str, url: &str) -> Email {
  render(
    format!("Redefinição de senha · {BRAND}"),
    Body {
      heading: "Redefina sua senha",
      paragraphs: vec![
        greeting(name),
        "Recebemos um pedido para redefinir a senha da sua conta. O link abaixo vale por 1 hora e só pode ser usado uma vez.".to_string(),
      ],
      action: Some(("Redefinir senha", url)),
      code: None,
      footnote: Some(IGNORE_NOTE),
    },
  )
}

pub fn change_email(name: &str, new_email: &str, url: &str) -> Email {
  render(
    format!("Confirme seu novo e-mail · {BRAND}"),
    Body {
      heading: "Confirme a troca de e-mail",
      paragraphs: vec![
        greeting(name),
        format!(
          "Recebemos um pedido para alterar o e-mail da sua conta para {new_email}. Confirme pelo botão abaixo; o link vale por 30 minutos."
        ),
      ],
      action: Some(("Confirmar novo e-mail", url)),
      code: None,
      footnote: Some(IGNORE_NOTE),
    },
  )
}

pub fn delete_account(name: &str, url: &str) -> Email {
  render(
    format!("Confirme a exclusão da sua conta · {BRAND}"),
    Body {
      heading: "Excluir sua conta",
      paragraphs: vec![
        greeting(name),
        "Recebemos um pedido para excluir sua conta. Essa ação é permanente: seus dados de acesso serão apagados. O link abaixo vale por 30 minutos.".to_string(),
      ],
      action: Some(("Confirmar exclusão", url)),
      code: None,
      footnote: Some(
        "Se você não pediu a exclusão, ignore este e-mail e considere trocar sua senha.",
      ),
    },
  )
}

// --- Newsletter ---

/// Welcome mail for a new newsletter subscriber; repeats the store's
/// first-purchase coupon when one is configured.
pub fn newsletter_welcome(
  name: &str,
  shop_url: &str,
  first_purchase_code: Option<&str>,
  unsubscribe_url: &str,
) -> Email {
  let mut paragraphs = vec![
    greeting(name),
    "Obrigada por se inscrever na newsletter! Você vai receber em primeira mão nossos lançamentos, coleções e promoções.".to_string(),
  ];
  if let Some(code) = first_purchase_code
    .map(str::trim)
    .filter(|code| !code.is_empty())
  {
    paragraphs.push(format!(
      "Presente de boas-vindas: use o cupom {code} na sua primeira compra."
    ));
  }
  render_with_unsubscribe(
    format!("Boas-vindas à newsletter · {BRAND}"),
    Body {
      heading: "Que bom ter você por aqui",
      paragraphs,
      action: Some(("Visitar a loja", shop_url)),
      code: None,
      footnote: Some(
        "Se você não se inscreveu, use o link \"Cancelar inscrição\" no fim deste e-mail.",
      ),
    },
    Some(unsubscribe_url),
  )
}

// --- Commerce ---

/// What every order email needs. Built by [`super::orders`].
#[derive(Debug, Clone)]
pub struct OrderMail {
  /// Short human reference (first 8 hex chars of the id, uppercase).
  pub code: String,
  pub name: String,
  pub total_cents: i64,
  /// Absolute `/pedido/{id}?t=...` link.
  pub url: String,
  pub pix_copy_paste: Option<String>,
  /// Payment deadline, already formatted (`dd/mm/aaaa hh:mm`, Brasília).
  pub pay_until: Option<String>,
  /// Paid through Mercado Pago: the QR Code lives on the order page.
  pub mercado_pago: bool,
  /// Shipping carrier (`orders.carrier`, e.g. "Correios").
  pub carrier: String,
  pub tracking_code: Option<String>,
  /// Carrier's public tracking page, when known
  /// (see [`crate::app::store::order::tracking_url`]).
  pub tracking_url: Option<String>,
}

pub fn order_code(order_id: uuid::Uuid) -> String {
  order_id.simple().to_string()[..8].to_ascii_uppercase()
}

pub fn order_created(order: &OrderMail) -> Email {
  let mut paragraphs = vec![
    greeting(&order.name),
    format!(
      "Recebemos seu pedido #{} no valor de {}. Para concluir, pague via PIX.",
      order.code,
      format_price(order.total_cents)
    ),
  ];
  paragraphs.push(match &order.pay_until {
    Some(until) => format!(
      "Reservamos seus itens até {until}. Depois disso o pedido é cancelado automaticamente."
    ),
    None => "Seus itens ficam reservados por tempo limitado.".to_string(),
  });
  paragraphs.push(if order.mercado_pago {
    "O pagamento é processado pelo Mercado Pago: abra a página do pedido para ver o QR Code e o código copia-e-cola. A confirmação é automática.".to_string()
  } else {
    "Na página do pedido você encontra o QR Code e o código copia-e-cola, e acompanha a confirmação do pagamento.".to_string()
  });
  let (action_label, code) = if order.mercado_pago {
    ("Pagar com PIX", None)
  } else {
    (
      "Ver instruções de pagamento",
      order
        .pix_copy_paste
        .as_deref()
        .map(|code| ("PIX copia-e-cola:", code)),
    )
  };
  render(
    format!("Pedido #{} recebido · {BRAND}", order.code),
    Body {
      heading: "Pedido recebido — falta o pagamento",
      paragraphs,
      action: Some((action_label, &order.url)),
      code,
      footnote: Some(
        "Guarde este e-mail: o link acima dá acesso ao seu pedido.",
      ),
    },
  )
}

pub fn payment_confirmed(order: &OrderMail) -> Email {
  render(
    format!("Pagamento confirmado · pedido #{} · {BRAND}", order.code),
    Body {
      heading: "Pagamento confirmado!",
      paragraphs: vec![
        greeting(&order.name),
        format!(
          "Confirmamos o pagamento de {} do pedido #{}. Agora vamos separar e preparar seus itens com carinho.",
          format_price(order.total_cents),
          order.code
        ),
        "Avisaremos por e-mail assim que o pedido for enviado.".to_string(),
      ],
      action: Some(("Acompanhar pedido", &order.url)),
      code: None,
      footnote: None,
    },
  )
}

pub fn order_shipped(order: &OrderMail) -> Email {
  let carrier = if order.carrier.is_empty() {
    "a transportadora"
  } else {
    order.carrier.as_str()
  };
  let mut paragraphs = vec![greeting(&order.name)];
  match &order.tracking_code {
    Some(_) => paragraphs.push(format!(
      "O pedido #{} foi enviado por {carrier}. Use o código de rastreio abaixo para acompanhar a entrega.",
      order.code
    )),
    None => paragraphs.push(format!(
      "O pedido #{} foi enviado por {carrier}. Acompanhe o status pela página do pedido.",
      order.code
    )),
  }
  // The carrier page is the main action when we have one; the order page
  // stays reachable through the footnote.
  let action = match &order.tracking_url {
    Some(link) => ("Rastrear entrega", link.as_str()),
    None => ("Acompanhar pedido", order.url.as_str()),
  };
  let footnote = order
    .tracking_url
    .as_ref()
    .map(|_| format!("Detalhes do pedido: {}", order.url));
  render(
    format!("Pedido #{} enviado · {BRAND}", order.code),
    Body {
      heading: "Seu pedido está a caminho",
      paragraphs,
      action: Some(action),
      code: order
        .tracking_code
        .as_deref()
        .map(|code| ("Código de rastreio:", code)),
      footnote: footnote.as_deref(),
    },
  )
}

pub fn order_cancelled(order: &OrderMail, expired: bool) -> Email {
  let (subject, heading, reason) = if expired {
    (
      format!("Pedido #{} expirado · {BRAND}", order.code),
      "Pedido cancelado por falta de pagamento",
      format!(
        "Não identificamos o pagamento do pedido #{} dentro do prazo, então ele foi cancelado e os itens voltaram ao estoque.",
        order.code
      ),
    )
  } else {
    (
      format!("Pedido #{} cancelado · {BRAND}", order.code),
      "Pedido cancelado",
      format!("O pedido #{} foi cancelado.", order.code),
    )
  };
  render(
    subject,
    Body {
      heading,
      paragraphs: vec![
        greeting(&order.name),
        reason,
        "Se ainda quiser os produtos, é só fazer um novo pedido na loja. Em caso de dúvida, fale com a gente.".to_string(),
      ],
      action: Some(("Ver pedido", &order.url)),
      code: None,
      footnote: None,
    },
  )
}

pub fn order_refunded(order: &OrderMail) -> Email {
  render(
    format!("Reembolso do pedido #{} · {BRAND}", order.code),
    Body {
      heading: "Reembolso realizado",
      paragraphs: vec![
        greeting(&order.name),
        format!(
          "O reembolso do pedido #{} ({}) foi processado. O valor é devolvido via PIX para a conta de origem do pagamento.",
          order.code,
          format_price(order.total_cents)
        ),
      ],
      action: Some(("Ver pedido", &order.url)),
      code: None,
      footnote: None,
    },
  )
}

// --- Notifications ---

/// Back-in-stock alert ("Avise-me"). `cancel_url` stops every pending alert
/// of the address (signed link, see `store::stock_alerts`).
pub fn back_in_stock(
  product_name: &str,
  variant_label: &str,
  product_url: &str,
  cancel_url: &str,
) -> Email {
  render_with_unsubscribe(
    format!("{product_name} voltou ao estoque · {BRAND}"),
    Body {
      heading: "A peça que você queria voltou!",
      paragraphs: vec![
        "Olá!".to_string(),
        format!(
          "{product_name} ({variant_label}) está disponível de novo. O estoque é limitado: garanta a sua antes que acabe."
        ),
      ],
      action: Some(("Ver peça", product_url)),
      code: None,
      footnote: Some(
        "Você recebeu este e-mail porque pediu para ser avisada quando a peça voltasse. Este aviso é enviado uma única vez.",
      ),
    },
    Some(cancel_url),
  )
}

/// One line of an abandoned-cart reminder.
#[derive(Debug, Clone)]
pub struct CartReminderLine {
  pub product_name: String,
  /// Size + color, e.g. `M · Azul`.
  pub variant_label: String,
  pub quantity: i32,
  pub unit_price_cents: i64,
}

/// Abandoned-cart reminder (marketing: carries the newsletter unsubscribe
/// link, and the outbox adds `List-Unsubscribe`).
pub fn abandoned_cart(
  name: &str,
  lines: &[CartReminderLine],
  checkout_url: &str,
  unsubscribe_url: &str,
) -> Email {
  let mut paragraphs = vec![
    greeting(name),
    "Você deixou algumas peças no carrinho. Separamos tudo para você finalizar a compra:".to_string(),
  ];
  for line in lines {
    let quantity = if line.quantity > 1 {
      format!(" (x{})", line.quantity)
    } else {
      String::new()
    };
    paragraphs.push(format!(
      "• {} — {}{} — {}",
      line.product_name,
      line.variant_label,
      quantity,
      format_price(line.unit_price_cents)
    ));
  }
  paragraphs.push(
    "Os itens não ficam reservados até o pagamento: o estoque pode acabar."
      .to_string(),
  );
  render_with_unsubscribe(
    format!("Suas peças estão esperando · {BRAND}"),
    Body {
      heading: "Esqueceu algo no carrinho?",
      paragraphs,
      action: Some(("Finalizar compra", checkout_url)),
      code: None,
      footnote: Some(
        "Abra o link no mesmo navegador em que você montou o carrinho.",
      ),
    },
    Some(unsubscribe_url),
  )
}

// --- Returns / exchanges (see `crate::app::store::returns`) ---

/// What every return-request email needs. Built by [`super::returns`].
#[derive(Debug, Clone)]
pub struct ReturnMail {
  /// Order code ([`order_code`]).
  pub order_code: String,
  pub name: String,
  /// "Troca de tamanho" / "Devolução" / "Defeito".
  pub kind_label: String,
  /// One line per returned item, e.g. "1x Biquíni Flor (M / azul)".
  pub items: Vec<String>,
  /// Absolute `/pedido/{id}?t=...` link.
  pub url: String,
}

fn return_paragraphs(mail: &ReturnMail, lead: String) -> Vec<String> {
  let mut paragraphs = vec![greeting(&mail.name), lead];
  if !mail.items.is_empty() {
    paragraphs.push(format!("Itens: {}.", mail.items.join("; ")));
  }
  paragraphs
}

pub fn return_requested(mail: &ReturnMail) -> Email {
  render(
    format!(
      "Solicitação de troca/devolução · pedido #{} · {BRAND}",
      mail.order_code
    ),
    Body {
      heading: "Recebemos sua solicitação",
      paragraphs: return_paragraphs(
        mail,
        format!(
          "Recebemos sua solicitação ({}) para o pedido #{}. Vamos analisar e responder por e-mail com as instruções de envio.",
          mail.kind_label, mail.order_code
        ),
      ),
      action: Some(("Acompanhar solicitação", &mail.url)),
      code: None,
      footnote: Some(
        "Lembre-se: a peça deve estar sem uso, com etiquetas e na embalagem original.",
      ),
    },
  )
}

pub fn return_approved(mail: &ReturnMail, instructions: &str) -> Email {
  render(
    format!(
      "Troca/devolução aprovada · pedido #{} · {BRAND}",
      mail.order_code
    ),
    Body {
      heading: "Solicitação aprovada — envie a peça",
      paragraphs: return_paragraphs(
        mail,
        format!(
          "Sua solicitação ({}) do pedido #{} foi aprovada. Siga as instruções abaixo para enviar a peça de volta.",
          mail.kind_label, mail.order_code
        ),
      ),
      action: Some(("Ver pedido", &mail.url)),
      code: Some(("Instruções de envio:", instructions)),
      footnote: Some(
        "O frete de devolução por arrependimento é por conta do cliente, exceto em casos de defeito ou erro no envio.",
      ),
    },
  )
}

pub fn return_rejected(mail: &ReturnMail, reason: &str) -> Email {
  render(
    format!(
      "Solicitação de troca/devolução · pedido #{} · {BRAND}",
      mail.order_code
    ),
    Body {
      heading: "Não pudemos aceitar sua solicitação",
      paragraphs: {
        let mut paragraphs = return_paragraphs(
          mail,
          format!(
            "Analisamos sua solicitação ({}) do pedido #{} e infelizmente não pudemos aceitá-la.",
            mail.kind_label, mail.order_code
          ),
        );
        paragraphs.push(format!("Motivo: {reason}"));
        paragraphs.push(
          "Se tiver dúvidas, fale com a gente pelos canais de atendimento."
            .to_string(),
        );
        paragraphs
      },
      action: Some(("Ver pedido", &mail.url)),
      code: None,
      footnote: None,
    },
  )
}

/// `refund_cents`: `Some` for a refund, `None` for a completed exchange.
pub fn return_resolved(mail: &ReturnMail, refund_cents: Option<i64>) -> Email {
  let lead = match refund_cents {
    Some(cents) => format!(
      "Recebemos e conferimos a peça do pedido #{}. O reembolso de {} foi processado e é devolvido via PIX para a conta de origem do pagamento.",
      mail.order_code,
      format_price(cents)
    ),
    None => format!(
      "Recebemos e conferimos a peça do pedido #{}. A troca foi concluída e a nova peça segue para você; avisaremos o código de rastreio.",
      mail.order_code
    ),
  };
  render(
    format!(
      "Troca/devolução concluída · pedido #{} · {BRAND}",
      mail.order_code
    ),
    Body {
      heading: if refund_cents.is_some() {
        "Reembolso concluído"
      } else {
        "Troca concluída"
      },
      paragraphs: return_paragraphs(mail, lead),
      action: Some(("Ver pedido", &mail.url)),
      code: None,
      footnote: None,
    },
  )
}

#[cfg(test)]
mod tests {
  use super::*;

  fn sample_order() -> OrderMail {
    OrderMail {
      code: "0192ABCD".to_string(),
      name: "Ana <b>".to_string(),
      total_cents: 12_990,
      url: "https://loja.example.com/pedido/abc?t=tok&x=1".to_string(),
      pix_copy_paste: Some("00020126BR.GOV.BCB.PIX".to_string()),
      pay_until: Some("01/11/2026 10:00".to_string()),
      mercado_pago: false,
      carrier: "Correios".to_string(),
      tracking_code: None,
      tracking_url: None,
    }
  }

  #[test]
  fn order_created_for_mercado_pago_links_order_page_without_static_pix() {
    let order = OrderMail {
      mercado_pago: true,
      ..sample_order()
    };
    let email = order_created(&order);
    assert!(!email.text.contains("00020126BR.GOV.BCB.PIX"));
    assert!(!email.html.contains("00020126BR.GOV.BCB.PIX"));
    assert!(email.text.contains("Mercado Pago"));
    assert!(email.text.contains(
      "Pagar com PIX: https://loja.example.com/pedido/abc?t=tok&x=1"
    ));
  }

  #[test]
  fn order_shipped_carries_carrier_code_and_tracking_link() {
    let link = "https://rastreamento.correios.com.br/app/index.php?objetos=AA123456789BR";
    let order = OrderMail {
      tracking_code: Some("AA123456789BR".to_string()),
      tracking_url: Some(link.to_string()),
      ..sample_order()
    };
    let email = order_shipped(&order);
    for part in [&email.text, &email.html] {
      assert!(part.contains("Correios"), "{part}");
      assert!(part.contains("AA123456789BR"), "{part}");
      assert!(part.contains("rastreamento.correios.com.br"), "{part}");
      assert!(part.contains("/pedido/abc?t=tok"), "{part}");
    }
    assert!(email.text.contains(&format!("Rastrear entrega: {link}")));

    // Other carriers: code shown, no link we can't vouch for.
    let other = OrderMail {
      carrier: "Jadlog".to_string(),
      tracking_code: Some("JD123".to_string()),
      tracking_url: None,
      ..sample_order()
    };
    let email = order_shipped(&other);
    assert!(email.text.contains("Jadlog"));
    assert!(email.text.contains("JD123"));
    assert!(
      email
        .text
        .contains("Acompanhar pedido: https://loja.example.com/pedido/abc")
    );
  }

  #[test]
  fn escape_covers_html_specials() {
    assert_eq!(
      escape(r#"<a href="x">'&'</a>"#),
      "&lt;a href=&quot;x&quot;&gt;&#39;&amp;&#39;&lt;/a&gt;"
    );
  }

  #[test]
  fn auth_templates_carry_link_in_both_parts() {
    let url = "https://loja.example.com/reset-password?token=abc123";
    for email in [
      verify_email("Ana", url, None),
      password_reset("Ana", url),
      change_email("Ana", "nova@example.com", url),
      delete_account("Ana", url),
    ] {
      assert!(email.subject.contains("Lindaflor"), "{}", email.subject);
      assert!(email.text.contains(url), "text link: {}", email.subject);
      assert!(email.html.contains(url), "html link: {}", email.subject);
      assert!(email.html.contains("lang=\"pt-BR\""));
      assert!(email.text.contains("Olá, Ana!"));
    }
  }

  #[test]
  fn order_created_includes_pix_and_escapes() {
    let email = order_created(&sample_order());
    assert!(email.subject.contains("#0192ABCD"));
    assert!(email.text.contains("R$"), "price: {}", email.text);
    assert!(email.text.contains("00020126BR.GOV.BCB.PIX"));
    assert!(email.text.contains("01/11/2026 10:00"));
    assert!(email.text.contains("/pedido/abc?t=tok&x=1"));
    // HTML escapes user data and the `&` in the link.
    assert!(email.html.contains("Ana &lt;b&gt;"));
    assert!(!email.html.contains("Ana <b>"));
    assert!(email.html.contains("/pedido/abc?t=tok&amp;x=1"));
  }

  #[test]
  fn order_status_templates_render() {
    let order = sample_order();
    assert!(payment_confirmed(&order).subject.contains("Pagamento"));
    assert!(order_shipped(&order).subject.contains("enviado"));
    assert!(order_cancelled(&order, true).subject.contains("expirado"));
    assert!(order_cancelled(&order, false).subject.contains("cancelado"));
    assert!(order_refunded(&order).subject.contains("Reembolso"));
  }

  #[test]
  fn verify_email_mentions_first_purchase_coupon() {
    let url = "https://x/verify-email?token=t";
    let with = verify_email("Ana", url, Some(" BEMVINDA10 "));
    assert!(with.text.contains("use o cupom BEMVINDA10 na sua primeira"));
    assert!(with.html.contains("BEMVINDA10"));
    for code in [None, Some("  ")] {
      assert!(!verify_email("Ana", url, code).text.contains("cupom"));
    }
  }

  #[test]
  fn newsletter_welcome_mentions_coupon_only_when_configured() {
    let with = newsletter_welcome(
      "Ana",
      "https://loja.example.com/",
      Some("BEMVINDA10"),
      "https://loja.example.com/newsletter/cancelar?email=a&token=b",
    );
    assert!(with.subject.contains("newsletter"));
    assert!(with.text.contains("cupom BEMVINDA10"));
    assert!(with.html.contains("https://loja.example.com/"));
    let without = newsletter_welcome(
      "",
      "https://loja.example.com/",
      None,
      "https://loja.example.com/newsletter/cancelar?email=a&token=b",
    );
    assert!(!without.text.contains("cupom"));
    assert!(without.text.contains("Olá!"));
  }

  #[test]
  fn newsletter_welcome_carries_unsubscribe_link() {
    let url = "https://loja.example.com/newsletter/cancelar?email=a&token=b";
    let email =
      newsletter_welcome("Ana", "https://loja.example.com/", None, url);
    assert!(email.text.contains(&format!("Cancelar inscrição: {url}")));
    assert!(email.html.contains(
      "href=\"https://loja.example.com/newsletter/cancelar?email=a&amp;token=b\""
    ));
    // Transactional mail has no unsubscribe footer.
    let reset = password_reset("Ana", "https://x/reset?token=t");
    assert!(!reset.text.contains("Cancelar inscrição"));
  }

  #[test]
  fn greeting_without_name() {
    assert!(verify_email("  ", "https://x", None).text.contains("Olá!"));
  }
}
