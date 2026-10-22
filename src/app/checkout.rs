use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{content::Form, error::redirect, href, page, query_params},
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

use crate::app::store::addresses::{self, AddressInput, SavedAddress};
use crate::components::checkbox::checkbox;
use crate::components::label::label;

use crate::app::store::cart::{
  QuoteError, cart_coupon_code, cart_subtotal_cents, clear_items_in_tx,
  find_cart, hydrate_cart, quote_cart_for_checkout,
};
use crate::app::store::coupons::{
  COUPON_GONE_MESSAGE, CouponReject, ResolvedCoupon, record_redemption,
  resolve_coupon,
};
use crate::app::store::inventory::{ReserveLine, reserve_for_order};
use crate::app::store::pix::{PIX_UNAVAILABLE_MESSAGE, generate_pix_payload};
use crate::app::store::pricing::pix_discount_cents;
use crate::app::store::queries::{format_price, get_store_settings};
use crate::app::store::shipping::{
  BRAZILIAN_STATES, calculate_shipping, cep_digits, is_brazilian_state,
  resolve_shipping_uf, uf_from_cep,
};

#[derive(Debug)]
enum CheckoutError {
  Form(String),
  CouponRemoved,
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
      Self::CouponRemoved => {
        write!(f, "Cupom removido, revise o total.")
      }
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

  /// "Salvar este endereço" (signed-in buyers only).
  #[serde(default)]
  save_address: Option<String>,
}

#[query_params(error = bad_request)]
struct CheckoutQuery {
  /// Saved address id to prefill the delivery fields with.
  endereco: Option<String>,
}

