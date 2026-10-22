//! `/pedido/{id}/troca`: the customer's return/exchange request form, plus
//! the "Trocas e devoluções" section embedded in the order page. Rules
//! live in `crate::app::store::returns`.

use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form, href, page, path_param, query_params, response::Response,
    route,
  },
  view::{View, ViewExt, attributes, component, view},
};
use uuid::Uuid;

use super::{Id, PedidoQuery, token_or_owner_allows};
use crate::app::store::queries::{format_datetime_br, format_price};
use crate::app::store::returns::{
  NewReturn, OrderReturns, REASON_MAX_CHARS, ReturnError, ReturnKind,
  ReturnSummary, SIZE_MAX_CHARS, create_request, order_returns,
  returnable_items,
};
use crate::auth::user::current_user_owned;
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::card::{card, card_content, card_header, card_title};
use crate::components::container::{ContainerVariant, container};
use crate::components::input::input;
use crate::components::label::label;
use crate::components::select::select;
use crate::components::textarea::textarea;
use crate::components::toast::{Toast, set_toast, toast_redirect};

/// `/pedido/{id}` (or `/pedido/{id}/troca`) keeping the guest token.
fn order_link(cx: &Cx, order_id: Uuid, token: Option<&str>) -> String {
  match token {
    Some(token) => href!(super::page, Id(order_id))
      .query([("t", token)])
      .resolve(cx),
    None => href!(super::page, Id(order_id)).resolve(cx),
  }
}

fn form_link(cx: &Cx, order_id: Uuid, token: Option<&str>) -> String {
  match token {
    Some(token) => href!(page, Id(order_id)).query([("t", token)]).resolve(cx),
    None => href!(page, Id(order_id)).resolve(cx),
  }
}

/// Order page section: existing requests and the link to open one.
#[component]
pub async fn return_section(
  cx: &Cx,
  order_id: Uuid,
  token: Option<String>,
  info: OrderReturns,
) -> Result<impl View> {
  if !info.visible {
    return Ok(view! {}.boxed());
  }
  let link = form_link(cx, order_id, token.as_deref());
  let can_open = info.can_open();
  let deadline = info
    .open_kinds
    .iter()
    .find(|(kind, _)| *kind == ReturnKind::Return)
    .map(|(_, at)| format_datetime_br(*at));
  Ok(view! {
      card(
          card_header(card_title("Trocas e devoluções"))
          card_content(
              <div class="flex flex-col gap-3 text-sm">
                  for request in info.requests {
                      request_line(request: request)
                  }
                  if can_open {
                      if let Some(until) = deadline {
                          <p class="text-muted-foreground">
                              "Troca fácil: você pode pedir troca ou devolução até " (until) "."
                          </p>
                      }
                      <a href=(link) class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))>
                          "Solicitar troca ou devolução"
                      </a>
                  } else if let Some(message) = info.blocked {
                      <p class="text-muted-foreground">(message)</p>
                  }
              </div>
          )
      )
  }
  .boxed())
}

#[component]
async fn request_line(request: ReturnSummary) -> Result<impl View> {
  let opened = format_datetime_br(request.created_at);
  Ok(view! {
      <div class="rounded-lg border border-border p-3">
          <p class="font-medium">
              (request.kind_label()) " · " (request.status_label())
          </p>
          <p class="text-xs text-muted-foreground">"Solicitada em " (opened)</p>
          if let Some(instructions) = request.instructions.clone().filter(|_| request.status == "approved") {
              <p class="mt-2 whitespace-pre-line">(instructions)</p>
          }
          if let Some(reason) = request.rejection_reason.clone() {
              <p class="mt-2 text-muted-foreground">"Motivo: " (reason)</p>
          }
          if let Some(cents) = request.refund_cents {
              <p class="mt-2">"Reembolso: " (format_price(cents))</p>
          }
      </div>
  })
}

