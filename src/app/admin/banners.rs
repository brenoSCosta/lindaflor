use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::{Form, multipart::Multipart},
    href, page,
    response::Response,
    route,
  },
  view::{View, attributes, view},
};
use uuid::Uuid;

use super::produtos::{collect_multipart, first_text};
use crate::app::auth_helpers::require_admin;
use crate::app::utils::{
  MSG_UNAVAILABLE, object_store, resolve_storage_url, validate_image,
};
use crate::components::button::{ButtonSize, ButtonVariant, button};
use crate::components::card::{card, card_content};
use crate::components::checkbox::checkbox;
use crate::components::container::container;
use crate::components::input::input;
use crate::components::label::label;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};
use crate::components::toast::{Toast, set_toast, toast_redirect};
use crate::storage::ObjectStore;

const MAX_TITLE: usize = 120;
const MAX_TEXT: usize = 200;

type BannerDbRow = (
  Uuid,
  String,
  Option<String>,
  Option<String>,
  String,
  i32,
  bool,
);

struct BannerRow {
  id: String,
  title: String,
  subtitle: String,
  link_url: String,
  sort_order: i32,
  active: bool,
  image_url: Option<String>,
}

#[derive(Deserialize)]
struct IdForm {
  banner_id: String,
}

/// Admin home banners: list, toggle, delete and create.
#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let store = object_store(cx);

  let rows: Vec<BannerDbRow> = sqlx::query_as(
    "SELECT id, title, subtitle, link_url, image_url, sort_order, active
       FROM home_banners ORDER BY sort_order ASC, created_at DESC",
  )
  .fetch_all(pool)
  .await?;

  let mut banners = Vec::with_capacity(rows.len());
  for (id, title, subtitle, link_url, image_url, sort_order, active) in rows {
    banners.push(BannerRow {
      id: id.to_string(),
      title,
      subtitle: subtitle.unwrap_or_default(),
      link_url: link_url.unwrap_or_default(),
      sort_order,
      active,
      image_url: resolve_storage_url(&store, Some(&image_url)).await,
    });
  }
  let empty = banners.is_empty();

  Ok(view! {
      container(
          <div class="space-y-8">
              <div>
                  <h1 class="text-2xl font-semibold">"Banners da home"</h1>
                  <p class="text-sm text-muted-foreground">"Os banners ativos aparecem no carrossel da página inicial, na ordem definida."</p>
              </div>

              if empty {
                  <p class="text-sm text-muted-foreground">"Nenhum banner cadastrado ainda."</p>
              } else {
                  table(
                      table_header(
                          table_row(
                              table_head("Imagem")
                              table_head("Título")
                              table_head("Link")
                              table_head("Ordem")
                              table_head("Status")
                              table_head("Ações")
                          )
                      )
                      table_body(
                          #[key(row.id.clone())]
                          for row in banners {
                              table_row(
                                  table_cell(
                                      if let Some(src) = row.image_url.clone() {
                                          <img src=(src) alt=(row.title.clone()) class="h-12 w-20 rounded object-cover">
                                      }
                                  )
                                  table_cell(
                                      <div>
                                          <p class="font-medium">(row.title.clone())</p>
                                          <p class="text-xs text-muted-foreground">(row.subtitle.clone())</p>
                                      </div>
                                  )
                                  table_cell(<span class="text-xs text-muted-foreground">(row.link_url.clone())</span>)
                                  table_cell((row.sort_order))
                                  table_cell(if row.active { "Ativo" } else { "Inativo" })
                                  table_cell(
                                      <div class="flex items-center gap-2">
                                          <form method="post" action=(href!(toggle_banner)) class="inline" data-toast-promise="" data-toast-loading="Atualizando…">
                                              <input type="hidden" name="banner_id" value=(row.id.clone())>
                                              button(
                                                  variant: ButtonVariant::Ghost,
                                                  size: ButtonSize::Sm,
                                                  attrs: attributes! { type="submit" },
                                                  if row.active { "Desativar" } else { "Ativar" }
                                              )
                                          </form>
                                          <form method="post" action=(href!(delete_banner)) class="inline" data-toast-promise="" data-toast-loading="Excluindo…">
                                              <input type="hidden" name="banner_id" value=(row.id.clone())>
                                              button(
                                                  variant: ButtonVariant::Destructive,
                                                  size: ButtonSize::Sm,
                                                  attrs: attributes! { type="submit" },
                                                  "Excluir"
                                              )
                                          </form>
                                      </div>
                                  )
                              )
                          }
                      )
                  )
              }

              card(
                  card_content(
                      <h2 class="mb-4 text-lg font-semibold">"Novo banner"</h2>
                      <form method="post" action=(href!(create_banner)) enctype="multipart/form-data" class="grid gap-4 md:grid-cols-2">
                          <div class="space-y-2">
                              label(attrs: attributes! { for="title" }, "Título")
                              input(attrs: attributes! { type="text" name="title" id="title" required="" maxlength="120" })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="subtitle" }, "Subtítulo")
                              input(attrs: attributes! { type="text" name="subtitle" id="subtitle" maxlength="200" })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="cta_label" }, "Texto do botão")
                              input(attrs: attributes! { type="text" name="cta_label" id="cta_label" maxlength="200" placeholder="Ver coleção" })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="link_url" }, "Link (caminho interno)")
                              input(attrs: attributes! { type="text" name="link_url" id="link_url" maxlength="200" placeholder="/produtos?sort=novos" })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="sort_order" }, "Ordem")
                              input(attrs: attributes! { type="number" name="sort_order" id="sort_order" value="0" min="0" max="999" })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="images" }, "Imagem (JPG, PNG ou WebP, até 2MB)")
                              input(attrs: attributes! { type="file" name="images" id="images" accept="image/jpeg,image/png,image/webp" required="" })
                          </div>
                          <div class="flex items-center gap-2">
                              checkbox(attrs: attributes! { name="active" id="active" checked="" })
                              label(attrs: attributes! { for="active" }, "Ativo")
                          </div>
                          <div class="md:col-span-2">
                              button(attrs: attributes! { type="submit" }, "Criar banner")
                          </div>
                      </form>
                  )
              )
          </div>
      )
  })
}