#[page([GET, POST])]
pub async fn page(
  cx: &Cx,
  body: Option<Form<CheckoutInput>>,
) -> Result<impl View> {
  let pool = app_context::<PgPool>(cx);
  let session_user_id: Option<Uuid> =
    current_user_owned(cx).await?.map(|session| session.user.id);
  // Read-only lookup: no cart row for cookieless visitors (created on add).
  let cart = find_cart(cx, pool, session_user_id).await?;
  let mut items = match cart {
    Some(cart) => hydrate_cart(pool, cart.id).await?.items,
    None => Vec::new(),
  };
  let store = crate::app::utils::object_store(cx);
  for item in &mut items {
    item.image_url =
      crate::app::utils::resolve_storage_url(&store, item.image_url.as_deref())
        .await;
  }

  let Some(cart) = cart.filter(|_| !items.is_empty()) else {
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
  };

  let subtotal = cart_subtotal_cents(&items).ok_or_else(|| {
    topcoat::Error::msg("Carrinho inválido. Atualize a página.")
  })?;

  let stored_coupon = cart_coupon_code(pool, cart.id).await?;
  let coupon_input = input_coupon(body.as_ref(), stored_coupon.as_deref());
  let preview_email = body
    .as_ref()
    .map(|posted| posted.0.guest_email.as_str())
    .filter(|email| !email.trim().is_empty());

  // DB-backed coupon validation. Preview reads through a rolled-back
  // transaction; order creation re-validates inside its own transaction
  // with `SELECT ... FOR UPDATE`.
  let mut preview_tx = pool.begin().await?;
  let coupon_outcome = resolve_coupon(
    &mut preview_tx,
    &coupon_input,
    subtotal,
    session_user_id,
    preview_email,
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
    Err(CouponReject::Gone) => (None, Some(COUPON_GONE_MESSAGE.to_string())),
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
      let provider = crate::payments::PaymentProvider::from_cx(cx);
      match create_order_with(
        pool,
        &provider,
        cart.id,
        posted.clone(),
        session_user_id,
      )
      .await
      {
        Ok(created) => {
          if let Some(user_id) = session_user_id
            && posted.save_address.is_some()
          {
            save_checkout_address(pool, user_id, posted).await;
          }
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
        Err(CheckoutError::CouponRemoved) => {
          checkout_error = Some("Cupom removido, revise o total.".to_string());
        }
        Err(CheckoutError::Db(error)) => return Err(error.into()),
      }
    }
  }

  // Signed-in buyers: a fresh GET prefills the delivery fields from
  // `?endereco=` or the default saved address; a POST keeps what was typed.
  let saved_addresses = match session_user_id {
    Some(user_id) => addresses::list_for_user(pool, user_id).await?,
    None => Vec::new(),
  };
  let picked = query_params::<CheckoutQuery>(cx)?.endereco.clone();
  let prefill = body
    .is_none()
    .then(|| saved_address_prefill(&saved_addresses, picked.as_deref()))
    .flatten();
  let selected_address = prefill.as_ref().map(|(id, _)| *id);
  let prefill = prefill.map(|(_, prefilled)| Form(prefilled));
  let fill = body.as_ref().or(prefill.as_ref());
  let can_save_address = session_user_id.is_some();
  let save_address_checked = body
    .as_ref()
    .is_some_and(|posted| posted.0.save_address.is_some());

  let zip_digits: String = input_zip_digits(fill);
  let state = input_state(fill);
  let settings = get_store_settings(pool).await?;
  let pix_available = payment_available(
    &crate::payments::PaymentProvider::from_cx(cx),
    &settings,
  );
  // Preview only: until the CEP resolves, estimate from the selected state.
  // The charged rate is always derived from the CEP in `create_order`.
  let preview_uf = uf_from_cep(&zip_digits).unwrap_or_else(|| {
    if is_brazilian_state(&state) {
      state.as_str()
    } else {
      "SE"
    }
  });
  let shipping = calculate_shipping(
    subtotal,
    preview_uf,
    settings.free_shipping_threshold_cents,
  );
  let coupon_discount = resolved_coupon
    .as_ref()
    .map(|coupon| coupon.discount_cents)
    .unwrap_or(0);
  let after_coupon = subtotal.saturating_sub(coupon_discount);
  let pix_discount =
    pix_discount_cents(after_coupon, settings.pix_discount_percent);
  let total = after_coupon
    .saturating_sub(pix_discount)
    .saturating_add(shipping.0);

  let email_init = input_email(body.as_ref());
  let name_init = input_name(fill);
  let phone_init = input_phone(fill);
  let street_init = input_street(fill);
  let number_init = input_number(fill);
  let complement_init = input_complement(fill);
  let neighborhood_init = input_neighborhood(fill);
  let city_init = input_city(fill);
  let zip_init = input_zip(fill);

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
  let zip_digit_count = expr!({
    let z = zip_code.get();
    raw!(
      r#"cx.hydrate(String(${z}).replace(/\D/g, "").length)"#,
      cep_digits(&z).len() as f64
    )
  });
  let zip_code_error = expr!({
    if !zip_code_touched.get() {
      "".to_owned()
    } else if zip_code.get().trim().is_empty() {
      "Informe o CEP.".to_owned()
    } else if zip_digit_count != 8.0 {
      "Informe um CEP válido com 8 dígitos.".to_owned()
    } else {
      "".to_owned()
    }
  });

  let phone = signal(cx, || phone_init);
  let phone_touched = signal(cx, || false);
  let min_whatsapp_len = 10.0;
  let max_whatsapp_len = 11.0;
  // Count digits like the server's `phone_digit_count`, so "(79) 99981-6511"
  // passes.
  let phone_digits = expr!({
    let p = phone.get();
    raw!(
      r#"cx.hydrate(String(${p}).replace(/\D/g, "").length)"#,
      phone_digit_count(&p) as f64
    )
  });
  let phone_error = expr!({
    if !phone_touched.get() {
      "".to_owned()
    } else if phone_digits < min_whatsapp_len {
      "Informe um WhatsApp válido (DDD + número).".to_owned()
    } else if phone_digits > max_whatsapp_len {
      "Informe um WhatsApp válido (DDD + número).".to_owned()
    } else {
      "".to_owned()
    }
  });
  let pay_blocked = expr!({
    if !pix_available {
      true
    } else if guest_email.get().trim().is_empty() {
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
    } else if zip_digit_count != 8.0 {
      true
    } else if phone_digits < min_whatsapp_len {
      true
    } else if phone_digits > max_whatsapp_len {
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
            if !pix_available {
                alert(
                    variant: AlertVariant::Destructive,
                    alert_title((PIX_UNAVAILABLE_MESSAGE))
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
                                        maxlength="254"
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
                            if !saved_addresses.is_empty() {
                                <div class="flex flex-wrap items-center gap-2">
                                    <span class="text-sm text-muted-foreground">"Endereços salvos:"</span>
                                    for saved in saved_addresses.iter() {
                                        let saved_id = saved.id.to_string();
                                        <a
                                            href=(href!(page).query([("endereco", saved_id.as_str())]))
                                            class=(button_variants(
                                                if selected_address == Some(saved.id) {
                                                    ButtonVariant::Primary
                                                } else {
                                                    ButtonVariant::Outline
                                                },
                                                ButtonSize::Sm,
                                            ))
                                        >
                                            (saved_address_title(saved))
                                        </a>
                                    }
                                </div>
                            }
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
                                                maxlength="100"
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
                                                maxlength="20"
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
                                                maxlength="120"
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
                                            maxlength="20"
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
                                        maxlength="80"
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
                                            maxlength="80"
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
                                            maxlength="80"
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
                                if can_save_address {
                                    <div class="flex items-center gap-2 @sm/page:col-span-2">
                                        checkbox(attrs: attributes! {
                                            id="save_address" name="save_address" value="1"
                                            checked=(save_address_checked)
                                        })
                                        label(attrs: attributes! { for="save_address" }, "Salvar este endereço")
                                    </div>
                                }
                            </div>
                        )
                    )

                    card(
                        card_header(
                            card_title("Observações")
                        )
                        card_content(
                            textarea(
                                attrs: attributes! { id="notes" name="notes" rows="3" maxlength="500" },
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
                                        <span>(format_price(item.unit_price_cents.saturating_mul(i64::from(item.quantity))))</span>
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
                                if coupon_discount > 0 {
                                    <div class="flex justify-between">
                                        <span class="text-muted-foreground">"Desconto"</span>
                                        <span>"-" (format_price(coupon_discount))</span>
                                    </div>
                                }
                                if pix_discount > 0 {
                                    <div class="flex justify-between">
                                        <span class="text-muted-foreground">"Desconto PIX (" (settings.pix_discount_percent) "%)"</span>
                                        <span>"-" (format_price(pix_discount))</span>
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

/// Server-side caps (in characters) for the free-text checkout fields,
/// mirrored as `maxlength` on the inputs. They bound what one order can
/// store in `shipping_address`/`notes` (rendered on admin pages).
const MAX_EMAIL_LEN: usize = 254;
const MAX_NAME_LEN: usize = 100;
const MAX_PHONE_LEN: usize = 20;
const MAX_STREET_LEN: usize = 120;
const MAX_NUMBER_LEN: usize = 20;
const MAX_COMPLEMENT_LEN: usize = 80;
const MAX_NEIGHBORHOOD_LEN: usize = 80;
const MAX_CITY_LEN: usize = 80;
const MAX_NOTES_LEN: usize = 500;

fn check_field_lengths(form_data: &CheckoutInput) -> Result<(), String> {
  for (value, max, what) in [
    (&form_data.guest_email, MAX_EMAIL_LEN, "O e-mail"),
    (&form_data.name, MAX_NAME_LEN, "O nome"),
    (&form_data.phone, MAX_PHONE_LEN, "O WhatsApp"),
    (&form_data.street, MAX_STREET_LEN, "A rua"),
    (&form_data.number, MAX_NUMBER_LEN, "O número"),
    (&form_data.complement, MAX_COMPLEMENT_LEN, "O complemento"),
    (&form_data.neighborhood, MAX_NEIGHBORHOOD_LEN, "O bairro"),
    (&form_data.city, MAX_CITY_LEN, "A cidade"),
    (&form_data.notes, MAX_NOTES_LEN, "As observações"),
  ] {
    if value.trim().chars().count() > max {
      let verb = if what.starts_with("As ") {
        "devem"
      } else {
        "deve"
      };
      return Err(format!("{what} {verb} ter no máximo {max} caracteres."));
    }
  }
  Ok(())
}

#[cfg(test)]
mod field_length_tests {
  use super::*;

  fn valid_input() -> CheckoutInput {
    CheckoutInput {
      guest_email: "cliente@example.com".into(),
      name: "Maria Silva".into(),
      phone: "79999816511".into(),
      street: "Rua das Flores".into(),
      number: "10".into(),
      complement: String::new(),
      neighborhood: "Centro".into(),
      city: "Aracaju".into(),
      state: "SE".into(),
      zip_code: "49000000".into(),
      notes: String::new(),
      coupon_code: None,
      intent: Some("pay".into()),
      save_address: None,
    }
  }

  #[test]
  fn accepts_values_at_the_limit() {
    let mut form = valid_input();
    form.name = "a".repeat(MAX_NAME_LEN);
    form.street = "r".repeat(MAX_STREET_LEN);
    form.complement = "ç".repeat(MAX_COMPLEMENT_LEN);
    form.notes = "n".repeat(MAX_NOTES_LEN);
    assert_eq!(check_field_lengths(&form), Ok(()));
  }

  #[test]
  fn rejects_each_oversized_field_with_pt_br_message() {
    type Setter = fn(&mut CheckoutInput, String);
    let cases: [(Setter, usize, &str); 9] = [
      (|i, v| i.guest_email = v, MAX_EMAIL_LEN, "O e-mail deve"),
      (|i, v| i.name = v, MAX_NAME_LEN, "O nome deve"),
      (|i, v| i.phone = v, MAX_PHONE_LEN, "O WhatsApp deve"),
      (|i, v| i.street = v, MAX_STREET_LEN, "A rua deve"),
      (|i, v| i.number = v, MAX_NUMBER_LEN, "O número deve"),
      (
        |i, v| i.complement = v,
        MAX_COMPLEMENT_LEN,
        "O complemento deve",
      ),
      (
        |i, v| i.neighborhood = v,
        MAX_NEIGHBORHOOD_LEN,
        "O bairro deve",
      ),
      (|i, v| i.city = v, MAX_CITY_LEN, "A cidade deve"),
      (|i, v| i.notes = v, MAX_NOTES_LEN, "As observações devem"),
    ];
    for (set, max, prefix) in cases {
      let mut form = valid_input();
      set(&mut form, "x".repeat(max + 1));
      let message = check_field_lengths(&form).expect_err("too long");
      assert_eq!(message, format!("{prefix} ter no máximo {max} caracteres."));
      // Enforced on the pay path, before any other validation.
      assert_eq!(validate_pay_fields(&form), Err(message));
    }
  }
}

fn phone_digit_count(value: &str) -> usize {
  value.chars().filter(|c| c.is_ascii_digit()).count()
}

/// Returns the shipping UF derived from the CEP.
fn validate_pay_fields(
  form_data: &CheckoutInput,
) -> Result<&'static str, String> {
  check_field_lengths(form_data)?;
  if crate::auth::validation::validate_email(&form_data.guest_email).is_err() {
    return Err("Informe um e-mail válido.".to_string());
  }
  if crate::auth::validation::validate_name(&form_data.name).is_err() {
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
  resolve_shipping_uf(&form_data.zip_code, &form_data.state)
}

#[derive(Debug)]
struct CreatedOrder {
  id: Uuid,
  access_token: Uuid,
}

/// Static-PIX order creation (the pre-PSP path the tests exercise).
#[cfg(test)]
async fn create_order(
  pool: &PgPool,
  cart_id: Uuid,
  form_data: CheckoutInput,
  user_id: Option<Uuid>,
) -> Result<CreatedOrder, CheckoutError> {
  create_order_with(
    pool,
    &crate::payments::PaymentProvider::StaticPix,
    cart_id,
    form_data,
    user_id,
  )
  .await
}

async fn create_order_with(
  pool: &PgPool,
  provider: &crate::payments::PaymentProvider,
  cart_id: Uuid,
  form_data: CheckoutInput,
  user_id: Option<Uuid>,
) -> Result<CreatedOrder, CheckoutError> {
  let shipping_uf =
    validate_pay_fields(&form_data).map_err(CheckoutError::Form)?;

  let settings = crate::app::store::queries::get_store_settings(pool).await?;
  let provider_kind = provider.kind();
  match provider_kind {
    crate::payments::ProviderKind::StaticPix if !pix_configured(&settings) => {
      return Err(CheckoutError::Form(PIX_UNAVAILABLE_MESSAGE.to_string()));
    }
    // Circuit open: fail fast before holding stock for an unpayable order.
    crate::payments::ProviderKind::MercadoPago
      if !provider.accepting_charges() =>
    {
      return Err(CheckoutError::Form(
        crate::payments::PAYMENT_UNAVAILABLE_MESSAGE.to_string(),
      ));
    }
    _ => {}
  }
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
      shipping_uf,
      provider_kind,
    },
  )
  .await;
  match outcome {
    Ok(()) => {
      tx.commit().await?;
      // Charge after commit (no PSP call while holding row locks). On
      // failure the order page retries with the same idempotency key.
      if provider_kind == crate::payments::ProviderKind::MercadoPago
        && let Err(error) =
          crate::payments::service::ensure_charge(pool, provider, order_id)
            .await
      {
        tracing::warn!(%order_id, %error, "charge creation deferred");
      }
      Ok(CreatedOrder {
        id: order_id,
        access_token,
      })
    }
    Err(error) => {
      let _ = tx.rollback().await;
      if matches!(error, CheckoutError::CouponRemoved) {
        crate::app::store::cart::set_cart_coupon(pool, cart_id, None).await?;
      }
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
  /// Derived from the CEP; the posted `state` is never trusted for pricing.
  shipping_uf: &'static str,
  provider_kind: crate::payments::ProviderKind,
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
    shipping_uf,
    provider_kind,
  } = params;
  // No global expiry sweep here (the background sweeper owns it); the quote
  // only reclaims expired holds on this cart's own inventory rows.
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
  let subtotal = cart_subtotal_cents(&items).ok_or_else(|| {
    CheckoutError::Form("Carrinho inválido. Atualize a página.".to_string())
  })?;
  let shipping = calculate_shipping(
    subtotal,
    shipping_uf,
    settings.free_shipping_threshold_cents,
  );

  let mut coupon_code = form_data.coupon_code.clone().unwrap_or_default();
  if coupon_code.trim().is_empty() {
    let stored: Option<String> =
      sqlx::query_scalar("SELECT coupon_code FROM carts WHERE id = $1")
        .bind(cart_id)
        .fetch_one(&mut **tx)
        .await?;
    coupon_code = stored.unwrap_or_default();
  }
  let resolved = match resolve_coupon(
    tx,
    &coupon_code,
    subtotal,
    user_id,
    Some(form_data.guest_email.as_str()),
    true,
  )
  .await
  {
    Ok(resolved) => resolved,
    Err(CouponReject::Db(error)) => return Err(CheckoutError::Db(error)),
    Err(CouponReject::Gone) => return Err(CheckoutError::CouponRemoved),
    Err(CouponReject::Invalid(message)) => {
      return Err(CheckoutError::Form(message));
    }
  };
  let discount = resolved
    .as_ref()
    .map(|coupon| coupon.discount_cents)
    .unwrap_or(0);
  let coupon_id = resolved.as_ref().map(|coupon| coupon.id);
  let applied_code = resolved.as_ref().map(|coupon| coupon.code.clone());
  let after_coupon = subtotal.saturating_sub(discount);
  let pix_discount =
    pix_discount_cents(after_coupon, settings.pix_discount_percent);
  let total = after_coupon
    .checked_sub(pix_discount)
    .and_then(|n| n.checked_add(shipping.0))
    .ok_or_else(|| {
      CheckoutError::Form("Carrinho inválido. Atualize a página.".to_string())
    })?;

  let address = serde_json::json!({
      "name": form_data.name,
      "street": form_data.street,
      "number": form_data.number,
      "complement": form_data.complement,
      "neighborhood": form_data.neighborhood,
      "city": form_data.city,
      "state": shipping_uf,
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
        "INSERT INTO orders (id, user_id, guest_email, status, subtotal_cents, shipping_cents, discount_cents, pix_discount_cents, total_cents, shipping_address, notes, coupon_id, access_token)
         VALUES ($1, $2, $3, 'pending_payment', $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(order_id)
    .bind(user_id)
    .bind(&form_data.guest_email)
    .bind(subtotal)
    .bind(shipping.0)
    .bind(discount)
    .bind(pix_discount)
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
    let set_id: Option<Uuid> = match item.set_id.as_deref() {
      None => None,
      Some(raw) => Some(Uuid::parse_str(raw).map_err(|_| {
        CheckoutError::Form("Carrinho inválido. Atualize a página.".to_string())
      })?),
    };
    sqlx::query(
            "INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents, set_id, set_name)
             VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)",
        )
        .bind(Uuid::now_v7())
        .bind(order_id)
        .bind(variant_id)
        .bind(&item.product_name)
        .bind(&item.variant_label)
        .bind(item.quantity)
        .bind(item.unit_price_cents)
        .bind(set_id)
        .bind(item.set_name.as_deref())
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
    let guest_email =
      user_id.is_none().then_some(form_data.guest_email.as_str());
    record_redemption(tx, coupon, user_id, order_id, guest_email).await?;
  }

  // Mercado Pago orders get their PIX code from the PSP after commit.
  let pix_code = if provider_kind == crate::payments::ProviderKind::StaticPix {
    Some(
      generate_pix_payload(
        settings.pix_key.as_deref().unwrap_or(""),
        settings.pix_key_type.as_deref().unwrap_or("phone"),
        settings
          .pix_merchant_name
          .as_deref()
          .unwrap_or("Linda Flor"),
        settings.pix_merchant_city.as_deref().unwrap_or("Aracaju"),
        total,
        &order_id.to_string(),
      )
      .map_err(|message| CheckoutError::Form(message.to_string()))?,
    )
  } else {
    None
  };

  let payment_meta = serde_json::json!({
      "pix_copy_paste": pix_code,
      "total_cents": total,
      "coupon_code": applied_code,
      "coupon_id": coupon_id,
      "discount_cents": discount,
  });

  sqlx::query(
    "UPDATE orders SET payment_meta = $1, payment_provider = $3 WHERE id = $2",
  )
  .bind(payment_meta)
  .bind(order_id)
  .bind(provider_kind.as_str())
  .execute(&mut **tx)
  .await?;

  clear_items_in_tx(tx, cart_id).await?;
  crate::mail::orders::enqueue_order_created(tx, order_id).await?;

  Ok(())
}

/// Whether checkout can take payment: Mercado Pago (breaker closed) or a
/// configured static PIX key.
fn payment_available(
  provider: &crate::payments::PaymentProvider,
  settings: &crate::app::store::queries::StoreSettings,
) -> bool {
  match provider.kind() {
    crate::payments::ProviderKind::MercadoPago => provider.accepting_charges(),
    crate::payments::ProviderKind::StaticPix => pix_configured(settings),
  }
}

fn pix_configured(
  settings: &crate::app::store::queries::StoreSettings,
) -> bool {
  settings
    .pix_key
    .as_deref()
    .is_some_and(|key| !key.trim().is_empty())
    && settings
      .pix_key_type
      .as_deref()
      .is_some_and(|key_type| !key_type.trim().is_empty())
}

/// `(id, form)` for the picked saved address, else the default one.
fn saved_address_prefill(
  saved: &[SavedAddress],
  picked: Option<&str>,
) -> Option<(Uuid, CheckoutInput)> {
  let picked = picked.and_then(|raw| Uuid::parse_str(raw.trim()).ok());
  let address = picked
    .and_then(|id| saved.iter().find(|address| address.id == id))
    .or_else(|| saved.iter().find(|address| address.is_default))?;
  Some((
    address.id,
    CheckoutInput {
      guest_email: String::new(),
      name: address.recipient_name.clone(),
      phone: address.phone.clone(),
      street: address.street.clone(),
      number: address.number.clone(),
      complement: address.complement.clone(),
      neighborhood: address.neighborhood.clone(),
      city: address.city.clone(),
      state: address.state.clone(),
      zip_code: address.formatted_zip(),
      notes: String::new(),
      coupon_code: None,
      intent: None,
      save_address: None,
    },
  ))
}

fn saved_address_title(address: &SavedAddress) -> String {
  if address.label.is_empty() {
    format!("{}, {}", address.street, address.number)
  } else {
    address.label.clone()
  }
}

/// Best effort: the order already exists, so a failure here is only logged.
async fn save_checkout_address(
  pool: &PgPool,
  user_id: Uuid,
  posted: &CheckoutInput,
) {
  let address = AddressInput {
    label: String::new(),
    recipient_name: posted.name.clone(),
    zip_code: posted.zip_code.clone(),
    street: posted.street.clone(),
    number: posted.number.clone(),
    complement: posted.complement.clone(),
    neighborhood: posted.neighborhood.clone(),
    city: posted.city.clone(),
    state: posted.state.clone(),
    phone: posted.phone.clone(),
    is_default: None,
  };
  if let Err(error) =
    addresses::save_from_checkout(pool, user_id, &address).await
  {
    tracing::warn!(%user_id, %error, "saving checkout address failed");
  }
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
    convert_reservation, release_reservation,
  };
  use crate::app::store::order::expire_pending_reservations;

  const UNIT_PRICE_CENTS: i64 = 10_000;
  const DISCOUNT_CENTS: i64 = 1_500;

  async fn ensure_pix_settings(pool: &PgPool) {
    // Singleton upsert: parallel tests can't create a second row.
    sqlx::query(
      "INSERT INTO store_settings
        (id, pix_key, pix_key_type, pix_merchant_name, pix_merchant_city)
       VALUES (true, '79999816511', 'phone', 'Linda Flor', 'Aracaju')
       ON CONFLICT (id) DO UPDATE SET pix_key = EXCLUDED.pix_key,
         pix_key_type = EXCLUDED.pix_key_type,
         pix_merchant_name = EXCLUDED.pix_merchant_name,
         pix_merchant_city = EXCLUDED.pix_merchant_city",
    )
    .execute(pool)
    .await
    .expect("upsert pix settings");
  }

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
      save_address: None,
    }
  }

  async fn seed_variant(pool: &PgPool, price: i64, stock: i32) -> (Uuid, Uuid) {
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
    ensure_pix_settings(&pool).await;
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
    let (shipping_cents, _) = calculate_shipping(
      subtotal,
      "SE",
      crate::app::store::queries::DEFAULT_FREE_SHIPPING_THRESHOLD_CENTS,
    );
    // Default store settings apply a 5% PIX discount after the coupon.
    let expected_pix = crate::app::store::pricing::pix_discount_cents(
      subtotal - DISCOUNT_CENTS,
      5,
    );
    let expected_total =
      subtotal - DISCOUNT_CENTS - expected_pix + shipping_cents;

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
      discount_cents: i64,
      total_cents: i64,
      subtotal_cents: i64,
      shipping_cents: i64,
      pix_discount_cents: i64,
    }

    let saved = sqlx::query_as::<_, SavedOrder>(
      "SELECT coupon_id, discount_cents, total_cents, subtotal_cents, shipping_cents, pix_discount_cents
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
      saved.subtotal_cents - saved.discount_cents - saved.pix_discount_cents
        + saved.shipping_cents
    );
    assert_eq!(saved.pix_discount_cents, expected_pix);
    assert_eq!(saved.total_cents, expected_total);

    let coupon_code: Option<String> = sqlx::query_scalar(
      "SELECT payment_meta->>'coupon_code' FROM orders WHERE id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .expect("coupon code");
    assert_eq!(coupon_code.as_deref(), Some(code.as_str()));

    let redemptions: i64 =     sqlx::query_scalar(
      "SELECT COUNT(*) FROM coupon_redemptions WHERE coupon_id = $1 AND refunded_at IS NULL",
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

    let redemptions: i64 =     sqlx::query_scalar(
      "SELECT COUNT(*) FROM coupon_redemptions WHERE coupon_id = $1 AND refunded_at IS NULL",
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
    ensure_pix_settings(&pool).await;
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

    let subtotal: i64 =
      sqlx::query_scalar("SELECT subtotal_cents FROM orders WHERE id = $1")
        .bind(order_id)
        .fetch_one(&pool)
        .await
        .expect("subtotal");
    assert_eq!(subtotal, 24_000);

    let unit: i64 = sqlx::query_scalar(
      "SELECT unit_price_cents FROM order_items WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .expect("unit price");
    assert_eq!(unit, 24_000);
  }

  #[tokio::test]
  async fn create_order_queues_pix_mail() {
    let pool = crate::test_support::fresh_pool().await;
    ensure_pix_settings(&pool).await;
    let (_, variant_id) = seed_variant(&pool, UNIT_PRICE_CENTS, 2).await;
    let cart_id = seed_cart(&pool, variant_id, 1).await;
    let email = format!("mail-{}@example.com", Uuid::now_v7().simple());
    let created =
      create_order(&pool, cart_id, checkout_input(&email, ""), None)
        .await
        .expect("create order");
    let (kind, text): (String, String) = sqlx::query_as(
      "SELECT kind, text_body FROM email_outbox WHERE to_address = $1",
    )
    .bind(&email)
    .fetch_one(&pool)
    .await
    .expect("queued order mail");
    assert_eq!(kind, "order_created");
    let link = format!("/pedido/{}?t={}", created.id, created.access_token);
    assert!(text.contains(&link), "missing order link: {text}");
    assert!(text.contains("PIX copia-e-cola"));
  }

  #[tokio::test]
  async fn create_order_reserves_inventory() {
    let pool = crate::test_support::fresh_pool().await;
    ensure_pix_settings(&pool).await;
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
    ensure_pix_settings(&pool).await;
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
    // Runs the global reservation sweep: needs a database of its own.
    let pool = crate::test_support::isolated_pool().await;
    ensure_pix_settings(&pool).await;
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

    expire_pending_reservations(&pool)
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
    ensure_pix_settings(&pool).await;
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
  async fn shipping_uses_cep_uf_not_posted_state() {
    let pool = crate::test_support::fresh_pool().await;
    ensure_pix_settings(&pool).await;
    let (_product_id, variant_id) =
      seed_variant(&pool, UNIT_PRICE_CENTS, 2).await;
    let cart_id = seed_cart(&pool, variant_id, 1).await;
    let email = format!("uf-{}@example.com", Uuid::now_v7().simple());

    // Belém (PA) CEP posted as SE to get the local rate: rejected.
    let mut posted = checkout_input(&email, "");
    posted.zip_code = "66010-000".to_string();
    let err = create_order(&pool, cart_id, posted.clone(), None)
      .await
      .expect_err("state mismatch");
    assert!(err.to_string().contains("PA"), "unexpected error: {err}");

    posted.state = "PA".to_string();
    let created = create_order(&pool, cart_id, posted, None)
      .await
      .expect("order");
    let (shipping_cents, state): (i64, String) = sqlx::query_as(
      "SELECT shipping_cents, shipping_address->>'state' FROM orders WHERE id = $1",
    )
    .bind(created.id)
    .fetch_one(&pool)
    .await
    .expect("order row");
    assert_eq!(shipping_cents, 5_990);
    assert_eq!(state, "PA");
  }

  #[tokio::test]
  async fn create_order_fails_when_pix_not_configured() {
    // Own database so clearing the settings can't affect other tests.
    let pool = crate::test_support::isolated_pool().await;
    let (_product_id, variant_id) =
      seed_variant(&pool, UNIT_PRICE_CENTS, 2).await;

    // No settings row at all: no order, no stock reservation.
    let cart_id = seed_cart(&pool, variant_id, 1).await;
    let email = format!("nopix-{}@example.com", Uuid::now_v7().simple());
    let err = create_order(&pool, cart_id, checkout_input(&email, ""), None)
      .await
      .expect_err("pix not configured");
    assert_eq!(
      err.to_string(),
      crate::app::store::pix::PIX_UNAVAILABLE_MESSAGE
    );
    let orders: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orders")
      .fetch_one(&pool)
      .await
      .expect("count orders");
    assert_eq!(orders, 0);
    let reserved: i32 = sqlx::query_scalar(
      "SELECT reserved FROM inventory WHERE variant_id = $1",
    )
    .bind(variant_id)
    .fetch_one(&pool)
    .await
    .expect("reserved");
    assert_eq!(reserved, 0);

    // Key without a type is still not configured.
    sqlx::query(
      "INSERT INTO store_settings (pix_key, pix_key_type)
       VALUES ('79999816511', NULL)",
    )
    .execute(&pool)
    .await
    .expect("insert partial settings");
    let cart_id = seed_cart(&pool, variant_id, 1).await;
    let email = format!("nopix-{}@example.com", Uuid::now_v7().simple());
    let err = create_order(&pool, cart_id, checkout_input(&email, ""), None)
      .await
      .expect_err("pix type missing");
    assert_eq!(
      err.to_string(),
      crate::app::store::pix::PIX_UNAVAILABLE_MESSAGE
    );
    let orders: i64 = sqlx::query_scalar("SELECT COUNT(*) FROM orders")
      .fetch_one(&pool)
      .await
      .expect("count orders");
    assert_eq!(orders, 0);
  }

  #[tokio::test]
  async fn stored_cart_coupon_applies_when_checkout_omits_code() {
    let pool = crate::test_support::fresh_pool().await;
    ensure_pix_settings(&pool).await;
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

  #[tokio::test]
  async fn stale_coupon_clears_cart_instead_of_hard_fail() {
    let pool = crate::test_support::fresh_pool().await;
    ensure_pix_settings(&pool).await;
    let (_product_id, variant_id) =
      seed_variant(&pool, UNIT_PRICE_CENTS, 2).await;
    let cart_id = seed_cart(&pool, variant_id, 1).await;
    set_cart_coupon(&pool, cart_id, Some("GONE-NOW"))
      .await
      .expect("persist stale");
    let email = format!("stale-{}@example.com", Uuid::now_v7().simple());
    let err = create_order(&pool, cart_id, checkout_input(&email, ""), None)
      .await
      .expect_err("stale coupon");
    assert!(
      err.to_string().contains("Cupom removido"),
      "unexpected: {err}"
    );
    let stored: Option<String> =
      sqlx::query_scalar("SELECT coupon_code FROM carts WHERE id = $1")
        .bind(cart_id)
        .fetch_one(&pool)
        .await
        .expect("cart coupon");
    assert!(stored.is_none());
  }

  #[tokio::test]
  async fn expiry_restores_unique_coupon() {
    // Runs the global reservation sweep: needs a database of its own.
    let pool = crate::test_support::isolated_pool().await;
    ensure_pix_settings(&pool).await;
    let (_product_id, variant_id) =
      seed_variant(&pool, UNIT_PRICE_CENTS, 2).await;
    let coupon_id = Uuid::now_v7();
    let code = format!("EXP{}", Uuid::now_v7().simple());
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
    let email = format!("expc-{}@example.com", Uuid::now_v7().simple());
    let order_id =
      create_order(&pool, cart_id, checkout_input(&email, &code), None)
        .await
        .expect("create")
        .id;
    sqlx::query(
      "UPDATE orders SET reservation_expires_at = now() - interval '1 hour' WHERE id = $1",
    )
    .bind(order_id)
    .execute(&pool)
    .await
    .expect("expire");
    expire_pending_reservations(&pool).await.expect("sweep");
    let refunded: Option<time::OffsetDateTime> = sqlx::query_scalar(
      "SELECT refunded_at FROM coupon_redemptions WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .expect("redemption");
    assert!(refunded.is_some());
    let cart_id = seed_cart(&pool, variant_id, 1).await;
    create_order(&pool, cart_id, checkout_input(&email, &code), None)
      .await
      .expect("coupon reusable after expiry");
  }

  async fn seed_split_warehouses(
    pool: &PgPool,
    price: i64,
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
    ensure_pix_settings(&pool).await;
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

  #[test]
  fn saved_address_prefill_prefers_picked_then_default() {
    let address = |name: &str, is_default: bool| SavedAddress {
      id: Uuid::now_v7(),
      label: name.to_string(),
      recipient_name: format!("Quem recebe {name}"),
      zip_code: "49000000".to_string(),
      street: "Rua A".to_string(),
      number: "1".to_string(),
      complement: String::new(),
      neighborhood: "Centro".to_string(),
      city: "Aracaju".to_string(),
      state: "SE".to_string(),
      phone: "79999816511".to_string(),
      is_default,
    };
    let saved = vec![address("Casa", true), address("Trabalho", false)];
    let (id, prefilled) = saved_address_prefill(&saved, None).unwrap();
    assert_eq!(id, saved[0].id);
    assert_eq!(prefilled.zip_code, "49000-000");
    let picked = saved[1].id.to_string();
    let (id, prefilled) = saved_address_prefill(&saved, Some(&picked)).unwrap();
    assert_eq!(id, saved[1].id);
    assert_eq!(prefilled.name, "Quem recebe Trabalho");
    // Unknown ids (e.g. another user's address) fall back to the default.
    let foreign = Uuid::now_v7().to_string();
    let (id, _) = saved_address_prefill(&saved, Some(&foreign)).unwrap();
    assert_eq!(id, saved[0].id);
    assert!(saved_address_prefill(&[], None).is_none());
  }

  #[tokio::test]
  async fn mercadopago_checkout_charges_server_total() {
    let pool = crate::test_support::fresh_pool().await;
    let mock = crate::payments::test_mock::MockMp::start().await;
    let provider = mock.provider();
    let (_product_id, variant_id) =
      seed_variant(&pool, UNIT_PRICE_CENTS, 3).await;
    let cart_id = seed_cart(&pool, variant_id, 2).await;
    let email = format!("mp-{}@example.com", Uuid::now_v7().simple());
    let order_id = create_order_with(
      &pool,
      &provider,
      cart_id,
      checkout_input(&email, ""),
      None,
    )
    .await
    .expect("create order")
    .id;

    let (total, provider_kind, mp_payment_id, static_code): (
      i64,
      String,
      Option<i64>,
      Option<String>,
    ) = sqlx::query_as(
      "SELECT total_cents, payment_provider, mp_payment_id,
              payment_meta->>'pix_copy_paste'
       FROM orders WHERE id = $1",
    )
    .bind(order_id)
    .fetch_one(&pool)
    .await
    .expect("order");
    assert_eq!(provider_kind, "mercadopago");
    assert!(mp_payment_id.is_some(), "charge created after commit");
    assert_eq!(static_code, None, "no local BR Code for MP orders");
    let creates: Vec<_> = mock
      .requests()
      .into_iter()
      .filter(|r| r.method == "POST")
      .collect();
    assert_eq!(creates.len(), 1);
    assert_eq!(
      creates[0].body["transaction_amount"],
      serde_json::json!(total as f64 / 100.0)
    );
    assert_eq!(
      creates[0].idempotency_key.as_deref(),
      Some(order_id.to_string().as_str())
    );
  }

  #[tokio::test]
  async fn mercadopago_checkout_fails_fast_when_circuit_open() {
    let pool = crate::test_support::fresh_pool().await;
    let mock = crate::payments::test_mock::MockMp::start().await;
    let provider =
      mock.provider_with(crate::payments::circuit::CircuitBreaker::new(
        1,
        std::time::Duration::from_secs(60),
      ));
    mock.force(&[500, 500, 500]);
    let _ = provider.fetch_status(1).await;
    let requests_before = mock.requests().len();

    let (_product_id, variant_id) =
      seed_variant(&pool, UNIT_PRICE_CENTS, 3).await;
    let cart_id = seed_cart(&pool, variant_id, 1).await;
    let email = format!("mp-open-{}@example.com", Uuid::now_v7().simple());
    let err = create_order_with(
      &pool,
      &provider,
      cart_id,
      checkout_input(&email, ""),
      None,
    )
    .await
    .expect_err("circuit open");
    assert_eq!(
      err.to_string(),
      crate::payments::PAYMENT_UNAVAILABLE_MESSAGE
    );
    let reserved: i32 = sqlx::query_scalar(
      "SELECT reserved FROM inventory WHERE variant_id = $1",
    )
    .bind(variant_id)
    .fetch_one(&pool)
    .await
    .expect("reserved");
    assert_eq!(reserved, 0, "no order, no hold");
    assert_eq!(mock.requests().len(), requests_before, "no MP call");
  }
}
