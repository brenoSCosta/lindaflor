//! `/admin/trocas/{id}`: one return/exchange request, its transitions,
//! notes and audit trail. Rules live in `crate::app::store::returns`.

use serde::Deserialize;
use sqlx::PgPool;
use time::OffsetDateTime;
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
use crate::app::store::queries::{format_datetime_br, format_price};
use crate::app::store::returns::{
  AdminReturnAction, ReturnError, ReturnKind, ReturnStatus, ReturnSummary,
  admin_apply, check_mp_refund, finish_mp_refund, refund_options,
};
use crate::components::badge::badge;
use crate::components::button::{ButtonVariant, button};
use crate::components::card::{card, card_content, card_header, card_title};
use crate::components::checkbox::checkbox;
use crate::components::container::container;
use crate::components::input::input;
use crate::components::label::label;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};
use crate::components::textarea::textarea;
use crate::components::toast::{Toast, set_toast, toast_redirect};

path_param!(pub(crate) id: String, error = not_found);

#[derive(Deserialize)]
pub struct ActionInput {
  action: String,
  #[serde(default)]
  text: String,
  #[serde(default)]
  amount: String,
  restock: Option<String>,
}

/// Parsed form → domain action, or the pt-BR error.
fn parse_action(form: &ActionInput) -> Result<FormAction, &'static str> {
  let restock = form.restock.is_some();
  Ok(FormAction::Local(match form.action.trim() {
    "approve" => AdminReturnAction::Approve {
      instructions: form.text.clone(),
    },
    "reject" => AdminReturnAction::Reject {
      reason: form.text.clone(),
    },
    "receive" => AdminReturnAction::MarkReceived,
    "exchange" => AdminReturnAction::ResolveExchanged,
    "refund_order" => AdminReturnAction::RefundOrder { restock },
    "record_refund" => AdminReturnAction::RecordRefund {
      refund_cents: crate::app::admin::produtos::parse_price_cents(
        &form.amount,
      )
      .map(i64::from)?,
      restock,
    },
    "note" => AdminReturnAction::Note {
      text: form.text.clone(),
    },
    "mp_refund" => return Ok(FormAction::MercadoPagoRefund { restock }),
    _ => return Err("Ação inválida."),
  }))
}

enum FormAction {
  Local(AdminReturnAction),
  MercadoPagoRefund { restock: bool },
}

fn error_message(error: &ReturnError) -> String {
  match error {
    ReturnError::Db(_) => {
      "Não foi possível atualizar a solicitação. Tente novamente.".to_string()
    }
    other => other.to_string(),
  }
}

#[route(POST "/admin/trocas/{id}")]
async fn update_return(
  cx: &Cx,
  Form(form): Form<ActionInput>,
) -> Result<Response> {
  let actor = require_admin(cx).await?;
  let id = path_param::<Id>(cx)?;
  let Ok(return_id) = Uuid::parse_str(id.trim()) else {
    set_toast(cx, Toast::error("Solicitação inválida."));
    return toast_redirect(cx, href!(super::page).resolve(cx));
  };
  let back = href!(page, Id(return_id.to_string())).resolve(cx);
  let fail = |message: &str| {
    set_toast(cx, Toast::error(message));
    toast_redirect(cx, back.clone())
  };
  let pool = app_context::<PgPool>(cx);
  let actor_id = Some(actor.user.id);
  let actor_name = Some(actor.user.name.as_str());

  let action = match parse_action(&form) {
    Ok(action) => action,
    Err(message) => return fail(message),
  };
  match action {
    FormAction::Local(action) => {
      match admin_apply(pool, return_id, action, actor_id, actor_name).await {
        Ok(_) => {
          set_toast(cx, Toast::success("Solicitação atualizada."));
          toast_redirect(cx, back)
        }
        Err(error) => {
          if let ReturnError::Db(db) = &error {
            tracing::error!(%return_id, error = %db, "return update failed");
          }
          fail(&error_message(&error))
        }
      }
    }
    FormAction::MercadoPagoRefund { restock } => {
      let order_id = match check_mp_refund(pool, return_id).await {
        Ok(order_id) => order_id,
        Err(error) => return fail(&error_message(&error)),
      };
      // Same PSP path as "Estornar no Mercado Pago" on the order page:
      // full refund at MP, then `Refund` applied locally with its audit.
      let provider = crate::payments::PaymentProvider::from_cx(cx);
      let payment_actor = crate::payments::service::Actor {
        id: actor_id,
        name: actor.user.name.as_str(),
      };
      if let Err(message) = crate::payments::service::run_admin_action(
        pool,
        &provider,
        order_id,
        crate::payments::service::AdminAction::Refund,
        payment_actor,
      )
      .await
      {
        return fail(&message);
      }
      match finish_mp_refund(pool, return_id, restock, actor_id, actor_name)
        .await
      {
        Ok(()) => {
          set_toast(
            cx,
            Toast::success(
              "Estornado no Mercado Pago e solicitação concluída.",
            ),
          );
          toast_redirect(cx, back)
        }
        Err(error) => {
          tracing::error!(%return_id, %error, "mp refund done but return not resolved");
          fail(&format!(
            "Estorno feito no Mercado Pago, mas a solicitação não foi concluída: {}",
            error_message(&error)
          ))
        }
      }
    }
  }
}

