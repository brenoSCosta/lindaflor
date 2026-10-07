use std::collections::{HashMap, HashSet};

use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::multipart::Multipart, href, page, path_param, response::Response,
    route,
  },
  view::{View, ViewExt, attributes, view},
};
use uuid::Uuid;

use super::{
  MAX_IMAGES_PER_PRODUCT, MSG_COUNT, collect_multipart, db_error_message,
  delete_product_image, ensure_default_warehouse, first_text,
  optional_description, parse_price_cents, resolve_storage_url,
  save_product_image, validate_category, validate_image, validate_name,
  validate_size, validate_slug,
};
use crate::app::auth_helpers::require_admin;
use crate::app::utils::object_store;
use crate::components::button::{ButtonVariant, button};
use crate::components::card::{card, card_content};
use crate::components::checkbox::checkbox;
use crate::components::container::container;
use crate::components::input::input;
use crate::components::label::label;
use crate::components::select::select;
use crate::components::textarea::textarea;
use crate::components::toast::{Toast, set_toast, toast_redirect};

path_param!(pub(crate) id: String, error = bad_request);

struct VariantRow {
  sku: String,
  size: String,
  color: String,
  low_stock_threshold: i32,
}

struct ImageRow {
  id: Uuid,
  url: Option<String>,
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let store = object_store(cx);
  let id = path_param::<Id>(cx)?;

  let product = sqlx::query!(
        "SELECT id::text AS \"id!\", name, slug, description, price_in_cents, category::text AS \"category!\", featured, active FROM products WHERE id::text = $1",
        id.to_string()
    )
    .fetch_optional(pool)
    .await?;

  let product = match product {
    Some(p) => p,
    None => {
      return Ok(
        view! {
            container(
                <h1 class="text-2xl font-semibold tracking-tight">"Produto não encontrado"</h1>
                <a href=(href!(crate::app::admin::produtos::page)) class="text-primary hover:underline">
                    "Voltar para produtos"
                </a>
            )
        }
        .boxed(),
      );
    }
  };

  let variants = sqlx::query!(
        "SELECT id::text AS \"id!\", sku, size::text AS \"size!\", color, price_in_cents, low_stock_threshold FROM product_variants WHERE product_id::text = $1 ORDER BY created_at",
        id.to_string()
    )
    .fetch_all(pool)
    .await?;

  let image_rows = sqlx::query!(
        "SELECT id, url, alt FROM product_images WHERE product_id::text = $1 ORDER BY sort_order ASC",
        id.to_string()
    )
    .fetch_all(pool)
    .await?;
  let mut images = Vec::with_capacity(image_rows.len());
  for row in image_rows {
    let url = resolve_storage_url(&store, Some(&row.url)).await;
    images.push(ImageRow { id: row.id, url });
  }

  let product_id = product.id.clone();
  let price_text = format_price_input(product.price_in_cents);
  let category = product.category.clone();
  let edit_action = href!(update_product);

