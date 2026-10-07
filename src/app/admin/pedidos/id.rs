use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{content::Form, error::bad_request, href, page, path_param},
  view::{View, attributes, view},
};
use uuid::Uuid;

use crate::app::auth_helpers::require_admin;
use crate::app::store::inventory::{convert_reservation, release_reservation};
use crate::app::store::queries::format_price;
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{ButtonVariant, button};
use crate::components::card::{card, card_content, card_header, card_title};
use crate::components::container::container;
use crate::components::separator::separator;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};

path_param!(pub(crate) id: String, error = bad_request);

#[derive(Deserialize)]
pub struct StatusInput {
  status: String,
}

#[derive(sqlx::FromRow)]
struct OrderRow {
  status: String,
  guest_email: Option<String>,
  subtotal_cents: i32,
  shipping_cents: i32,
  discount_cents: i32,
  total_cents: i32,
  shipping_address: Option<serde_json::Value>,
  payment_meta: Option<serde_json::Value>,
  reservation_expires_at: Option<String>,
  created_at: String,
}

#[derive(sqlx::FromRow)]
struct ItemRow {
  product_name: String,
  variant_label: String,
  quantity: i32,
  unit_price_cents: i32,
}

fn status_label(status: &str) -> &'static str {
  match status {
    "pending_payment" => "Aguardando pagamento",
    "paid" => "Pago",
    "processing" => "Em separação",
    "shipped" => "Enviado",
    "delivered" => "Entregue",
    "cancelled" => "Cancelado",
    _ => "Desconhecido",
  }
}

fn status_badge_variant(status: &str) -> BadgeVariant {
  match status {
    "pending_payment" => BadgeVariant::Secondary,
    "cancelled" => BadgeVariant::Destructive,
    _ => BadgeVariant::Primary,
  }
}

