use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Form, href, page, query_params, response::Response, route,
  },
  view::{View, attributes, view},
};
use uuid::Uuid;

use crate::app::auth_helpers::require_user;
use crate::app::store::addresses::{
  self, AddressError, AddressInput, MAX_ADDRESSES_PER_USER, SavedAddress,
};
use crate::app::store::shipping::BRAZILIAN_STATES;
use crate::components::badge::{BadgeVariant, badge};
use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::card::{
  card, card_content, card_footer, card_header, card_title,
};
use crate::components::checkbox::checkbox;
use crate::components::container::container;
use crate::components::field::{field, field_label};
use crate::components::input::input;
use crate::components::label::label;
use crate::components::select::select;
use crate::components::toast::{Toast, set_toast, toast_redirect};

#[query_params(error = bad_request)]
struct EnderecosQuery {
  editar: Option<String>,
}

#[derive(Deserialize)]
pub struct SaveAddressForm {
  #[serde(default)]
  id: String,
  #[serde(flatten)]
  address: AddressInput,
}

#[derive(Deserialize)]
pub struct AddressIdForm {
  #[serde(default)]
  id: String,
}

fn parse_id(raw: &str) -> Option<Uuid> {
  Uuid::parse_str(raw.trim()).ok()
}

fn error_message(error: &AddressError) -> String {
  match error {
    AddressError::Db(error) => {
      tracing::error!(%error, "address write failed");
      "Não foi possível salvar o endereço. Tente novamente.".to_string()
    }
    other => other.to_string(),
  }
}

