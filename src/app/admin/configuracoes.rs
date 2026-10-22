use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{content::Form, href, page, response::Response, route},
  view::{View, ViewExt, attributes, view},
};

use crate::app::auth_helpers::require_admin;
use crate::app::store::queries::{
  DEFAULT_FREE_SHIPPING_THRESHOLD_CENTS, get_store_settings,
};
use crate::components::button::{ButtonVariant, button};
use crate::components::card::{card, card_content};
use crate::components::container::container;
use crate::components::input::input;
use crate::components::label::label;
use crate::components::select::select;
use crate::components::textarea::textarea;
use crate::components::toast::{Toast, set_toast, toast_redirect};

const PIX_KEY_TYPES: [&str; 5] = ["cpf", "cnpj", "email", "phone", "random"];

#[derive(Deserialize)]
pub struct SettingsForm {
  pix_key: String,
  pix_key_type: String,
  pix_merchant_name: String,
  pix_merchant_city: String,
  pix_discount_percent: String,
  announcement_text: String,
  first_purchase_coupon_code: String,
  whatsapp_number: String,
  whatsapp_message_template: String,
  free_shipping_threshold: String,
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);

  let settings = get_store_settings(pool).await?;
  let pix_key = settings.pix_key.unwrap_or_default();
  let pix_key_type = settings.pix_key_type.unwrap_or_else(|| "phone".into());
  let pix_merchant_name = settings.pix_merchant_name.unwrap_or_default();
  let pix_merchant_city = settings.pix_merchant_city.unwrap_or_default();
  let pix_discount_percent = settings.pix_discount_percent.to_string();
  let announcement_text = settings.announcement_text.unwrap_or_default();
  let first_purchase_coupon_code =
    settings.first_purchase_coupon_code.unwrap_or_default();
  let whatsapp_number = settings.whatsapp_number.unwrap_or_default();
  let whatsapp_message_template =
    settings.whatsapp_message_template.unwrap_or_default();
  let threshold_text =
    format_price_input(settings.free_shipping_threshold_cents);

  Ok(view! {
      container(
        <div class="flex flex-col gap-1.5">
          <h1 class="text-2xl font-semibold tracking-tight">"Configurações da loja"</h1>
          <p class="text-muted-foreground">"Chave PIX, WhatsApp e frete grátis usados no checkout."</p>
        </div>
          card(
              card_content(
                  <form method="post" action=(href!(crate::app::admin::configuracoes::page)) class="flex flex-col gap-8">
                      <fieldset class="space-y-4 border-0">
                          <legend class="mb-4 text-base font-semibold">"PIX (chave fixa)"</legend>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="pix_key_type" }, "Tipo da chave")
                              select(
                                  attrs: attributes! { name="pix_key_type" id="pix_key_type" },
                                  <option value="cpf" selected=(pix_key_type == "cpf")>"CPF"</option>
                                  <option value="cnpj" selected=(pix_key_type == "cnpj")>"CNPJ"</option>
                                  <option value="email" selected=(pix_key_type == "email")>"E-mail"</option>
                                  <option value="phone" selected=(pix_key_type == "phone")>"Telefone"</option>
                                  <option value="random" selected=(pix_key_type == "random")>"Chave aleatória"</option>
                              )
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="pix_key" }, "Chave PIX")
                              input(attrs: attributes! {
                                  type="text"
                                  name="pix_key"
                                  id="pix_key"
                                  value=(pix_key)
                                  placeholder="CPF, CNPJ, e-mail, telefone ou chave aleatória"
                              })
                          </div>
                          <div class="grid gap-4 @sm/page:grid-cols-2">
                              <div class="space-y-2">
                                  label(attrs: attributes! { for="pix_merchant_name" }, "Nome do recebedor")
                                  input(attrs: attributes! {
                                      type="text"
                                      name="pix_merchant_name"
                                      id="pix_merchant_name"
                                      value=(pix_merchant_name)
                                      placeholder="Linda Flor"
                                      maxlength="25"
                                  })
                                  <p class="text-xs text-muted-foreground">"Máx. 25 caracteres (Bacen)"</p>
                              </div>
                              <div class="space-y-2">
                                  label(attrs: attributes! { for="pix_merchant_city" }, "Cidade")
                                  input(attrs: attributes! {
                                      type="text"
                                      name="pix_merchant_city"
                                      id="pix_merchant_city"
                                      value=(pix_merchant_city)
                                      placeholder="Aracaju"
                                      maxlength="15"
                                  })
                                  <p class="text-xs text-muted-foreground">"Máx. 15 caracteres (Bacen)"</p>
                              </div>
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="pix_discount_percent" }, "Desconto PIX (%)")
                              input(attrs: attributes! {
                                  type="number"
                                  name="pix_discount_percent"
                                  id="pix_discount_percent"
                                  value=(pix_discount_percent)
                                  min="0"
                                  max="30"
                                  placeholder="5"
                              })
                              <p class="text-xs text-muted-foreground">"Percentual de desconto para pagamentos PIX (0–30)"</p>
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="announcement_text" }, "Barra de Anúncio")
                              textarea(
                                  attrs: attributes! {
                                      name="announcement_text"
                                      id="announcement_text"
                                      rows="2"
                                  },
                                  (announcement_text)
                              )
                              <p class="text-xs text-muted-foreground">"Mensagem exibida no topo da loja (opcional)"</p>
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="first_purchase_coupon_code" }, "Cupom Primeira Compra")
                              input(attrs: attributes! {
                                  type="text"
                                  name="first_purchase_coupon_code"
                                  id="first_purchase_coupon_code"
                                  value=(first_purchase_coupon_code)
                                  placeholder="BOAS_VINDAS10"
                              })
                              <p class="text-xs text-muted-foreground">"Código do cupom oferecido para primeira compra (opcional)"</p>
                          </div>
                      </fieldset>
                      <fieldset class="space-y-4 border-0">
                          <legend class="mb-4 text-base font-semibold">"WhatsApp da loja"</legend>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="whatsapp_number" }, "Número (com DDI)")
                              input(attrs: attributes! {
                                  type="text"
                                  name="whatsapp_number"
                                  id="whatsapp_number"
                                  value=(whatsapp_number)
                                  placeholder="Somente números, com DDI e DDD"
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="whatsapp_message_template" }, "Modelo da mensagem do cliente")
                              textarea(
                                  attrs: attributes! {
                                      name="whatsapp_message_template"
                                      id="whatsapp_message_template"
                                      rows="3"
                                  },
                                  (whatsapp_message_template)
                              )
                              <p class="text-xs text-muted-foreground">"Mensagem da página do pedido. Use {{order_id}} e {{total}}."</p>
                          </div>
                      </fieldset>
                      <fieldset class="space-y-4 border-0">
                          <legend class="mb-4 text-base font-semibold">"Frete grátis"</legend>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="free_shipping_threshold" }, "Valor mínimo (R$)")
                              input(attrs: attributes! {
                                  type="text"
                                  name="free_shipping_threshold"
                                  id="free_shipping_threshold"
                                  value=(threshold_text)
                                  placeholder="299.00"
                              })
                              <p class="text-xs text-muted-foreground">"Use 0 para frete grátis em todas as compras."</p>
                          </div>
                      </fieldset>
                      button(
                          variant: ButtonVariant::Primary,
                          attrs: attributes! { type="submit" },
                          "Salvar configurações"
                      )
                  </form>
              )
          )
      )
  }.boxed())
}