#[derive(sqlx::FromRow)]
struct DetailRow {
  id: Uuid,
  order_id: Uuid,
  kind: String,
  status: String,
  resolution: Option<String>,
  created_at: OffsetDateTime,
  refund_cents: Option<i64>,
  instructions: Option<String>,
  rejection_reason: Option<String>,
  reason: String,
  desired_size: Option<String>,
  email: Option<String>,
  order_status: String,
  order_total_cents: i64,
}

#[derive(sqlx::FromRow)]
struct ItemRow {
  product_name: String,
  variant_label: String,
  quantity: i32,
  ordered: i32,
  unit_price_cents: i64,
}

#[derive(sqlx::FromRow)]
struct AuditRow {
  occurred_at: OffsetDateTime,
  actor_name: Option<String>,
  metadata: serde_json::Value,
}

fn action_label(action: &str) -> &'static str {
  match action {
    "create" => "Solicitação aberta",
    "approve" => "Aprovada",
    "reject" => "Recusada",
    "receive" => "Peça recebida",
    "resolve_exchanged" => "Troca concluída",
    "refund_order" => "Pedido estornado",
    "record_refund" => "Reembolso registrado",
    "mp_refund" => "Estornado no Mercado Pago",
    "note" => "Observação",
    _ => "Evento",
  }
}

