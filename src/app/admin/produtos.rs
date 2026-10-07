pub mod id;
pub mod novo;

use std::collections::HashMap;

use sqlx::PgPool;
use topcoat::{
  Error, Result,
  context::Cx,
  context::app_context,
  router::{content::multipart::Multipart, href, page, query_params},
  runtime::{Event, shard, signal},
  view::{View, attributes, view},
};
use uuid::Uuid;

use crate::app::utils::{
  MSG_UNAVAILABLE, is_storage_key, resolve_storage_url, storage_failure,
  validate_image,
};
use crate::storage::ObjectStore;

use crate::app::auth_helpers::require_admin;
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{ButtonSize, ButtonVariant, button_variants};
use crate::components::container::container;
use crate::components::input::input;
use crate::components::pagination::{
  pagination, pagination_content, pagination_item,
};
use crate::components::table::{
  table, table_body, table_cell, table_head, table_header, table_row,
};

#[query_params(error = bad_request)]
struct ProductsAdminQuery {
  q: Option<String>,
  active: Option<String>,
  page: Option<String>,
}

const PRODUCT_QUERY_MAX: usize = 80;

fn clamp_product_query(raw: &str) -> String {
  raw.trim().chars().take(PRODUCT_QUERY_MAX).collect()
}

fn clamp_product_active(raw: &str) -> String {
  match raw.trim() {
    "true" | "false" => raw.trim().to_string(),
    _ => "all".to_string(),
  }
}

fn clamp_product_page(page_num: i64) -> i64 {
  page_num.clamp(1, 10_000)
}

fn parse_page(raw: &Option<String>) -> i64 {
  raw
    .as_deref()
    .and_then(|value| value.parse().ok())
    .filter(|number| *number > 0)
    .unwrap_or(1)
}

fn category_labels(cat: &str) -> &str {
  match cat {
    "biquini" => "Biquíni",
    "maio" => "Maiô",
    "saida_praia" => "Saída de Praia",
    "acessorio" => "Acessório",
    _ => cat,
  }
}

