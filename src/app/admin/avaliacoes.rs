//! Review moderation: pending reviews first, approve/reject (audited).

use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{content::Form, href, page},
  view::{View, attributes, view},
};
use uuid::Uuid;

use crate::app::auth_helpers::require_admin;
use crate::app::store::reviews::{
  Fit, count_pending, list_for_admin, moderate_review, star_rating,
};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{ButtonSize, ButtonVariant, button};
use crate::components::container::container;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};
use crate::components::toast::{Toast, set_toast};

/// Rows shown at once; pending reviews always sort first.
const ADMIN_REVIEWS_LIMIT: i64 = 200;

#[derive(Deserialize)]
pub struct ModerateForm {
  review_id: String,
  action: String,
}

fn status_badge(status: &str) -> (BadgeVariant, &'static str) {
  match status {
    "approved" => (BadgeVariant::Primary, "Aprovada"),
    "rejected" => (BadgeVariant::Destructive, "Rejeitada"),
    _ => (BadgeVariant::Secondary, "Pendente"),
  }
}

#[page([GET, POST])]
pub async fn page(
  cx: &Cx,
  body: Option<Form<ModerateForm>>,
) -> Result<impl View> {
  let actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);

  if let Some(Form(form)) = body {
    let approve = match form.action.as_str() {
      "approve" => Some(true),
      "reject" => Some(false),
      _ => None,
    };
    match (Uuid::parse_str(form.review_id.trim()), approve) {
      (Ok(review_id), Some(approve)) => {
        let found = moderate_review(
          pool,
          review_id,
          approve,
          actor.user.id,
          &actor.user.name,
        )
        .await?;
        if !found {
          set_toast(cx, Toast::error("Avaliação não encontrada."));
        } else if approve {
          set_toast(cx, Toast::success("Avaliação aprovada."));
        } else {
          set_toast(cx, Toast::success("Avaliação rejeitada."));
        }
      }
      _ => set_toast(cx, Toast::error("Ação inválida.")),
    }
  }

  let pending = count_pending(pool).await?;
  let reviews = list_for_admin(pool, ADMIN_REVIEWS_LIMIT).await?;
  let empty = reviews.is_empty();
  let action = href!(page).resolve(cx);

  Ok(view! {
      container(
          <div>
              <h1 class="text-2xl font-semibold tracking-tight">"Avaliações"</h1>
              <p class="text-muted-foreground">
                  (pending) " pendentes de moderação. Só avaliações aprovadas aparecem na loja."
              </p>
          </div>

          if empty {
              <div class="py-12 text-center text-muted-foreground">"Nenhuma avaliação ainda"</div>
          } else {
              table(
                  table_header(
                      table_row(
                          table_head("Produto")
                          table_head("Cliente")
                          table_head("Avaliação")
                          table_head("Status")
                          table_head("Data")
                          table_head("Ações")
                      )
                  )
                  table_body(
                      #[key(review.id.to_string())]
                      for review in reviews {
                          let (variant, label) = status_badge(&review.status);
                          let fit = review.fit.as_deref().and_then(Fit::parse);
                          let is_pending = review.status == "pending";
                          let approve_id = review.id.to_string();
                          let reject_id = review.id.to_string();
                          table_row(
                              table_cell(
                                  <a
                                      href=(href!(crate::app::produtos::slug::page, crate::app::produtos::slug::Slug(review.product_slug)))
                                      class="text-primary hover:underline"
                                  >
                                      (review.product_name)
                                  </a>
                              )
                              table_cell(
                                  <div class="text-sm">(review.author_name)</div>
                                  <div class="text-xs text-muted-foreground">(review.author_email)</div>
                              )
                              table_cell(
                                  <div class="max-w-md space-y-1 whitespace-normal">
                                      star_rating(rating: f64::from(review.rating), small: true)
                                      <p class="font-medium">(review.title)</p>
                                      <p class="whitespace-pre-line text-sm text-muted-foreground">(review.body)</p>
                                      if let Some(fit) = fit {
                                          <p class="text-xs text-muted-foreground">"Caimento: " (fit.label())</p>
                                      }
                                  </div>
                              )
                              table_cell(badge(variant: variant, (label)))
                              table_cell(
                                  <span class="text-muted-foreground">(crate::app::store::queries::format_datetime_br(review.created_at))</span>
                              )
                              table_cell(
                                  <div class="flex flex-wrap gap-2">
                                      if review.status != "approved" {
                                          <form method="post" action=(action.clone())>
                                              <input type="hidden" name="review_id" value=(approve_id)>
                                              <input type="hidden" name="action" value="approve">
                                              button(
                                                  variant: ButtonVariant::Primary,
                                                  size: ButtonSize::Sm,
                                                  attrs: attributes! { type="submit" },
                                                  "Aprovar"
                                              )
                                          </form>
                                      }
                                      if review.status != "rejected" {
                                          <form method="post" action=(action.clone())>
                                              <input type="hidden" name="review_id" value=(reject_id)>
                                              <input type="hidden" name="action" value="reject">
                                              button(
                                                  variant: if is_pending { ButtonVariant::Outline } else { ButtonVariant::Ghost },
                                                  size: ButtonSize::Sm,
                                                  attrs: attributes! { type="submit" },
                                                  "Rejeitar"
                                              )
                                          </form>
                                      }
                                  </div>
                              )
                          )
                      }
                  )
              )
          }
      )
  })
}
