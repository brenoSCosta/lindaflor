use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{href, page, path_param, query_params},
  runtime::{Event, procedure, signal},
  view::{View, ViewExt, attributes, view},
};
use uuid::Uuid;

use crate::auth::user::current_user_owned;

use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::card::{
  card, card_content, card_footer, card_header, card_title,
};
use crate::components::container::{ContainerVariant, container};
use crate::components::separator::separator;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};

use crate::app::store::queries::{
  DEFAULT_WHATSAPP_NUMBER, format_price, get_store_settings,
  render_whatsapp_template, whatsapp_link,
};

path_param!(pub(crate) id: Uuid, error = not_found);

#[query_params(error = not_found)]
struct PedidoQuery {
  t: Option<String>,
}

struct OrderItem {
  product_name: String,
  variant_label: String,
  quantity: i32,
  unit_price_cents: i32,
}

struct OrderDetail {
  id: Uuid,
  status: String,
  user_id: Option<Uuid>,
  access_token: Uuid,
  guest_email: Option<String>,
  subtotal_cents: i32,
  shipping_cents: i32,
  discount_cents: i32,
  total_cents: i32,
  shipping_address: Option<serde_json::Value>,
  payment_meta: Option<serde_json::Value>,
  created_at: String,
  items: Vec<OrderItem>,
}

fn token_or_owner_allows(
  stored_token: Uuid,
  order_user_id: Option<Uuid>,
  query_t: Option<&str>,
  session_user_id: Option<Uuid>,
) -> bool {
  if let Some(raw) = query_t
    && let Ok(parsed) = Uuid::parse_str(raw.trim())
    && parsed == stored_token
  {
    return true;
  }
  matches!((order_user_id, session_user_id), (Some(owner), Some(user)) if owner == user)
}

#[derive(sqlx::FromRow)]
struct OrderRow {
  id: Uuid,
  status: String,
  user_id: Option<Uuid>,
  access_token: Uuid,
  guest_email: Option<String>,
  subtotal_cents: i32,
  shipping_cents: i32,
  discount_cents: i32,
  total_cents: i32,
  shipping_address: Option<serde_json::Value>,
  payment_meta: Option<serde_json::Value>,
  created_at: String,
}

#[derive(sqlx::FromRow)]
struct ItemRow {
  product_name: String,
  variant_label: String,
  quantity: i32,
  unit_price_cents: i32,
}

#[derive(sqlx::FromRow)]
struct StatusAccessRow {
  status: String,
  access_token: Uuid,
  user_id: Option<Uuid>,
}