/// Search, active filter, and pagination. Re-renders on the server when any
/// signal changes, without reloading the page.
///
/// The shard endpoint does not run the page guard, so this checks the admin
/// role itself. Restored signal values are treated as user input.
#[shard("/admin/produtos/lista")]
async fn product_directory(
  cx: &Cx,
  q: String,
  active: String,
  initial_page: i64,
) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let q = signal(cx, || clamp_product_query(&q));
  let active = signal(cx, || clamp_product_active(&active));
  let current_page = signal(cx, || clamp_product_page(initial_page));

  let query = clamp_product_query(&q.get());
  let active_filter = clamp_product_active(&active.get());
  let mut page_num = clamp_product_page(current_page.get());
  let page_size: i64 = 20;

  let total: i64 = sqlx::query_scalar!(
    "SELECT COUNT(*) FROM products p WHERE ($1 = '' OR p.name ILIKE '%' || $1 || '%') AND ($2 = 'all' OR p.active = ($2 = 'true'))",
    query,
    active_filter,
  )
  .fetch_one(pool)
  .await?
  .unwrap_or(0);
  let total_pages = if total == 0 {
    0
  } else {
    (total + page_size - 1) / page_size
  };
  if total_pages > 0 && page_num > total_pages {
    page_num = total_pages;
  }

  let products = sqlx::query!(
        "SELECT p.id::text AS \"id!\", p.name, p.slug, p.category::text AS \"category!\", p.price_in_cents, p.active,
                COUNT(DISTINCT pv.id) as variant_count,
                COALESCE(SUM(i.quantity), 0) as available_total
         FROM products p
         LEFT JOIN product_variants pv ON pv.product_id = p.id
         LEFT JOIN inventory i ON i.variant_id = pv.id
         WHERE ($1 = '' OR p.name ILIKE '%' || $1 || '%')
           AND ($2 = 'all' OR p.active = ($2 = 'true'))
         GROUP BY p.id
         ORDER BY p.name
         LIMIT $3 OFFSET $4",
        query,
        active_filter,
        page_size,
        (page_num - 1) * page_size,
    )
    .fetch_all(pool)
    .await?;

  let is_empty = products.is_empty();
  let show_pagination = total_pages > 1;
  let has_prev = page_num > 1;
  let has_next = total_pages > 0 && page_num < total_pages;
  let prev_page = page_num - 1;
  let next_page = page_num + 1;
  let page_label = if total_pages == 0 {
    "Página 1 de 1".to_string()
  } else {
    format!("Página {page_num} de {total_pages}")
  };

  const FILTERS: [(&str, &str); 3] =
    [("all", "Todos"), ("true", "Ativos"), ("false", "Inativos")];

  Ok(view! {
          input(attrs: attributes! {
              id="product-search"
              type="search"
              placeholder="Buscar por nome"
              aria-label="Buscar produtos"
              :value=$(q.get())
              @input=$(|e: Event| {
                  q.set(e.target.value);
                  current_page.set(1i64);
              })
          })
      <div class="flex flex-wrap items-center gap-2">
          for (key, label) in FILTERS {
              let key_owned = key.to_string();
              let selected = active_filter == key;
              <button
                  type="button"
                  class=(button_variants(
                      if selected {
                          ButtonVariant::Primary
                      } else {
                          ButtonVariant::Outline
                      },
                      ButtonSize::Sm,
                  ))
                  @click=$(|_e: Event| {
                      active.set(key_owned.to_owned());
                      current_page.set(1i64);
                  })
              >
                  (label)
              </button>
          }
      </div>

      if is_empty {
          <p class="text-sm text-muted-foreground">"Nenhum produto encontrado."</p>
      } else {
          table(
              table_header(
                  table_row(
                      table_head("Nome")
                      table_head("Categoria")
                      table_head("Preço")
                      table_head("Variantes")
                      table_head("Disponível")
                      table_head("Status")
                  )
              )
              table_body(
                  #[key(product.id.clone())]
                  for product in products {
                      table_row(
                          table_cell(
                              <a href=(href!(crate::app::admin::produtos::id::page, crate::app::admin::produtos::id::Id(product.id.clone()))) class="font-medium text-primary hover:underline">
                                  (product.name)
                              </a>
                          )
                          table_cell((category_labels(&product.category)))
                          table_cell("R$ " (product.price_in_cents / 100))
                          table_cell((product.variant_count.unwrap_or(0)))
                          table_cell((product.available_total.unwrap_or(0)))
                          table_cell(
                              if product.active {
                                  badge(variant: BadgeVariant::Primary, "Ativo")
                              } else {
                                  badge(variant: BadgeVariant::Secondary, "Inativo")
                              }
                          )
                      )
                  }
              )
          )
      }

      if show_pagination {
              pagination(
                  pagination_content(
                      if has_prev {
                          pagination_item(
                              <button
                                  type="button"
                                  class=(button_variants(ButtonVariant::Ghost, ButtonSize::Md))
                                  @click=$(|_e: Event| current_page.set(prev_page))
                              >
                                  "Anterior"
                              </button>
                          )
                      }
                      pagination_item(
                          <span class="px-3 text-sm text-muted-foreground">(page_label)</span>
                      )
                      if has_next {
                          pagination_item(
                              <button
                                  type="button"
                                  class=(button_variants(ButtonVariant::Ghost, ButtonSize::Md))
                                  @click=$(|_e: Event| current_page.set(next_page))
                              >
                                  "Próxima"
                              </button>
                          )
                      }
                  )
              )
      }
  })
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let query = query_params::<ProductsAdminQuery>(cx)?;
  let q = clamp_product_query(query.q.as_deref().unwrap_or(""));
  let active = clamp_product_active(query.active.as_deref().unwrap_or("all"));
  let page_num = clamp_product_page(parse_page(&query.page));

  Ok(view! {
      container(
          <div class="flex flex-wrap items-center justify-between gap-4">
              <div>
                  <h1 class="text-2xl font-semibold tracking-tight">"Produtos"</h1>
                  <p class="text-muted-foreground">"Lista de produtos cadastrados no catálogo."</p>
              </div>
              <a
                  href=(href!(crate::app::admin::produtos::novo::page))
                  class=(button_variants(ButtonVariant::Primary, ButtonSize::Md))
              >
                  "Novo produto"
              </a>
          </div>

          product_directory(q: q, active: active, initial_page: page_num)
      )
  })
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

/// Shared validation for the product create/edit multipart forms.
pub(crate) const VALID_CATEGORIES: [&str; 4] =
  ["biquini", "maio", "saida_praia", "acessorio"];