#[page([GET, POST])]
pub async fn page(
  cx: &Cx,
  body: Option<Form<StatusInput>>,
) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let id = path_param::<Id>(cx)?;
  let order_id = Uuid::parse_str(id).map_err(|_| bad_request("invalid id"))?;

  if let Some(Form(input)) = body {
    let mut tx = pool.begin().await?;
    let current: Option<String> = sqlx::query_scalar(
      "SELECT status::text FROM orders WHERE id = $1 FOR UPDATE",
    )
    .bind(order_id)
    .fetch_optional(&mut *tx)
    .await?;
    let Some(current) = current else {
      return Err(bad_request("order not found").into());
    };
    if current == "pending_payment" {
      match input.status.as_str() {
        "paid" => {
          convert_reservation(&mut tx, order_id).await?;
          sqlx::query(
            "UPDATE orders SET status = 'paid', updated_at = now() WHERE id = $1",
          )
          .bind(order_id)
          .execute(&mut *tx)
          .await?;
        }
        "cancelled" => {
          release_reservation(&mut tx, order_id).await?;
          sqlx::query(
            "UPDATE orders SET status = 'cancelled', updated_at = now() WHERE id = $1",
          )
          .bind(order_id)
          .execute(&mut *tx)
          .await?;
        }
        _ => {}
      }
    } else if current == "paid" && input.status == "delivered" {
      sqlx::query(
        "UPDATE orders SET status = 'delivered', updated_at = now() WHERE id = $1",
      )
      .bind(order_id)
      .execute(&mut *tx)
      .await?;
    }
    tx.commit().await?;
  }

  let order = sqlx::query_as::<_, OrderRow>(
    "SELECT status::text AS status, guest_email, subtotal_cents, shipping_cents,
            discount_cents, total_cents, shipping_address, payment_meta,
            reservation_expires_at::text AS reservation_expires_at,
            created_at::text AS created_at
     FROM orders WHERE id = $1",
  )
  .bind(order_id)
  .fetch_optional(pool)
  .await?
  .ok_or_else(|| bad_request("order not found"))?;

  let items = sqlx::query_as::<_, ItemRow>(
    "SELECT product_name, variant_label, quantity, unit_price_cents
     FROM order_items WHERE order_id = $1",
  )
  .bind(order_id)
  .fetch_all(pool)
  .await?;

  let pending = order.status == "pending_payment";
  let paid = order.status == "paid";
  let email = order.guest_email.clone().unwrap_or_default();
  let pix_code = order
    .payment_meta
    .as_ref()
    .and_then(|meta| meta.get("pix_copy_paste"))
    .and_then(|value| value.as_str())
    .map(str::to_string);
  let coupon_code = order
    .payment_meta
    .as_ref()
    .and_then(|meta| meta.get("coupon_code"))
    .and_then(|value| value.as_str())
    .filter(|code| !code.is_empty())
    .map(str::to_string);
  let address_display = order
    .shipping_address
    .as_ref()
    .and_then(|a| {
      let street = a.get("street")?.as_str()?;
      let number = a.get("number")?.as_str()?;
      let neighborhood = a.get("neighborhood")?.as_str()?;
      let city = a.get("city")?.as_str()?;
      let state = a.get("state")?.as_str()?;
      let zip = a.get("zip_code")?.as_str()?;
      Some(format!(
        "{}, {} — {}, {}/{} — CEP {}",
        street, number, neighborhood, city, state, zip
      ))
    })
    .unwrap_or_default();
  let phone = order
    .shipping_address
    .as_ref()
    .and_then(|a| a.get("phone"))
    .and_then(|value| value.as_str())
    .unwrap_or("")
    .to_string();
  let expires = order.reservation_expires_at.clone().unwrap_or_default();
  let created = order.created_at.clone();
  let status = order.status.clone();

  Ok(view! {
      container(
          <div class="flex flex-wrap items-center gap-3">
              <h1 class="text-2xl font-semibold tracking-tight">"Pedido"</h1>
              badge(variant: status_badge_variant(&status), (status_label(&status)))
          </div>
          <p class="font-mono text-sm text-muted-foreground">(order_id.to_string())</p>
          <p class="text-sm text-muted-foreground">"Criado em " (created)</p>

          if pending {
              <div class="mt-2 flex flex-wrap gap-3">
                  <form method="post" action=(href!(page, Id(order_id.to_string())))>
                      <input type="hidden" name="status" value="paid">
                      button(
                          variant: ButtonVariant::Primary,
                          attrs: attributes! { type="submit" },
                          "Marcar como pago"
                      )
                  </form>
                  <form method="post" action=(href!(page, Id(order_id.to_string())))>
                      <input type="hidden" name="status" value="cancelled">
                      button(
                          variant: ButtonVariant::Outline,
                          attrs: attributes! { type="submit" },
                          "Cancelar"
                      )
                  </form>
              </div>
          }
          if paid {
              <form method="post" action=(href!(page, Id(order_id.to_string()))) class="mt-2">
                  <input type="hidden" name="status" value="delivered">
                  button(
                      variant: ButtonVariant::Primary,
                      attrs: attributes! { type="submit" },
                      "Marcar como entregue"
                  )
              </form>
          }

          card(
              card_header(card_title("Cliente"))
              card_content(
                  if !email.is_empty() {
                      <p class="text-sm">"E-mail: " (email.clone())</p>
                  }
                  if !phone.is_empty() {
                      <p class="text-sm">"WhatsApp: " (phone)</p>
                  }
                  if !address_display.is_empty() {
                      <p class="text-sm text-muted-foreground">(address_display)</p>
                  }
              )
          )

          if pending {
              card(
                  card_header(card_title("Pagamento PIX"))
                  card_content(
                      if !expires.is_empty() {
                          <p class="text-sm text-muted-foreground">
                              "Reserva de estoque até " (expires)
                          </p>
                      }
                      if let Some(code) = pix_code {
                          <p class="text-sm text-muted-foreground">"PIX copia e cola:"</p>
                          <code class="block overflow-x-auto rounded-lg border border-border bg-background p-3 text-xs">
                              (code)
                          </code>
                      }
                  )
              )
          }

          card(
              card_header(card_title("Itens"))
              card_content(
                  table(
                      table_header(
                          table_row(
                              table_head("Produto")
                              table_head("Variação")
                              table_head("Qtd.")
                              table_head("Preço")
                          )
                      )
                      table_body(
                          for item in items {
                              table_row(
                                  table_cell((item.product_name))
                                  table_cell((item.variant_label))
                                  table_cell((item.quantity))
                                  table_cell((format_price(item.unit_price_cents * item.quantity)))
                              )
                          }
                      )
                  )
              )
          )

          card(
              card_header(card_title("Totais"))
              card_content(
                  <div class="flex flex-col gap-2 text-sm">
                      <div class="flex justify-between">
                          <span class="text-muted-foreground">"Subtotal"</span>
                          <span>(format_price(order.subtotal_cents))</span>
                      </div>
                      <div class="flex justify-between">
                          <span class="text-muted-foreground">"Frete"</span>
                          if order.shipping_cents == 0 {
                              <span>"Grátis"</span>
                          } else {
                              <span>(format_price(order.shipping_cents))</span>
                          }
                      </div>
                      if order.discount_cents > 0 {
                          <div class="flex justify-between">
                              <span class="text-muted-foreground">
                                  if let Some(code) = coupon_code {
                                      "Desconto (" (code) ")"
                                  } else {
                                      "Desconto"
                                  }
                              </span>
                              <span>"-" (format_price(order.discount_cents))</span>
                          </div>
                      }
                      separator(attrs: attributes! { class="my-1" })
                      <div class="flex justify-between text-base font-medium">
                          <span>"Total"</span>
                          <span class="text-primary">(format_price(order.total_cents))</span>
                      </div>
                  </div>
              )
          )
      )
  })
}