fn limited(raw: &str, max: usize) -> Option<String> {
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    None
  } else {
    Some(trimmed.chars().take(max).collect())
  }
}

/// Only same-site paths are allowed so a banner can't link off-site.
fn validate_link(raw: &str) -> Result<Option<String>, &'static str> {
  match limited(raw, MAX_TEXT) {
    None => Ok(None),
    Some(link) => crate::redirect::safe_local_redirect(&link)
      .map(Some)
      .ok_or("O link deve ser um caminho interno, começando com /."),
  }
}

#[route(POST "/admin/banners/create")]
async fn create_banner(cx: &Cx, multipart: Multipart) -> Result<Response> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let store = object_store(cx);
  let back = href!(page).resolve(cx);
  let fail = |message: &str| {
    set_toast(cx, Toast::error(message));
    toast_redirect(cx, back.clone())
  };

  let form = collect_multipart(multipart).await?;
  let Some(title) = limited(&first_text(&form.texts, "title"), MAX_TITLE)
  else {
    return fail("Informe o título do banner.");
  };
  let subtitle = limited(&first_text(&form.texts, "subtitle"), MAX_TEXT);
  let cta_label = limited(&first_text(&form.texts, "cta_label"), MAX_TEXT);
  let link_url = match validate_link(&first_text(&form.texts, "link_url")) {
    Ok(link) => link,
    Err(message) => return fail(message),
  };
  let sort_order = first_text(&form.texts, "sort_order")
    .trim()
    .parse::<i32>()
    .unwrap_or(0)
    .clamp(0, 999);
  let active = form.texts.contains_key("active");

  let Some((content_type, bytes)) = form.files.first() else {
    return fail("Envie uma imagem para o banner.");
  };
  let kind = match validate_image(bytes, content_type) {
    Ok(kind) => kind,
    Err(err) => return fail(&err.to_string()),
  };
  if matches!(store, ObjectStore::Unavailable) {
    return fail(MSG_UNAVAILABLE);
  }

  let banner_id = Uuid::now_v7();
  let key = format!("banners/{banner_id}.{}", kind.extension);
  if store
    .put(&key, bytes.clone(), kind.content_type)
    .await
    .is_err()
  {
    return fail(MSG_UNAVAILABLE);
  }

  sqlx::query(
    "INSERT INTO home_banners (id, title, subtitle, cta_label, link_url, image_url, sort_order, active, created_at)
     VALUES ($1, $2, $3, $4, $5, $6, $7, $8, now())",
  )
  .bind(banner_id)
  .bind(&title)
  .bind(&subtitle)
  .bind(&cta_label)
  .bind(&link_url)
  .bind(&key)
  .bind(sort_order)
  .bind(active)
  .execute(pool)
  .await?;

  set_toast(cx, Toast::success("Banner criado."));
  toast_redirect(cx, back)
}

#[route(POST "/admin/banners/toggle")]
async fn toggle_banner(cx: &Cx, Form(body): Form<IdForm>) -> Result<Response> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let back = href!(page).resolve(cx);
  let Ok(id) = Uuid::parse_str(body.banner_id.trim()) else {
    set_toast(cx, Toast::error("Banner inválido."));
    return toast_redirect(cx, back);
  };
  sqlx::query("UPDATE home_banners SET active = NOT active WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await?;
  set_toast(cx, Toast::success("Banner atualizado."));
  toast_redirect(cx, back)
}

#[route(POST "/admin/banners/delete")]
async fn delete_banner(cx: &Cx, Form(body): Form<IdForm>) -> Result<Response> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let store = object_store(cx);
  let back = href!(page).resolve(cx);
  let Ok(id) = Uuid::parse_str(body.banner_id.trim()) else {
    set_toast(cx, Toast::error("Banner inválido."));
    return toast_redirect(cx, back);
  };
  let key: Option<String> = sqlx::query_scalar(
    "DELETE FROM home_banners WHERE id = $1 RETURNING image_url",
  )
  .bind(id)
  .fetch_optional(pool)
  .await?;
  if let Some(key) = key.filter(|key| key.starts_with("banners/")) {
    let _ = store.delete(&key).await;
  }
  set_toast(cx, Toast::success("Banner excluído."));
  toast_redirect(cx, back)
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn link_must_be_internal_path() {
    assert_eq!(validate_link(""), Ok(None));
    assert_eq!(
      validate_link(" /produtos?sale=1 "),
      Ok(Some("/produtos?sale=1".into()))
    );
    assert!(validate_link("https://evil.example").is_err());
    assert!(validate_link("//evil.example").is_err());
    assert!(validate_link("/\\evil.example").is_err());
    assert!(validate_link("/\t/evil.example").is_err());
  }
}