#[route(POST "/admin/configuracoes")]
async fn save_settings(
  cx: &Cx,
  Form(form): Form<SettingsForm>,
) -> Result<Response> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let back = href!(page).resolve(cx);
  let fail = |message: &str| {
    set_toast(cx, Toast::error(message));
    toast_redirect(cx, back.clone())
  };

  let pix_key_type = match validate_pix_key_type(&form.pix_key_type) {
    Ok(value) => value,
    Err(message) => return fail(message),
  };
  let pix_key = optional_text(&form.pix_key);
  let (pix_key, pix_key_type) = match (pix_key, pix_key_type) {
    (None, None) => (None, None),
    (Some(_), None) | (None, Some(_)) => {
      return fail(
        "Informe a chave e o tipo da chave PIX, ou deixe ambos em branco.",
      );
    }
    (Some(key), Some(key_type)) => {
      match crate::app::store::pix::normalize_pix_key(&key_type, &key) {
        Ok(normalized) => (Some(normalized), Some(key_type)),
        Err(message) => return fail(message),
      }
    }
  };
  let pix_merchant_name = match optional_limited(
    &form.pix_merchant_name,
    25,
    "Nome do recebedor deve ter no máximo 25 caracteres.",
  ) {
    Ok(value) => value,
    Err(message) => return fail(message),
  };
  let pix_merchant_city = match optional_limited(
    &form.pix_merchant_city,
    15,
    "Cidade deve ter no máximo 15 caracteres.",
  ) {
    Ok(value) => value,
    Err(message) => return fail(message),
  };
  let pix_discount_percent =
    match parse_pix_discount_percent(&form.pix_discount_percent) {
      Ok(value) => value,
      Err(message) => return fail(message),
    };
  let announcement_text = optional_text(&form.announcement_text);
  let first_purchase_coupon_code =
    optional_text(&form.first_purchase_coupon_code);
  let whatsapp_number = optional_text(&form.whatsapp_number);
  let whatsapp_message_template =
    optional_text(&form.whatsapp_message_template);
  let threshold = match parse_threshold_cents(&form.free_shipping_threshold) {
    Ok(value) => value,
    Err(message) => return fail(message),
  };

  upsert_store_settings(
    pool,
    &SettingsWrite {
      pix_key: pix_key.as_deref(),
      pix_key_type: pix_key_type.as_deref(),
      pix_merchant_name: pix_merchant_name.as_deref(),
      pix_merchant_city: pix_merchant_city.as_deref(),
      pix_discount_percent: pix_discount_percent as i16,
      announcement_text: announcement_text.as_deref(),
      first_purchase_coupon_code: first_purchase_coupon_code.as_deref(),
      whatsapp_number: whatsapp_number.as_deref(),
      whatsapp_message_template: whatsapp_message_template.as_deref(),
      free_shipping_threshold_cents: threshold,
    },
  )
  .await?;

  set_toast(cx, Toast::success("Configurações salvas com sucesso."));
  toast_redirect(cx, back)
}

