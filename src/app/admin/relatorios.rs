//! `/admin/relatorios`: sales report for a range of Brasília days, plus
//! the orders CSV export. Definitions live in `crate::app::store::reports`.

use http::{
  HeaderValue,
  header::{CONTENT_DISPOSITION, CONTENT_TYPE},
};
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    href, page, query_params,
    response::{IntoResponse, Response},
    route,
  },
  view::{View, attributes, view},
};

use crate::app::auth_helpers::require_admin;
use crate::app::store::queries::format_price;
use crate::app::store::reports::{
  CSV_MAX_ORDERS, DateRange, MAX_RANGE_DAYS, TopBy, coupon_usage, csv_orders,
  format_date_br, format_date_iso, provider_label, provider_split, render_csv,
  revenue_by_day, summary, top_sellers,
};
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::card::{card, card_content, card_header, card_title};
use crate::components::container::container;
use crate::components::input::input;
use crate::components::label::label;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};
use crate::components::toast::{Toast, set_toast, toast_redirect};

#[query_params(error = bad_request)]
struct RangeQuery {
  de: Option<String>,
  ate: Option<String>,
}

const TOP_LIMIT: i64 = 10;

fn range_from(cx: &Cx) -> Result<DateRange> {
  let query = query_params::<RangeQuery>(cx)?;
  Ok(DateRange::parse(query.de.as_deref(), query.ate.as_deref()))
}

/// Width (0–100) of a day's bar relative to the best day.
fn bar_percent(value: i64, max: i64) -> i64 {
  if max <= 0 {
    0
  } else {
    (value.saturating_mul(100) / max).clamp(0, 100)
  }
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let range = range_from(cx)?;

  let totals = summary(pool, range).await?;
  let days = revenue_by_day(pool, range).await?;
  let top_variants =
    top_sellers(pool, range, TopBy::VariantUnits, TOP_LIMIT).await?;
  let top_products =
    top_sellers(pool, range, TopBy::ProductRevenue, TOP_LIMIT).await?;
  let coupons = coupon_usage(pool, range).await?;
  let providers = provider_split(pool, range).await?;

  let from_iso = format_date_iso(range.from);
  let to_iso = format_date_iso(range.to);
  let period = format!(
    "{} a {} ({} dias, horário de Brasília)",
    format_date_br(range.from),
    format_date_br(range.to),
    range.days()
  );
  let csv_href = href!(export_orders_csv)
    .query([("de", from_iso.as_str()), ("ate", to_iso.as_str())])
    .resolve(cx);
  let best_day = days.iter().map(|d| d.revenue_cents).max().unwrap_or(0);
  let coupon_orders: i64 = coupons.iter().map(|c| c.orders).sum();
  let coupon_discount: i64 = coupons.iter().map(|c| c.discount_cents).sum();
  let no_coupons = coupons.is_empty();
  let no_providers = providers.is_empty();
  let no_variants = top_variants.is_empty();
  let no_products = top_products.is_empty();
  let stats = [
    ("Receita", format_price(totals.revenue_cents)),
    ("Pedidos pagos", totals.orders.to_string()),
    ("Ticket médio", format_price(totals.aov_cents())),
    ("Receita líquida", format_price(totals.net_revenue_cents())),
  ];
  let secondary = [
    (
      "Reembolsos parciais (trocas)",
      format_price(totals.partial_refund_cents),
    ),
    ("Frete cobrado", format_price(totals.shipping_cents)),
    ("Desconto PIX", format_price(totals.pix_discount_cents)),
    ("Desconto de cupons", format_price(totals.discount_cents)),
    (
      "Abandonados / expirados",
      totals.abandoned_orders.to_string(),
    ),
    ("Aguardando pagamento", totals.pending_orders.to_string()),
    ("Estornados", totals.refunded_orders.to_string()),
  ];

  Ok(view! {
      container(
          <h1 class="text-2xl font-semibold tracking-tight">"Relatórios"</h1>
          <p class="text-muted-foreground">(period)</p>
          <form method="get" action=(href!(page)) class="flex flex-wrap items-end gap-3">
              <div class="flex flex-col gap-1.5">
                  label(attrs: attributes! { for="de" }, "De")
                  input(attrs: attributes! { id="de" name="de" type="date" value=(from_iso.clone()) })
              </div>
              <div class="flex flex-col gap-1.5">
                  label(attrs: attributes! { for="ate" }, "Até")
                  input(attrs: attributes! { id="ate" name="ate" type="date" value=(to_iso.clone()) })
              </div>
              <button type="submit" class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))>"Aplicar"</button>
              <a href=(csv_href) class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))>
                  "Exportar pedidos (CSV)"
              </a>
          </form>
          <p class="text-xs text-muted-foreground">
              "Receita: pedidos pagos, em separação, enviados ou entregues, pela data do pagamento; estornados e cancelados ficam de fora. Intervalo máximo de " (MAX_RANGE_DAYS) " dias."
          </p>

          <div class="grid gap-4 @sm/page:grid-cols-2 @lg/page:grid-cols-4">
              for (title, value) in stats {
                  card(
                      card_content(
                          <p class="text-sm text-muted-foreground">(title)</p>
                          <p class="mt-2 text-2xl font-semibold">(value)</p>
                      )
                  )
              }
          </div>
          card(
              card_content(
                  <dl class="grid gap-3 text-sm @sm/page:grid-cols-2 @lg/page:grid-cols-4">
                      for (title, value) in secondary {
                          <div>
                              <dt class="text-muted-foreground">(title)</dt>
                              <dd class="font-medium">(value)</dd>
                          </div>
                      }
                  </dl>
              )
          )

          card(
              card_header(card_title("Receita por dia"))
              card_content(
                  table(
                      table_header(
                          table_row(
                              table_head("Dia")
                              table_head("Pedidos")
                              table_head("Receita")
                              table_head("")
                          )
                      )
                      table_body(
                          for day in days {
                              let width = format!("width: {}%", bar_percent(day.revenue_cents, best_day));
                              table_row(
                                  table_cell((format_date_br(day.day)))
                                  table_cell((day.orders))
                                  table_cell((format_price(day.revenue_cents)))
                                  table_cell(
                                      <div class="h-2 w-40 rounded-full bg-muted" aria-hidden="true">
                                          <div class="h-2 rounded-full bg-primary" style=(width)></div>
                                      </div>
                                  )
                              )
                          }
                      )
                  )
              )
          )

          <div class="grid gap-4 @lg/page:grid-cols-2">
              card(
                  card_header(card_title("Variações mais vendidas (unidades)"))
                  card_content(
                      if no_variants {
                          <p class="text-sm text-muted-foreground">"Sem vendas no período."</p>
                      } else {
                          table(
                              table_header(
                                  table_row(
                                      table_head("Produto")
                                      table_head("Unid.")
                                      table_head("Receita")
                                  )
                              )
                              table_body(
                                  for row in top_variants {
                                      table_row(
                                          table_cell((row.name) " (" (row.detail) ")")
                                          table_cell((row.units))
                                          table_cell((format_price(row.revenue_cents)))
                                      )
                                  }
                              )
                          )
                      }
                  )
              )
              card(
                  card_header(card_title("Produtos com maior receita"))
                  card_content(
                      if no_products {
                          <p class="text-sm text-muted-foreground">"Sem vendas no período."</p>
                      } else {
                          table(
                              table_header(
                                  table_row(
                                      table_head("Produto")
                                      table_head("Unid.")
                                      table_head("Receita")
                                  )
                              )
                              table_body(
                                  for row in top_products {
                                      table_row(
                                          table_cell((row.name))
                                          table_cell((row.units))
                                          table_cell((format_price(row.revenue_cents)))
                                      )
                                  }
                              )
                          )
                      }
                      <p class="mt-2 text-xs text-muted-foreground">"Receita dos itens antes de descontos do pedido."</p>
                  )
              )
              card(
                  card_header(card_title("Cupons"))
                  card_content(
                      if no_coupons {
                          <p class="text-sm text-muted-foreground">"Nenhum cupom usado no período."</p>
                      } else {
                          <p class="text-sm text-muted-foreground">
                              (coupon_orders) " pedidos com cupom · " (format_price(coupon_discount)) " em descontos"
                          </p>
                          table(
                              table_header(
                                  table_row(
                                      table_head("Cupom")
                                      table_head("Pedidos")
                                      table_head("Desconto")
                                      table_head("Receita")
                                  )
                              )
                              table_body(
                                  for row in coupons {
                                      table_row(
                                          table_cell(<span class="font-mono">(row.code)</span>)
                                          table_cell((row.orders))
                                          table_cell((format_price(row.discount_cents)))
                                          table_cell((format_price(row.revenue_cents)))
                                      )
                                  }
                              )
                          )
                      }
                  )
              )
              card(
                  card_header(card_title("Forma de pagamento"))
                  card_content(
                      if no_providers {
                          <p class="text-sm text-muted-foreground">"Sem vendas no período."</p>
                      } else {
                          table(
                              table_header(
                                  table_row(
                                      table_head("Provedor")
                                      table_head("Pedidos")
                                      table_head("Receita")
                                  )
                              )
                              table_body(
                                  for row in providers {
                                      table_row(
                                          table_cell((provider_label(&row.provider)))
                                          table_cell((row.orders))
                                          table_cell((format_price(row.revenue_cents)))
                                      )
                                  }
                              )
                          )
                      }
                  )
              )
          </div>
      )
  })
}

