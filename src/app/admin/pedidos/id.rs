use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form,
    error::{RouterErrorExt, not_found},
    href, page, path_param,
    response::Response,
    route,
  },
  view::{View, attributes, view},
};
use uuid::Uuid;

use crate::app::auth_helpers::require_admin;
use crate::app::store::order::{
  ApplyOptions, DEFAULT_CARRIER, OrderEvent, Tracking, apply_with_audit,
  tracking_url,
};
use crate::app::store::queries::format_price;
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{ButtonVariant, button};
use crate::components::card::{card, card_content, card_header, card_title};
use crate::components::checkbox::checkbox;
use crate::components::container::container;
use crate::components::input::input;
use crate::components::label::label;
use crate::components::separator::separator;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};
use crate::components::toast::{Toast, set_toast, toast_redirect};

path_param!(pub(crate) id: String, error = not_found);

#[derive(Deserialize)]
pub struct StatusInput {
  status: String,
  restock: Option<String>,
  #[serde(default)]
  carrier: String,
  #[serde(default)]
  tracking_code: String,
}

#[derive(sqlx::FromRow)]
struct OrderRow {
  status: String,
  guest_email: Option<String>,
  subtotal_cents: i64,
  shipping_cents: i64,
  discount_cents: i64,
  total_cents: i64,
  shipping_address: Option<serde_json::Value>,
  payment_meta: Option<serde_json::Value>,
  paid_at: Option<String>,
  reservation_expires_at: Option<String>,
  created_at: String,
  carrier: String,
  tracking_code: Option<String>,
  shipped_at: Option<String>,
  delivered_at: Option<String>,
}

#[derive(sqlx::FromRow)]
struct ItemRow {
  product_name: String,
  variant_label: String,
  quantity: i32,
  unit_price_cents: i64,
}

/// Local-only transitions that record money moving (`Pay`, `ReopenPay`)
/// or returned (`Refund`) without talking to any PSP. For Mercado Pago
/// orders the PSP is the source of truth: "Sincronizar" / "Estornar no
/// Mercado Pago" drive those edges, so the manual buttons are hidden and
/// the POST is refused (a local "Estornar" would not move any money).
fn manual_money_event_blocked(mercado_pago: bool, event: OrderEvent) -> bool {
  mercado_pago
    && matches!(
      event,
      OrderEvent::Pay | OrderEvent::ReopenPay | OrderEvent::Refund
    )
}

const MP_MANUAL_BLOCKED_MSG: &str = "Pedido do Mercado Pago: use \"Sincronizar com Mercado Pago\" ou \"Estornar no Mercado Pago\". A baixa manual não movimenta dinheiro.";