pub(crate) const VALID_SIZES: [&str; 5] = ["pp", "p", "m", "g", "gg"];

pub(crate) fn validate_name(raw: &str) -> Result<String, &'static str> {
  let name = raw.trim();
  if name.is_empty() {
    return Err("Informe o nome do produto.");
  }
  if name.chars().count() > 200 {
    return Err("O nome deve ter no máximo 200 caracteres.");
  }
  Ok(name.to_string())
}

pub(crate) fn validate_slug(raw: &str) -> Result<String, &'static str> {
  let slug = raw.trim().to_lowercase();
  if slug.is_empty() {
    return Err("Informe o slug do produto.");
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

pub(crate) fn validate_category(raw: &str) -> Result<String, &'static str> {
  let category = raw.trim();
  if VALID_CATEGORIES.contains(&category) {
    Ok(category.to_string())
  } else {
    Err("Categoria inválida.")
  }
}

pub(crate) fn validate_size(raw: &str) -> Result<String, &'static str> {
  let size = raw.trim().to_lowercase();
  if VALID_SIZES.contains(&size.as_str()) {
    Ok(size)
  } else {
    Err("Tamanho inválido.")
  }
}

/// Parse a Brazilian/standard price (`"199.90"`, `"199,90"`, `"199"`) to cents.
pub(crate) fn parse_price_cents(raw: &str) -> Result<i32, &'static str> {
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    return Err("Informe o preço do produto.");
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
  Ok((value * 100.0).round() as i32)
}

pub(crate) fn parse_quantity(raw: &str) -> Result<i32, &'static str> {
  let quantity: i32 = raw.trim().parse().map_err(|_| "Estoque inválido.")?;
  if !(0..=1_000_000).contains(&quantity) {
    return Err("Estoque inválido.");
  }
  Ok(quantity)
}

pub(crate) fn optional_description(raw: &str) -> Option<String> {
  let description = raw.trim();
  if description.is_empty() {
    None
  } else {
    Some(description.to_string())
  }
}

/// Default warehouse for new variants/inventory, creating `PRINCIPAL` when
/// no warehouse exists yet (mirrors the seed default).
pub(crate) async fn ensure_default_warehouse(
  pool: &PgPool,
) -> Result<Uuid, sqlx::Error> {
  if let Some(id) = sqlx::query_scalar::<_, Option<Uuid>>(
    "SELECT id FROM warehouses WHERE is_default = true ORDER BY created_at LIMIT 1",
  )
  .fetch_one(pool)
  .await?
  {
    return Ok(id);
  }
  if let Some(id) = sqlx::query_scalar::<_, Option<Uuid>>(
    "SELECT id FROM warehouses ORDER BY created_at LIMIT 1",
  )
  .fetch_one(pool)
  .await?
  {
    return Ok(id);
  }
  let id = Uuid::now_v7();
  sqlx::query(
    "INSERT INTO warehouses (id, code, name, is_default, active, created_at)
     VALUES ($1, 'PRINCIPAL', 'Depósito Principal', true, true, now())",
  )
  .bind(id)
  .execute(pool)
  .await?;
  Ok(id)
}

pub(crate) fn db_error_message(err: &sqlx::Error) -> String {
  use std::borrow::Cow;
  let message: Cow<'_, str> = if let sqlx::Error::Database(db) = err {
    if db.constraint() == Some("product_variants_sku_uidx") {
      "Este SKU já está em uso por outra variante.".into()
    } else if db.constraint() == Some("products_slug_uidx") {
      "Este slug já está em uso por outro produto.".into()
    } else if db.code().as_deref() == Some("23503") {
      "Não é possível remover variante com pedidos vinculados.".into()
    } else {
      "Não foi possível salvar. Tente novamente.".into()
    }
  } else {
    "Não foi possível salvar. Tente novamente.".into()
  };
  message.into_owned()
}

// Product image uploads.
//
// Files are validated ([`validate_image`]) and stored in the configured
// [`ObjectStore`] (S3 or memory) — never in the repo tree — under
// `products/{product_id}/…`. `product_images.url` holds the storage key for
// uploads or the original external URL (seed data); readers must resolve keys
// because presigned URLs expire.

pub(crate) const MAX_IMAGES_PER_PRODUCT: usize = 8;

pub(crate) const MSG_COUNT: &str = "O produto pode ter no máximo 8 imagens";