/// Orders created in the range as CSV (admin only). Bounded: at most
/// [`MAX_RANGE_DAYS`] days and [`CSV_MAX_ORDERS`] orders; a bigger export
/// is refused with a toast instead of silently truncated.
#[route(GET "/admin/relatorios/pedidos.csv")]
pub async fn export_orders_csv(cx: &Cx) -> Result<Response> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let range = range_from(cx)?;
  let orders = csv_orders(pool, range, CSV_MAX_ORDERS + 1).await?;
  if orders.len() as i64 > CSV_MAX_ORDERS {
    set_toast(
      cx,
      Toast::error(format!(
        "Mais de {CSV_MAX_ORDERS} pedidos no período. Reduza o intervalo."
      )),
    );
    let back = href!(page)
      .query([
        ("de", format_date_iso(range.from).as_str()),
        ("ate", format_date_iso(range.to).as_str()),
      ])
      .resolve(cx);
    return toast_redirect(cx, back);
  }
  let filename = format!(
    "attachment; filename=\"pedidos-{}_{}.csv\"",
    format_date_iso(range.from),
    format_date_iso(range.to)
  );
  (
    [
      (
        CONTENT_TYPE,
        HeaderValue::from_static("text/csv; charset=utf-8"),
      ),
      (
        CONTENT_DISPOSITION,
        HeaderValue::from_str(&filename)
          .unwrap_or_else(|_| HeaderValue::from_static("attachment")),
      ),
    ],
    render_csv(&orders),
  )
    .into_response(cx)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn bars_scale_to_best_day() {
    assert_eq!(bar_percent(50, 200), 25);
    assert_eq!(bar_percent(200, 200), 100);
    assert_eq!(bar_percent(10, 0), 0);
  }
}
