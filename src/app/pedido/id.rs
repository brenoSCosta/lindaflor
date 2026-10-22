pub mod troca;

use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{
    content::Form, href, page, path_param, query_params, response::Response,
    route,
  },
  runtime::{Event, procedure, signal},
  view::{View, ViewExt, attributes, view},
};
use uuid::Uuid;

use crate::app::store::reviews::order_review_links;
use crate::auth::user::current_user_owned;

use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::card::{
  card, card_content, card_footer, card_header, card_title,
};
use crate::components::container::{ContainerVariant, container};
use crate::components::separator::separator;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};
use crate::components::toast::{Toast, set_toast, toast_redirect};

use crate::app::store::order::{
  ApplyError, ApplyOptions, OrderEvent, apply_with_audit,
};
use crate::app::store::order_timeline::{
  OrderProgress, PROGRESS_COLUMNS, ProgressRow, order_timeline,
};

use crate::app::store::queries::{
  format_price, get_store_settings, render_whatsapp_template, whatsapp_link,
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
  unit_price_cents: i64,
}

struct OrderDetail {
  id: Uuid,
  status: String,
  user_id: Option<Uuid>,
  access_token: Uuid,
  token_expired: bool,
  guest_email: Option<String>,
  subtotal_cents: i64,
  shipping_cents: i64,
  discount_cents: i64,
  pix_discount_cents: i64,
  total_cents: i64,
  shipping_address: Option<serde_json::Value>,
  payment_meta: Option<serde_json::Value>,
  created_at: String,
  progress: OrderProgress,
  items: Vec<OrderItem>,
}

/// Guest links (`?t=`) stop working once `access_token_expires_at` passes;
/// an expired token gets the same "not found" as a wrong one. The signed-in
/// owner is never affected by the expiry.
pub(crate) fn token_or_owner_allows(
  stored_token: Uuid,
  token_expired: bool,
  order_user_id: Option<Uuid>,
  query_t: Option<&str>,
  session_user_id: Option<Uuid>,
) -> bool {
  if !token_expired
    && let Some(raw) = query_t
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
  token_expired: bool,
  guest_email: Option<String>,
  subtotal_cents: i64,
  shipping_cents: i64,
  discount_cents: i64,
  pix_discount_cents: i64,
  total_cents: i64,
  shipping_address: Option<serde_json::Value>,
  payment_meta: Option<serde_json::Value>,
  created_at: String,
}

#[derive(sqlx::FromRow)]
struct ItemRow {
  product_name: String,
  variant_label: String,
  quantity: i32,
  unit_price_cents: i64,
}

#[derive(sqlx::FromRow)]
struct StatusAccessRow {
  status: String,
  access_token: Uuid,
  token_expired: bool,
  user_id: Option<Uuid>,
}