/// Submitted form: `t`, `kind`, `reason`, `desired_size` and one
/// `qty_<order_item_id>` per item.
fn parse_form(
  pairs: Vec<(String, String)>,
) -> (Option<String>, Option<NewReturn>) {
  let mut token = None;
  let mut kind = None;
  let mut reason = String::new();
  let mut desired_size = None;
  let mut lines = Vec::new();
  for (key, value) in pairs {
    match key.as_str() {
      "t" => token = Some(value.trim().to_string()).filter(|t| !t.is_empty()),
      "kind" => kind = ReturnKind::parse(value.trim()),
      "reason" => reason = value,
      "desired_size" => desired_size = Some(value),
      _ => {
        if let Some(raw_id) = key.strip_prefix("qty_")
          && let Ok(item_id) = Uuid::parse_str(raw_id)
        {
          let quantity = value.trim().parse::<i32>().unwrap_or(-1);
          lines.push((item_id, quantity));
        }
      }
    }
  }
  let request = kind.map(|kind| NewReturn {
    kind,
    reason,
    desired_size,
    lines,
  });
  (token, request)
}

#[route(POST "/pedido/{id}/troca")]
async fn submit_return(
  cx: &Cx,
  Form(pairs): Form<Vec<(String, String)>>,
) -> Result<Response> {
  let order_id = *path_param::<Id>(cx)?;
  let pool = app_context::<PgPool>(cx);
  let session_user_id =
    current_user_owned(cx).await?.map(|session| session.user.id);
  let (token, request) = parse_form(pairs);
  let back = form_link(cx, order_id, token.as_deref());
  let Some(request) = request else {
    set_toast(cx, Toast::error("Escolha o tipo de solicitação."));
    return toast_redirect(cx, back);
  };
  match create_request(
    pool,
    order_id,
    token.as_deref(),
    session_user_id,
    request,
  )
  .await
  {
    Ok(_) => {
      set_toast(
        cx,
        Toast::success("Solicitação enviada. Responderemos por e-mail."),
      );
      toast_redirect(cx, order_link(cx, order_id, token.as_deref()))
    }
    Err(ReturnError::Db(error)) => Err(error.into()),
    Err(ReturnError::NotFound) => {
      set_toast(cx, Toast::error("Pedido não encontrado."));
      toast_redirect(cx, back)
    }
    Err(error) => {
      set_toast(cx, Toast::error(error.to_string()));
      toast_redirect(cx, back)
    }
  }
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let order_id = *path_param::<Id>(cx)?;
  let query = query_params::<PedidoQuery>(cx)?;
  let token = query
    .t
    .as_deref()
    .map(str::trim)
    .filter(|value| !value.is_empty())
    .map(str::to_string);
  let session_user_id =
    current_user_owned(cx).await?.map(|session| session.user.id);

  let access: Option<(Uuid, bool, Option<Uuid>)> = sqlx::query_as(
    "SELECT access_token, access_token_expires_at <= now(), user_id
     FROM orders WHERE id = $1",
  )
  .bind(order_id)
  .fetch_optional(pool)
  .await?;
  let allowed = access.is_some_and(|(stored, expired, owner)| {
    token_or_owner_allows(
      stored,
      expired,
      owner,
      token.as_deref(),
      session_user_id,
    )
  });
  if !allowed {
    return Ok(view! {
        container(
            variant: ContainerVariant::Narrow,
            attrs: attributes! { class="py-24 text-center" },
            <h1 class="text-4xl font-bold tracking-tight">"Pedido não encontrado"</h1>
        )
    }
    .boxed());
  }

  let info = order_returns(pool, order_id).await?;
  let items: Vec<_> = returnable_items(pool, order_id)
    .await?
    .into_iter()
    .filter(|item| item.returnable > 0)
    .collect();
  let back = order_link(cx, order_id, token.as_deref());
  let action = href!(submit_return, Id(order_id)).resolve(cx);
  let can_open = info.can_open() && !items.is_empty();
  let blocked = info.blocked.clone().unwrap_or_default();
  let kinds = info.open_kinds.clone();
  let order_code = crate::mail::templates::order_code(order_id);
  let token_value = token.unwrap_or_default();
  let policy = href!(crate::app::trocas_devolucoes::page);

  Ok(view! {
      container(
          variant: ContainerVariant::Narrow,
          <p class="text-xs uppercase tracking-widest text-muted-foreground">"Pedido #" (order_code)</p>
          <h1 class="text-4xl font-bold tracking-tight">"Troca ou devolução"</h1>
          <p class="text-sm text-muted-foreground">
              "A peça deve estar sem uso, com etiquetas e na embalagem original. "
              <a href=(policy) class="text-primary hover:underline">"Ver política de trocas"</a>
          </p>
          if can_open {
              <form method="post" action=(action) class="flex flex-col gap-4">
                  <input type="hidden" name="t" value=(token_value)>
                  card(
                      card_header(card_title("Itens"))
                      card_content(
                          <div class="flex flex-col gap-3">
                              for item in items {
                                  let field = format!("qty_{}", item.order_item_id);
                                  <div class="flex items-center justify-between gap-3 text-sm">
                                      label(
                                          attrs: attributes! { for=(field.clone()) },
                                          (item.product_name) " (" (item.variant_label) ")"
                                      )
                                      <div class="w-24">
                                          select(
                                              attrs: attributes! { id=(field.clone()) name=(field) },
                                              for n in 0..=item.returnable {
                                                  <option value=(n.to_string())>(n.to_string())</option>
                                              }
                                          )
                                      </div>
                                  </div>
                              }
                          </div>
                      )
                  )
                  card(
                      card_header(card_title("Solicitação"))
                      card_content(
                          <div class="flex flex-col gap-3">
                              <div class="flex flex-col gap-1.5">
                                  label(attrs: attributes! { for="kind" }, "Tipo")
                                  select(
                                      attrs: attributes! { id="kind" name="kind" required="" },
                                      for (kind, deadline) in kinds {
                                          <option value=(kind.as_str())>
                                              (kind.label()) " (até " (format_datetime_br(deadline)) ")"
                                          </option>
                                      }
                                  )
                              </div>
                              <div class="flex flex-col gap-1.5">
                                  label(attrs: attributes! { for="desired_size" }, "Tamanho desejado (para troca)")
                                  input(attrs: attributes! {
                                      id="desired_size"
                                      name="desired_size"
                                      maxlength=(SIZE_MAX_CHARS.to_string())
                                      placeholder="Ex.: M"
                                      autocomplete="off"
                                  })
                              </div>
                              <div class="flex flex-col gap-1.5">
                                  label(attrs: attributes! { for="reason" }, "Motivo")
                                  textarea(attrs: attributes! {
                                      id="reason"
                                      name="reason"
                                      required=""
                                      maxlength=(REASON_MAX_CHARS.to_string())
                                      placeholder="Conte o que aconteceu (tamanho, defeito, arrependimento...)"
                                  })
                              </div>
                          </div>
                      )
                  )
                  <div class="flex flex-wrap justify-end gap-3">
                      <a href=(back.clone()) class=(button_variants(ButtonVariant::Ghost, ButtonSize::Md))>"Voltar ao pedido"</a>
                      <button type="submit" class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))>
                          "Enviar solicitação"
                      </button>
                  </div>
              </form>
          } else {
              <p class="text-sm text-muted-foreground">
                  if blocked.is_empty() {
                      "Este pedido não pode receber uma solicitação de troca ou devolução."
                  } else {
                      (blocked)
                  }
              </p>
              <a href=(back) class=(button_variants(ButtonVariant::Outline, ButtonSize::Md))>"Voltar ao pedido"</a>
          }
      )
  }
  .boxed())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn form_parses_quantities_and_kind() {
    let item = Uuid::now_v7();
    let (token, request) = parse_form(vec![
      ("t".into(), " tok ".into()),
      ("kind".into(), "exchange".into()),
      ("reason".into(), "Ficou pequeno".into()),
      ("desired_size".into(), "G".into()),
      (format!("qty_{item}"), "2".into()),
      ("qty_not-a-uuid".into(), "1".into()),
    ]);
    assert_eq!(token.as_deref(), Some("tok"));
    let request = request.expect("kind parsed");
    assert_eq!(request.kind, ReturnKind::Exchange);
    assert_eq!(request.lines, vec![(item, 2)]);
    assert_eq!(request.desired_size.as_deref(), Some("G"));

    let (_, missing) = parse_form(vec![("kind".into(), "outro".into())]);
    assert!(missing.is_none());
  }
}
