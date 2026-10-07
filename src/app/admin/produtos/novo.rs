use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::multipart::Multipart, href, page, response::Response, route,
  },
  view::{View, attributes, view},
};
use uuid::Uuid;

use super::{
  MAX_IMAGES_PER_PRODUCT, MSG_COUNT, collect_multipart, db_error_message,
  ensure_default_warehouse, first_text, optional_description,
  parse_price_cents, parse_quantity, save_product_image, validate_category,
  validate_image, validate_name, validate_size, validate_slug,
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

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  Ok(view! {
      container(
          <h1 class="text-2xl font-semibold tracking-tight">"Novo produto"</h1>
          <p class="text-muted-foreground">"Cadastre um produto com variantes, estoque inicial e imagens."</p>

          card(
              card_content(
                  <form method="post" action=(href!(create_product)) enctype="multipart/form-data" class="flex flex-col gap-4">
                      <div class="grid gap-4 @sm/page:grid-cols-2">
                          <div class="space-y-2">
                              label(attrs: attributes! { for="name" }, "Nome")
                              input(attrs: attributes! {
                                  type="text"
                                  name="name"
                                  id="name"
                                  required=""
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="slug" }, "Slug")
                              input(attrs: attributes! {
                                  type="text"
                                  name="slug"
                                  id="slug"
                                  required=""
                              })
                          </div>
                      </div>

                      <div class="space-y-2">
                          label(attrs: attributes! { for="description" }, "Descrição")
                          textarea(attrs: attributes! { name="description" id="description" rows="3" })
                      </div>

                      <div class="grid gap-4 @sm/page:grid-cols-2">
                          <div class="space-y-2">
                              label(attrs: attributes! { for="price" }, "Preço (R$)")
                              input(attrs: attributes! {
                                  type="text"
                                  name="price"
                                  id="price"
                                  placeholder="199.90"
                                  required=""
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="category" }, "Categoria")
                              select(
                                  attrs: attributes! { name="category" id="category" },
                                  <option value="biquini">"Biquíni"</option>
                                  <option value="maio">"Maiô"</option>
                                  <option value="saida_praia">"Saída de Praia"</option>
                                  <option value="acessorio">"Acessório"</option>
                              )
                          </div>
                      </div>

                      <div class="flex items-center gap-2">
                          checkbox(attrs: attributes! { id="featured" name="featured" })
                          label(attrs: attributes! { for="featured" }, "Destaque na home")
                      </div>

                      <div class="space-y-2">
                          label(attrs: attributes! { for="images" }, "Imagens (JPG, PNG ou WebP · 2MB cada · até 8)")
                          <input
                              type="file"
                              name="images"
                              id="images"
                              accept="image/jpeg,image/png,image/webp"
                              multiple=""
                              class="block w-full text-sm text-foreground file:mr-4 file:rounded-lg file:border file:border-border file:bg-foreground/5 file:px-4 file:py-2 file:text-sm file:font-medium hover:file:bg-foreground/10"
                          >
                      </div>

                      <fieldset class="space-y-4 rounded-xl border border-border p-2">
                          <legend class="px-2 font-medium">"Variantes"</legend>
                          <div class="flex flex-col gap-4">
                              <div class="grid grid-cols-1 gap-4 rounded-lg border border-border p-4 @sm/page:grid-cols-2 @lg/page:grid-cols-4">
                                  <div class="space-y-2">
                                      label("SKU")
                                      input(attrs: attributes! {
                                          type="text"
                                          name="variant_sku[]"
                                          placeholder="SKU"
                                          required=""
                                      })
                                  </div>
                                  <div class="space-y-2">
                                      label("Tamanho")
                                      select(
                                          attrs: attributes! { name="variant_size[]" },
                                          <option value="pp">"PP"</option>
                                          <option value="p">"P"</option>
                                          <option value="m">"M"</option>
                                          <option value="g">"G"</option>
                                          <option value="gg">"GG"</option>
                                      )
                                  </div>
                                  <div class="space-y-2">
                                      label("Cor")
                                      input(attrs: attributes! {
                                          type="text"
                                          name="variant_color[]"
                                          placeholder="Cor"
                                          required=""
                                      })
                                  </div>
                                  <div class="space-y-2">
                                      label("Estoque")
                                      input(attrs: attributes! {
                                          type="number"
                                          name="variant_quantity[]"
                                          placeholder="Estoque"
                                          min="0"
                                          value="0"
                                          required=""
                                      })
                                  </div>
                              </div>
                          </div>
                          <p class="text-xs text-muted-foreground">"Adicione mais variantes enviando o formulário e editando o produto."</p>
                      </fieldset>

                      button(
                          variant: ButtonVariant::Primary,
                          attrs: attributes! { type="submit" },
                          "Criar produto"
                      )
                  </form>
              )
          )
      )
  })
}