struct SettingsWrite<'a> {
  pix_key: Option<&'a str>,
  pix_key_type: Option<&'a str>,
  pix_merchant_name: Option<&'a str>,
  pix_merchant_city: Option<&'a str>,
  pix_discount_percent: i16,
  announcement_text: Option<&'a str>,
  first_purchase_coupon_code: Option<&'a str>,
  whatsapp_number: Option<&'a str>,
  whatsapp_message_template: Option<&'a str>,
  free_shipping_threshold_cents: i64,
}

/// `store_settings` is a singleton (`id` is always `true`, migration 0019),
/// so a single upsert is race-free: concurrent saves never create a second row.
async fn upsert_store_settings(
  pool: &PgPool,
  settings: &SettingsWrite<'_>,
) -> Result<(), sqlx::Error> {
  sqlx::query(
    "INSERT INTO store_settings (
          id, pix_key, pix_key_type, pix_merchant_name, pix_merchant_city,
          pix_discount_percent, announcement_text, first_purchase_coupon_code,
          whatsapp_number, whatsapp_message_template, free_shipping_threshold_cents,
          created_at, updated_at
       ) VALUES (true, $1, $2::pix_key_type, $3, $4, $5, $6, $7, $8, $9, $10, now(), now())
       ON CONFLICT (id) DO UPDATE SET
          pix_key = EXCLUDED.pix_key,
          pix_key_type = EXCLUDED.pix_key_type,
          pix_merchant_name = EXCLUDED.pix_merchant_name,
          pix_merchant_city = EXCLUDED.pix_merchant_city,
          pix_discount_percent = EXCLUDED.pix_discount_percent,
          announcement_text = EXCLUDED.announcement_text,
          first_purchase_coupon_code = EXCLUDED.first_purchase_coupon_code,
          whatsapp_number = EXCLUDED.whatsapp_number,
          whatsapp_message_template = EXCLUDED.whatsapp_message_template,
          free_shipping_threshold_cents = EXCLUDED.free_shipping_threshold_cents,
          updated_at = now()",
  )
  .bind(settings.pix_key)
  .bind(settings.pix_key_type)
  .bind(settings.pix_merchant_name)
  .bind(settings.pix_merchant_city)
  .bind(settings.pix_discount_percent)
  .bind(settings.announcement_text)
  .bind(settings.first_purchase_coupon_code)
  .bind(settings.whatsapp_number)
  .bind(settings.whatsapp_message_template)
  .bind(settings.free_shipping_threshold_cents)
  .execute(pool)
  .await?;
  Ok(())
}

