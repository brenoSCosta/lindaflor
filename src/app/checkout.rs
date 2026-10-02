use serde::Deserialize;
use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  context::app_context,
  router::{content::Form, error::redirect, page},
  view::{View, ViewExt, attributes, view},
};
use uuid::Uuid;

use crate::components::button::{
  ButtonSize, ButtonVariant, button, button_variants,
};
use crate::components::card::{
  card, card_content, card_footer, card_header, card_title,
};
use crate::components::field::{field, field_label};
use crate::components::input::input;
use crate::components::select::select;
use crate::components::separator::separator;
use crate::components::textarea::textarea;

use crate::app::store::cart::{cart_subtotal_cents, clear_cart, read_cart};
use crate::app::store::queries::format_price;

#[derive(Deserialize)]
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
  #[allow(dead_code)]
  coupon_code: String,
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
  let items = read_cart(cx);

  if items.is_empty() {
    return Ok(view! {
            <div class="mx-auto max-w-2xl px-4 py-24 text-center md:px-8">
                <h1 class="text-4xl font-bold tracking-tight">"Seu carrinho está vazio"</h1>
                <p class="mt-4 text-muted-foreground">
                    "Adicione peças ao carrinho antes de finalizar a compra."
                </p>
                <a
                    href="/produtos"
                    class=(button_variants(ButtonVariant::Primary, ButtonSize::Lg))
                >
                    "Ver catálogo"
                </a>
            </div>
        }
        .boxed());
  }

  let subtotal = cart_subtotal_cents(&items);

  if let Some(Form(form_input)) = body {
    let order_id = create_order(pool, &items, form_input).await?;
    clear_cart(cx);
    return Err(redirect(format!("/pedido/{}", order_id)).into());
  }

  let zip_digits: String = input_zip_digits(body.as_ref());
  let shipping = calculate_shipping(subtotal, "SE", &zip_digits);
  let total = subtotal + shipping.0;

  let state = input_state(body.as_ref());

  Ok(view! {
        <main class="mx-auto max-w-6xl px-4 py-12 md:px-8">
            <h1 class="text-4xl font-bold tracking-tight md:text-5xl">"Checkout"</h1>

            <form method="post" action="/checkout" class="mt-10 grid gap-12 lg:grid-cols-[1.2fr_0.8fr]">
                <div class="flex flex-col gap-8">
                    card(
                        card_header(
                            card_title("Contato")
                        )
                        card_content(
                            field(
                                field_label(attrs: attributes! { for="email" }, "E-mail")
                                input(attrs: attributes! {
                                    id="email"
                                    type="email"
                                    name="guest_email"
                                    required="required"
                                    value=(input_email(body.as_ref()))
                                })
                            )
                        )
                    )

                    card(
                        card_header(
                            card_title("Endereço de entrega")
                        )
                        card_content(
                            <div class="grid gap-4 sm:grid-cols-2">
                                <div class="sm:col-span-2">
                                    field(
                                        field_label(attrs: attributes! { for="name" }, "Nome completo")
                                        input(attrs: attributes! {
                                            id="name"
                                            name="name"
                                            required="required"
                                            value=(input_name(body.as_ref()))
                                        })
                                    )
                                </div>
                                <div class="sm:col-span-2">
                                    field(
                                        field_label(attrs: attributes! { for="phone" }, "WhatsApp (opcional)")
                                        input(attrs: attributes! {
                                            id="phone"
                                            name="phone"
                                            inputMode="tel"
                                            placeholder="79999816511"
                                            value=(input_phone(body.as_ref()))
                                        })
                                    )
                                </div>
                                <div class="sm:col-span-2">
                                    field(
                                        field_label(attrs: attributes! { for="street" }, "Rua")
                                        input(attrs: attributes! {
                                            id="street"
                                            name="street"
                                            required="required"
                                            value=(input_street(body.as_ref()))
                                        })
                                    )
                                </div>
                                field(
                                    field_label(attrs: attributes! { for="number" }, "Número")
                                    input(attrs: attributes! {
                                        id="number"
                                        name="number"
                                        required="required"
                                        value=(input_number(body.as_ref()))
                                    })
                                )
                                field(
                                    field_label(attrs: attributes! { for="complement" }, "Complemento")
                                    input(attrs: attributes! {
                                        id="complement"
                                        name="complement"
                                        value=(input_complement(body.as_ref()))
                                    })
                                )
                                field(
                                    field_label(attrs: attributes! { for="neighborhood" }, "Bairro")
                                    input(attrs: attributes! {
                                        id="neighborhood"
                                        name="neighborhood"
                                        required="required"
                                        value=(input_neighborhood(body.as_ref()))
                                    })
                                )
                                field(
                                    field_label(attrs: attributes! { for="city" }, "Cidade")
                                    input(attrs: attributes! {
                                        id="city"
                                        name="city"
                                        required="required"
                                        value=(input_city(body.as_ref()))
                                    })
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
                                    field_label(attrs: attributes! { for="zip" }, "CEP")
                                    input(attrs: attributes! {
                                        id="zip"
                                        name="zip_code"
                                        required="required"
                                        placeholder="49000-000"
                                        value=(input_zip(body.as_ref()))
                                    })
                                )
                            </div>
                        )
                    )

                    card(
                        card_header(
                            card_title("Observações")
                        )
                        card_content(
                            textarea(attrs: attributes! { id="notes" name="notes" rows="3" })
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
                                <div class="flex justify-between pt-2 text-base font-medium">
                                    <span>"Total"</span>
                                    <span class="text-primary">(format_price(total))</span>
                                </div>
                            </div>
                        )
                        card_footer(
                            button(
                                variant: ButtonVariant::Primary,
                                size: ButtonSize::Lg,
                                attrs: attributes! { type="submit" class="w-full" },
                                "Pagar com PIX"
                            )
                        )
                    )
                    <p class="mt-4 text-xs text-muted-foreground">
                        "Pagamento via PIX. Após confirmar, você verá o QR Code na próxima tela."
                    </p>
                </aside>
            </form>
        </main>
    }.boxed())
}

