//! `/admin/trocas`: return/exchange requests (see `store::returns`).

pub mod id;

use sqlx::PgPool;
use time::OffsetDateTime;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{href, page, query_params},
  view::{View, component, view},
};
use uuid::Uuid;

use crate::app::auth_helpers::require_admin;
use crate::app::store::queries::format_datetime_br;
use crate::app::store::returns::{ReturnStatus, ReturnSummary};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::card::{card, card_content, card_header, card_title};
use crate::components::container::container;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};

#[query_params(error = bad_request)]
struct TrocasQuery {
  status: Option<String>,
}

/// List filters: `abertas` (default) or one status, or `todas`.
const FILTERS: [(&str, &str); 7] = [
  ("abertas", "Em aberto"),
  ("requested", "Em análise"),
  ("approved", "Aprovadas"),
  ("received", "Recebidas"),
  ("resolved", "Concluídas"),
  ("rejected", "Recusadas"),
  ("todas", "Todas"),
];

const LIST_LIMIT: i64 = 200;

fn clamp_filter(raw: Option<&str>) -> &'static str {
  let raw = raw.unwrap_or("").trim();
  FILTERS
    .iter()
    .find(|(key, _)| *key == raw)
    .map_or("abertas", |(key, _)| key)
}

pub(crate) fn status_badge(status: &str) -> BadgeVariant {
  match ReturnStatus::parse(status) {
    Some(ReturnStatus::Requested) => BadgeVariant::Secondary,
    Some(ReturnStatus::Rejected) => BadgeVariant::Destructive,
    _ => BadgeVariant::Primary,
  }
}

#[derive(sqlx::FromRow)]
struct ListRow {
  id: Uuid,
  order_id: Uuid,
  kind: String,
  status: String,
  resolution: Option<String>,
  created_at: OffsetDateTime,
  refund_cents: Option<i64>,
  instructions: Option<String>,
  rejection_reason: Option<String>,
  email: Option<String>,
  units: i64,
}

impl ListRow {
  fn summary(&self) -> ReturnSummary {
    ReturnSummary {
      id: self.id,
      order_id: self.order_id,
      kind: self.kind.clone(),
      status: self.status.clone(),
      resolution: self.resolution.clone(),
      created_at: self.created_at,
      refund_cents: self.refund_cents,
      instructions: self.instructions.clone(),
      rejection_reason: self.rejection_reason.clone(),
    }
  }
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let query = query_params::<TrocasQuery>(cx)?;
  let filter = clamp_filter(query.status.as_deref());

  let rows = sqlx::query_as::<_, ListRow>(
    "SELECT r.id, r.order_id, r.kind, r.status, r.resolution, r.created_at,
            r.refund_cents, r.instructions, r.rejection_reason,
            COALESCE(NULLIF(btrim(o.guest_email), ''), u.email) AS email,
            COALESCE((SELECT SUM(ri.quantity) FROM return_request_items ri
                      WHERE ri.return_request_id = r.id), 0)::bigint AS units
     FROM return_requests r
     JOIN orders o ON o.id = r.order_id
     LEFT JOIN users u ON u.id = o.user_id
     WHERE CASE $1
             WHEN 'todas' THEN true
             WHEN 'abertas' THEN r.status IN ('requested', 'approved', 'received')
             ELSE r.status = $1
           END
     ORDER BY r.created_at DESC
     LIMIT $2",
  )
  .bind(filter)
  .bind(LIST_LIMIT)
  .fetch_all(pool)
  .await?;
  let empty = rows.is_empty();

  Ok(view! {
      container(
          <h1 class="text-2xl font-semibold tracking-tight">"Trocas e devoluções"</h1>
          <p class="text-muted-foreground">"Solicitações abertas pelos clientes na página do pedido."</p>
          <div class="flex flex-wrap items-center gap-2">
              for (key, label) in FILTERS {
                  <a
                      href=(href!(page).query([("status", key)]))
                      class=(button_variants(
                          if key == filter { ButtonVariant::Primary } else { ButtonVariant::Outline },
                          ButtonSize::Sm,
                      ))
                  >
                      (label)
                  </a>
              }
          </div>
          if empty {
              <p class="text-sm text-muted-foreground">"Nenhuma solicitação encontrada."</p>
          } else {
              table(
                  table_header(
                      table_row(
                          table_head("Data")
                          table_head("Pedido")
                          table_head("Cliente")
                          table_head("Tipo")
                          table_head("Peças")
                          table_head("Status")
                      )
                  )
                  table_body(
                      #[key(row.id.to_string())]
                      for row in rows {
                          let summary = row.summary();
                          table_row(
                              table_cell(
                                  <a
                                      href=(href!(id::page, id::Id(row.id.to_string())))
                                      class="text-primary hover:underline"
                                  >
                                      (format_datetime_br(row.created_at))
                                  </a>
                              )
                              table_cell(
                                  <span class="font-mono text-sm">"#" (crate::mail::templates::order_code(row.order_id))</span>
                              )
                              table_cell((row.email.clone().unwrap_or_default()))
                              table_cell((summary.kind_label()))
                              table_cell((row.units))
                              table_cell(badge(variant: status_badge(&row.status), (summary.status_label())))
                          )
                      }
                  )
              )
          }
      )
  })
}

/// Admin order page card: this order's return requests, linking to
/// `/admin/trocas/{id}`. Renders nothing without requests.
#[component]
pub async fn order_returns_card(
  requests: Vec<ReturnSummary>,
) -> Result<impl View> {
  let empty = requests.is_empty();
  Ok(view! {
      if !empty {
          card(
              card_header(card_title("Trocas e devoluções"))
              card_content(
                  <ul class="flex flex-col gap-2 text-sm">
                      for request in requests {
                          <li class="flex flex-wrap items-center gap-2">
                              <a
                                  href=(href!(id::page, id::Id(request.id.to_string())))
                                  class="text-primary hover:underline"
                              >
                                  (request.kind_label()) " · " (format_datetime_br(request.created_at))
                              </a>
                              badge(variant: status_badge(&request.status), (request.status_label()))
                          </li>
                      }
                  </ul>
              )
          )
      }
  })
}
