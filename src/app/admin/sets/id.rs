use std::collections::HashSet;

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
  MAX_IMAGES_PER_SET, MSG_COUNT, collect_multipart, delete_set_image,
  first_text, optional_description, parse_set_compare, parse_set_price,
  save_set_image, validate_image, validate_name, validate_slug,
};
use crate::app::auth_helpers::require_admin;
use crate::app::utils::{object_store, resolve_storage_url};
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

struct ImageRow {
  id: Uuid,
  url: Option<String>,
}

struct ProductOption {
  id: String,
  name: String,
}

#[derive(Clone)]
struct VariantOption {
  id: String,
  label: String,
}

struct SlotEdit {
  label: String,
  variant_id: String,
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let store = object_store(cx);
  let id = path_param::<Id>(cx)?;

  let Ok(set_id) = Uuid::parse_str(id.to_string().trim()) else {
    return Ok(
      view! {
          container(
              <h1 class="text-2xl font-semibold tracking-tight">"Conjunto não encontrado"</h1>
              <a href=(href!(super::page)) class="text-primary hover:underline">
                  "Voltar para conjuntos"
              </a>
          )
      }
      .boxed(),
    );
  };

  let set = sqlx::query!(
    "SELECT id, name, slug, description, price_in_cents, compare_at_price_in_cents, active FROM sets WHERE id = $1",
    set_id,
  )
  .fetch_optional(pool)
  .await?;

  let Some(set) = set else {
    return Ok(
      view! {
          container(
              <h1 class="text-2xl font-semibold tracking-tight">"Conjunto não encontrado"</h1>
              <a href=(href!(super::page)) class="text-primary hover:underline">
                  "Voltar para conjuntos"
              </a>
          )
      }
      .boxed(),
    );
  };

  let image_rows = sqlx::query!(
    "SELECT id, url FROM set_images WHERE set_id = $1 ORDER BY sort_order ASC",
    set_id,
  )
  .fetch_all(pool)
  .await?;
  let mut images = Vec::with_capacity(image_rows.len());
  for row in image_rows {
    let url = resolve_storage_url(&store, Some(&row.url)).await;
    images.push(ImageRow { id: row.id, url });
  }

  let member_rows = sqlx::query!(
    "SELECT sm.product_id, p.name FROM set_members sm
      INNER JOIN products p ON p.id = sm.product_id
      WHERE sm.set_id = $1 ORDER BY sm.sort_order ASC",
    set_id,
  )
  .fetch_all(pool)
  .await?;
  let member_ids: HashSet<String> = member_rows
    .iter()
    .map(|row| row.product_id.to_string())
    .collect();

  let product_rows = sqlx::query!(
    "SELECT id, name FROM products WHERE active = true ORDER BY name ASC LIMIT 100",
  )
  .fetch_all(pool)
  .await?;
  let mut products: Vec<ProductOption> = product_rows
    .into_iter()
    .map(|row| ProductOption {
      id: row.id.to_string(),
      name: row.name,
    })
    .collect();
  // Keep current members visible even when inactive so saving doesn't
  // silently drop them.
  let listed: HashSet<String> = products.iter().map(|p| p.id.clone()).collect();
  for row in &member_rows {
    let id = row.product_id.to_string();
    if !listed.contains(id.as_str()) {
      products.push(ProductOption {
        id,
        name: format!("{} (inativo)", row.name),
      });
    }
  }

  let slot_rows = sqlx::query!(
    "SELECT id, label FROM set_slots WHERE set_id = $1 ORDER BY sort_order ASC",
    set_id,
  )
  .fetch_all(pool)
  .await?;
  let slot_ids: Vec<Uuid> = slot_rows.iter().map(|row| row.id).collect();
  let option_rows = sqlx::query!(
    "SELECT slot_id, variant_id FROM set_slot_options WHERE slot_id = ANY($1)",
    &slot_ids,
  )
  .fetch_all(pool)
  .await?;
  let variant_by_slot: std::collections::HashMap<Uuid, Uuid> = option_rows
    .into_iter()
    .map(|row| (row.slot_id, row.variant_id))
    .collect();
  let mut slots: Vec<SlotEdit> = slot_rows
    .into_iter()
    .map(|row| SlotEdit {
      label: row.label.unwrap_or_default(),
      variant_id: variant_by_slot
        .get(&row.id)
        .map(Uuid::to_string)
        .unwrap_or_default(),
    })
    .collect();
  // Blank row for a new slot.
  slots.push(SlotEdit {
    label: String::new(),
    variant_id: String::new(),
  });

