pub mod id;

use std::collections::HashMap;

use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Error, Result,
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

use crate::app::auth_helpers::require_admin;
use crate::app::utils::{
  MSG_UNAVAILABLE, is_storage_key, object_store, resolve_storage_url,
  storage_failure, validate_image,
};
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{ButtonSize, ButtonVariant, button};
use crate::components::card::{card, card_content};
use crate::components::checkbox::checkbox;
use crate::components::container::container;
use crate::components::input::input;
use crate::components::label::label;
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};
use crate::components::textarea::textarea;
use crate::components::toast::{Toast, set_toast, toast_redirect};
use crate::storage::ObjectStore;

pub(crate) const MAX_IMAGES_PER_SET: usize = 8;

pub(crate) const MSG_COUNT: &str = "O conjunto pode ter no máximo 8 imagens";

struct SetRow {
  id: String,
  name: String,
  slug: String,
  price: Option<i64>,
  active: bool,
  cover: Option<String>,
  member_count: i64,
  image_count: i64,
}

#[derive(Deserialize)]
struct IdForm {
  set_id: String,
}

/// Admin conjuntos: list, toggle, delete and create.
#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let store = object_store(cx);

  let rows = sqlx::query!(
    "SELECT s.id, s.name, s.slug, s.price_in_cents, s.active,
            (SELECT si.url FROM set_images si WHERE si.set_id = s.id ORDER BY si.sort_order ASC LIMIT 1) AS cover_url,
            (SELECT COUNT(*) FROM set_members sm WHERE sm.set_id = s.id) AS \"member_count!\",
            (SELECT COUNT(*) FROM set_images si WHERE si.set_id = s.id) AS \"image_count!\"
       FROM sets s ORDER BY s.name ASC",
  )
  .fetch_all(pool)
  .await?;

  let mut sets = Vec::with_capacity(rows.len());
  for row in rows {
    sets.push(SetRow {
      id: row.id.to_string(),
      name: row.name,
      slug: row.slug,
      price: row.price_in_cents,
      active: row.active,
      cover: resolve_storage_url(&store, row.cover_url.as_deref()).await,
      member_count: row.member_count,
      image_count: row.image_count,
    });
  }
  let empty = sets.is_empty();

  Ok(view! {
      container(
          <div class="space-y-8">
              <div>
                  <h1 class="text-2xl font-semibold tracking-tight">"Conjuntos"</h1>
                  <p class="text-sm text-muted-foreground">"Monte conjuntos com galeria, produtos e preço de bundle."</p>
              </div>

              if empty {
                  <p class="text-sm text-muted-foreground">"Nenhum conjunto cadastrado ainda."</p>
              } else {
                  table(
                      table_header(
                          table_row(
                              table_head("Conjunto")
                              table_head("Slug")
                              table_head("Produtos")
                              table_head("Imagens")
                              table_head("Preço")
                              table_head("Status")
                              table_head("Ações")
                          )
                      )
                      table_body(
                          #[key(row.id.clone())]
                          for row in sets {
                              table_row(
                                  table_cell(
                                      <div class="flex items-center gap-3">
                                          if let Some(src) = row.cover.clone() {
                                              <img src=(src) alt=(row.name.clone()) class="h-12 w-20 rounded object-cover">
                                          }
                                          <a href=(href!(id::page, id::Id(row.id.clone()))) class="font-medium text-primary hover:underline">
                                              (row.name.clone())
                                          </a>
                                      </div>
                                  )
                                  table_cell(<span class="font-mono text-xs text-muted-foreground">(row.slug.clone())</span>)
                                  table_cell((row.member_count))
                                  table_cell((row.image_count))
                                  table_cell(
                                      if let Some(cents) = row.price {
                                          <span>"R$ " (cents / 100)</span>
                                      } else {
                                          <span class="text-muted-foreground">"—"</span>
                                      }
                                  )
                                  table_cell(
                                      if row.active {
                                          badge(variant: BadgeVariant::Primary, "Ativo")
                                      } else {
                                          badge(variant: BadgeVariant::Secondary, "Inativo")
                                      }
                                  )
                                  table_cell(
                                      <div class="flex items-center gap-2">
                                          <form method="post" action=(href!(toggle_set)) class="inline" data-toast-promise="" data-toast-loading="Atualizando…">
                                              <input type="hidden" name="set_id" value=(row.id.clone())>
                                              button(
                                                  variant: ButtonVariant::Ghost,
                                                  size: ButtonSize::Sm,
                                                  attrs: attributes! { type="submit" },
                                                  if row.active { "Desativar" } else { "Ativar" }
                                              )
                                          </form>
                                          <form method="post" action=(href!(delete_set)) class="inline" data-toast-promise="" data-toast-loading="Excluindo…">
                                              <input type="hidden" name="set_id" value=(row.id.clone())>
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
                      <h2 class="mb-4 text-lg font-semibold">"Novo conjunto"</h2>
                      <form method="post" action=(href!(create_set)) enctype="multipart/form-data" class="grid gap-4 md:grid-cols-2">
                          <div class="space-y-2">
                              label(attrs: attributes! { for="name" }, "Nome")
                              input(attrs: attributes! { type="text" name="name" id="name" required="" maxlength="200" })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="slug" }, "Slug")
                              input(attrs: attributes! { type="text" name="slug" id="slug" required="" maxlength="120" })
                          </div>
                          <div class="space-y-2 md:col-span-2">
                              label(attrs: attributes! { for="description" }, "Descrição")
                              textarea(attrs: attributes! { name="description" id="description" rows="3" })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="price" }, "Preço do bundle (R$, opcional)")
                              input(attrs: attributes! { type="text" name="price" id="price" placeholder="199.90" })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="compare_at_price" }, "Preço original (de, opcional)")
                              input(attrs: attributes! { type="text" name="compare_at_price" id="compare_at_price" placeholder="249.90" })
                          </div>
                          <div class="space-y-2 md:col-span-2">
                              label(attrs: attributes! { for="images" }, "Imagens (JPG, PNG ou WebP · 2MB cada · até 8 no total)")
                              <input
                                  type="file"
                                  name="images"
                                  id="images"
                                  accept="image/jpeg,image/png,image/webp"
                                  multiple=""
                                  class="block w-full text-sm text-foreground file:mr-4 file:rounded-lg file:border file:border-border file:bg-foreground/5 file:px-4 file:py-2 file:text-sm file:font-medium hover:file:bg-foreground/10"
                              >
                          </div>
                          <div class="flex items-center gap-2">
                              checkbox(attrs: attributes! { name="active" id="active" checked="" })
                              label(attrs: attributes! { for="active" }, "Ativo na loja")
                          </div>
                          <div class="md:col-span-2">
                              button(attrs: attributes! { type="submit" }, "Criar conjunto")
                          </div>
                      </form>
                  )
              )
          </div>
      )
  })
}