async fn create_order(
  pool: &PgPool,
  items: &[crate::app::store::cart::CartItem],
  form_data: CheckoutInput,
) -> Result<Uuid, sqlx::Error> {
  let order_id = Uuid::now_v7();
  let zip_digits: String = form_data
    .zip_code
    .chars()
    .filter(|c| c.is_ascii_digit())
    .collect();
  let shipping = calculate_shipping(
    cart_subtotal_cents(items),
    &form_data.state,
    &zip_digits,
  );
  let subtotal = cart_subtotal_cents(items);
  let total = subtotal + shipping.0;

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

  sqlx::query!(
        "INSERT INTO orders (id, guest_email, status, subtotal_cents, shipping_cents, total_cents, shipping_address, notes)
         VALUES ($1, $2, 'pending_payment', $3, $4, $5, $6, $7)",
        order_id,
        form_data.guest_email,
        subtotal,
        shipping.0,
        total,
        address,
        if form_data.notes.trim().is_empty() { None } else { Some(form_data.notes.trim()) },
    )
    .execute(pool)
    .await?;

  for item in items {
    sqlx::query!(
            "INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
             VALUES ($1, $2, $3, $4, $5, $6, $7)",
            Uuid::now_v7(),
            order_id,
            Uuid::parse_str(&item.variant_id).unwrap_or(Uuid::now_v7()),
            item.product_name,
            item.variant_label,
            item.quantity,
            item.unit_price_cents,
        )
        .execute(pool)
        .await?;
  }

  let settings = crate::app::store::queries::get_store_settings(pool).await?;
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
  });

  sqlx::query!(
    "UPDATE orders SET payment_meta = $1 WHERE id = $2",
    payment_meta,
    order_id
  )
  .execute(pool)
  .await?;

  Ok(order_id)
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
    emv_field("00", "br.gov.bcb.pix") + &emv_field("01", &key);
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