/// Validate `bytes`, store them under `products/{product_id}/…`, and insert
/// the `product_images` row. Returns the storage key.
pub(crate) async fn save_product_image(
  pool: &PgPool,
  store: &ObjectStore,
  product_id: Uuid,
  bytes: &[u8],
  content_type: &str,
  sort_order: i32,
) -> Result<String> {
  let kind = validate_image(bytes, content_type)?;
  if matches!(store, ObjectStore::Unavailable) {
    return Err(Error::msg(MSG_UNAVAILABLE));
  }

  let key = format!(
    "products/{product_id}/{}.{ext}",
    Uuid::now_v7(),
    ext = kind.extension
  );
  store
    .put(&key, bytes.to_vec(), kind.content_type)
    .await
    .map_err(storage_failure)?;

  let inserted = sqlx::query(
    "INSERT INTO product_images (id, product_id, url, alt, sort_order, created_at)
     VALUES ($1, $2, $3, NULL, $4, now())",
  )
  .bind(Uuid::now_v7())
  .bind(product_id)
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

/// Delete a `product_images` row and its stored object (when key-like).
/// External (`://`) URLs only lose the row. Missing rows are Ok.
pub(crate) async fn delete_product_image(
  pool: &PgPool,
  store: &ObjectStore,
  image_id: Uuid,
) -> Result<()> {
  let url = sqlx::query_scalar::<_, Option<String>>(
    "SELECT url FROM product_images WHERE id = $1",
  )
  .bind(image_id)
  .fetch_optional(pool)
  .await?
  .flatten();
  let Some(url) = url else { return Ok(()) };

  sqlx::query("DELETE FROM product_images WHERE id = $1")
    .bind(image_id)
    .execute(pool)
    .await?;

  if is_storage_key(&url) && !matches!(store, ObjectStore::Unavailable) {
    let _ = store.delete(&url).await;
  }
  Ok(())
}

/// Resolve the cover `image_url` of each product summary (storage keys become
/// presigned URLs; external URLs pass through).
pub(crate) async fn resolve_summary_images(
  store: &ObjectStore,
  products: Vec<crate::app::store::queries::ProductSummary>,
) -> Vec<crate::app::store::queries::ProductSummary> {
  let mut resolved = Vec::with_capacity(products.len());
  for mut product in products {
    let url = product.image_url.as_deref();
    product.image_url = resolve_storage_url(store, url).await;
    resolved.push(product);
  }
  resolved
}

/// Resolve every gallery image of a product detail in place.
pub(crate) async fn resolve_detail_images(
  store: &ObjectStore,
  detail: &mut crate::app::store::queries::ProductDetail,
) {
  for image in &mut detail.images {
    if let Some(url) = resolve_storage_url(store, Some(&image.url)).await {
      image.url = url;
    } else {
      image.url = String::new();
    }
  }
  detail.images.retain(|image| !image.url.is_empty());
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::storage::StorageError;

  fn png() -> Vec<u8> {
    vec![0x89, 0x50, 0x4E, 0x47, 0x0D, 0x0A, 0x1A, 0x0A, 0, 0, 0, 0]
  }

  #[tokio::test]
  async fn save_and_delete_roundtrip() {
    let pool = crate::test_support::pool().await;
    let store = ObjectStore::memory();
    let product_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO products (id, name, slug, price_in_cents, category, active, featured, created_at, updated_at)
       VALUES ($1, 'Camiseta', $2, 1000, 'acessorio', true, false, now(), now())",
    )
    .bind(product_id)
    .bind(format!("img-test-{}", product_id.simple()))
    .execute(&pool)
    .await
    .unwrap();

    let key =
      save_product_image(&pool, &store, product_id, &png(), "image/png", 0)
        .await
        .unwrap();
    assert!(key.starts_with(&format!("products/{product_id}/")));
    assert!(key.ends_with(".png"));

    let image_id: Uuid =
      sqlx::query_scalar("SELECT id FROM product_images WHERE product_id = $1")
        .bind(product_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    delete_product_image(&pool, &store, image_id).await.unwrap();
    let remaining: i64 =
      sqlx::query_scalar("SELECT COUNT(*) FROM product_images WHERE id = $1")
        .bind(image_id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(remaining, 0);
    assert!(
      store
        .presign_get(&key)
        .await
        .is_err_and(|e| matches!(e, StorageError::NotFound))
    );
  }
}