  Ok(view! {
      container(
          <p class="text-muted-foreground">
              <a href=(href!(crate::app::admin::produtos::page)) class="text-primary hover:underline">"Produtos"</a>
              " / "
              (product.name.clone())
          </p>
          <h1 class="text-2xl font-semibold tracking-tight">"Editar produto"</h1>

          card(
              card_content(
                  <form method="post" action=(edit_action) enctype="multipart/form-data" class="flex flex-col gap-4">
                      <input type="hidden" name="product_id" value=(product_id)>
                      <div class="grid gap-4 @sm/page:grid-cols-2">
                          <div class="space-y-2">
                              label(attrs: attributes! { for="name" }, "Nome")
                              input(attrs: attributes! {
                                  type="text"
                                  name="name"
                                  id="name"
                                  value=(product.name)
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="slug" }, "Slug")
                              input(attrs: attributes! {
                                  type="text"
                                  name="slug"
                                  id="slug"
                                  value=(product.slug)
                              })
                          </div>
                      </div>

                      <div class="space-y-2">
                          label(attrs: attributes! { for="description" }, "Descrição")
                          textarea(
                              attrs: attributes! { name="description" id="description" rows="3" },
                              (product.description)
                          )
                      </div>

                      <div class="grid gap-4 @sm/page:grid-cols-2">
                          <div class="space-y-2">
                              label(attrs: attributes! { for="price" }, "Preço (R$)")
                              input(attrs: attributes! {
                                  type="text"
                                  name="price"
                                  id="price"
                                  value=(price_text)
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="category" }, "Categoria")
                              select(
                                  attrs: attributes! { name="category" id="category" },
                                  <option value="biquini" selected=(category == "biquini")>"Biquíni"</option>
                                  <option value="maio" selected=(category == "maio")>"Maiô"</option>
                                  <option value="saida_praia" selected=(category == "saida_praia")>"Saída de Praia"</option>
                                  <option value="acessorio" selected=(category == "acessorio")>"Acessório"</option>
                              )
                          </div>
                      </div>

                      <div class="flex flex-wrap items-center gap-6">
                          <div class="flex items-center gap-2">
                              checkbox(attrs: attributes! {
                                  id="featured"
                                  name="featured"
                                  checked=(product.featured)
                              })
                              label(attrs: attributes! { for="featured" }, "Destaque")
                          </div>
                          <div class="flex items-center gap-2">
                              checkbox(attrs: attributes! {
                                  id="active"
                                  name="active"
                                  checked=(product.active)
                              })
                              label(attrs: attributes! { for="active" }, "Ativo na loja")
                          </div>
                      </div>

                      <fieldset class="space-y-4 rounded-xl border border-border p-2">
                          <legend class="px-2 font-medium">"Imagens atuais"</legend>
                          if images.is_empty() {
                              <p class="text-sm text-muted-foreground">"Nenhuma imagem cadastrada."</p>
                          } else {
                              <div class="grid grid-cols-2 gap-4 @sm/page:grid-cols-4">
                                  for image in images {
                                      <div class="space-y-2 rounded-lg border border-border p-2">
                                          if let Some(url) = image.url {
                                              <img src=(url) alt="Imagem do produto" class="aspect-square w-full rounded-md object-cover">
                                          } else {
                                              <div class="flex aspect-square w-full items-center justify-center rounded-md bg-foreground/5 text-xs text-muted-foreground">"indisponível"</div>
                                          }
                                          <label class="flex items-center gap-2 text-xs text-muted-foreground">
                                              <input type="checkbox" name="delete_image_id" value=(image.id.to_string())>
                                              "Excluir"
                                          </label>
                                      </div>
                                  }
                              </div>
                          }
                          <div class="space-y-2">
                              label(attrs: attributes! { for="images" }, "Adicionar imagens (JPG, PNG ou WebP · 2MB cada · até 8 no total)")
                              <input
                                  type="file"
                                  name="images"
                                  id="images"
                                  accept="image/jpeg,image/png,image/webp"
                                  multiple=""
                                  class="block w-full text-sm text-foreground file:mr-4 file:rounded-lg file:border file:border-border file:bg-foreground/5 file:px-4 file:py-2 file:text-sm file:font-medium hover:file:bg-foreground/10"
                              >
                          </div>
                      </fieldset>

                      <fieldset class="space-y-4 rounded-xl border border-border p-2">
                          <legend class="px-2 font-medium">"Variantes"</legend>
                          <div class="flex flex-col gap-4">
                              for variant in variants {
                                  <div class="grid grid-cols-1 gap-4 rounded-lg border border-border p-4 @sm/page:grid-cols-2 @lg/page:grid-cols-4">
                                      <div class="space-y-2">
                                          label("SKU")
                                          input(attrs: attributes! {
                                              type="text"
                                              name="variant_sku[]"
                                              value=(variant.sku)
                                          })
                                      </div>
                                      <div class="space-y-2">
                                          label("Tamanho")
                                          input(attrs: attributes! {
                                              type="text"
                                              name="variant_size[]"
                                              value=(variant.size)
                                          })
                                      </div>
                                      <div class="space-y-2">
                                          label("Cor")
                                          input(attrs: attributes! {
                                              type="text"
                                              name="variant_color[]"
                                              value=(variant.color)
                                          })
                                      </div>
                                      <div class="space-y-2">
                                          label("Estoque baixo")
                                          input(attrs: attributes! {
                                              type="number"
                                              name="variant_threshold[]"
                                              value=(variant.low_stock_threshold)
                                              min="0"
                                          })
                                      </div>
                                  </div>
                              }
                          </div>
                      </fieldset>

                      button(
                          variant: ButtonVariant::Primary,
                          attrs: attributes! { type="submit" },
                          "Salvar alterações"
                      )
                  </form>
              )
          )
      )
  }.boxed())
}

fn format_price_input(cents: i32) -> String {
  format!("{:.2}", f64::from(cents) / 100.0)
}