fn audit_detail(metadata: &serde_json::Value) -> String {
  let text = |key: &str| metadata.get(key).and_then(|v| v.as_str());
  let mut parts = Vec::new();
  if let Some(note) = text("note").or_else(|| text("reason")) {
    parts.push(note.to_string());
  }
  if let Some(cents) = metadata.get("refund_cents").and_then(|v| v.as_i64()) {
    parts.push(format_price(cents));
  }
  if metadata.get("restock").and_then(|v| v.as_bool()) == Some(true) {
    parts.push("devolvido ao estoque".to_string());
  }
  parts.join(" · ")
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let id = path_param::<Id>(cx)?;
  let return_id = Uuid::parse_str(id.trim()).map_err(|_| not_found())?;

  let row = sqlx::query_as::<_, DetailRow>(
    "SELECT r.id, r.order_id, r.kind, r.status, r.resolution, r.created_at,
            r.refund_cents, r.instructions, r.rejection_reason, r.reason,
            r.desired_size,
            COALESCE(NULLIF(btrim(o.guest_email), ''), u.email) AS email,
            o.status::text AS order_status, o.total_cents AS order_total_cents
     FROM return_requests r
     JOIN orders o ON o.id = r.order_id
     LEFT JOIN users u ON u.id = o.user_id
     WHERE r.id = $1",
  )
  .bind(return_id)
  .fetch_optional(pool)
  .await?
  .ok_or_not_found()?;
  let items = sqlx::query_as::<_, ItemRow>(
    "SELECT oi.product_name, oi.variant_label, ri.quantity,
            oi.quantity AS ordered, oi.unit_price_cents
     FROM return_request_items ri
     JOIN order_items oi ON oi.id = ri.order_item_id
     WHERE ri.return_request_id = $1
     ORDER BY oi.created_at, oi.id",
  )
  .bind(return_id)
  .fetch_all(pool)
  .await?;
  let audit = sqlx::query_as::<_, AuditRow>(
    "SELECT occurred_at, actor_name, metadata FROM audit_events
     WHERE aggregate_type = 'return' AND aggregate_id = $1
     ORDER BY occurred_at, id",
  )
  .bind(return_id.to_string())
  .fetch_all(pool)
  .await?;

  let summary = ReturnSummary {
    id: row.id,
    order_id: row.order_id,
    kind: row.kind.clone(),
    status: row.status.clone(),
    resolution: row.resolution.clone(),
    created_at: row.created_at,
    refund_cents: row.refund_cents,
    instructions: row.instructions.clone(),
    rejection_reason: row.rejection_reason.clone(),
  };
  let kind = ReturnKind::parse(&row.kind);
  let status = ReturnStatus::parse(&row.status);
  let requested = status == Some(ReturnStatus::Requested);
  let approved = status == Some(ReturnStatus::Approved);
  let received = status == Some(ReturnStatus::Received);
  let open = status.is_some_and(ReturnStatus::is_open);
  let can_exchange = received && kind.is_some_and(ReturnKind::can_exchange);
  let can_refund = received && kind.is_some_and(ReturnKind::can_refund);
  let refund = if can_refund {
    Some(refund_options(pool, return_id).await?)
  } else {
    None
  };
  let full_refund = refund.filter(|options| options.full_order);
  let partial_refund =
    refund.filter(|options| !options.full_order || options.order_refunded);
  let action = href!(update_return, Id(return_id.to_string())).resolve(cx);
  let order_href = href!(
    crate::app::admin::pedidos::id::page,
    crate::app::admin::pedidos::id::Id(row.order_id.to_string())
  );
  let order_code = crate::mail::templates::order_code(row.order_id);
  let default_instructions = "Poste a peça nos Correios em até 7 dias usando o código de postagem: \nEnvie na embalagem original, com etiquetas, e inclua o número do pedido.";

  Ok(view! {
      container(
          <div class="flex flex-wrap items-center gap-3">
              <h1 class="text-2xl font-semibold tracking-tight">(summary.kind_label())</h1>
              badge(variant: super::status_badge(&row.status), (summary.status_label()))
          </div>
          <p class="text-sm text-muted-foreground">
              "Pedido "
              <a href=(order_href) class="font-mono text-primary hover:underline">"#" (order_code)</a>
              " · " (row.order_status.clone()) " · " (format_price(row.order_total_cents))
              " · aberta em " (format_datetime_br(row.created_at))
          </p>

          card(
              card_header(card_title("Solicitação"))
              card_content(
                  <div class="flex flex-col gap-2 text-sm">
                      <p>"Cliente: " (row.email.clone().unwrap_or_default())</p>
                      if let Some(size) = row.desired_size.clone() {
                          <p>"Tamanho desejado: " <strong>(size)</strong></p>
                      }
                      <p class="whitespace-pre-line">"Motivo: " (row.reason.clone())</p>
                      if let Some(text) = row.instructions.clone() {
                          <p class="whitespace-pre-line text-muted-foreground">"Instruções enviadas: " (text)</p>
                      }
                      if let Some(text) = row.rejection_reason.clone() {
                          <p class="text-muted-foreground">"Motivo da recusa: " (text)</p>
                      }
                      if let Some(cents) = row.refund_cents {
                          <p>"Reembolso: " (format_price(cents))</p>
                      }
                  </div>
                  table(
                      table_header(
                          table_row(
                              table_head("Produto")
                              table_head("Variação")
                              table_head("Qtd. (pedido)")
                              table_head("Valor")
                          )
                      )
                      table_body(
                          for item in items {
                              table_row(
                                  table_cell((item.product_name))
                                  table_cell((item.variant_label))
                                  table_cell((item.quantity) " (" (item.ordered) ")")
                                  table_cell((format_price(item.unit_price_cents.saturating_mul(i64::from(item.quantity)))))
                              )
                          }
                      )
                  )
              )
          )

          if open {
              card(
                  card_header(card_title("Ações"))
                  card_content(
                      <div class="flex flex-col gap-4">
                          if requested {
                              <form method="post" action=(action.clone()) class="flex flex-col gap-2">
                                  <input type="hidden" name="action" value="approve">
                                  label(attrs: attributes! { for="approve-text" }, "Instruções de envio para o cliente (logística reversa / código de postagem)")
                                  textarea(attrs: attributes! { id="approve-text" name="text" required="" maxlength="2000" }, (default_instructions))
                                  <div>
                                      button(variant: ButtonVariant::Primary, attrs: attributes! { type="submit" }, "Aprovar e enviar instruções")
                                  </div>
                              </form>
                          }
                          if approved {
                              <form method="post" action=(action.clone())>
                                  <input type="hidden" name="action" value="receive">
                                  button(variant: ButtonVariant::Primary, attrs: attributes! { type="submit" }, "Marcar peça como recebida")
                              </form>
                          }
                          if can_exchange {
                              <form method="post" action=(action.clone())>
                                  <input type="hidden" name="action" value="exchange">
                                  button(variant: ButtonVariant::Primary, attrs: attributes! { type="submit" }, "Concluir troca (nova peça enviada)")
                              </form>
                          }
                          if let Some(options) = full_refund {
                              <form method="post" action=(action.clone()) class="flex flex-wrap items-center gap-2">
                                  if options.mercado_pago {
                                      <input type="hidden" name="action" value="mp_refund">
                                  } else {
                                      <input type="hidden" name="action" value="refund_order">
                                  }
                                  checkbox(attrs: attributes! { id="restock-full" name="restock" value="1" checked=(true) })
                                  label(attrs: attributes! { for="restock-full" }, "Devolver ao estoque")
                                  button(
                                      variant: ButtonVariant::Destructive,
                                      attrs: attributes! { type="submit" },
                                      if options.mercado_pago { "Estornar no Mercado Pago" } else { "Estornar pedido (PIX devolvido)" }
                                  )
                              </form>
                              <p class="text-xs text-muted-foreground">
                                  "A solicitação cobre o pedido inteiro: o estorno é do pedido todo (" (format_price(row.order_total_cents)) ")."
                              </p>
                          }
                          if let Some(options) = partial_refund {
                              <form method="post" action=(action.clone()) class="flex flex-col gap-2">
                                  <input type="hidden" name="action" value="record_refund">
                                  <p class="text-xs text-muted-foreground">
                                      if options.order_refunded {
                                          "O pedido já foi estornado; registre o valor para concluir a solicitação."
                                      } else if options.mercado_pago {
                                          "Devolução parcial: faça o reembolso parcial no painel do Mercado Pago e registre o valor aqui."
                                      } else {
                                          "Devolução parcial: faça o PIX ao cliente e registre o valor aqui. O status do pedido não muda."
                                      }
                                  </p>
                                  <div class="flex flex-wrap items-end gap-2">
                                      <div class="flex flex-col gap-1.5">
                                          label(attrs: attributes! { for="amount" }, "Valor reembolsado (máx. " (format_price(options.max_manual_cents)) ")")
                                          input(attrs: attributes! { id="amount" name="amount" required="" inputmode="decimal" placeholder="49,90" })
                                      </div>
                                      if !options.order_refunded {
                                          <div class="flex items-center gap-2">
                                              checkbox(attrs: attributes! { id="restock-partial" name="restock" value="1" checked=(true) })
                                              label(attrs: attributes! { for="restock-partial" }, "Devolver peças ao estoque")
                                          </div>
                                      }
                                      button(variant: ButtonVariant::Destructive, attrs: attributes! { type="submit" }, "Registrar reembolso")
                                  </div>
                              </form>
                          }
                          <form method="post" action=(action.clone()) class="flex flex-col gap-2">
                              <input type="hidden" name="action" value="reject">
                              label(attrs: attributes! { for="reject-text" }, "Recusar — motivo enviado ao cliente")
                              input(attrs: attributes! { id="reject-text" name="text" required="" maxlength="2000" })
                              <div>
                                  button(variant: ButtonVariant::Outline, attrs: attributes! { type="submit" }, "Recusar solicitação")
                              </div>
                          </form>
                      </div>
                  )
              )
          }

          card(
              card_header(card_title("Histórico"))
              card_content(
                  <ul class="flex flex-col gap-2 text-sm">
                      for event in audit {
                          let action_name = event.metadata.get("action").and_then(|v| v.as_str()).unwrap_or("").to_string();
                          let detail = audit_detail(&event.metadata);
                          <li>
                              <span class="text-muted-foreground">(format_datetime_br(event.occurred_at)) " · "</span>
                              <span class="font-medium">(action_label(&action_name))</span>
                              " — " (event.actor_name.clone().unwrap_or_else(|| "sistema".to_string()))
                              if !detail.is_empty() {
                                  <span class="block whitespace-pre-line text-muted-foreground">(detail)</span>
                              }
                          </li>
                      }
                  </ul>
                  <form method="post" action=(action.clone()) class="mt-4 flex flex-col gap-2">
                      <input type="hidden" name="action" value="note">
                      label(attrs: attributes! { for="note-text" }, "Adicionar observação interna")
                      textarea(attrs: attributes! { id="note-text" name="text" required="" maxlength="2000" })
                      <div>
                          button(variant: ButtonVariant::Outline, attrs: attributes! { type="submit" }, "Salvar observação")
                      </div>
                  </form>
              )
          )
      )
  })
}
