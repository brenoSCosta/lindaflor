use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{page, path_param},
  view::{View, ViewExt, attributes, view},
};
use uuid::Uuid;

use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::card::{
  card, card_content, card_footer, card_header, card_title,
};
use crate::components::separator::separator;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};

use crate::app::store::queries::format_price;

path_param!(id: Uuid, error = not_found);

struct OrderItem {
  #[allow(dead_code)]
  id: Uuid,
  product_name: String,
  variant_label: String,
  quantity: i32,
  unit_price_cents: i32,
}

struct OrderDetail {
  id: Uuid,
  status: String,
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

async fn get_order(
  pool: &PgPool,
  id: Uuid,
) -> Result<Option<OrderDetail>, sqlx::Error> {
  let row = sqlx::query!(
        "SELECT id, status::text AS \"status!\", guest_email, subtotal_cents, shipping_cents,
            discount_cents, total_cents, shipping_address, payment_meta, created_at
         FROM orders WHERE id = $1",
        id
    )
    .fetch_optional(pool)
    .await?;

  let order = match row {
    Some(o) => o,
    None => return Ok(None),
  };

  let items = sqlx::query!(
    "SELECT id, product_name, variant_label, quantity, unit_price_cents
         FROM order_items WHERE order_id = $1",
    id
  )
  .fetch_all(pool)
  .await?;

  Ok(Some(OrderDetail {
    id: order.id,
    status: order.status,
    guest_email: order.guest_email,
    subtotal_cents: order.subtotal_cents,
    shipping_cents: order.shipping_cents,
    discount_cents: order.discount_cents,
    total_cents: order.total_cents,
    shipping_address: order.shipping_address,
    payment_meta: order.payment_meta,
    created_at: order.created_at.to_string(),
    items: items
      .into_iter()
      .map(|i| OrderItem {
        id: i.id,
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

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let id = path_param::<Id>(cx)?;

  let order = match get_order(pool, *id).await? {
    Some(o) => o,
    None => {
      return Ok(view! {
                <div class="mx-auto max-w-3xl px-4 py-24 text-center md:px-8">
                    <h1 class="text-4xl font-bold tracking-tight">"Pedido não encontrado"</h1>
                    <a
                        href="/produtos"
                        class=(button_variants(ButtonVariant::Primary, ButtonSize::Lg))
                    >
                        "Voltar ao catálogo"
                    </a>
                </div>
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

  Ok(view! {
        <main class="mx-auto max-w-3xl px-4 py-12 md:px-8">
            <p class="text-xs uppercase tracking-widest text-muted-foreground">"Pedido #" (order_id_short)</p>
            <div class="mt-3 flex items-center gap-3">
                <h1 class="text-5xl font-bold tracking-tight">(heading)</h1>
                badge(variant: status_badge_variant(&order.status), (status_label(&order.status)))
            </div>
            <p class="mt-2 text-muted-foreground">
                "Enviamos as instruções para " (guest_email)
            </p>

            if is_pending {
                card(
                    attrs: attributes! { class="mt-10" },
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
                            <div class="space-y-2">
                                <p class="text-sm text-muted-foreground">"PIX copia e cola:"</p>
                                <code class="block overflow-x-auto rounded-lg border border-border bg-background p-3 text-xs">
                                    (code)
                                </code>
                            </div>
                        } else {
                            <p class="text-sm text-destructive">
                                "Instruções de pagamento indisponíveis. Fale conosco no WhatsApp."
                            </p>
                        }
                    )
                    card_footer(
                        <a
                            href="https://wa.me/5579998165115"
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
                attrs: attributes! { class="mt-10" },
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
                attrs: attributes! { class="mt-8" },
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
                                <span class="text-muted-foreground">"Desconto"</span>
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
                    attrs: attributes! { class="mt-8" },
                    card_header(
                        card_title("Endereço de entrega")
                    )
                    card_content(
                        <p class="text-sm text-muted-foreground">(address_display)</p>
                        <p class="text-sm text-muted-foreground">"Pedido criado em " (order.created_at)</p>
                    )
                )
            }
        </main>
    }.boxed())
}