#[route(POST "/admin/produtos/update")]
async fn update_product(cx: &Cx, multipart: Multipart) -> Result<Response> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let store = object_store(cx);

  let form = collect_multipart(multipart).await?;
  let raw_id = first_text(&form.texts, "product_id");
  let Ok(product_id) = Uuid::parse_str(raw_id.trim()) else {
    set_toast(cx, Toast::error("Produto inválido."));
    return toast_redirect(
      cx,
      href!(crate::app::admin::produtos::page).resolve(cx),
    );
  };
  let back_url = href!(page, Id(product_id.to_string())).resolve(cx);
  let fail = |message: &str| {
    set_toast(cx, Toast::error(message));
    toast_redirect(cx, back_url.clone())
  };

  let exists: Option<Uuid> =
    sqlx::query_scalar("SELECT id FROM products WHERE id = $1")
      .bind(product_id)
      .fetch_optional(pool)
      .await?;
  if exists.is_none() {
    set_toast(cx, Toast::error("Produto não encontrado."));
    return toast_redirect(
      cx,
      href!(crate::app::admin::produtos::page).resolve(cx),
    );
  }

  let name = match validate_name(&first_text(&form.texts, "name")) {
    Ok(name) => name,
    Err(message) => return fail(message),
  };
  let slug = match validate_slug(&first_text(&form.texts, "slug")) {
    Ok(slug) => slug,
    Err(message) => return fail(message),
  };
  let price_in_cents =
    match parse_price_cents(&first_text(&form.texts, "price")) {
      Ok(price) => price,
      Err(message) => return fail(message),
    };
  let category = match validate_category(&first_text(&form.texts, "category")) {
    Ok(category) => category,
    Err(message) => return fail(message),
  };
  let description =
    optional_description(&first_text(&form.texts, "description"));
  let featured = form.texts.contains_key("featured");
  let active = form.texts.contains_key("active");

  let skus = form.texts.get("variant_sku[]").cloned().unwrap_or_default();
  let sizes = form
    .texts
    .get("variant_size[]")
    .cloned()
    .unwrap_or_default();
  let colors = form
    .texts
    .get("variant_color[]")
    .cloned()
    .unwrap_or_default();
  let thresholds = form
    .texts
    .get("variant_threshold[]")
    .cloned()
    .unwrap_or_default();
  if skus.len() != sizes.len()
    || skus.len() != colors.len()
    || skus.len() != thresholds.len()
  {
    return fail("Variantes inconsistentes. Confira os campos.");
  }
  let mut variants = Vec::with_capacity(skus.len());
  for index in 0..skus.len() {
    let sku = skus[index].trim();
    if sku.is_empty() {
      continue;
    }
    let size = match validate_size(&sizes[index]) {
      Ok(size) => size,
      Err(message) => return fail(message),
    };
    let color = colors[index].trim();
    if color.is_empty() {
      return fail("Toda variante precisa de uma cor.");
    }
    let threshold: i32 = match thresholds[index].trim().parse() {
      Ok(threshold) if (0..=1_000_000).contains(&threshold) => threshold,
      _ => return fail("Limite de estoque baixo inválido."),
    };
    variants.push(VariantRow {
      sku: sku.to_string(),
      size,
      color: color.to_string(),
      low_stock_threshold: threshold,
    });
  }

  let delete_ids: Vec<Uuid> = form
    .texts
    .get("delete_image_id")
    .cloned()
    .unwrap_or_default()
    .iter()
    .filter_map(|raw| Uuid::parse_str(raw.trim()).ok())
    .collect();

  let current_images: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM product_images WHERE product_id = $1",
  )
  .bind(product_id)
  .fetch_one(pool)
  .await?;
  let remaining =
    current_images - delete_ids.len() as i64 + form.files.len() as i64;
  if remaining < 0 || remaining as usize > MAX_IMAGES_PER_PRODUCT {
    return fail(MSG_COUNT);
  }
  for (content_type, bytes) in &form.files {
    if let Err(err) = validate_image(bytes, content_type) {
      return fail(&err.to_string());
    }
  }

  let slug_taken: Option<Uuid> =
    sqlx::query_scalar("SELECT id FROM products WHERE slug = $1 AND id != $2")
      .bind(&slug)
      .bind(product_id)
      .fetch_optional(pool)
      .await?;
  if slug_taken.is_some() {
    return fail("Este slug já está em uso por outro produto.");
  }

  if let Err(err) = sqlx::query(
    "UPDATE products SET name = $2, slug = $3, description = $4, price_in_cents = $5,
     category = $6::product_category, featured = $7, active = $8, updated_at = now() WHERE id = $1",
  )
  .bind(product_id)
  .bind(&name)
  .bind(&slug)
  .bind(&description)
  .bind(price_in_cents)
  .bind(&category)
  .bind(featured)
  .bind(active)
  .execute(pool)
  .await
  {
    return fail(&db_error_message(&err));
  }

  if let Err(message) = sync_variants(pool, product_id, &variants).await {
    return fail(&message);
  }

  for image_id in delete_ids {
    let owner: Option<Uuid> =
      sqlx::query_scalar("SELECT product_id FROM product_images WHERE id = $1")
        .bind(image_id)
        .fetch_optional(pool)
        .await?;
    if owner == Some(product_id)
      && let Err(err) = delete_product_image(pool, &store, image_id).await
    {
      return fail(&err.to_string());
    }
  }

  let next_order: Option<i32> = sqlx::query_scalar(
    "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM product_images WHERE product_id = $1",
  )
  .bind(product_id)
  .fetch_one(pool)
  .await?;
  let base_order = next_order.unwrap_or(0);
  for (index, (content_type, bytes)) in form.files.iter().enumerate() {
    if save_product_image(
      pool,
      &store,
      product_id,
      bytes,
      content_type,
      base_order + index as i32,
    )
    .await
    .is_err()
    {
      return fail(
        "Alterações salvas, mas alguma imagem falhou. Tente novamente.",
      );
    }
  }

  set_toast(cx, Toast::success("Alterações salvas com sucesso."));
  toast_redirect(cx, back_url)
}