#[route(POST "/admin/sets/create")]
async fn create_set(cx: &Cx, multipart: Multipart) -> Result<Response> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let store = object_store(cx);
  let back = href!(page).resolve(cx);
  let fail = |message: &str| {
    set_toast(cx, Toast::error(message));
    toast_redirect(cx, back.clone())
  };

  let form = collect_multipart(multipart).await?;
  let name = match validate_name(&first_text(&form.texts, "name")) {
    Ok(name) => name,
    Err(message) => return fail(message),
  };
  let slug = match validate_slug(&first_text(&form.texts, "slug")) {
    Ok(slug) => slug,
    Err(message) => return fail(message),
  };
  let price = match parse_set_price(&first_text(&form.texts, "price")) {
    Ok(price) => price,
    Err(message) => return fail(message),
  };
  let compare = match parse_set_compare(
    &first_text(&form.texts, "compare_at_price"),
    price,
  ) {
    Ok(compare) => compare,
    Err(message) => return fail(message),
  };
  let description =
    optional_description(&first_text(&form.texts, "description"));
  let active = form.texts.contains_key("active");

  if form.files.len() > MAX_IMAGES_PER_SET {
    return fail(MSG_COUNT);
  }
  for (content_type, bytes) in &form.files {
    if let Err(err) = validate_image(bytes, content_type) {
      return fail(&err.to_string());
    }
  }

  let existing: Option<Uuid> =
    sqlx::query_scalar("SELECT id FROM sets WHERE slug = $1")
      .bind(&slug)
      .fetch_optional(pool)
      .await?;
  if existing.is_some() {
    return fail("Este slug já está em uso por outro conjunto.");
  }

  let set_id = Uuid::now_v7();
  sqlx::query(
    "INSERT INTO sets (id, name, slug, description, price_in_cents, compare_at_price_in_cents, active, created_at, updated_at)
     VALUES ($1, $2, $3, $4, $5, $6, $7, now(), now())",
  )
  .bind(set_id)
  .bind(&name)
  .bind(&slug)
  .bind(&description)
  .bind(price)
  .bind(compare)
  .bind(active)
  .execute(pool)
  .await?;

  let mut image_failed = false;
  for (index, (content_type, bytes)) in form.files.iter().enumerate() {
    if save_set_image(pool, &store, set_id, bytes, content_type, index as i32)
      .await
      .is_err()
    {
      image_failed = true;
      break;
    }
  }

  let edit_url = href!(id::page, id::Id(set_id.to_string())).resolve(cx);
  if image_failed {
    set_toast(
      cx,
      Toast::error(
        "Conjunto criado, mas alguma imagem falhou. Tente adicioná-la na edição.",
      ),
    );
  } else {
    set_toast(cx, Toast::success("Conjunto criado com sucesso."));
  }
  toast_redirect(cx, edit_url)
}

