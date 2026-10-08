use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{content::Form, error::redirect, href, page},
  runtime::{expr, signal},
  view::{View, ViewExt, attributes, view},
};
use uuid::Uuid;

use crate::auth::user::current_user_owned;

use crate::components::alert::{AlertVariant, alert, alert_title};
use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::card::{
  card, card_content, card_footer, card_header, card_title,
};
use crate::components::container::{ContainerVariant, container};
use crate::components::field::{field, field_error, field_label};
use crate::components::input::input;
use crate::components::select::select;
use crate::components::separator::separator;
use crate::components::textarea::textarea;

use crate::app::store::cart::{
  QuoteError, cart_coupon_code, cart_subtotal_cents, clear_items_in_tx,
  hydrate_cart, load_or_create_cart, quote_cart_for_checkout,
};
use crate::app::store::coupons::{
  CouponReject, ResolvedCoupon, resolve_coupon,
};
use crate::app::store::inventory::{
  ReserveLine, release_expired_reservations_in_tx, reserve_for_order,
};
use crate::app::store::queries::format_price;

#[derive(Debug)]
enum CheckoutError {
  Form(String),
  Db(sqlx::Error),
}

impl From<sqlx::Error> for CheckoutError {
  fn from(error: sqlx::Error) -> Self {
    Self::Db(error)
  }
}

impl std::fmt::Display for CheckoutError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Form(message) => write!(f, "{message}"),
      Self::Db(error) => write!(f, "{error}"),
    }
  }
}

#[derive(Deserialize, Clone)]
pub struct CheckoutInput {
  guest_email: String,
  name: String,
  phone: String,
  street: String,
  number: String,
  complement: String,
  neighborhood: String,
  city: String,
  state: String,
  zip_code: String,
  notes: String,

  #[serde(default)]
  coupon_code: Option<String>,

  #[serde(default)]
  intent: Option<String>,
}

const BRAZILIAN_STATES: [&str; 27] = [
  "AC", "AL", "AP", "AM", "BA", "CE", "DF", "ES", "GO", "MA", "MT", "MS", "MG",
  "PA", "PB", "PR", "PE", "PI", "RJ", "RN", "RS", "RO", "RR", "SC", "SP", "SE",
  "TO",
];