fn status_label(status: &str) -> &'static str {
  match status {
    "pending_payment" => "Aguardando pagamento",
    "paid" => "Pago",
    "processing" => "Em separação",
    "shipped" => "Enviado",
    "delivered" => "Entregue",
    "cancelled" => "Cancelado",
    "refunded" => "Estornado",
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

/// Status change (POST/redirect/GET): applies the transition with an audit
/// row in one transaction, then redirects back with a toast. Refresh-safe:
/// the browser lands on a GET, so a refresh never re-submits the form.
#[route(POST "/admin/pedidos/{id}")]
async fn update_order(
  cx: &Cx,
  Form(form): Form<StatusInput>,
) -> Result<Response> {
  let actor = require_admin(cx).await?;
  let id = path_param::<Id>(cx)?;
  let list_url = href!(super::page).resolve(cx);
  let Ok(order_id) = Uuid::parse_str(id.trim()) else {
    set_toast(cx, Toast::error("Pedido inválido."));
    return toast_redirect(cx, list_url);
  };
  let back = href!(page, Id(order_id.to_string())).resolve(cx);
  let fail = |message: &str| {
    set_toast(cx, Toast::error(message));
    toast_redirect(cx, back.clone())
  };

  let pool = app_context::<PgPool>(cx);
  // "Sincronizar com Mercado Pago" / "Estornar no Mercado Pago".
  if let Some(action) =
    crate::payments::service::AdminAction::parse(form.status.trim())
  {
    let provider = crate::payments::PaymentProvider::from_cx(cx);
    let actor = crate::payments::service::Actor {
      id: Some(actor.user.id),
      name: actor.user.name.as_str(),
    };
    return match crate::payments::service::run_admin_action(
      pool, &provider, order_id, action, actor,
    )
    .await
    {
      Ok(message) => {
        set_toast(cx, Toast::success(message));
        toast_redirect(cx, back)
      }
      Err(message) => fail(&message),
    };
  }

  let Some(event) = OrderEvent::parse(form.status.trim()) else {
    return fail("Status inválido.");
  };
  let provider: Option<String> =
    sqlx::query_scalar("SELECT payment_provider FROM orders WHERE id = $1")
      .bind(order_id)
      .fetch_optional(pool)
      .await?;
  let mercado_pago = provider.as_deref()
    == Some(crate::payments::ProviderKind::MercadoPago.as_str());
  if manual_money_event_blocked(mercado_pago, event) {
    return fail(MP_MANUAL_BLOCKED_MSG);
  }
  // The checkbox is always the explicit restock choice: browsers omit an
  // unchecked box, so absent = "do not restock". The forms pre-check it for
  // Paid/Processing (the transition default) and leave it off otherwise.
  let restock = form.restock.is_some();
  // Only `Ship` reads the tracking; `apply` validates it in the same
  // transaction as the status change.
  let opts = ApplyOptions::with_restock(restock).with_tracking(Tracking {
    carrier: form.carrier.as_str(),
    code: form.tracking_code.as_str(),
  });
  let mut tx = pool.begin().await?;
  let outcome = apply_with_audit(
    &mut tx,
    order_id,
    event,
    opts,
    Some(actor.user.id),
    Some(actor.user.name.as_str()),
  )
  .await;
  match outcome {
    Ok(_) => {
      tx.commit().await?;
      set_toast(cx, Toast::success("Pedido atualizado com sucesso."));
      toast_redirect(cx, back)
    }
    Err(error) => {
      let _ = tx.rollback().await;
      match &error {
        crate::app::store::order::ApplyError::Db(_) => {
          fail("Não foi possível atualizar o pedido. Tente novamente.")
        }
        _ => fail(&error.to_string()),
      }
    }
  }
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let id = path_param::<Id>(cx)?;
  let order_id = Uuid::parse_str(id).map_err(|_| not_found())?;

  let order = sqlx::query_as::<_, OrderRow>(
    "SELECT status::text AS status, guest_email, subtotal_cents, shipping_cents,
            discount_cents, total_cents, shipping_address, payment_meta,
            paid_at::text AS paid_at,
            to_char(reservation_expires_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS reservation_expires_at,
            to_char(created_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS created_at,
            carrier, tracking_code,
            to_char(shipped_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS shipped_at,
            to_char(delivered_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS delivered_at
     FROM orders WHERE id = $1",
  )
  .bind(order_id)
  .fetch_optional(pool)
  .await?
  .ok_or_not_found()?;

  let items = sqlx::query_as::<_, ItemRow>(
    "SELECT product_name, variant_label, quantity, unit_price_cents
     FROM order_items WHERE order_id = $1",
  )
  .bind(order_id)
  .fetch_all(pool)
  .await?;

  let pending = order.status == "pending_payment";
  let paid = order.status == "paid";
  let processing = order.status == "processing";
  let shipped = order.status == "shipped";
  let delivered = order.status == "delivered";
  // Reopen is only offered for orders cancelled while still unpaid.
  let reopenable = order.status == "cancelled" && order.paid_at.is_none();
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
  let payment = crate::payments::service::admin_payment_info(pool, order_id)
    .await?
    .filter(|info| info.provider == crate::payments::ProviderKind::MercadoPago);
  let mp_refundable = payment
    .as_ref()
    .is_some_and(|info| info.refundable(&order.status));
  // Static-PIX orders keep the manual "Marcar como pago" / "Estornar";
  // Mercado Pago orders only get the PSP-backed actions.
  let manual_money = payment.is_none();
  let expires = order.reservation_expires_at.clone().unwrap_or_default();
  let created = order.created_at.clone();
  let status = order.status.clone();
  let can_ship = paid || processing;
  let tracking_link = order
    .tracking_code
    .as_deref()
    .and_then(|code| tracking_url(&order.carrier, code));

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
                  if manual_money {
                      <form method="post" action=(href!(page, Id(order_id.to_string())))>
                          <input type="hidden" name="status" value="paid">
                          button(
                              variant: ButtonVariant::Primary,
                              attrs: attributes! { type="submit" },
                              "Marcar como pago"
                          )
                      </form>
                  }
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
              <div class="mt-2 flex flex-wrap items-center gap-3">
                  <form method="post" action=(href!(page, Id(order_id.to_string())))>
                      <input type="hidden" name="status" value="processing">
                      button(
                          variant: ButtonVariant::Primary,
                          attrs: attributes! { type="submit" },
                          "Em separação"
                      )
                  </form>
                  <form method="post" action=(href!(page, Id(order_id.to_string()))) class="flex items-center gap-2">
                      <input type="hidden" name="status" value="cancelled">
                      checkbox(attrs: attributes! { id="restock-cancel" name="restock" value="1" checked=(true) })
                      label(attrs: attributes! { for="restock-cancel" }, "Devolver ao estoque")
                      button(
                          variant: ButtonVariant::Outline,
                          attrs: attributes! { type="submit" },
                          "Cancelar"
                      )
                  </form>
                  if manual_money {
                      <form method="post" action=(href!(page, Id(order_id.to_string()))) class="flex items-center gap-2">
                          <input type="hidden" name="status" value="refunded">
                          checkbox(attrs: attributes! { id="restock-refund" name="restock" value="1" checked=(true) })
                          label(attrs: attributes! { for="restock-refund" }, "Devolver ao estoque")
                          button(
                              variant: ButtonVariant::Outline,
                              attrs: attributes! { type="submit" },
                              "Estornar"
                          )
                      </form>
                  }
              </div>
          }
          if processing {
              <div class="mt-2 flex flex-wrap items-center gap-3">
                  <form method="post" action=(href!(page, Id(order_id.to_string()))) class="flex items-center gap-2">
                      <input type="hidden" name="status" value="cancelled">
                      checkbox(attrs: attributes! { id="restock-cancel" name="restock" value="1" checked=(true) })
                      label(attrs: attributes! { for="restock-cancel" }, "Devolver ao estoque")
                      button(
                          variant: ButtonVariant::Outline,
                          attrs: attributes! { type="submit" },
                          "Cancelar"
                      )
                  </form>
                  if manual_money {
                      <form method="post" action=(href!(page, Id(order_id.to_string()))) class="flex items-center gap-2">
                          <input type="hidden" name="status" value="refunded">
                          checkbox(attrs: attributes! { id="restock-refund" name="restock" value="1" checked=(true) })
                          label(attrs: attributes! { for="restock-refund" }, "Devolver ao estoque")
                          button(
                              variant: ButtonVariant::Outline,
                              attrs: attributes! { type="submit" },
                              "Estornar"
                          )
                      </form>
                  }
              </div>
          }
          if shipped {
              <div class="mt-2 flex flex-wrap items-center gap-3">
                  <form method="post" action=(href!(page, Id(order_id.to_string())))>
                      <input type="hidden" name="status" value="delivered">
                      button(
                          variant: ButtonVariant::Primary,
                          attrs: attributes! { type="submit" },
                          "Marcar como entregue"
                      )
                  </form>
                  <form method="post" action=(href!(page, Id(order_id.to_string()))) class="flex items-center gap-2">
                      <input type="hidden" name="status" value="cancelled">
                      checkbox(attrs: attributes! { id="restock-cancel" name="restock" value="1" })
                      label(attrs: attributes! { for="restock-cancel" }, "Devolver ao estoque")
                      button(
                          variant: ButtonVariant::Outline,
                          attrs: attributes! { type="submit" },
                          "Cancelar"
                      )
                  </form>
                  if manual_money {
                      <form method="post" action=(href!(page, Id(order_id.to_string()))) class="flex items-center gap-2">
                          <input type="hidden" name="status" value="refunded">
                          checkbox(attrs: attributes! { id="restock-refund" name="restock" value="1" })
                          label(attrs: attributes! { for="restock-refund" }, "Devolver ao estoque")
                          button(
                              variant: ButtonVariant::Outline,
                              attrs: attributes! { type="submit" },
                              "Estornar"
                          )
                      </form>
                  }
              </div>
          }
          if delivered {
              <div class="mt-2 flex flex-wrap items-center gap-3">
                  if manual_money {
                      <form method="post" action=(href!(page, Id(order_id.to_string()))) class="flex items-center gap-2">
                          <input type="hidden" name="status" value="refunded">
                          checkbox(attrs: attributes! { id="restock-refund" name="restock" value="1" })
                          label(attrs: attributes! { for="restock-refund" }, "Devolver ao estoque")
                          button(
                              variant: ButtonVariant::Outline,
                              attrs: attributes! { type="submit" },
                              "Estornar"
                          )
                      </form>
                  }
              </div>
          }
          if reopenable {
              <div class="mt-2 flex flex-wrap items-center gap-3">
                  <form method="post" action=(href!(page, Id(order_id.to_string())))>
                      <input type="hidden" name="status" value="reopen">
                      button(
                          variant: ButtonVariant::Primary,
                          attrs: attributes! { type="submit" },
                          "Reabrir pedido"
                      )
                  </form>
                  if manual_money {
                      <form method="post" action=(href!(page, Id(order_id.to_string())))>
                          <input type="hidden" name="status" value="reopen_pay">
                          button(
                              variant: ButtonVariant::Outline,
                              attrs: attributes! { type="submit" },
                              "Recebido após expirar"
                          )
                      </form>
                  }
              </div>
          }

          if can_ship {
              card(
                  card_header(card_title("Envio"))
                  card_content(
                      <form
                          method="post"
                          action=(href!(page, Id(order_id.to_string())))
                          class="grid gap-3 @sm/page:grid-cols-[12rem_1fr_auto] @sm/page:items-end"
                      >
                          <input type="hidden" name="status" value="shipped">
                          <div class="flex flex-col gap-1.5">
                              label(attrs: attributes! { for="carrier" }, "Transportadora")
                              input(attrs: attributes! {
                                  id="carrier"
                                  name="carrier"
                                  value=(DEFAULT_CARRIER)
                                  maxlength="40"
                              })
                          </div>
                          <div class="flex flex-col gap-1.5">
                              label(attrs: attributes! { for="tracking_code" }, "Código de rastreio")
                              input(attrs: attributes! {
                                  id="tracking_code"
                                  name="tracking_code"
                                  placeholder="AA123456789BR"
                                  required=""
                                  maxlength="40"
                                  autocomplete="off"
                              })
                          </div>
                          button(
                              variant: ButtonVariant::Primary,
                              attrs: attributes! { type="submit" },
                              "Marcar como enviado"
                          )
                      </form>
                  )
              )
          }

          if let Some(code) = order.tracking_code.clone() {
              card(
                  card_header(card_title("Rastreamento"))
                  card_content(
                      <p class="text-sm">(order.carrier.clone()) ": " <span class="font-mono">(code)</span></p>
                      if let Some(url) = tracking_link {
                          <a href=(url) target="_blank" rel="noreferrer" class="text-sm text-primary hover:underline">
                              "Abrir rastreamento"
                          </a>
                      }
                      if let Some(at) = order.shipped_at.clone() {
                          <p class="text-sm text-muted-foreground">"Enviado em " (at)</p>
                      }
                      if let Some(at) = order.delivered_at.clone() {
                          <p class="text-sm text-muted-foreground">"Entregue em " (at)</p>
                      }
                  )
              )
          }

          if let Some(info) = payment {
              card(
                  card_header(card_title("Mercado Pago"))
                  card_content(
                      <p class="text-sm">
                          "Pagamento: "
                          <span class="font-mono">
                              (info.mp_payment_id.map(|id| id.to_string()).unwrap_or_else(|| "não criado".to_string()))
                          </span>
                      </p>
                      <p class="text-sm">
                          "Status MP: " (info.mp_status.clone().unwrap_or_else(|| "—".to_string()))
                          if let Some(detail) = info.mp_status_detail.clone() {
                              " (" (detail) ")"
                          }
                      </p>
                      if let Some(synced) = info.synced_at.clone() {
                          <p class="text-sm text-muted-foreground">"Última sincronização: " (synced)</p>
                      }
                      if info.mp_payment_id.is_some() {
                          <div class="mt-2 flex flex-wrap gap-3">
                              <form method="post" action=(href!(page, Id(order_id.to_string())))>
                                  <input type="hidden" name="status" value="mp_sync">
                                  button(
                                      variant: ButtonVariant::Outline,
                                      attrs: attributes! { type="submit" },
                                      "Sincronizar com Mercado Pago"
                                  )
                              </form>
                              if mp_refundable {
                                  <form method="post" action=(href!(page, Id(order_id.to_string())))>
                                      <input type="hidden" name="status" value="mp_refund">
                                      button(
                                          variant: ButtonVariant::Destructive,
                                          attrs: attributes! { type="submit" },
                                          "Estornar no Mercado Pago"
                                      )
                                  </form>
                              }
                          </div>
                      }
                  )
              )
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
                                  table_cell((format_price(item.unit_price_cents.saturating_mul(i64::from(item.quantity)))))
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

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn mercado_pago_orders_refuse_manual_money_transitions() {
    for event in [OrderEvent::Pay, OrderEvent::ReopenPay, OrderEvent::Refund] {
      assert!(manual_money_event_blocked(true, event), "{event:?}");
      assert!(!manual_money_event_blocked(false, event), "{event:?}");
    }
    for event in [
      OrderEvent::Process,
      OrderEvent::Ship,
      OrderEvent::Deliver,
      OrderEvent::Cancel,
      OrderEvent::Reopen,
    ] {
      assert!(!manual_money_event_blocked(true, event), "{event:?}");
    }
  }
}