#[route(POST "/admin/sets/toggle")]
async fn toggle_set(cx: &Cx, Form(body): Form<IdForm>) -> Result<Response> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let back = href!(page).resolve(cx);
  let Ok(id) = Uuid::parse_str(body.set_id.trim()) else {
    set_toast(cx, Toast::error("Conjunto inválido."));
    return toast_redirect(cx, back);
  };
  sqlx::query(
    "UPDATE sets SET active = NOT active, updated_at = now() WHERE id = $1",
  )
  .bind(id)
  .execute(pool)
  .await?;
  set_toast(cx, Toast::success("Conjunto atualizado."));
  toast_redirect(cx, back)
}

#[route(POST "/admin/sets/delete")]
async fn delete_set(cx: &Cx, Form(body): Form<IdForm>) -> Result<Response> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let store = object_store(cx);
  let back = href!(page).resolve(cx);
  let Ok(id) = Uuid::parse_str(body.set_id.trim()) else {
    set_toast(cx, Toast::error("Conjunto inválido."));
    return toast_redirect(cx, back);
  };
  let keys: Vec<String> =
    sqlx::query_scalar("SELECT url FROM set_images WHERE set_id = $1")
      .bind(id)
      .fetch_all(pool)
      .await?;
  sqlx::query("DELETE FROM sets WHERE id = $1")
    .bind(id)
    .execute(pool)
    .await?;
  for key in keys {
    if is_storage_key(&key) && key.starts_with("sets/") {
      let _ = store.delete(&key).await;
    }
  }
  set_toast(cx, Toast::success("Conjunto excluído."));
  toast_redirect(cx, back)
}

/// Collected multipart submission: repeated text values per field name plus
/// uploaded files (`content_type`, `bytes`). Empty file parts are skipped.
pub(crate) struct CollectedForm {
  pub texts: HashMap<String, Vec<String>>,
  pub files: Vec<(String, Vec<u8>)>,
}

pub(crate) async fn collect_multipart(
  mut multipart: Multipart,
) -> Result<CollectedForm> {
  let mut texts: HashMap<String, Vec<String>> = HashMap::new();
  let mut files = Vec::new();
  while let Some(field) = multipart.next_field().await? {
    let name = field.name().unwrap_or("").to_owned();
    let is_file = field.file_name().is_some();
    let content_type = field
      .content_type()
      .unwrap_or("application/octet-stream")
      .to_owned();
    if is_file || name == "images" {
      let bytes = field.bytes().await?.to_vec();
      if bytes.is_empty() {
        continue;
      }
      files.push((content_type, bytes));
    } else {
      let text = field.text().await?;
      texts.entry(name).or_default().push(text);
    }
  }
  Ok(CollectedForm { texts, files })
}

pub(crate) fn first_text(
  texts: &HashMap<String, Vec<String>>,
  key: &str,
) -> String {
  texts
    .get(key)
    .and_then(|v| v.first())
    .cloned()
    .unwrap_or_default()
}

pub(crate) fn validate_name(raw: &str) -> Result<String, &'static str> {
  let name = raw.trim();
  if name.is_empty() {
    return Err("Informe o nome do conjunto.");
  }
  if name.chars().count() > 200 {
    return Err("O nome deve ter no máximo 200 caracteres.");
  }
  Ok(name.to_string())
}

pub(crate) fn validate_slug(raw: &str) -> Result<String, &'static str> {
  let slug = raw.trim().to_lowercase();
  if slug.is_empty() {
    return Err("Informe o slug do conjunto.");
  }
  if slug.len() > 120 {
    return Err("O slug deve ter no máximo 120 caracteres.");
  }
  let valid = !slug.starts_with('-')
    && !slug.ends_with('-')
    && !slug.contains("--")
    && slug
      .chars()
      .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '-');
  if !valid {
    return Err(
      "O slug deve conter apenas letras minúsculas, números e hífens.",
    );
  }
  Ok(slug)
}