#[page([GET, POST])]
pub async fn page(
  cx: &Cx,
  body: Option<Form<CheckoutInput>>,
) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let session_user_id: Option<Uuid> =
    current_user_owned(cx).await?.map(|session| session.user.id);
  let cart = load_or_create_cart(cx, pool, session_user_id).await?;
  let mut hydrated = hydrate_cart(pool, cart.id).await?;
  let store = crate::app::utils::object_store(cx);
  for item in &mut hydrated.items {
    item.image_url =
      crate::app::utils::resolve_storage_url(&store, item.image_url.as_deref())
        .await;
  }
  let items = hydrated.items;

  if items.is_empty() {
    return Ok(view! {
            container(
                variant: ContainerVariant::Narrow,
                <h1 class="text-4xl font-bold tracking-tight">"Seu carrinho está vazio"</h1>
                <p class="text-muted-foreground">
                    "Adicione peças ao carrinho antes de finalizar a compra."
                </p>
                <a
                    href=(href!(crate::app::produtos::page))
                    class=(button_variants(ButtonVariant::Primary, ButtonSize::Lg))
                >
                    "Ver catálogo"
                </a>
            )
        }
        .boxed());
  }

  let subtotal = cart_subtotal_cents(&items);

  let stored_coupon = cart_coupon_code(pool, cart.id).await?;
  let coupon_input = input_coupon(body.as_ref(), stored_coupon.as_deref());

  // DB-backed coupon validation. Preview reads through a rolled-back
  // transaction; order creation re-validates inside its own transaction
  // with `SELECT ... FOR UPDATE`.
  let mut preview_tx = pool.begin().await?;
  let coupon_outcome = resolve_coupon(
    &mut preview_tx,
    &coupon_input,
    subtotal,
    session_user_id,
    false,
  )
  .await;
  preview_tx.rollback().await?;

  let (resolved_coupon, coupon_error): (
    Option<ResolvedCoupon>,
    Option<String>,
  ) = match coupon_outcome {
    Ok(coupon) => (coupon, None),
    Err(CouponReject::Db(error)) => return Err(error.into()),
    Err(CouponReject::Invalid(message)) => (None, Some(message)),
  };

  let intent = body
    .as_ref()
    .and_then(|posted| posted.0.intent.clone())
    .unwrap_or_default();

  let mut checkout_error: Option<String> = None;
  if coupon_error.is_none()
    && intent == "pay"
    && let Some(Form(posted)) = body.as_ref()
  {
    if let Err(message) = validate_pay_fields(posted) {
      checkout_error = Some(message);
    } else {
      match create_order(pool, cart.id, posted.clone(), session_user_id).await {
        Ok(created) => {
          let token = created.access_token.to_string();
          return Err(
            redirect(
              href!(
                crate::app::pedido::id::page,
                crate::app::pedido::id::Id(created.id)
              )
              .query([("t", token.as_str())])
              .resolve(cx),
            )
            .into(),
          );
        }
        Err(CheckoutError::Form(message)) => checkout_error = Some(message),
        Err(CheckoutError::Db(error)) => return Err(error.into()),
      }
    }
  }

  let zip_digits: String = input_zip_digits(body.as_ref());
  let state = input_state(body.as_ref());
  let shipping = calculate_shipping(subtotal, &state, &zip_digits);
  let discount = resolved_coupon
    .as_ref()
    .map(|coupon| coupon.discount_cents)
    .unwrap_or(0);
  let total = subtotal - discount + shipping.0;

  let email_init = input_email(body.as_ref());
  let name_init = input_name(body.as_ref());
  let phone_init = input_phone(body.as_ref());
  let street_init = input_street(body.as_ref());
  let number_init = input_number(body.as_ref());
  let complement_init = input_complement(body.as_ref());
  let neighborhood_init = input_neighborhood(body.as_ref());
  let city_init = input_city(body.as_ref());
  let zip_init = input_zip(body.as_ref());

  let guest_email = signal(cx, || email_init);
  let guest_email_touched = signal(cx, || false);
  let guest_email_error = expr!({
    if !guest_email_touched.get() {
      "".to_owned()
    } else if guest_email.get().trim().is_empty() {
      "Informe o e-mail.".to_owned()
    } else if !guest_email.get().contains("@") {
      "E-mail inválido.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let full_name = signal(cx, || name_init);
  let full_name_touched = signal(cx, || false);
  let full_name_error = expr!({
    if !full_name_touched.get() {
      "".to_owned()
    } else if full_name.get().trim().is_empty() {
      "Informe o nome completo.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let street = signal(cx, || street_init);
  let street_touched = signal(cx, || false);
  let street_error = expr!({
    if !street_touched.get() {
      "".to_owned()
    } else if street.get().trim().is_empty() {
      "Informe a rua.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let number = signal(cx, || number_init);
  let number_touched = signal(cx, || false);
  let number_error = expr!({
    if !number_touched.get() {
      "".to_owned()
    } else if number.get().trim().is_empty() {
      "Informe o número.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let neighborhood = signal(cx, || neighborhood_init);
  let neighborhood_touched = signal(cx, || false);
  let neighborhood_error = expr!({
    if !neighborhood_touched.get() {
      "".to_owned()
    } else if neighborhood.get().trim().is_empty() {
      "Informe o bairro.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let city = signal(cx, || city_init);
  let city_touched = signal(cx, || false);
  let city_error = expr!({
    if !city_touched.get() {
      "".to_owned()
    } else if city.get().trim().is_empty() {
      "Informe a cidade.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let zip_code = signal(cx, || zip_init);
  let zip_code_touched = signal(cx, || false);
  let zip_code_error = expr!({
    if !zip_code_touched.get() {
      "".to_owned()
    } else if zip_code.get().trim().is_empty() {
      "Informe o CEP.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let phone = signal(cx, || phone_init);
  let phone_touched = signal(cx, || false);
  let min_whatsapp_len = 10.0;
  let max_whatsapp_len = 11.0;
  let phone_error = expr!({
    if !phone_touched.get() {
      "".to_owned()
    } else if phone.get().len() < min_whatsapp_len {
      "Informe um WhatsApp válido (DDD + número).".to_owned()
    } else if phone.get().len() > max_whatsapp_len {
      "Informe um WhatsApp válido (DDD + número).".to_owned()
    } else {
      "".to_owned()
    }
  });
  let pay_blocked = expr!({
    if guest_email.get().trim().is_empty() {
      true
    } else if !guest_email.get().contains("@") {
      true
    } else if full_name.get().trim().is_empty() {
      true
    } else if street.get().trim().is_empty() {
      true
    } else if number.get().trim().is_empty() {
      true
    } else if neighborhood.get().trim().is_empty() {
      true
    } else if city.get().trim().is_empty() {
      true
    } else if zip_code.get().trim().is_empty() {
      true
    } else if phone.get().len() < min_whatsapp_len {
      true
    } else if phone.get().len() > max_whatsapp_len {
      true
    } else {
      false
    }
  });

  Ok(view! {
        container(
            variant: ContainerVariant::Wide,
            <h1 class="text-4xl font-bold tracking-tight @md/page:text-5xl">"Checkout"</h1>
            if let Some(ref message) = checkout_error {
                alert(
                    variant: AlertVariant::Destructive,
                    alert_title((message.as_str()))
                )
            }

            <form
                    method="post"
                    action=(href!(page))
                    class="grid gap-4 @lg/page:grid-cols-[1.2fr_0.8fr]"
                    novalidate=""
                >
                <div class="flex flex-col gap-4">
                    card(
                        card_header(
                            card_title("Contato")
                        )
                        card_content(
                            field(
                                attrs: attributes! {
                                    :data-invalid=$( (!guest_email_error.is_empty()).then_some("true") )
                                },
                                field_label(attrs: attributes! { for="email" }, "E-mail")
                                input(
                                    value: guest_email,
                                    touched: guest_email_touched.clone(),
                                    error: guest_email_error.clone(),
                                    attrs: attributes! {
                                        id="email"
                                        name="guest_email"
                                        type="email"
                                        autocomplete="email"
                                        aria-describedby="email-error"
                                    }
                                )
                                field_error(
                                    message: guest_email_error,
                                    attrs: attributes! { id="email-error" }
                                )
                            )
                        )
                    )

                    card(
                        card_header(
                            card_title("Endereço de entrega")
                        )
                        card_content(
                            <div class="grid gap-4 @sm/page:grid-cols-2">
                                <div class="@sm/page:col-span-2">
                                    field(
                                        attrs: attributes! {
                                            :data-invalid=$( (!full_name_error.is_empty()).then_some("true") )
                                        },
                                        field_label(attrs: attributes! { for="name" }, "Nome completo")
                                        input(
                                            value: full_name,
                                            touched: full_name_touched.clone(),
                                            error: full_name_error.clone(),
                                            attrs: attributes! {
                                                id="name"
                                                name="name"
                                                type="text"
                                                autocomplete="name"
                                                aria-describedby="name-error"
                                            }
                                        )
                                        field_error(
                                            message: full_name_error,
                                            attrs: attributes! { id="name-error" }
                                        )
                                    )
                                </div>
                                <div class="@sm/page:col-span-2">
                                    field(
                                        attrs: attributes! {
                                            :data-invalid=$( (!phone_error.is_empty()).then_some("true") )
                                        },
                                        field_label(attrs: attributes! { for="phone" }, "WhatsApp")
                                        input(
                                            value: phone,
                                            touched: phone_touched.clone(),
                                            error: phone_error.clone(),
                                            attrs: attributes! {
                                                id="phone"
                                                name="phone"
                                                inputMode="tel"
                                                autocomplete="tel"
                                                placeholder="79999816511"
                                                aria-describedby="phone-error"
                                            }
                                        )
                                        field_error(
                                            message: phone_error,
                                            attrs: attributes! { id="phone-error" }
                                        )
                                    )
                                </div>
                                <div class="@sm/page:col-span-2">
                                    field(
                                        attrs: attributes! {
                                            :data-invalid=$( (!street_error.is_empty()).then_some("true") )
                                        },
                                        field_label(attrs: attributes! { for="street" }, "Rua")
                                        input(
                                            value: street,
                                            touched: street_touched.clone(),
                                            error: street_error.clone(),
                                            attrs: attributes! {
                                                id="street"
                                                name="street"
                                                aria-describedby="street-error"
                                            }
                                        )
                                        field_error(
                                            message: street_error,
                                            attrs: attributes! { id="street-error" }
                                        )
                                    )
                                </div>
                                field(
                                    attrs: attributes! {
                                        :data-invalid=$( (!number_error.is_empty()).then_some("true") )
                                    },
                                    field_label(attrs: attributes! { for="number" }, "Número")
                                    input(
                                        value: number,
                                        touched: number_touched.clone(),
                                        error: number_error.clone(),
                                        attrs: attributes! {
                                            id="number"
                                            name="number"
                                            aria-describedby="number-error"
                                        }
                                    )
                                    field_error(
                                        message: number_error,
                                        attrs: attributes! { id="number-error" }
                                    )
                                )
                                field(
                                    field_label(attrs: attributes! { for="complement" }, "Complemento")
                                    input(attrs: attributes! {
                                        id="complement"
                                        name="complement"
                                        value=(complement_init)
                                    })
                                )
                                field(
                                    attrs: attributes! {
                                        :data-invalid=$( (!neighborhood_error.is_empty()).then_some("true") )
                                    },
                                    field_label(attrs: attributes! { for="neighborhood" }, "Bairro")
                                    input(
                                        value: neighborhood,
                                        touched: neighborhood_touched.clone(),
                                        error: neighborhood_error.clone(),
                                        attrs: attributes! {
                                            id="neighborhood"
                                            name="neighborhood"
                                            aria-describedby="neighborhood-error"
                                        }
                                    )
                                    field_error(
                                        message: neighborhood_error,
                                        attrs: attributes! { id="neighborhood-error" }
                                    )
                                )
                                field(
                                    attrs: attributes! {
                                        :data-invalid=$( (!city_error.is_empty()).then_some("true") )
                                    },
                                    field_label(attrs: attributes! { for="city" }, "Cidade")
                                    input(
                                        value: city,
                                        touched: city_touched.clone(),
                                        error: city_error.clone(),
                                        attrs: attributes! {
                                            id="city"
                                            name="city"
                                            aria-describedby="city-error"
                                        }
                                    )
                                    field_error(
                                        message: city_error,
                                        attrs: attributes! { id="city-error" }
                                    )
                                )
                                field(
                                    field_label(attrs: attributes! { for="state" }, "Estado")
                                    select(
                                        attrs: attributes! { id="state" name="state" },
                                        for uf in BRAZILIAN_STATES {
                                            <option value=(uf) selected=(state == uf)>(uf)</option>
                                        }
                                    )
                                )
                                field(
                                    attrs: attributes! {
                                        :data-invalid=$( (!zip_code_error.is_empty()).then_some("true") )
                                    },
                                    field_label(attrs: attributes! { for="zip" }, "CEP")
                                    input(
                                        value: zip_code,
                                        touched: zip_code_touched.clone(),
                                        error: zip_code_error.clone(),
                                        attrs: attributes! {
                                            id="zip"
                                            name="zip_code"
                                            placeholder="49000-000"
                                            aria-describedby="zip-error"
                                        }
                                    )
                                    field_error(
                                        message: zip_code_error,
                                        attrs: attributes! { id="zip-error" }
                                    )
                                )
                            </div>
                        )
                    )

                    card(
                        card_header(
                            card_title("Observações")
                        )
                        card_content(
                            textarea(
                                attrs: attributes! { id="notes" name="notes" rows="3" },
                                (input_notes(body.as_ref()))
                            )
                        )
                    )
                </div>

                <aside class="h-fit">
                    card(
                        card_header(
                            card_title("Resumo")
                        )
                        card_content(
                            <div class="flex flex-col gap-3 text-sm">
                                for item in items {
                                    <div class="flex justify-between gap-4 text-muted-foreground">
                                        <span>(item.product_name.clone()) " × " (item.quantity)</span>
                                        <span>(format_price(item.unit_price_cents * item.quantity))</span>
                                    </div>
                                }
                            </div>
                            separator(attrs: attributes! { class="my-1" })
                            <div class="flex flex-col gap-2 text-sm">
                                <div class="flex justify-between">
                                    <span class="text-muted-foreground">"Subtotal"</span>
                                    <span>(format_price(subtotal))</span>
                                </div>
                                <div class="flex justify-between">
                                    <span class="text-muted-foreground">"Frete"</span>
                                    if shipping.1 {
                                        <span>"Grátis"</span>
                                    } else {
                                        <span>(format_price(shipping.0))</span>
                                    }
                                </div>
                                if discount > 0 {
                                    <div class="flex justify-between">
                                        <span class="text-muted-foreground">"Desconto"</span>
                                        <span>"-" (format_price(discount))</span>
                                    </div>
                                }
                                <div class="flex justify-between pt-2 text-base font-medium">
                                    <span>"Total"</span>
                                    <span class="text-primary">(format_price(total))</span>
                                </div>
                            </div>
                            field(
                                field_label(attrs: attributes! { for="coupon" }, "Cupom")
                                <div class="flex items-center gap-2">
                                    input(attrs: attributes! {
                                        id="coupon"
                                        name="coupon_code"
                                        placeholder="CUPOM10"
                                        class="min-w-0 flex-1"
                                        value=(coupon_input.clone())
                                    })
                                    button(
                                        variant: ButtonVariant::Outline,
                                        attrs: attributes! {
                                            type="submit"
                                            name="intent"
                                            value="apply"
                                            formnovalidate="formnovalidate"
                                        },
                                        "Aplicar"
                                    )
                                </div>
                                if let Some(ref message) = coupon_error {
                                    field_error((message.as_str()))
                                }
                            )
                            if let Some(ref coupon) = resolved_coupon {
                                <p class="text-sm text-muted-foreground">
                                    "Cupom " (coupon.code.as_str()) " aplicado."
                                </p>
                            }
                        )
                        card_footer(
                            button(
                                size: ButtonSize::Lg,
                                blocked: pay_blocked,
                                attrs: attributes! {
                                    type="submit"
                                    name="intent"
                                    value="pay"
                                    class="w-full"
                                },
                                "Pagar com PIX"
                            )
                        )
                    )
                    <p class="mt-4 text-xs text-muted-foreground">
                        "Pagamento via PIX. Após confirmar, você verá o QR Code na próxima tela."
                    </p>
                </aside>
            </form>
        )
    }.boxed())
}

fn phone_digit_count(value: &str) -> usize {
  value.chars().filter(|c| c.is_ascii_digit()).count()
}

fn validate_pay_fields(form_data: &CheckoutInput) -> Result<(), String> {
  if form_data.guest_email.trim().is_empty()
    || !form_data.guest_email.contains('@')
  {
    return Err("Informe um e-mail válido.".to_string());
  }
  if form_data.name.trim().is_empty() {
    return Err("Informe o nome completo.".to_string());
  }
  let digits = phone_digit_count(&form_data.phone);
  if !(10..=11).contains(&digits) {
    return Err("Informe um WhatsApp válido (DDD + número).".to_string());
  }
  if form_data.street.trim().is_empty() {
    return Err("Informe a rua.".to_string());
  }
  if form_data.number.trim().is_empty() {
    return Err("Informe o número.".to_string());
  }
  if form_data.neighborhood.trim().is_empty() {
    return Err("Informe o bairro.".to_string());
  }
  if form_data.city.trim().is_empty() {
    return Err("Informe a cidade.".to_string());
  }
  let zip_digits: String = form_data
    .zip_code
    .chars()
    .filter(|c| c.is_ascii_digit())
    .collect();
  if zip_digits.is_empty() {
    return Err("Informe o CEP.".to_string());
  }
  Ok(())
}

#[derive(Debug)]
struct CreatedOrder {
  id: Uuid,
  access_token: Uuid,
}

async fn create_order(
  pool: &PgPool,
  cart_id: Uuid,
  form_data: CheckoutInput,
  user_id: Option<Uuid>,
) -> Result<CreatedOrder, CheckoutError> {
  if let Err(message) = validate_pay_fields(&form_data) {
    return Err(CheckoutError::Form(message));
  }

  let settings = crate::app::store::queries::get_store_settings(pool).await?;
  let order_id = Uuid::now_v7();
  let access_token = Uuid::new_v4();
  let zip_digits: String = form_data
    .zip_code
    .chars()
    .filter(|c| c.is_ascii_digit())
    .collect();

  let mut tx = pool.begin().await?;
  let outcome = create_order_in_tx(
    &mut tx,
    &settings,
    CreateOrderParams {
      order_id,
      access_token,
      cart_id,
      form_data,
      user_id,
      zip_digits: &zip_digits,
    },
  )
  .await;
  match outcome {
    Ok(()) => {
      tx.commit().await?;
      Ok(CreatedOrder {
        id: order_id,
        access_token,
      })
    }
    Err(error) => {
      let _ = tx.rollback().await;
      Err(error)
    }
  }
}

struct CreateOrderParams<'a> {
  order_id: Uuid,
  access_token: Uuid,
  cart_id: Uuid,
  form_data: CheckoutInput,
  user_id: Option<Uuid>,
  zip_digits: &'a str,
}

async fn create_order_in_tx(
  tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  settings: &crate::app::store::queries::StoreSettings,
  params: CreateOrderParams<'_>,
) -> Result<(), CheckoutError> {
  let CreateOrderParams {
    order_id,
    access_token,
    cart_id,
    form_data,
    user_id,
    zip_digits,
  } = params;
  release_expired_reservations_in_tx(tx).await?;
  let items = quote_cart_for_checkout(tx, cart_id)
    .await
    .map_err(|error| match error {
      QuoteError::Empty => {
        CheckoutError::Form("Seu carrinho está vazio.".to_string())
      }
      QuoteError::Insufficient => CheckoutError::Form(
        "Estoque insuficiente para concluir o pedido.".to_string(),
      ),
      QuoteError::Db(error) => CheckoutError::Db(error),
    })?;
  let subtotal = cart_subtotal_cents(&items);
  let shipping = calculate_shipping(subtotal, &form_data.state, zip_digits);

  let mut coupon_code = form_data.coupon_code.clone().unwrap_or_default();
  if coupon_code.trim().is_empty() {
    let stored: Option<String> =
      sqlx::query_scalar("SELECT coupon_code FROM carts WHERE id = $1")
        .bind(cart_id)
        .fetch_one(&mut **tx)
        .await?;
    coupon_code = stored.unwrap_or_default();
  }
  let resolved = resolve_coupon(tx, &coupon_code, subtotal, user_id, true)
    .await
    .map_err(|outcome| match outcome {
      CouponReject::Db(error) => CheckoutError::Db(error),
      CouponReject::Invalid(message) => CheckoutError::Form(message),
    })?;
  let discount = resolved
    .as_ref()
    .map(|coupon| coupon.discount_cents)
    .unwrap_or(0);
  let coupon_id = resolved.as_ref().map(|coupon| coupon.id);
  let applied_code = resolved.as_ref().map(|coupon| coupon.code.clone());
  let total = subtotal - discount + shipping.0;

  let address = serde_json::json!({
      "name": form_data.name,
      "street": form_data.street,
      "number": form_data.number,
      "complement": form_data.complement,
      "neighborhood": form_data.neighborhood,
      "city": form_data.city,
      "state": form_data.state,
      "zip_code": zip_digits,
      "phone": form_data.phone,
  });

  let trimmed_notes = form_data.notes.trim().to_string();
  let notes: Option<String> = if trimmed_notes.is_empty() {
    None
  } else {
    Some(trimmed_notes)
  };

  sqlx::query(
        "INSERT INTO orders (id, user_id, guest_email, status, subtotal_cents, shipping_cents, discount_cents, total_cents, shipping_address, notes, coupon_id, access_token)
         VALUES ($1, $2, $3, 'pending_payment', $4, $5, $6, $7, $8, $9, $10, $11)",
    )
    .bind(order_id)
    .bind(user_id)
    .bind(form_data.guest_email)
    .bind(subtotal)
    .bind(shipping.0)
    .bind(discount)
    .bind(total)
    .bind(address)
    .bind(notes.as_deref())
    .bind(coupon_id)
    .bind(access_token)
    .execute(&mut **tx)
    .await?;

  let mut reserve_lines = Vec::new();
  for item in &items {
    let variant_id = Uuid::parse_str(&item.variant_id).map_err(|_| {
      CheckoutError::Form("Carrinho inválido. Atualize a página.".to_string())
    })?;
    sqlx::query(
            "INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
        )
        .bind(Uuid::now_v7())
        .bind(order_id)
        .bind(variant_id)
        .bind(&item.product_name)
        .bind(&item.variant_label)
        .bind(item.quantity)
        .bind(item.unit_price_cents)
        .execute(&mut **tx)
        .await?;
    reserve_lines.push(ReserveLine {
      variant_id,
      quantity: item.quantity,
    });
  }

  reserve_for_order(tx, order_id, &reserve_lines)
    .await
    .map_err(|error| match error {
      crate::app::store::inventory::InventoryError::Insufficient => {
        CheckoutError::Form(
          "Estoque insuficiente para concluir o pedido.".to_string(),
        )
      }
      crate::app::store::inventory::InventoryError::Db(error) => {
        CheckoutError::Db(error)
      }
    })?;

  if let Some(coupon) = resolved.as_ref() {
    sqlx::query(
      "INSERT INTO coupon_redemptions (id, coupon_id, user_id, order_id)
             VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(coupon.id)
    .bind(user_id)
    .bind(order_id)
    .execute(&mut **tx)
    .await?;
  }

  let pix_code = generate_pix_payload(
    settings.pix_key.as_deref().unwrap_or(""),
    settings.pix_key_type.as_deref().unwrap_or("phone"),
    settings
      .pix_merchant_name
      .as_deref()
      .unwrap_or("Linda Flor"),
    settings.pix_merchant_city.as_deref().unwrap_or("Aracaju"),
    total,
    &order_id.to_string(),
  );

  let payment_meta = serde_json::json!({
      "pix_copy_paste": pix_code,
      "total_cents": total,
      "coupon_code": applied_code,
      "coupon_id": coupon_id,
      "discount_cents": discount,
  });

  sqlx::query("UPDATE orders SET payment_meta = $1 WHERE id = $2")
    .bind(payment_meta)
    .bind(order_id)
    .execute(&mut **tx)
    .await?;

  clear_items_in_tx(tx, cart_id).await?;

  Ok(())
}

fn calculate_shipping(
  subtotal_cents: i32,
  state: &str,
  zip_digits: &str,
) -> (i32, bool) {
  const FREE_SHIPPING_THRESHOLD_CENTS: i32 = 29_900;
  const SHIPPING_SE_CENTS: i32 = 1_990;
  const SHIPPING_DEFAULT_CENTS: i32 = 3_990;
  const SHIPPING_REMOTE_CENTS: i32 = 5_990;

  if subtotal_cents >= FREE_SHIPPING_THRESHOLD_CENTS {
    return (0, true);
  }

  let is_remote = zip_digits.len() >= 5
    && !zip_digits.starts_with("49")
    && (zip_digits.starts_with("69")
      || zip_digits.starts_with("68")
      || zip_digits.starts_with("66")
      || zip_digits.starts_with("78")
      || zip_digits.starts_with("79"));

  let shipping_cents = if state == "SE" {
    SHIPPING_SE_CENTS
  } else if is_remote {
    SHIPPING_REMOTE_CENTS
  } else {
    SHIPPING_DEFAULT_CENTS
  };

  (shipping_cents, false)
}

fn emv_field(id: &str, value: &str) -> String {
  format!("{}{}{}", id, value.len(), value)
}

fn crc16_ccitt_false(payload: &str) -> String {
  let mut crc: u16 = 0xffff;
  for b in payload.bytes() {
    crc ^= (b as u16) << 8;
    for _ in 0..8 {
      if (crc & 0x8000) != 0 {
        crc = (crc << 1) ^ 0x1021;
      } else {
        crc <<= 1;
      }
    }
  }
  format!("{:04X}", crc)
}

fn sanitize_merchant_field(value: &str, max_length: usize) -> String {
  value
    .chars()
    .filter(|c| c.is_alphanumeric() || *c == ' ')
    .take(max_length)
    .collect::<String>()
    .trim()
    .to_uppercase()
}

fn generate_pix_payload(
  pix_key: &str,
  pix_key_type: &str,
  merchant_name: &str,
  merchant_city: &str,
  amount_cents: i32,
  order_id: &str,
) -> String {
  let key = match pix_key_type {
    "cpf" | "cnpj" | "phone" => {
      pix_key.chars().filter(|c| c.is_ascii_digit()).collect()
    }
    "email" => pix_key.trim().to_lowercase(),
    _ => pix_key.trim().to_string(),
  };
  let merchant_name = sanitize_merchant_field(merchant_name, 25);
  let merchant_city = sanitize_merchant_field(merchant_city, 15);
  let txid = order_id
    .replace('-', "")
    .chars()
    .take(25)
    .collect::<String>();
  let amount = format!("{:.2}", amount_cents as f64 / 100.0);

  let merchant_account =
    emv_field("00", "br.gov.bcb.pix") + &emv_field("01", key.as_str());
  let additional_data = emv_field("05", &txid);

  let payload_without_crc = emv_field("00", "01")
    + &emv_field("26", &merchant_account)
    + &emv_field("52", "0000")
    + &emv_field("53", "986")
    + &emv_field("54", &amount)
    + &emv_field("58", "BR")
    + &emv_field("59", &merchant_name)
    + &emv_field("60", &merchant_city)
    + &emv_field("62", &additional_data)
    + "6304";

  format!(
    "{}{}",
    payload_without_crc,
    crc16_ccitt_false(&payload_without_crc)
  )
}

fn input_email(body: Option<&Form<CheckoutInput>>) -> String {
  body.map(|b| b.0.guest_email.clone()).unwrap_or_default()
}

fn input_name(body: Option<&Form<CheckoutInput>>) -> String {
  body.map(|b| b.0.name.clone()).unwrap_or_default()
}

fn input_phone(body: Option<&Form<CheckoutInput>>) -> String {
  body.map(|b| b.0.phone.clone()).unwrap_or_default()
}

fn input_street(body: Option<&Form<CheckoutInput>>) -> String {
  body.map(|b| b.0.street.clone()).unwrap_or_default()
}

fn input_number(body: Option<&Form<CheckoutInput>>) -> String {
  body.map(|b| b.0.number.clone()).unwrap_or_default()
}

fn input_complement(body: Option<&Form<CheckoutInput>>) -> String {
  body.map(|b| b.0.complement.clone()).unwrap_or_default()
}

fn input_neighborhood(body: Option<&Form<CheckoutInput>>) -> String {
  body.map(|b| b.0.neighborhood.clone()).unwrap_or_default()
}

fn input_city(body: Option<&Form<CheckoutInput>>) -> String {
  body.map(|b| b.0.city.clone()).unwrap_or_default()
}

fn input_state(body: Option<&Form<CheckoutInput>>) -> String {
  body
    .map(|b| b.0.state.clone())
    .unwrap_or_else(|| "SE".to_string())
}

fn input_zip(body: Option<&Form<CheckoutInput>>) -> String {
  body.map(|b| b.0.zip_code.clone()).unwrap_or_default()
}

fn input_zip_digits(body: Option<&Form<CheckoutInput>>) -> String {
  input_zip(body)
    .chars()
    .filter(|c| c.is_ascii_digit())
    .collect()
}

fn input_notes(body: Option<&Form<CheckoutInput>>) -> String {
  body.map(|b| b.0.notes.clone()).unwrap_or_default()
}

fn input_coupon(
  body: Option<&Form<CheckoutInput>>,
  stored: Option<&str>,
) -> String {
  if let Some(posted) = body.and_then(|b| b.0.coupon_code.clone())
    && !posted.trim().is_empty()
  {
    return posted;
  }
  stored.unwrap_or_default().to_string()
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app::store::cart::set_cart_coupon;
  use crate::app::store::inventory::{
    convert_reservation, release_expired_reservations, release_reservation,
  };

  const UNIT_PRICE_CENTS: i32 = 10_000;
  const DISCOUNT_CENTS: i32 = 1_500;

  fn checkout_input(email: &str, code: &str) -> CheckoutInput {
    CheckoutInput {
      guest_email: email.to_string(),
      name: "Maria Silva".to_string(),
      phone: "79999816511".to_string(),
      street: "Rua da Praia".to_string(),
      number: "10".to_string(),
      complement: String::new(),
      neighborhood: "Centro".to_string(),
      city: "Aracaju".to_string(),
      state: "SE".to_string(),
      zip_code: "49000-000".to_string(),
      notes: String::new(),
      coupon_code: Some(code.to_string()),
      intent: Some("pay".to_string()),
    }
  }

  async fn seed_variant(pool: &PgPool, price: i32, stock: i32) -> (Uuid, Uuid) {
    let product_id = Uuid::now_v7();
    let variant_id = Uuid::now_v7();
    let warehouse_id = Uuid::now_v7();

    sqlx::query(
      "INSERT INTO products (id, name, slug, price_in_cents)
       VALUES ($1, 'Peça teste', $2, $3)",
    )
    .bind(product_id)
    .bind(format!("peca-{product_id}"))
    .bind(price)
    .execute(pool)
    .await
    .expect("insert product");

    sqlx::query(
      "INSERT INTO product_variants (id, product_id, sku, size, color)
       VALUES ($1, $2, $3, 'm', 'azul')",
    )
    .bind(variant_id)
    .bind(product_id)
    .bind(format!("sku-{variant_id}"))
    .execute(pool)
    .await
    .expect("insert variant");

    sqlx::query(
      "INSERT INTO warehouses (id, code, name, is_default, active)
       VALUES ($1, $2, 'Principal', true, true)",
    )
    .bind(warehouse_id)
    .bind(format!("wh-{}", warehouse_id.simple()))
    .execute(pool)
    .await
    .expect("insert warehouse");

    sqlx::query(
      "INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved)
       VALUES ($1, $2, $3, $4, 0)",
    )
    .bind(Uuid::now_v7())
    .bind(variant_id)
    .bind(warehouse_id)
    .bind(stock)
    .execute(pool)
    .await
    .expect("insert inventory");

    (product_id, variant_id)
  }

  async fn seed_cart(pool: &PgPool, variant_id: Uuid, quantity: i32) -> Uuid {
    let cart_id = Uuid::now_v7();
    sqlx::query("INSERT INTO carts (id, token) VALUES ($1, $2)")
      .bind(cart_id)
      .bind(Uuid::now_v7())
      .execute(pool)
      .await
      .expect("insert cart");
    sqlx::query(
      "INSERT INTO cart_items (id, cart_id, variant_id, quantity)
       VALUES ($1, $2, $3, $4)",
    )
    .bind(Uuid::now_v7())
    .bind(cart_id)
    .bind(variant_id)
    .bind(quantity)
    .execute(pool)
    .await
    .expect("insert cart item");
    cart_id
  }

  #[tokio::test]
  async fn create_order_redeems_unique_coupon_once() {
    let pool = crate::test_support::fresh_pool().await;
    let (_product_id, variant_id) =
      seed_variant(&pool, UNIT_PRICE_CENTS, 5).await;
    let coupon_id = Uuid::now_v7();
    let code = format!("Pay{}", Uuid::now_v7().simple());
    let email = format!("checkout-{}@example.com", Uuid::now_v7().simple());

    sqlx::query(
      "INSERT INTO coupons (id, code, discount_type, discount_value, usage_type)
       VALUES ($1, $2, 'fixed', $3, 'unique')",
    )
    .bind(coupon_id)
    .bind(&code)
    .bind(DISCOUNT_CENTS)
    .execute(&pool)
    .await
    .expect("insert coupon");

    let cart_id = seed_cart(&pool, variant_id, 1).await;
    let subtotal = UNIT_PRICE_CENTS;
    let zip_digits = "49000000";
    let (shipping_cents, _) = calculate_shipping(subtotal, "SE", zip_digits);
    let expected_total = subtotal - DISCOUNT_CENTS + shipping_cents;

    let order_id = create_order(
      &pool,
      cart_id,
      checkout_input(&email, &code.to_lowercase()),
      None,
    )
    .await
    .expect("create order")
    .id;

    #[derive(sqlx::FromRow)]
    struct SavedOrder {
      coupon_id: Option<Uuid>,
      discount_cents: i32,
      total_cents: i32,
      subtotal_cents: i32,
      shipping_cents: i32,
    }

    let saved = sqlx::query_as::<_, SavedOrder>(
      "SELECT coupon_id, discount_cents, total_cents, subtotal_cents, shipping_cents
       FROM orders WHERE id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .expect("load order");

    assert_eq!(saved.coupon_id, Some(coupon_id));
    assert_eq!(saved.discount_cents, DISCOUNT_CENTS);
    assert_eq!(saved.subtotal_cents, subtotal);
    assert_eq!(saved.shipping_cents, shipping_cents);
    assert_eq!(
      saved.total_cents,
      saved.subtotal_cents - saved.discount_cents + saved.shipping_cents
    );
    assert_eq!(saved.total_cents, expected_total);

    let coupon_code: Option<String> = sqlx::query_scalar(
      "SELECT payment_meta->>'coupon_code' FROM orders WHERE id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .expect("coupon code");
    assert_eq!(coupon_code.as_deref(), Some(code.as_str()));

    let redemptions: i64 = sqlx::query_scalar(
      "SELECT COUNT(*) FROM coupon_redemptions WHERE coupon_id = $1",
    )
    .bind(coupon_id)
    .fetch_one(&pool)
    .await
    .expect("count redemptions");
    assert_eq!(redemptions, 1);

    let second_cart = seed_cart(&pool, variant_id, 1).await;
    let second = create_order(
      &pool,
      second_cart,
      checkout_input(&email, &code.to_lowercase()),
      None,
    )
    .await;
    let err = second.expect_err("unique coupon is already redeemed");
    assert!(
      err.to_string().contains("limite de utilizações"),
      "unexpected error: {err}"
    );

    let orders: i64 =
      sqlx::query_scalar("SELECT COUNT(*) FROM orders WHERE coupon_id = $1")
        .bind(coupon_id)
        .fetch_one(&pool)
        .await
        .expect("count orders");
    assert_eq!(orders, 1);

    let redemptions: i64 = sqlx::query_scalar(
      "SELECT COUNT(*) FROM coupon_redemptions WHERE coupon_id = $1",
    )
    .bind(coupon_id)
    .fetch_one(&pool)
    .await
    .expect("count redemptions after retry");
    assert_eq!(redemptions, 1);
  }

  #[tokio::test]
  async fn create_order_uses_live_db_price() {
    let pool = crate::test_support::fresh_pool().await;
    let (product_id, variant_id) =
      seed_variant(&pool, UNIT_PRICE_CENTS, 2).await;
    let cart_id = seed_cart(&pool, variant_id, 1).await;
    sqlx::query("UPDATE products SET price_in_cents = $2 WHERE id = $1")
      .bind(product_id)
      .bind(24_000)
      .execute(&pool)
      .await
      .expect("bump price");

    let email = format!("price-{}@example.com", Uuid::now_v7().simple());
    let order_id =
      create_order(&pool, cart_id, checkout_input(&email, ""), None)
        .await
        .expect("create order")
        .id;

    let subtotal: i32 =
      sqlx::query_scalar("SELECT subtotal_cents FROM orders WHERE id = $1")
        .bind(order_id)
        .fetch_one(&pool)
        .await
        .expect("subtotal");
    assert_eq!(subtotal, 24_000);

    let unit: i32 = sqlx::query_scalar(
      "SELECT unit_price_cents FROM order_items WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .expect("unit price");
    assert_eq!(unit, 24_000);
  }

  #[tokio::test]
  async fn create_order_reserves_inventory() {
    let pool = crate::test_support::fresh_pool().await;
    let (_product_id, variant_id) =
      seed_variant(&pool, UNIT_PRICE_CENTS, 3).await;
    let cart_id = seed_cart(&pool, variant_id, 2).await;
    let email = format!("reserve-{}@example.com", Uuid::now_v7().simple());
    let order_id =
      create_order(&pool, cart_id, checkout_input(&email, ""), None)
        .await
        .expect("create order")
        .id;

    let reserved: i32 = sqlx::query_scalar(
      "SELECT reserved FROM inventory WHERE variant_id = $1",
    )
    .bind(variant_id)
    .fetch_one(&pool)
    .await
    .expect("reserved");
    assert_eq!(reserved, 2);

    let expires: Option<time::OffsetDateTime> = sqlx::query_scalar(
      "SELECT reservation_expires_at FROM orders WHERE id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .expect("expires");
    assert!(expires.is_some());

    let mut tx = pool.begin().await.expect("tx");
    convert_reservation(&mut tx, order_id)
      .await
      .expect("convert");
    tx.commit().await.expect("commit");

    let row = sqlx::query_as::<_, (i32, i32)>(
      "SELECT quantity, reserved FROM inventory WHERE variant_id = $1",
    )
    .bind(variant_id)
    .fetch_one(&pool)
    .await
    .expect("inventory after convert");
    assert_eq!(row, (1, 0));
  }

  #[tokio::test]
  async fn last_unit_race_only_one_order_succeeds() {
    let pool = crate::test_support::fresh_pool().await;
    let (_product_id, variant_id) =
      seed_variant(&pool, UNIT_PRICE_CENTS, 1).await;
    let cart_a = seed_cart(&pool, variant_id, 1).await;
    let cart_b = seed_cart(&pool, variant_id, 1).await;
    let email_a = format!("race-a-{}@example.com", Uuid::now_v7().simple());
    let email_b = format!("race-b-{}@example.com", Uuid::now_v7().simple());

    let (first, second) = tokio::join!(
      create_order(&pool, cart_a, checkout_input(&email_a, ""), None),
      create_order(&pool, cart_b, checkout_input(&email_b, ""), None),
    );

    let ok_count = [&first, &second].iter().filter(|r| r.is_ok()).count();
    let err_count = [&first, &second].iter().filter(|r| r.is_err()).count();
    assert_eq!(ok_count, 1, "exactly one checkout should succeed");
    assert_eq!(err_count, 1, "the other checkout should fail");
    let err = first.as_ref().err().or(second.as_ref().err()).unwrap();
    let message = err.to_string();
    assert_eq!(
      message, "Estoque insuficiente para concluir o pedido.",
      "unexpected error: {err}"
    );

    let loser_cart = if first.is_err() { cart_a } else { cart_b };
    let remaining: Option<i32> = sqlx::query_scalar(
      "SELECT quantity FROM cart_items WHERE cart_id = $1 AND variant_id = $2",
    )
    .bind(loser_cart)
    .bind(variant_id)
    .fetch_optional(&pool)
    .await
    .expect("loser cart line");
    assert_eq!(remaining, Some(1), "loser cart should still have the line");
  }

  #[tokio::test]
  async fn expired_reservation_is_released() {
    let pool = crate::test_support::fresh_pool().await;
    let (_product_id, variant_id) =
      seed_variant(&pool, UNIT_PRICE_CENTS, 2).await;
    let cart_id = seed_cart(&pool, variant_id, 1).await;
    let email = format!("expire-{}@example.com", Uuid::now_v7().simple());
    let order_id =
      create_order(&pool, cart_id, checkout_input(&email, ""), None)
        .await
        .expect("create order")
        .id;

    sqlx::query(
      "UPDATE orders SET reservation_expires_at = now() - interval '1 hour' WHERE id = $1",
    )
    .bind(order_id)
    .execute(&pool)
    .await
    .expect("expire");

    release_expired_reservations(&pool)
      .await
      .expect("release expired");

    let status: String =
      sqlx::query_scalar("SELECT status::text FROM orders WHERE id = $1")
        .bind(order_id)
        .fetch_one(&pool)
        .await
        .expect("status");
    assert_eq!(status, "cancelled");

    let reserved: i32 = sqlx::query_scalar(
      "SELECT reserved FROM inventory WHERE variant_id = $1",
    )
    .bind(variant_id)
    .fetch_one(&pool)
    .await
    .expect("reserved");
    assert_eq!(reserved, 0);
  }

  #[tokio::test]
  async fn pay_requires_whatsapp() {
    let pool = crate::test_support::fresh_pool().await;
    let (_product_id, variant_id) =
      seed_variant(&pool, UNIT_PRICE_CENTS, 2).await;
    let cart_id = seed_cart(&pool, variant_id, 1).await;
    let email = format!("phone-{}@example.com", Uuid::now_v7().simple());
    let mut posted = checkout_input(&email, "");
    posted.phone = String::new();

    let err = create_order(&pool, cart_id, posted, None)
      .await
      .expect_err("phone required");
    assert!(
      err.to_string().contains("WhatsApp"),
      "unexpected error: {err}"
    );
  }

  #[tokio::test]
  async fn stored_cart_coupon_applies_when_checkout_omits_code() {
    let pool = crate::test_support::fresh_pool().await;
    let (_product_id, variant_id) =
      seed_variant(&pool, UNIT_PRICE_CENTS, 2).await;
    let coupon_id = Uuid::now_v7();
    let code = format!("CART{}", Uuid::now_v7().simple());
    sqlx::query(
      "INSERT INTO coupons (id, code, discount_type, discount_value, usage_type)
       VALUES ($1, $2, 'fixed', $3, 'unlimited')",
    )
    .bind(coupon_id)
    .bind(&code)
    .bind(DISCOUNT_CENTS)
    .execute(&pool)
    .await
    .expect("insert coupon");

    let cart_id = seed_cart(&pool, variant_id, 1).await;
    set_cart_coupon(&pool, cart_id, Some(&code))
      .await
      .expect("persist coupon");

    let email = format!("stored-{}@example.com", Uuid::now_v7().simple());
    let order_id =
      create_order(&pool, cart_id, checkout_input(&email, ""), None)
        .await
        .expect("create order")
        .id;

    let saved_coupon: Option<Uuid> =
      sqlx::query_scalar("SELECT coupon_id FROM orders WHERE id = $1")
        .bind(order_id)
        .fetch_one(&pool)
        .await
        .expect("coupon_id");
    assert_eq!(saved_coupon, Some(coupon_id));
  }

  async fn seed_split_warehouses(
    pool: &PgPool,
    price: i32,
    default_stock: i32,
    other_stock: i32,
  ) -> (Uuid, Uuid, Uuid) {
    let product_id = Uuid::now_v7();
    let variant_id = Uuid::now_v7();
    let warehouse_a = Uuid::now_v7();
    let warehouse_b = Uuid::now_v7();
    let inventory_a = Uuid::now_v7();
    let inventory_b = Uuid::now_v7();

    sqlx::query(
      "INSERT INTO products (id, name, slug, price_in_cents)
       VALUES ($1, 'Peça teste', $2, $3)",
    )
    .bind(product_id)
    .bind(format!("peca-{product_id}"))
    .bind(price)
    .execute(pool)
    .await
    .expect("insert product");

    sqlx::query(
      "INSERT INTO product_variants (id, product_id, sku, size, color)
       VALUES ($1, $2, $3, 'm', 'azul')",
    )
    .bind(variant_id)
    .bind(product_id)
    .bind(format!("sku-{variant_id}"))
    .execute(pool)
    .await
    .expect("insert variant");

    sqlx::query(
      "INSERT INTO warehouses (id, code, name, is_default, active)
       VALUES ($1, $2, 'Principal', true, true), ($3, $4, 'Secundário', false, true)",
    )
    .bind(warehouse_a)
    .bind(format!("wh-{}", warehouse_a.simple()))
    .bind(warehouse_b)
    .bind(format!("wh-{}", warehouse_b.simple()))
    .execute(pool)
    .await
    .expect("insert warehouses");

    sqlx::query(
      "INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved)
       VALUES ($1, $2, $3, $4, 0), ($5, $2, $6, $7, 0)",
    )
    .bind(inventory_a)
    .bind(variant_id)
    .bind(warehouse_a)
    .bind(default_stock)
    .bind(inventory_b)
    .bind(warehouse_b)
    .bind(other_stock)
    .execute(pool)
    .await
    .expect("insert inventory");

    (variant_id, inventory_a, inventory_b)
  }

  #[tokio::test]
  async fn reservations_convert_and_release_by_inventory_id() {
    let pool = crate::test_support::fresh_pool().await;
    let (variant_id, inventory_a, inventory_b) =
      seed_split_warehouses(&pool, UNIT_PRICE_CENTS, 1, 1).await;
    let cart_id = seed_cart(&pool, variant_id, 2).await;
    let email = format!("rows-{}@example.com", Uuid::now_v7().simple());
    let order_id =
      create_order(&pool, cart_id, checkout_input(&email, ""), None)
        .await
        .expect("create order")
        .id;

    let reserved_ids: Vec<Uuid> = sqlx::query_scalar(
      "SELECT inventory_id FROM inventory_reservations WHERE order_id = $1 ORDER BY inventory_id",
    )
    .bind(order_id)
    .fetch_all(&pool)
    .await
    .expect("reservation rows");
    let mut expected = vec![inventory_a, inventory_b];
    expected.sort();
    let mut actual = reserved_ids;
    actual.sort();
    assert_eq!(actual, expected);

    let mut tx = pool.begin().await.expect("tx");
    convert_reservation(&mut tx, order_id)
      .await
      .expect("convert");
    tx.commit().await.expect("commit");

    let leftover: i64 = sqlx::query_scalar(
      "SELECT COUNT(*) FROM inventory_reservations WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .expect("count after convert");
    assert_eq!(leftover, 0);

    let qty_a: i32 =
      sqlx::query_scalar("SELECT quantity FROM inventory WHERE id = $1")
        .bind(inventory_a)
        .fetch_one(&pool)
        .await
        .expect("qty a");
    let qty_b: i32 =
      sqlx::query_scalar("SELECT quantity FROM inventory WHERE id = $1")
        .bind(inventory_b)
        .fetch_one(&pool)
        .await
        .expect("qty b");
    assert_eq!((qty_a, qty_b), (0, 0));

    let (variant_release, inv_r_a, inv_r_b) =
      seed_split_warehouses(&pool, UNIT_PRICE_CENTS, 1, 1).await;
    let release_cart = seed_cart(&pool, variant_release, 2).await;
    let release_email =
      format!("release-{}@example.com", Uuid::now_v7().simple());
    let release_order = create_order(
      &pool,
      release_cart,
      checkout_input(&release_email, ""),
      None,
    )
    .await
    .expect("create release order")
    .id;

    let reserved_count: i64 = sqlx::query_scalar(
      "SELECT COUNT(*) FROM inventory_reservations WHERE order_id = $1",
    )
    .bind(release_order)
    .fetch_one(&pool)
    .await
    .expect("reserved count");
    assert_eq!(reserved_count, 2);

    let mut tx = pool.begin().await.expect("tx");
    release_reservation(&mut tx, release_order)
      .await
      .expect("release");
    tx.commit().await.expect("commit");

    let leftover: i64 = sqlx::query_scalar(
      "SELECT COUNT(*) FROM inventory_reservations WHERE order_id = $1",
    )
    .bind(release_order)
    .fetch_one(&pool)
    .await
    .expect("count after release");
    assert_eq!(leftover, 0);

    let reserved_a: i32 =
      sqlx::query_scalar("SELECT reserved FROM inventory WHERE id = $1")
        .bind(inv_r_a)
        .fetch_one(&pool)
        .await
        .expect("reserved a");
    let reserved_b: i32 =
      sqlx::query_scalar("SELECT reserved FROM inventory WHERE id = $1")
        .bind(inv_r_b)
        .fetch_one(&pool)
        .await
        .expect("reserved b");
    assert_eq!((reserved_a, reserved_b), (0, 0));
  }
}