  let variant_rows = sqlx::query!(
    "SELECT pv.id, pv.sku, p.name AS product_name, pv.size::text AS \"size!\", pv.color
       FROM product_variants pv
       INNER JOIN products p ON p.id = pv.product_id
       WHERE p.active = true
       ORDER BY p.name ASC, pv.sku ASC LIMIT 200",
  )
  .fetch_all(pool)
  .await?;
  let variants: Vec<VariantOption> = variant_rows
    .into_iter()
    .map(|row| VariantOption {
      id: row.id.to_string(),
      label: format!(
        "{} — {} ({}/{})",
        row.product_name, row.sku, row.size, row.color
      ),
    })
    .collect();

  let set_id_text = set.id.to_string();
  let price_text = set
    .price_in_cents
    .map(format_price_input)
    .unwrap_or_default();
  let compare_text = set
    .compare_at_price_in_cents
    .map(format_price_input)
    .unwrap_or_default();
  let edit_action = href!(update_set, Id(set_id_text.clone()));

  Ok(view! {
      container(
          <p class="text-muted-foreground">
              <a href=(href!(super::page)) class="text-primary hover:underline">"Conjuntos"</a>
              " / "
              (set.name.clone())
          </p>
          <h1 class="text-2xl font-semibold tracking-tight">"Editar conjunto"</h1>

          card(
              card_content(
                  <form method="post" action=(edit_action) enctype="multipart/form-data" class="flex flex-col gap-4">
                      <div class="grid gap-4 @sm/page:grid-cols-2">
                          <div class="space-y-2">
                              label(attrs: attributes! { for="name" }, "Nome")
                              input(attrs: attributes! {
                                  type="text"
                                  name="name"
                                  id="name"
                                  value=(set.name)
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="slug" }, "Slug")
                              input(attrs: attributes! {
                                  type="text"
                                  name="slug"
                                  id="slug"
                                  value=(set.slug)
                              })
                          </div>
                      </div>

                      <div class="space-y-2">
                          label(attrs: attributes! { for="description" }, "Descrição")
                          textarea(
                              attrs: attributes! { name="description" id="description" rows="3" },
                              (set.description)
                          )
                      </div>

                      <div class="grid gap-4 @sm/page:grid-cols-2">
                          <div class="space-y-2">
                              label(attrs: attributes! { for="price" }, "Preço do bundle (R$, opcional)")
                              input(attrs: attributes! {
                                  type="text"
                                  name="price"
                                  id="price"
                                  value=(price_text)
                              })
                          </div>
                          <div class="space-y-2">
                              label(attrs: attributes! { for="compare_at_price" }, "Preço original (de, opcional)")
                              input(attrs: attributes! {
                                  type="text"
                                  name="compare_at_price"
                                  id="compare_at_price"
                                  value=(compare_text)
                              })
                          </div>
                      </div>

                      <div class="flex items-center gap-2">
                          checkbox(attrs: attributes! {
                              id="active"
                              name="active"
                              checked=(set.active)
                          })
                          label(attrs: attributes! { for="active" }, "Ativo na loja")
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
                                              <img src=(url) alt="Imagem do conjunto" class="aspect-square w-full rounded-md object-cover">
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
                          <legend class="px-2 font-medium">"Produtos do conjunto"</legend>
                          if products.is_empty() {
                              <p class="text-sm text-muted-foreground">"Nenhum produto ativo disponível."</p>
                          } else {
                              <div class="space-y-2">
                                  for product in products {
                                      let checked = member_ids.contains(&product.id);
                                      <label class="flex items-center gap-2">
                                          <input
                                              type="checkbox"
                                              name="member_product_ids[]"
                                              value=(product.id)
                                              checked=(checked)
                                          >
                                          <span class="text-sm">(product.name)</span>
                                      </label>
                                  }
                              </div>
                          }
                      </fieldset>

                      <fieldset class="space-y-4 rounded-xl border border-border p-2">
                          <legend class="px-2 font-medium">"Etapas do bundle (1 variante por etapa)"</legend>
                          <p class="text-xs text-muted-foreground">"Cada etapa com variante vira uma peça fixa do conjunto. Deixe a última linha em branco para não criar nova etapa."</p>
                          <div class="flex flex-col gap-4">
                              for slot in slots {
                                  <div class="grid grid-cols-1 gap-4 rounded-lg border border-border p-4 @sm/page:grid-cols-2">
                                      <div class="space-y-2">
                                          label("Etapa")
                                          input(attrs: attributes! {
                                              type="text"
                                              name="slot_label[]"
                                              value=(slot.label)
                                              placeholder="Ex.: Parte de cima"
                                          })
                                      </div>
                                      <div class="space-y-2">
                                          label("Variante")
                                          select(
                                              attrs: attributes! { name="slot_variant[]" },
                                              <option value="">"Selecione…"</option>
                                              for variant in variants.clone() {
                                                  <option value=(variant.id.clone()) selected=(slot.variant_id == variant.id)>(variant.label)</option>
                                              }
                                          )
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

fn format_price_input(cents: i64) -> String {
  format!("{:.2}", cents as f64 / 100.0)
}

struct SlotInput {
  label: Option<String>,
  variant_id: Uuid,
}

#[route(POST "/admin/sets/{id}/update")]
async fn update_set(cx: &Cx, multipart: Multipart) -> Result<Response> {
  let _actor = require_admin(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let store = object_store(cx);
  let id = path_param::<Id>(cx)?;

  let list_url = href!(super::page).resolve(cx);
  let Ok(set_id) = Uuid::parse_str(id.to_string().trim()) else {
    set_toast(cx, Toast::error("Conjunto inválido."));
    return toast_redirect(cx, list_url);
  };
  let back_url = href!(page, Id(set_id.to_string())).resolve(cx);
  let fail = |message: &str| {
    set_toast(cx, Toast::error(message));
    toast_redirect(cx, back_url.clone())
  };

  let exists: Option<Uuid> =
    sqlx::query_scalar("SELECT id FROM sets WHERE id = $1")
      .bind(set_id)
      .fetch_optional(pool)
      .await?;
  if exists.is_none() {
    set_toast(cx, Toast::error("Conjunto não encontrado."));
    return toast_redirect(cx, list_url);
  }

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

  let member_ids: Vec<Uuid> = form
    .texts
    .get("member_product_ids[]")
    .cloned()
    .unwrap_or_default()
    .iter()
    .filter_map(|raw| Uuid::parse_str(raw.trim()).ok())
    .collect();

  let labels = form.texts.get("slot_label[]").cloned().unwrap_or_default();
  let slot_variants = form
    .texts
    .get("slot_variant[]")
    .cloned()
    .unwrap_or_default();
  if labels.len() != slot_variants.len() {
    return fail("Etapas inconsistentes. Confira os campos.");
  }
  let mut slots = Vec::new();
  for index in 0..labels.len() {
    let slot_label = labels[index].trim().to_string();
    let raw_variant = slot_variants[index].trim();
    if slot_label.is_empty() && raw_variant.is_empty() {
      continue;
    }
    let Ok(variant_id) = Uuid::parse_str(raw_variant) else {
      return fail("Variante inválida em uma das etapas.");
    };
    slots.push(SlotInput {
      label: if slot_label.is_empty() {
        None
      } else {
        Some(slot_label)
      },
      variant_id,
    });
  }
  // V2 fixed composition: every persisted slot carries exactly one option
  // (blank rows are skipped above; any other row requires a variant).
  if !slots.is_empty() {
    let variant_ids: Vec<Uuid> =
      slots.iter().map(|slot| slot.variant_id).collect();
    let found: i64 = sqlx::query_scalar(
      "SELECT COUNT(*) FROM product_variants WHERE id = ANY($1)",
    )
    .bind(&variant_ids)
    .fetch_one(pool)
    .await?;
    if found != variant_ids.len() as i64 {
      return fail("Variante inválida em uma das etapas.");
    }
  }

  let delete_ids: Vec<Uuid> = form
    .texts
    .get("delete_image_id")
    .cloned()
    .unwrap_or_default()
    .iter()
    .filter_map(|raw| Uuid::parse_str(raw.trim()).ok())
    .collect();

  let current_images: i64 =
    sqlx::query_scalar("SELECT COUNT(*) FROM set_images WHERE set_id = $1")
      .bind(set_id)
      .fetch_one(pool)
      .await?;
  let remaining =
    current_images - delete_ids.len() as i64 + form.files.len() as i64;
  if remaining < 0 || remaining as usize > MAX_IMAGES_PER_SET {
    return fail(MSG_COUNT);
  }
  for (content_type, bytes) in &form.files {
    if let Err(err) = validate_image(bytes, content_type) {
      return fail(&err.to_string());
    }
  }

  let slug_taken: Option<Uuid> =
    sqlx::query_scalar("SELECT id FROM sets WHERE slug = $1 AND id != $2")
      .bind(&slug)
      .bind(set_id)
      .fetch_optional(pool)
      .await?;
  if slug_taken.is_some() {
    return fail("Este slug já está em uso por outro conjunto.");
  }

  sqlx::query(
    "UPDATE sets SET name = $2, slug = $3, description = $4, price_in_cents = $5,
      compare_at_price_in_cents = $6, active = $7, updated_at = now() WHERE id = $1",
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

  sqlx::query("DELETE FROM set_members WHERE set_id = $1")
    .bind(set_id)
    .execute(pool)
    .await?;
  sqlx::query(
    "INSERT INTO set_members (set_id, product_id, sort_order)
     SELECT $1, p.id, (ids.ord - 1)::int
     FROM unnest($2::uuid[]) WITH ORDINALITY AS ids(id, ord)
     INNER JOIN products p ON p.id = ids.id
     ON CONFLICT DO NOTHING",
  )
  .bind(set_id)
  .bind(&member_ids)
  .execute(pool)
  .await?;

  sqlx::query("DELETE FROM set_slots WHERE set_id = $1")
    .bind(set_id)
    .execute(pool)
    .await?;
  for (index, slot) in slots.iter().enumerate() {
    let slot_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO set_slots (id, set_id, label, sort_order) VALUES ($1, $2, $3, $4)",
    )
    .bind(slot_id)
    .bind(set_id)
    .bind(&slot.label)
    .bind(index as i32)
    .execute(pool)
    .await?;
    sqlx::query(
      "INSERT INTO set_slot_options (slot_id, variant_id) VALUES ($1, $2)",
    )
    .bind(slot_id)
    .bind(slot.variant_id)
    .execute(pool)
    .await?;
  }

  for image_id in delete_ids {
    let owner: Option<Uuid> =
      sqlx::query_scalar("SELECT set_id FROM set_images WHERE id = $1")
        .bind(image_id)
        .fetch_optional(pool)
        .await?;
    if owner == Some(set_id)
      && let Err(err) = delete_set_image(pool, &store, image_id).await
    {
      return fail(&err.to_string());
    }
  }

  let next_order: Option<i32> = sqlx::query_scalar(
    "SELECT COALESCE(MAX(sort_order), -1) + 1 FROM set_images WHERE set_id = $1",
  )
  .bind(set_id)
  .fetch_one(pool)
  .await?;
  let base_order = next_order.unwrap_or(0);
  for (index, (content_type, bytes)) in form.files.iter().enumerate() {
    if save_set_image(
      pool,
      &store,
      set_id,
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