/// Parse an optional Brazilian/standard bundle price to cents.
pub(crate) fn parse_set_price(raw: &str) -> Result<Option<i64>, &'static str> {
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    return Ok(None);
  }
  let normalized = if trimmed.contains(',') {
    trimmed.replace('.', "").replace(',', ".")
  } else {
    trimmed.to_string()
  };
  let value: f64 = normalized
    .parse()
    .map_err(|_| "Preço inválido. Use 199.90.")?;
  if !value.is_finite() || value <= 0.0 || value > 9_999_999.0 {
    return Err("Preço inválido. Use 199.90.");
  }
  Ok(Some((value * 100.0).round() as i64))
}

/// Parse the optional compare-at price (empty, or greater than the bundle price).
pub(crate) fn parse_set_compare(
  raw: &str,
  main_price_cents: Option<i64>,
) -> Result<Option<i64>, &'static str> {
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    return Ok(None);
  }
  let Some(main_price_cents) = main_price_cents else {
    return Err("Informe o preço do conjunto para usar o preço original.");
  };
  let normalized = if trimmed.contains(',') {
    trimmed.replace('.', "").replace(',', ".")
  } else {
    trimmed.to_string()
  };
  let value: f64 = normalized
    .parse()
    .map_err(|_| "Preço original inválido. Use 199.90.")?;
  if !value.is_finite() || value <= 0.0 || value > 9_999_999.0 {
    return Err("Preço original inválido. Use 199.90.");
  }
  let compare_cents = (value * 100.0).round() as i64;
  if compare_cents <= main_price_cents {
    return Err("Preço original deve ser maior que o preço do conjunto.");
  }
  Ok(Some(compare_cents))
}

pub(crate) fn optional_description(raw: &str) -> Option<String> {
  let description = raw.trim();
  if description.is_empty() {
    None
  } else {
    Some(description.to_string())
  }
}

// Set image uploads.
//
// Files are validated ([`validate_image`]) and stored in the configured
// [`ObjectStore`] (S3 or memory) — never in the repo tree — under
// `sets/{set_id}/…`. `set_images.url` holds the storage key.

/// Validate `bytes`, store them under `sets/{set_id}/…`, and insert the
/// `set_images` row. Returns the storage key.
pub(crate) async fn save_set_image(
  pool: &PgPool,
  store: &ObjectStore,
  set_id: Uuid,
  bytes: &[u8],
  content_type: &str,
  sort_order: i32,
) -> Result<String> {
  let kind = validate_image(bytes, content_type)?;
  if matches!(store, ObjectStore::Unavailable) {
    return Err(Error::msg(MSG_UNAVAILABLE));
  }

  let key = format!(
    "sets/{set_id}/{}.{ext}",
    Uuid::now_v7(),
    ext = kind.extension
  );
  store
    .put(&key, bytes.to_vec(), kind.content_type)
    .await
    .map_err(storage_failure)?;

  let inserted = sqlx::query(
    "INSERT INTO set_images (id, set_id, url, alt, sort_order, created_at)
     VALUES ($1, $2, $3, NULL, $4, now())",
  )
  .bind(Uuid::now_v7())
  .bind(set_id)
  .bind(&key)
  .bind(sort_order)
  .execute(pool)
  .await;
  if let Err(err) = inserted {
    let _ = store.delete(&key).await;
    return Err(Error::from(err));
  }

  Ok(key)
}

/// Delete a `set_images` row and its stored object (when key-like under
/// `sets/`). Missing rows are Ok.
pub(crate) async fn delete_set_image(
  pool: &PgPool,
  store: &ObjectStore,
  image_id: Uuid,
) -> Result<()> {
  let url = sqlx::query_scalar::<_, Option<String>>(
    "SELECT url FROM set_images WHERE id = $1",
  )
  .bind(image_id)
  .fetch_optional(pool)
  .await?
  .flatten();
  let Some(url) = url else { return Ok(()) };

  sqlx::query("DELETE FROM set_images WHERE id = $1")
    .bind(image_id)
    .execute(pool)
    .await?;

  if is_storage_key(&url)
    && url.starts_with("sets/")
    && !matches!(store, ObjectStore::Unavailable)
  {
    let _ = store.delete(&url).await;
  }
  Ok(())
}