/// Create (no `id`) or update (`id` of one of the user's addresses).
#[route(POST "/conta/enderecos/salvar")]
async fn save_address(
  cx: &Cx,
  Form(form): Form<SaveAddressForm>,
) -> Result<Response> {
  let actor = require_user(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let list_url = href!(page).resolve(cx);
  let editing = parse_id(&form.id);
  let outcome = match editing {
    Some(address_id) => {
      addresses::update(pool, actor.user.id, address_id, &form.address).await
    }
    None => addresses::create(pool, actor.user.id, &form.address)
      .await
      .map(|_| ()),
  };
  match outcome {
    Ok(()) => {
      set_toast(cx, Toast::success("Endereço salvo."));
      toast_redirect(cx, list_url)
    }
    Err(error) => {
      set_toast(cx, Toast::error(error_message(&error)));
      let back = match (editing, &error) {
        (Some(address_id), AddressError::Invalid(_)) => href!(page)
          .query([("editar", address_id.to_string().as_str())])
          .resolve(cx),
        _ => list_url,
      };
      toast_redirect(cx, back)
    }
  }
}

#[route(POST "/conta/enderecos/remover")]
async fn remove_address(
  cx: &Cx,
  Form(form): Form<AddressIdForm>,
) -> Result<Response> {
  let actor = require_user(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let list_url = href!(page).resolve(cx);
  let outcome = match parse_id(&form.id) {
    Some(address_id) => {
      addresses::delete(pool, actor.user.id, address_id).await
    }
    None => Err(AddressError::NotFound),
  };
  match outcome {
    Ok(()) => set_toast(cx, Toast::success("Endereço removido.")),
    Err(error) => set_toast(cx, Toast::error(error_message(&error))),
  }
  toast_redirect(cx, list_url)
}

#[route(POST "/conta/enderecos/padrao")]
async fn default_address(
  cx: &Cx,
  Form(form): Form<AddressIdForm>,
) -> Result<Response> {
  let actor = require_user(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let list_url = href!(page).resolve(cx);
  let outcome = match parse_id(&form.id) {
    Some(address_id) => {
      addresses::set_default(pool, actor.user.id, address_id).await
    }
    None => Err(AddressError::NotFound),
  };
  match outcome {
    Ok(()) => set_toast(cx, Toast::success("Endereço padrão atualizado.")),
    Err(error) => set_toast(cx, Toast::error(error_message(&error))),
  }
  toast_redirect(cx, list_url)
}

#[page]
pub async fn page(cx: &Cx) -> Result<impl View> {
  let actor = require_user(cx).await?;
  let pool = app_context::<PgPool>(cx);
  let query = query_params::<EnderecosQuery>(cx)?;
  let saved = addresses::list_for_user(pool, actor.user.id).await?;
  // `?editar=` only ever resolves among the user's own rows.
  let editing: Option<SavedAddress> = query
    .editar
    .as_deref()
    .and_then(parse_id)
    .and_then(|id| saved.iter().find(|address| address.id == id).cloned());
  let at_limit = saved.len() as i64 >= MAX_ADDRESSES_PER_USER;
  let show_form = editing.is_some() || !at_limit;
  let is_empty = saved.is_empty();

  let form_title = if editing.is_some() {
    "Editar endereço"
  } else {
    "Novo endereço"
  };
  let edit_id = editing
    .as_ref()
    .map(|address| address.id.to_string())
    .unwrap_or_default();
  let current = editing.clone();
  let value = move |pick: fn(&SavedAddress) -> String| -> String {
    current.as_ref().map(pick).unwrap_or_default()
  };
  let label_value = value(|a| a.label.clone());
  let recipient_value = value(|a| a.recipient_name.clone());
  let phone_value = value(|a| a.phone.clone());
  let zip_value = value(SavedAddress::formatted_zip);
  let street_value = value(|a| a.street.clone());
  let number_value = value(|a| a.number.clone());
  let complement_value = value(|a| a.complement.clone());
  let neighborhood_value = value(|a| a.neighborhood.clone());
  let city_value = value(|a| a.city.clone());
  let state_value = editing
    .as_ref()
    .map(|address| address.state.clone())
    .unwrap_or_else(|| "SE".to_string());
  let default_checked = editing.as_ref().is_some_and(|a| a.is_default);

  Ok(view! {
      container(
          <div class="flex flex-wrap items-center justify-between gap-3">
              <div>
                  <h1 class="text-2xl font-bold">"Meus Endereços"</h1>
                  <p class="text-sm text-muted-foreground">
                      "Endereços salvos aparecem no checkout para preencher a entrega."
                  </p>
              </div>
              <a
                  href=(href!(crate::app::conta::pedidos::page))
                  class=(button_variants(ButtonVariant::Ghost, ButtonSize::Md))
              >
                  "Meus pedidos"
              </a>
          </div>

          if is_empty {
              <div class="rounded-xl border border-border bg-background p-12 text-center shadow-sm">
                  <p class="text-sm text-muted-foreground">"Você ainda não salvou nenhum endereço."</p>
              </div>
          } else {
              <div class="grid gap-4 @md/page:grid-cols-2">
                  #[key(address.id.to_string())]
                  for address in saved {
                      let id = address.id.to_string();
                      let id_default = id.clone();
                      let id_remove = id.clone();
                      let title = if address.label.is_empty() {
                          address.recipient_name.clone()
                      } else {
                          address.label.clone()
                      };
                      card(
                          card_header(
                              <div class="flex items-center gap-2">
                                  card_title((title))
                                  if address.is_default {
                                      badge(variant: BadgeVariant::Primary, "Padrão")
                                  }
                              </div>
                          )
                          card_content(
                              <p class="text-sm">(address.recipient_name.clone())</p>
                              <p class="text-sm text-muted-foreground">(address.one_line())</p>
                              <p class="text-sm text-muted-foreground">"WhatsApp: " (address.phone.clone())</p>
                          )
                          card_footer(
                              <div class="flex flex-wrap items-center gap-2">
                                  <a
                                      href=(href!(page).query([("editar", id.as_str())]))
                                      class=(button_variants(ButtonVariant::Outline, ButtonSize::Sm))
                                  >
                                      "Editar"
                                  </a>
                                  if !address.is_default {
                                      <form method="post" action=(href!(default_address))>
                                          <input type="hidden" name="id" value=(id_default)>
                                          button(
                                              variant: ButtonVariant::Ghost,
                                              size: ButtonSize::Sm,
                                              attrs: attributes! { type="submit" },
                                              "Tornar padrão"
                                          )
                                      </form>
                                  }
                                  <form method="post" action=(href!(remove_address))>
                                      <input type="hidden" name="id" value=(id_remove)>
                                      button(
                                          variant: ButtonVariant::Ghost,
                                          size: ButtonSize::Sm,
                                          attrs: attributes! {
                                              type="submit"
                                              onclick="return confirm('Remover este endereço?')"
                                          },
                                          "Remover"
                                      )
                                  </form>
                              </div>
                          )
                      )
                  }
              </div>
          }

          if show_form {
              card(
                  card_header(card_title((form_title)))
                  card_content(
                      <form method="post" action=(href!(save_address)) class="grid gap-4 @sm/page:grid-cols-2">
                          <input type="hidden" name="id" value=(edit_id)>
                          field(
                              field_label(attrs: attributes! { for="label" }, "Apelido (opcional)")
                              input(attrs: attributes! {
                                  id="label" name="label" placeholder="Casa, Trabalho…"
                                  maxlength="40" value=(label_value)
                              })
                          )
                          field(
                              field_label(attrs: attributes! { for="recipient_name" }, "Nome de quem recebe")
                              input(attrs: attributes! {
                                  id="recipient_name" name="recipient_name" autocomplete="name"
                                  required="" value=(recipient_value)
                              })
                          )
                          field(
                              field_label(attrs: attributes! { for="phone" }, "WhatsApp")
                              input(attrs: attributes! {
                                  id="phone" name="phone" inputMode="tel" autocomplete="tel"
                                  placeholder="79999816511" required="" value=(phone_value)
                              })
                          )
                          field(
                              field_label(attrs: attributes! { for="zip" }, "CEP")
                              input(attrs: attributes! {
                                  id="zip" name="zip_code" placeholder="49000-000"
                                  autocomplete="postal-code" required="" value=(zip_value)
                              })
                          )
                          <div class="@sm/page:col-span-2">
                              field(
                                  field_label(attrs: attributes! { for="street" }, "Rua")
                                  input(attrs: attributes! {
                                      id="street" name="street" required="" value=(street_value)
                                  })
                              )
                          </div>
                          field(
                              field_label(attrs: attributes! { for="number" }, "Número")
                              input(attrs: attributes! {
                                  id="number" name="number" required="" value=(number_value)
                              })
                          )
                          field(
                              field_label(attrs: attributes! { for="complement" }, "Complemento")
                              input(attrs: attributes! {
                                  id="complement" name="complement" value=(complement_value)
                              })
                          )
                          field(
                              field_label(attrs: attributes! { for="neighborhood" }, "Bairro")
                              input(attrs: attributes! {
                                  id="neighborhood" name="neighborhood" required="" value=(neighborhood_value)
                              })
                          )
                          field(
                              field_label(attrs: attributes! { for="city" }, "Cidade")
                              input(attrs: attributes! {
                                  id="city" name="city" required="" value=(city_value)
                              })
                          )
                          field(
                              field_label(attrs: attributes! { for="state" }, "Estado")
                              select(
                                  attrs: attributes! { id="state" name="state" },
                                  for uf in BRAZILIAN_STATES {
                                      <option value=(uf) selected=(state_value == uf)>(uf)</option>
                                  }
                              )
                          )
                          <div class="flex items-center gap-2 self-end">
                              checkbox(attrs: attributes! {
                                  id="is_default" name="is_default" value="1" checked=(default_checked)
                              })
                              label(attrs: attributes! { for="is_default" }, "Usar como padrão")
                          </div>
                          <div class="flex flex-wrap gap-2 @sm/page:col-span-2">
                              button(
                                  variant: ButtonVariant::Primary,
                                  attrs: attributes! { type="submit" },
                                  "Salvar endereço"
                              )
                              if editing.is_some() {
                                  <a href=(href!(page)) class=(button_variants(ButtonVariant::Ghost, ButtonSize::Md))>
                                      "Cancelar"
                                  </a>
                              }
                          </div>
                      </form>
                  )
              )
          } else {
              <p class="text-sm text-muted-foreground">
                  "Você atingiu o limite de " (MAX_ADDRESSES_PER_USER) " endereços. Remova um para adicionar outro."
              </p>
          }
      )
  })
}