async fn get_order(
  pool: &PgPool,
  id: Uuid,
) -> Result<Option<OrderDetail>, sqlx::Error> {
  let order = sqlx::query_as::<_, OrderRow>(
    "SELECT id, status::text AS status, user_id, access_token,
            access_token_expires_at <= now() AS token_expired,
            guest_email, subtotal_cents, shipping_cents,
            discount_cents, pix_discount_cents, total_cents, shipping_address, payment_meta,
            to_char(created_at AT TIME ZONE 'America/Sao_Paulo', 'DD/MM/YYYY HH24:MI') AS created_at
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
  let progress: OrderProgress = sqlx::query_as::<_, ProgressRow>(&format!(
    "SELECT {PROGRESS_COLUMNS} FROM orders o WHERE o.id = $1"
  ))
  .bind(id)
  .fetch_one(pool)
  .await?
  .into();

  Ok(Some(OrderDetail {
    id: order.id,
    status: order.status,
    user_id: order.user_id,
    access_token: order.access_token,
    token_expired: order.token_expired,
    guest_email: order.guest_email,
    subtotal_cents: order.subtotal_cents,
    shipping_cents: order.shipping_cents,
    discount_cents: order.discount_cents,
    pix_discount_cents: order.pix_discount_cents,
    total_cents: order.total_cents,
    shipping_address: order.shipping_address,
    payment_meta: order.payment_meta,
    created_at: order.created_at,
    progress,
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
    "refunded" => "Estornado",
    _ => "Desconhecido",
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
    "SELECT status::text AS status, access_token,
            access_token_expires_at <= now() AS token_expired, user_id
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
    row.token_expired,
    row.user_id,
    (!query_t.is_empty()).then_some(query_t),
    session_user_id,
  ) {
    return Ok("unknown".to_string());
  }
  Ok(row.status)
}

#[derive(Deserialize)]
pub struct CancelInput {
  #[serde(default)]
  t: String,
}

#[derive(Debug, PartialEq, Eq)]
enum CustomerCancel {
  Cancelled,
  /// Unknown order, or the caller is neither the owner nor holds a valid
  /// guest token. Same answer either way.
  NotFound,
  /// Paid (or otherwise past `pending_payment`): only the store can cancel.
  NotPending,
}

/// Customer-initiated cancel of an unpaid order. The access check and the
/// status check run under the same row lock as the transition, so a
/// payment landing concurrently can't be cancelled from here.
async fn cancel_unpaid_order(
  pool: &PgPool,
  order_id: Uuid,
  query_t: Option<&str>,
  session_user_id: Option<Uuid>,
) -> Result<CustomerCancel, ApplyError> {
  let mut tx = pool.begin().await?;
  let row = sqlx::query_as::<_, StatusAccessRow>(
    "SELECT status::text AS status, access_token,
            access_token_expires_at <= now() AS token_expired, user_id
     FROM orders WHERE id = $1 FOR UPDATE",
  )
  .bind(order_id)
  .fetch_optional(&mut *tx)
  .await?;
  let Some(row) = row else {
    return Ok(CustomerCancel::NotFound);
  };
  if !token_or_owner_allows(
    row.access_token,
    row.token_expired,
    row.user_id,
    query_t,
    session_user_id,
  ) {
    return Ok(CustomerCancel::NotFound);
  }
  if row.status != "pending_payment" {
    return Ok(CustomerCancel::NotPending);
  }
  // Releases the stock hold and restores the coupon (see `order::apply`).
  apply_with_audit(
    &mut tx,
    order_id,
    OrderEvent::Cancel,
    ApplyOptions::default(),
    session_user_id,
    Some("cliente"),
  )
  .await?;
  tx.commit().await?;
  Ok(CustomerCancel::Cancelled)
}

#[route(POST "/pedido/{id}/cancelar")]
async fn cancel_order(
  cx: &Cx,
  Form(form): Form<CancelInput>,
) -> Result<Response> {
  let id = path_param::<Id>(cx)?;
  let pool = app_context::<PgPool>(cx);
  let session_user_id =
    current_user_owned(cx).await?.map(|session| session.user.id);
  let token = form.t.trim();
  let query_t = (!token.is_empty()).then_some(token);
  let back = match query_t {
    Some(token) => href!(page, Id(*id)).query([("t", token)]).resolve(cx),
    None => href!(page, Id(*id)).resolve(cx),
  };
  let outcome =
    match cancel_unpaid_order(pool, *id, query_t, session_user_id).await {
      Ok(outcome) => outcome,
      Err(ApplyError::Db(error)) => return Err(error.into()),
      Err(error) => {
        set_toast(cx, Toast::error(error.to_string()));
        return toast_redirect(cx, back);
      }
    };
  match outcome {
    CustomerCancel::Cancelled => {
      set_toast(cx, Toast::success("Pedido cancelado."));
    }
    CustomerCancel::NotFound => {
      set_toast(cx, Toast::error("Pedido não encontrado."));
    }
    CustomerCancel::NotPending => set_toast(
      cx,
      Toast::error(
        "Este pedido já foi pago. Fale conosco no WhatsApp para cancelar.",
      ),
    ),
  }
  toast_redirect(cx, back)
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
        o.token_expired,
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
  // Mercado Pago orders show the PSP's QR / copy-paste code (created
  // lazily here if MP was down at checkout); static orders keep the local
  // BR Code from `payment_meta`.
  let payment = if is_pending {
    crate::payments::service::order_payment(
      pool,
      &crate::payments::PaymentProvider::from_cx(cx),
      order.id,
    )
    .await?
  } else {
    crate::payments::service::OrderPayment::StaticPix
  };
  let (is_mp, mp_charge) = match payment {
    crate::payments::service::OrderPayment::MercadoPago(charge) => {
      (true, charge)
    }
    crate::payments::service::OrderPayment::StaticPix => (false, None),
  };
  let pix_code = if is_mp {
    mp_charge.as_ref().and_then(|charge| charge.qr_code.clone())
  } else {
    order
      .payment_meta
      .as_ref()
      .and_then(|m| m.get("pix_copy_paste"))
      .and_then(|v| v.as_str())
      .map(|s| s.to_string())
  };
  let mp_qr_image = mp_charge
    .as_ref()
    .and_then(|charge| charge.qr_code_base64.as_deref())
    .map(|b64| format!("data:image/png;base64,{b64}"));
  let mp_ticket_url = mp_charge
    .as_ref()
    .and_then(|charge| charge.ticket_url.clone());
  let mp_expires = mp_charge
    .as_ref()
    .and_then(|charge| charge.expires_label.clone());
  let order_id_short = order.id.to_string().chars().take(8).collect::<String>();
  let order_total = format_price(order.total_cents);
  let settings = get_store_settings(pool).await?;
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
  let whatsapp_url = settings
    .whatsapp_number
    .as_deref()
    .map(str::trim)
    .filter(|s| !s.is_empty())
    .map(|number| whatsapp_link(number, &whatsapp_message));
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
  let cancel_token = refresh_token.clone();
  let progress = order.progress.clone();
  let returns =
    crate::app::store::returns::order_returns(pool, order.id).await?;
  let returns_token = query_t.map(str::to_string);
  let off_track = matches!(order.status.as_str(), "cancelled" | "refunded");

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
                            if is_mp {
                                <li>"A confirmação é automática: depois de pagar, toque em Atualizar status"</li>
                            } else {
                                <li>"Avise a loja no WhatsApp para confirmarmos o pagamento"</li>
                            }
                        </ol>
                        separator(attrs: attributes! { class="my-3" })
                        if let Some(expires) = mp_expires {
                            <p class="text-sm text-muted-foreground">
                                "O código PIX e a reserva de estoque valem até " (expires) "."
                            </p>
                        } else {
                            <p class="text-sm text-muted-foreground">
                                "A reserva de estoque é válida por 48 horas após a criação do pedido."
                            </p>
                        }
                        if let Some(src) = mp_qr_image {
                            <img src=(src) alt="QR Code PIX" class="mx-auto my-3 h-56 w-56 rounded-lg border border-border">
                        }
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
                                if let Some(url) = mp_ticket_url {
                                    <a href=(url) target="_blank" rel="noreferrer" class="block text-sm underline">
                                        "Abrir pagamento no Mercado Pago"
                                    </a>
                                }
                            </div>
                        } else if is_mp {
                            <p class="text-sm text-destructive">
                                (crate::payments::PAYMENT_UNAVAILABLE_MESSAGE) " Atualize a página para gerar o código PIX."
                            </p>
                        } else {
                            <p class="text-sm text-destructive">
                                "Instruções de pagamento indisponíveis. Fale conosco no WhatsApp."
                            </p>
                        }
                    )
                    card_footer(
                        if let Some(url) = whatsapp_url {
                          <a
                              href=(url)
                              target="_blank"
                              rel="noreferrer"
                              class=(button_variants(ButtonVariant::Primary, ButtonSize::Lg))
                          >
                              "Avise no WhatsApp"
                          </a>
                        }
                    )
                )
            }

            card(
                card_header(
                    card_title("Acompanhamento")
                )
                card_content(
                    if off_track {
                        <p class="text-sm text-muted-foreground">
                            "Pedido " (status_label(&order.status).to_lowercase()) "."
                        </p>
                    }
                    order_timeline(progress: progress)
                )
            )

            troca::return_section(order_id: order.id, token: returns_token, info: returns)

            if is_pending {
                <form method="post" action=(href!(cancel_order, Id(order.id))) class="flex justify-end">
                    <input type="hidden" name="t" value=(cancel_token)>
                    <button
                        type="submit"
                        class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))
                        onclick="return confirm('Cancelar este pedido? A reserva dos itens será liberada.')"
                    >
                        "Cancelar pedido"
                    </button>
                </form>
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
                                    table_cell((format_price(item.unit_price_cents.saturating_mul(i64::from(item.quantity)))))
                                )
                            }
                        )
                    )
                )
            )

            order_review_links(order_id: order.id)

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
                        if order.pix_discount_cents > 0 {
                            <div class="flex justify-between">
                                <span class="text-muted-foreground">"Desconto PIX"</span>
                                <span>"-" (format_price(order.pix_discount_cents))</span>
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
      false,
      None,
      Some(&token.to_string()),
      None
    ));
  }

  #[test]
  fn wrong_or_missing_token_denied_for_guest() {
    let token = Uuid::now_v7();
    assert!(!token_or_owner_allows(token, false, None, None, None));
    assert!(!token_or_owner_allows(
      token,
      false,
      None,
      Some(&Uuid::now_v7().to_string()),
      None
    ));
  }

  #[test]
  fn owner_session_skips_token() {
    let token = Uuid::now_v7();
    let owner = Uuid::now_v7();
    assert!(token_or_owner_allows(
      token,
      false,
      Some(owner),
      None,
      Some(owner)
    ));
    assert!(!token_or_owner_allows(
      token,
      false,
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
      order.token_expired,
      order.user_id,
      Some(&access_token.to_string()),
      None
    ));
    assert!(!token_or_owner_allows(
      order.access_token,
      order.token_expired,
      order.user_id,
      Some(&Uuid::new_v4().to_string()),
      None
    ));
  }

  #[test]
  fn expired_token_denied_but_owner_still_allowed() {
    let token = Uuid::now_v7();
    let owner = Uuid::now_v7();
    assert!(!token_or_owner_allows(
      token,
      true,
      None,
      Some(&token.to_string()),
      None
    ));
    assert!(token_or_owner_allows(
      token,
      true,
      Some(owner),
      None,
      Some(owner)
    ));
  }

  async fn insert_guest_order(
    pool: &PgPool,
    expires_sql: &str,
  ) -> (Uuid, Uuid) {
    let order_id = Uuid::now_v7();
    let access_token = Uuid::new_v4();
    sqlx::query(&format!(
      "INSERT INTO orders (id, guest_email, status, total_cents, access_token, access_token_expires_at)
       VALUES ($1, 'guest@example.com', 'pending_payment', 1000, $2, {expires_sql})"
    ))
    .bind(order_id)
    .bind(access_token)
    .execute(pool)
    .await
    .expect("insert order");
    (order_id, access_token)
  }

  #[tokio::test]
  async fn token_expiry_defaults_to_90_days_and_hides_order() {
    let pool = crate::test_support::fresh_pool().await;
    let order_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO orders (id, guest_email, status, total_cents)
       VALUES ($1, 'guest@example.com', 'pending_payment', 1000)",
    )
    .bind(order_id)
    .execute(&pool)
    .await
    .unwrap();
    let days: f64 = sqlx::query_scalar(
      "SELECT EXTRACT(EPOCH FROM access_token_expires_at - now())::float8 / 86400
       FROM orders WHERE id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert!((89.9..=90.1).contains(&days), "{days}");

    let (expired_id, token) =
      insert_guest_order(&pool, "now() - interval '1 minute'").await;
    let order = get_order(&pool, expired_id).await.unwrap().unwrap();
    assert!(order.token_expired);
    assert!(!token_or_owner_allows(
      order.access_token,
      order.token_expired,
      order.user_id,
      Some(&token.to_string()),
      None
    ));
    // Cancelling with the expired link is refused like an unknown order.
    assert_eq!(
      cancel_unpaid_order(&pool, expired_id, Some(&token.to_string()), None)
        .await
        .unwrap(),
      CustomerCancel::NotFound
    );
  }

  #[tokio::test]
  async fn customer_cancels_unpaid_order() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, token) =
      insert_guest_order(&pool, "now() + interval '1 day'").await;
    // Wrong token: not found, order untouched.
    assert_eq!(
      cancel_unpaid_order(
        &pool,
        order_id,
        Some(&Uuid::new_v4().to_string()),
        None
      )
      .await
      .unwrap(),
      CustomerCancel::NotFound
    );
    // A signed-in stranger without the token is refused too.
    assert_eq!(
      cancel_unpaid_order(&pool, order_id, None, Some(Uuid::now_v7()))
        .await
        .unwrap(),
      CustomerCancel::NotFound
    );
    assert_eq!(
      cancel_unpaid_order(&pool, order_id, Some(&token.to_string()), None)
        .await
        .unwrap(),
      CustomerCancel::Cancelled
    );
    let status: String =
      sqlx::query_scalar("SELECT status::text FROM orders WHERE id = $1")
        .bind(order_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(status, "cancelled");
    let audit_actor: Option<String> = sqlx::query_scalar(
      "SELECT metadata->>'action' FROM audit_events WHERE entity_id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audit_actor.as_deref(), Some("cancel"));
    // Second attempt: no longer pending.
    assert_eq!(
      cancel_unpaid_order(&pool, order_id, Some(&token.to_string()), None)
        .await
        .unwrap(),
      CustomerCancel::NotPending
    );
  }

  #[tokio::test]
  async fn paid_order_cannot_be_cancelled_by_customer() {
    let pool = crate::test_support::fresh_pool().await;
    let (order_id, token) =
      insert_guest_order(&pool, "now() + interval '1 day'").await;
    sqlx::query(
      "UPDATE orders SET status = 'paid', paid_at = now() WHERE id = $1",
    )
    .bind(order_id)
    .execute(&pool)
    .await
    .unwrap();
    assert_eq!(
      cancel_unpaid_order(&pool, order_id, Some(&token.to_string()), None)
        .await
        .unwrap(),
      CustomerCancel::NotPending
    );
  }
}