fn format_price_input(cents: i64) -> String {
  format!("{:.2}", cents as f64 / 100.0)
}

fn optional_text(raw: &str) -> Option<String> {
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    None
  } else {
    Some(trimmed.to_string())
  }
}

fn optional_limited(
  raw: &str,
  max: usize,
  message: &'static str,
) -> Result<Option<String>, &'static str> {
  match optional_text(raw) {
    Some(value) if value.chars().count() > max => Err(message),
    other => Ok(other),
  }
}

fn validate_pix_key_type(raw: &str) -> Result<Option<String>, &'static str> {
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    return Ok(None);
  }
  if PIX_KEY_TYPES.contains(&trimmed) {
    Ok(Some(trimmed.to_string()))
  } else {
    Err("Tipo de chave PIX inválido.")
  }
}

fn parse_threshold_cents(raw: &str) -> Result<i64, &'static str> {
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    return Ok(DEFAULT_FREE_SHIPPING_THRESHOLD_CENTS);
  }
  let normalized = if trimmed.contains(',') {
    trimmed.replace('.', "").replace(',', ".")
  } else {
    trimmed.to_string()
  };
  let value: f64 = normalized
    .parse()
    .map_err(|_| "Valor inválido. Use 299.00.")?;
  if !value.is_finite() || value < 0.0 || value > 9_999_999.0 {
    return Err("Valor inválido. Use 299.00.");
  }
  Ok((value * 100.0).round() as i64)
}

fn parse_pix_discount_percent(raw: &str) -> Result<u32, &'static str> {
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    return Ok(5); // Default PIX discount
  }
  let value: u32 = trimmed
    .parse()
    .map_err(|_| "Desconto PIX inválido. Use um número entre 0 e 30.")?;
  if value > 30 {
    return Err("Desconto PIX deve estar entre 0 e 30%.");
  }
  Ok(value)
}

#[cfg(test)]
mod tests {
  use super::*;

  fn settings(city: &str) -> SettingsWrite<'_> {
    SettingsWrite {
      pix_key: Some("79999816511"),
      pix_key_type: Some("phone"),
      pix_merchant_name: Some("Linda Flor"),
      pix_merchant_city: Some(city),
      pix_discount_percent: 5,
      announcement_text: None,
      first_purchase_coupon_code: None,
      whatsapp_number: None,
      whatsapp_message_template: None,
      free_shipping_threshold_cents: DEFAULT_FREE_SHIPPING_THRESHOLD_CENTS,
    }
  }

  #[tokio::test]
  async fn concurrent_saves_leave_one_settings_row() {
    // Own database: these saves overwrite the PIX settings other tests use.
    let pool = crate::test_support::isolated_pool().await;
    let cities = ["Aracaju", "Estância", "Lagarto", "Itabaiana"];
    let saves = cities.map(|city| {
      let pool = pool.clone();
      tokio::spawn(async move {
        upsert_store_settings(&pool, &settings(city)).await
      })
    });
    for save in saves {
      save.await.expect("join").expect("save settings");
    }

    let rows: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM store_settings")
      .fetch_one(&pool)
      .await
      .expect("count settings");
    assert_eq!(rows, 1);
    let stored = get_store_settings(&pool).await.expect("read settings");
    assert!(
      cities.contains(&stored.pix_merchant_city.as_deref().unwrap_or("")),
      "unexpected city: {:?}",
      stored.pix_merchant_city
    );
  }

  #[tokio::test]
  async fn singleton_key_rejects_a_second_row() {
    let pool = crate::test_support::isolated_pool().await;
    upsert_store_settings(&pool, &settings("Aracaju"))
      .await
      .expect("first save");
    let inserted =
      sqlx::query("INSERT INTO store_settings (id) VALUES (false)")
        .execute(&pool)
        .await;
    assert!(
      inserted.is_err(),
      "id = false must violate the singleton check"
    );
  }
}