/// Reconcile submitted variants (keyed by SKU) with stored rows.
/// New SKUs gain an inventory row in the default warehouse; removed SKUs are
/// deleted (blocked by `order_items` FK when they have orders).
async fn sync_variants(
  pool: &PgPool,
  product_id: Uuid,
  variants: &[VariantRow],
) -> Result<(), String> {
  let existing: Vec<(Uuid, String)> = sqlx::query_as(
    "SELECT id, sku FROM product_variants WHERE product_id = $1",
  )
  .bind(product_id)
  .fetch_all(pool)
  .await
  .map_err(|_| "Não foi possível salvar as variantes.".to_string())?;
  let by_sku: HashMap<&str, Uuid> = existing
    .iter()
    .map(|(id, sku)| (sku.as_str(), *id))
    .collect();
  let submitted: HashSet<&str> =
    variants.iter().map(|v| v.sku.as_str()).collect();

  let warehouse = ensure_default_warehouse(pool)
    .await
    .map_err(|_| "Não foi possível salvar as variantes.".to_string())?;

  for variant in variants {
    if let Some(id) = by_sku.get(variant.sku.as_str()) {
      sqlx::query(
        "UPDATE product_variants SET size = $2::product_size, color = $3,
         low_stock_threshold = $4, updated_at = now() WHERE id = $1",
      )
      .bind(id)
      .bind(&variant.size)
      .bind(&variant.color)
      .bind(variant.low_stock_threshold)
      .execute(pool)
      .await
      .map_err(|err| db_error_message(&err))?;
    } else {
      let id = Uuid::now_v7();
      sqlx::query(
        "INSERT INTO product_variants (id, product_id, sku, size, color, price_in_cents, low_stock_threshold, created_at, updated_at)
         VALUES ($1, $2, $3, $4::product_size, $5, NULL, $6, now(), now())",
      )
      .bind(id)
      .bind(product_id)
      .bind(&variant.sku)
      .bind(&variant.size)
      .bind(&variant.color)
      .bind(variant.low_stock_threshold)
      .execute(pool)
      .await
      .map_err(|err| db_error_message(&err))?;
      sqlx::query(
        "INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved, updated_at)
         VALUES ($1, $2, $3, 0, 0, now())
         ON CONFLICT (variant_id, warehouse_id) DO NOTHING",
      )
      .bind(Uuid::now_v7())
      .bind(id)
      .bind(warehouse)
      .execute(pool)
      .await
      .map_err(|err| db_error_message(&err))?;
    }
  }

  for (id, sku) in &existing {
    if !submitted.contains(sku.as_str()) {
      sqlx::query("DELETE FROM product_variants WHERE id = $1")
        .bind(id)
        .execute(pool)
        .await
        .map_err(|err| db_error_message(&err))?;
    }
  }
  Ok(())
}