struct NewVariant {
  sku: String,
  size: String,
  color: String,
  quantity: i32,
}

#[route(POST "/admin/produtos/create")]
async fn create_product(cx: &Cx, multipart: Multipart) -> Result<Response> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let store = object_store(cx);
  let novo_url = href!(page).resolve(cx);

  let form = collect_multipart(multipart).await?;
  let fail = |message: &str| {
    set_toast(cx, Toast::error(message));
    toast_redirect(cx, novo_url.clone())
  };

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
  let quantities = form
    .texts
    .get("variant_quantity[]")
    .cloned()
    .unwrap_or_default();
  if skus.is_empty()
    || skus.len() != sizes.len()
    || skus.len() != colors.len()
    || skus.len() != quantities.len()
  {
    return fail("Informe ao menos uma variante completa.");
  }
  let mut variants = Vec::with_capacity(skus.len());
  for index in 0..skus.len() {
    let sku = skus[index].trim();
    if sku.is_empty() {
      return fail("Toda variante precisa de um SKU.");
    }
    let size = match validate_size(&sizes[index]) {
      Ok(size) => size,
      Err(message) => return fail(message),
    };
    let color = colors[index].trim();
    if color.is_empty() {
      return fail("Toda variante precisa de uma cor.");
    }
    let quantity = match parse_quantity(&quantities[index]) {
      Ok(quantity) => quantity,
      Err(message) => return fail(message),
    };
    variants.push(NewVariant {
      sku: sku.to_string(),
      size,
      color: color.to_string(),
      quantity,
    });
  }

  if form.files.len() > MAX_IMAGES_PER_PRODUCT {
    return fail(MSG_COUNT);
  }
  for (content_type, bytes) in &form.files {
    if let Err(err) = validate_image(bytes, content_type) {
      return fail(&err.to_string());
    }
  }

  let existing: Option<Uuid> =
    sqlx::query_scalar("SELECT id FROM products WHERE slug = $1")
      .bind(&slug)
      .fetch_optional(pool)
      .await?;
  if existing.is_some() {
    return fail("Este slug já está em uso por outro produto.");
  }

  let warehouse = match ensure_default_warehouse(pool).await {
    Ok(warehouse) => warehouse,
    Err(_) => return fail("Não foi possível salvar. Tente novamente."),
  };

  let product_id = Uuid::now_v7();
  if let Err(err) = sqlx::query(
    "INSERT INTO products (id, name, slug, description, price_in_cents, category, active, featured, created_at, updated_at)
     VALUES ($1, $2, $3, $4, $5, $6::product_category, true, $7, now(), now())",
  )
  .bind(product_id)
  .bind(&name)
  .bind(&slug)
  .bind(&description)
  .bind(price_in_cents)
  .bind(&category)
  .bind(featured)
  .execute(pool)
  .await
  {
    return fail(&db_error_message(&err));
  }

  for variant in &variants {
    let variant_id = Uuid::now_v7();
    let inserted = sqlx::query(
      "INSERT INTO product_variants (id, product_id, sku, size, color, price_in_cents, low_stock_threshold, created_at, updated_at)
       VALUES ($1, $2, $3, $4::product_size, $5, NULL, 5, now(), now())",
    )
    .bind(variant_id)
    .bind(product_id)
    .bind(&variant.sku)
    .bind(&variant.size)
    .bind(&variant.color)
    .execute(pool)
    .await;
    if let Err(err) = inserted {
      let _ = sqlx::query("DELETE FROM products WHERE id = $1")
        .bind(product_id)
        .execute(pool)
        .await;
      return fail(&db_error_message(&err));
    }
    sqlx::query(
      "INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved, updated_at)
       VALUES ($1, $2, $3, $4, 0, now())",
    )
    .bind(Uuid::now_v7())
    .bind(variant_id)
    .bind(warehouse)
    .bind(variant.quantity)
    .execute(pool)
    .await?;
  }

  let mut image_failed = false;
  for (index, (content_type, bytes)) in form.files.iter().enumerate() {
    if save_product_image(
      pool,
      &store,
      product_id,
      bytes,
      content_type,
      index as i32,
    )
    .await
    .is_err()
    {
      image_failed = true;
      break;
    }
  }

  let edit_url = href!(
    crate::app::admin::produtos::id::page,
    crate::app::admin::produtos::id::Id(product_id.to_string())
  )
  .resolve(cx);
  if image_failed {
    set_toast(
      cx,
      Toast::error(
        "Produto criado, mas alguma imagem falhou. Tente adicioná-la na edição.",
      ),
    );
  } else {
    set_toast(cx, Toast::success("Produto criado com sucesso."));
  }
  toast_redirect(cx, edit_url)
}
