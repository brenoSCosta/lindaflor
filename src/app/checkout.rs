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
  cart_coupon_code, cart_subtotal_cents, find_cart, hydrate_cart,
};
pub use crate::app::store::checkout_service::CheckoutInput;
use crate::app::store::checkout_service::{
  CheckoutError, create_order_with, payment_available, phone_digit_count,
  validate_pay_fields,
};
use crate::app::store::coupons::{
  COUPON_GONE_MESSAGE, CouponReject, ResolvedCoupon, resolve_coupon,
};
use crate::app::store::pix::PIX_UNAVAILABLE_MESSAGE;
use crate::app::store::pricing::pix_discount_cents;
use crate::app::store::queries::{format_price, get_store_settings};
use crate::app::store::shipping::{
  BRAZILIAN_STATES, calculate_shipping, cep_digits, is_brazilian_state,
  uf_from_cep,
};

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
}