async fn get_order(
  pool: &PgPool,
  id: Uuid,
) -> Result<Option<OrderDetail>, sqlx::Error> {
  let order = sqlx::query_as::<_, OrderRow>(
    "SELECT id, status::text AS status, user_id, access_token, guest_email, subtotal_cents, shipping_cents,
            discount_cents, total_cents, shipping_address, payment_meta, created_at::text AS created_at
     FROM orders WHERE id = $1",
  )
  .bind(id)
  .fetch_optional(pool)
  .await?;

  let Some(order) = order else {
    return Ok(None);
  };

  let items = sqlx::query_as::<_, ItemRow>(
    "SELECT product_name, variant_label, quantity, unit_price_cents
     FROM order_items WHERE order_id = $1",
  )
  .bind(id)
  .fetch_all(pool)
  .await?;

  Ok(Some(OrderDetail {
    id: order.id,
    status: order.status,
    user_id: order.user_id,
    access_token: order.access_token,
    guest_email: order.guest_email,
    subtotal_cents: order.subtotal_cents,
    shipping_cents: order.shipping_cents,
    discount_cents: order.discount_cents,
    total_cents: order.total_cents,
    shipping_address: order.shipping_address,
    payment_meta: order.payment_meta,
    created_at: order.created_at,
    items: items
      .into_iter()
      .map(|i| OrderItem {
        product_name: i.product_name,
        variant_label: i.variant_label,
        quantity: i.quantity,
        unit_price_cents: i.unit_price_cents,
      })
      .collect(),
  }))
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

fn coupon_code_from_meta(
  payment_meta: Option<&serde_json::Value>,
) -> Option<String> {
  payment_meta
    .and_then(|meta| meta.get("coupon_code"))
    .and_then(|value| value.as_str())
    .filter(|code| !code.is_empty())
    .map(str::to_string)
}

/// Re-read the order status without a document reload. No WebSocket:
/// the buyer taps "Atualizar status" after paying. The id comes from the
/// page URL; anything else is rejected as unknown.
#[procedure("/pedido/status-proc")]
async fn refresh_order_status(
  cx: &Cx,
  order_id: String,
  access_token: String,
) -> Result<String> {
  let Ok(id) = Uuid::parse_str(order_id.trim()) else {
    return Ok("unknown".to_string());
  };
  let pool = app_context::<PgPool>(cx);
  let row = sqlx::query_as::<_, StatusAccessRow>(
    "SELECT status::text AS status, access_token, user_id
     FROM orders WHERE id = $1",
  )
  .bind(id)
  .fetch_optional(pool)
  .await?;
  let Some(row) = row else {
    return Ok("unknown".to_string());
  };
  let session_user_id =
    current_user_owned(cx).await?.map(|session| session.user.id);
  let query_t = access_token.trim();
  if !token_or_owner_allows(
    row.access_token,
    row.user_id,
    (!query_t.is_empty()).then_some(query_t),
    session_user_id,
  ) {
    return Ok("unknown".to_string());
  }
  Ok(row.status)
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let id = path_param::<Id>(cx)?;
  let query = query_params::<PedidoQuery>(cx)?;
  let query_t = query
    .t
    .as_deref()
    .map(str::trim)
    .filter(|value| !value.is_empty());
  let session_user_id =
    current_user_owned(cx).await?.map(|session| session.user.id);

  let order = match get_order(pool, *id).await? {
    Some(o)
      if token_or_owner_allows(
        o.access_token,
        o.user_id,
        query_t,
        session_user_id,
      ) =>
    {
      o
    }
    Some(_) | None => {
      return Ok(view! {
                container(
                    variant: ContainerVariant::Narrow,
                    attrs: attributes! { class="py-24 text-center" },
                    <h1 class="text-4xl font-bold tracking-tight">"Pedido não encontrado"</h1>
                    <a
                        href=(href!(crate::app::produtos::page))
                        class=(button_variants(ButtonVariant::Primary, ButtonSize::Lg))
                    >
                        "Voltar ao catálogo"
                    </a>
                )
            }
            .boxed());
    }
  };

  let is_pending = order.status == "pending_payment";
  let heading = if is_pending {
    "Aguardando pagamento"
  } else {
    status_label(&order.status)
  };
  let pix_code = order
    .payment_meta
    .as_ref()
    .and_then(|m| m.get("pix_copy_paste"))
    .and_then(|v| v.as_str())
    .map(|s| s.to_string());
  let order_id_short = order.id.to_string().chars().take(8).collect::<String>();
  let order_total = format_price(order.total_cents);
  let settings = get_store_settings(pool).await?;
  let whatsapp_number = settings
    .whatsapp_number
    .as_deref()
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .unwrap_or(DEFAULT_WHATSAPP_NUMBER)
    .to_string();
  let whatsapp_fallback = format!(
    "Olá! Fiz o pedido {} no valor de {} e quero confirmar o pagamento via PIX.",
    order_id_short, order_total
  );
  let whatsapp_message = render_whatsapp_template(
    settings.whatsapp_message_template.as_deref(),
    &[
      ("order_id", order_id_short.as_str()),
      ("total", order_total.as_str()),
    ],
    &whatsapp_fallback,
  );
  let whatsapp_url = whatsapp_link(&whatsapp_number, &whatsapp_message);
  let guest_email = order.guest_email.clone().unwrap_or_default();
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
  let coupon_code = coupon_code_from_meta(order.payment_meta.as_ref());
  let order_id_string = order.id.to_string();
  let refresh_token = query_t.unwrap_or("").to_string();

  // Browser-only UI state: copy feedback + manually refreshed status.
  // No WebSocket; the buyer taps the button after paying on their bank app.
  let copied = signal(cx, || false);
  let live_status = signal(cx, || order.status.clone());
  let refresh_id = order_id_string.clone();

  Ok(view! {
        container(
            variant: ContainerVariant::Narrow,
            <p class="text-xs uppercase tracking-widest text-muted-foreground">"Pedido #" (order_id_short)</p>
            <div class="flex flex-wrap items-center gap-3">
                <h1 class="text-5xl font-bold tracking-tight">(heading)</h1>
                badge(variant: status_badge_variant(&order.status), (status_label(&order.status)))
                if is_pending {
                    <button
                        type="button"
                        class="rounded-lg border border-border px-3 py-1.5 text-sm font-medium text-muted-foreground hover:bg-foreground/5"
                        @click=$(async |_e: Event| {
                            let next = refresh_order_status(
                                refresh_id.to_owned(),
                                refresh_token.to_owned(),
                            )
                            .await;
                            live_status.set(next);
                        })
                    >
                        "Atualizar status"
                    </button>
                }
            </div>
            <p class="text-sm text-muted-foreground">
                "Status atual: " $(live_status.get())
            </p>
            <p class="text-muted-foreground">
                "Enviamos as instruções para " (guest_email)
            </p>

            if is_pending {
                card(
                    card_header(
                        card_title("Pague com PIX")
                    )
                    card_content(
                        <ol class="list-decimal space-y-2 pl-5 text-sm text-muted-foreground">
                            <li>"Copie o código PIX abaixo"</li>
                            <li>
                                "Pague exatamente "
                                <strong class="font-medium text-foreground">(format_price(order.total_cents))</strong>
                            </li>
                            <li>"Avise a loja no WhatsApp para confirmarmos o pagamento"</li>
                        </ol>
                        separator(attrs: attributes! { class="my-3" })
                        <p class="text-sm text-muted-foreground">
                            "A reserva de estoque é válida por 24 horas após a criação do pedido."
                        </p>
                        if let Some(code) = pix_code {
                            let code_text = code.clone();
                            <div class="space-y-2">
                                <p class="text-sm text-muted-foreground">"PIX copia e cola:"</p>
                                <code class="block overflow-x-auto rounded-lg border border-border bg-background p-3 text-xs">
                                    (code)
                                </code>
                                <button
                                    type="button"
                                    class="rounded-lg border border-border px-3 py-1.5 text-sm font-medium hover:bg-foreground/5"
                                    @click=$(|_e: Event| {
                                        let text = code_text.to_owned();
                                        raw!(
                                            "navigator.clipboard.writeText(${text})",
                                            {
                                                let _ = text.clone();
                                            }
                                        );
                                        copied.set(true);
                                    })
                                >
                                    $(if copied.get() { "Copiado!" } else { "Copiar código PIX" })
                                </button>
                            </div>
                        } else {
                            <p class="text-sm text-destructive">
                                "Instruções de pagamento indisponíveis. Fale conosco no WhatsApp."
                            </p>
                        }
                    )
                    card_footer(
                        <a
                            href=(whatsapp_url)
                            target="_blank"
                            rel="noreferrer"
                            class=(button_variants(ButtonVariant::Primary, ButtonSize::Lg))
                        >
                            "Avise no WhatsApp"
                        </a>
                    )
                )
            }

            card(
                card_header(
                    card_title("Itens")
                )
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
                            for item in order.items {
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
                card_header(
                    card_title("Totais")
                )
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

            if !address_display.is_empty() {
                card(
                    card_header(
                        card_title("Endereço de entrega")
                    )
                    card_content(
                        <p class="text-sm text-muted-foreground">(address_display)</p>
                        <p class="text-sm text-muted-foreground">"Pedido criado em " (order.created_at)</p>
                    )
                )
            }
        )
    }.boxed())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn matching_token_grants_access() {
    let token = Uuid::now_v7();
    assert!(token_or_owner_allows(
      token,
      None,
      Some(&token.to_string()),
      None
    ));
  }

  #[test]
  fn wrong_or_missing_token_denied_for_guest() {
    let token = Uuid::now_v7();
    assert!(!token_or_owner_allows(token, None, None, None));
    assert!(!token_or_owner_allows(
      token,
      None,
      Some(&Uuid::now_v7().to_string()),
      None
    ));
  }

  #[test]
  fn owner_session_skips_token() {
    let token = Uuid::now_v7();
    let owner = Uuid::now_v7();
    assert!(token_or_owner_allows(token, Some(owner), None, Some(owner)));
    assert!(!token_or_owner_allows(
      token,
      Some(owner),
      None,
      Some(Uuid::now_v7())
    ));
  }

  #[tokio::test]
  async fn get_order_loads_access_token() {
    let pool = crate::test_support::fresh_pool().await;
    let order_id = Uuid::now_v7();
    let access_token = Uuid::new_v4();
    sqlx::query(
      "INSERT INTO orders (id, guest_email, status, subtotal_cents, shipping_cents,
           discount_cents, total_cents, access_token)
       VALUES ($1, 'guest@example.com', 'pending_payment', 1000, 0, 0, 1000, $2)",
    )
    .bind(order_id)
    .bind(access_token)
    .execute(&pool)
    .await
    .expect("insert order");

    let order = get_order(&pool, order_id)
      .await
      .expect("load")
      .expect("present");
    assert_eq!(order.access_token, access_token);
    assert!(token_or_owner_allows(
      order.access_token,
      order.user_id,
      Some(&access_token.to_string()),
      None
    ));
    assert!(!token_or_owner_allows(
      order.access_token,
      order.user_id,
      Some(&Uuid::new_v4().to_string()),
      None
    ));
  }
}
