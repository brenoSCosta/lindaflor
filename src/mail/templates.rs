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
     {BRAND} · Esta é uma mensagem automática, não responda.</p>\
     </td></tr></table></body></html>"
  ));

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

pub fn verify_email(name: &str, url: &str) -> Email {
  render(
    format!("Confirme seu e-mail · {BRAND}"),
    Body {
      heading: "Confirme seu e-mail",
      paragraphs: vec![
        greeting(name),
        "Para ativar sua conta, confirme seu endereço de e-mail clicando no botão abaixo. O link vale por 24 horas.".to_string(),
      ],
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
  paragraphs.push(
    "Na página do pedido você encontra o QR Code e o código copia-e-cola, e acompanha a confirmação do pagamento.".to_string(),
  );
  render(
    format!("Pedido #{} recebido · {BRAND}", order.code),
    Body {
      heading: "Pedido recebido — falta o pagamento",
      paragraphs,
      action: Some(("Ver instruções de pagamento", &order.url)),
      code: order
        .pix_copy_paste
        .as_deref()
        .map(|code| ("PIX copia-e-cola:", code)),
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
  render(
    format!("Pedido #{} enviado · {BRAND}", order.code),
    Body {
      heading: "Seu pedido está a caminho",
      paragraphs: vec![
        greeting(&order.name),
        format!(
          "O pedido #{} foi enviado. Acompanhe o status pela página do pedido.",
          order.code
        ),
      ],
      action: Some(("Acompanhar pedido", &order.url)),
      code: None,
      footnote: None,
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
    }
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
      verify_email("Ana", url),
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
  fn greeting_without_name() {
    assert!(verify_email("  ", "https://x").text.contains("Olá!"));
  }
}
