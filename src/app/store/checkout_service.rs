//! Checkout order service: form validation and order creation (stock
//! reservation, coupon redemption, PIX payload) behind the `/checkout` page.

use serde::Deserialize;
use sqlx::PgPool;
use uuid::Uuid;

use crate::app::store::cart::{
  QuoteError, cart_subtotal_cents, clear_items_in_tx, quote_cart_for_checkout,
};
use crate::app::store::coupons::{
  CouponReject, record_redemption, resolve_coupon,
};
use crate::app::store::inventory::{ReserveLine, reserve_for_order};
use crate::app::store::pix::{PIX_UNAVAILABLE_MESSAGE, generate_pix_payload};
use crate::app::store::pricing::pix_discount_cents;
use crate::app::store::shipping::{calculate_shipping, resolve_shipping_uf};

#[derive(Debug, thiserror::Error)]
pub(crate) enum CheckoutError {
  #[error("{0}")]
  Form(String),
  #[error("Cupom removido, revise o total.")]
  CouponRemoved,
  #[error(transparent)]
  Db(#[from] sqlx::Error),
}

#[derive(Deserialize, Clone)]
pub struct CheckoutInput {
  pub(crate) guest_email: String,
  pub(crate) name: String,
  pub(crate) phone: String,
  pub(crate) street: String,
  pub(crate) number: String,
  pub(crate) complement: String,
  pub(crate) neighborhood: String,
  pub(crate) city: String,
  pub(crate) state: String,
  pub(crate) zip_code: String,
  pub(crate) notes: String,

  #[serde(default)]
  pub(crate) coupon_code: Option<String>,

  #[serde(default)]
  pub(crate) intent: Option<String>,

  /// "Salvar este endereço" (signed-in buyers only).
  #[serde(default)]
  pub(crate) save_address: Option<String>,

  /// Honeypot: an off-screen field humans never see or fill. See
  /// [`honeypot_tripped`].
  #[serde(default)]
  pub(crate) website: Option<String>,
}

/// Name of the honeypot input on the checkout form.
pub(crate) const HONEYPOT_FIELD: &str = "website";

/// A filled honeypot means a form-filling bot: the page drops the order
/// silently (no error that would teach the bot what went wrong).
pub(crate) fn honeypot_tripped(form_data: &CheckoutInput) -> bool {
  form_data
    .website
    .as_deref()
    .is_some_and(|value| !value.trim().is_empty())
}

/// Unpaid orders one buyer (normalized email, cart or account) may hold at
/// a time. Each one reserves stock for up to 48h, so this bounds how much
/// inventory a single visitor can lock up.
pub(crate) const MAX_PENDING_ORDERS: i64 = 3;

fn pending_limit_message(latest_code: &str) -> String {
  format!(
    "Você já tem {MAX_PENDING_ORDERS} pedidos aguardando pagamento \
     (o mais recente é o #{latest_code}). Pague pelo link que enviamos \
     para o seu e-mail ou aguarde a reserva expirar antes de fazer um \
     novo pedido."
  )
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
      website: None,
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

pub(crate) fn phone_digit_count(value: &str) -> usize {
  value.chars().filter(|c| c.is_ascii_digit()).count()
}

/// Returns the shipping UF derived from the CEP.
pub(crate) fn validate_pay_fields(
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
pub(crate) struct CreatedOrder {
  pub(crate) id: Uuid,
  pub(crate) access_token: Uuid,
}

/// Static-PIX order creation (the pre-PSP path the tests exercise).
#[cfg(test)]
pub(crate) async fn create_order(
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

pub(crate) async fn create_order_with(
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
  let email_norm =
    crate::app::store::coupons::normalize_email(&form_data.guest_email);
  enforce_pending_limit(tx, email_norm.as_deref(), cart_id, user_id).await?;

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
        "INSERT INTO orders (id, user_id, guest_email, status, subtotal_cents, shipping_cents, discount_cents, pix_discount_cents, total_cents, shipping_address, notes, coupon_id, access_token, email_norm, cart_id)
         VALUES ($1, $2, $3, 'pending_payment', $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14)",
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
    .bind(email_norm.as_deref())
    .bind(cart_id)
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

/// Refuse a new order while the buyer already holds
/// [`MAX_PENDING_ORDERS`] unpaid, unexpired ones (matched by normalized
/// email, cart, or account). Transaction-scoped advisory locks on the email
/// and the cart serialize concurrent checkouts of the same buyer, so two
/// racing submits cannot both slip under the cap. Always email first, then
/// cart: a fixed order, so no lock cycles.
async fn enforce_pending_limit(
  tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  email_norm: Option<&str>,
  cart_id: Uuid,
  user_id: Option<Uuid>,
) -> Result<(), CheckoutError> {
  if let Some(email) = email_norm {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
      .bind(format!("checkout:email:{email}"))
      .execute(&mut **tx)
      .await?;
  }
  sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
    .bind(format!("checkout:cart:{cart_id}"))
    .execute(&mut **tx)
    .await?;
  let pending: Vec<Uuid> = sqlx::query_scalar(
    "SELECT id FROM orders
     WHERE status = 'pending_payment'
       AND (reservation_expires_at IS NULL OR reservation_expires_at > now())
       AND (email_norm = $1 OR cart_id = $2
            OR ($3::uuid IS NOT NULL AND user_id = $3))
     ORDER BY created_at DESC, id DESC",
  )
  .bind(email_norm)
  .bind(cart_id)
  .bind(user_id)
  .fetch_all(&mut **tx)
  .await?;
  if pending.len() as i64 >= MAX_PENDING_ORDERS {
    let latest = crate::mail::templates::order_code(pending[0]);
    return Err(CheckoutError::Form(pending_limit_message(&latest)));
  }
  Ok(())
}

/// Whether checkout can take payment: Mercado Pago (breaker closed) or a
/// configured static PIX key.
pub(crate) fn payment_available(
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
      website: None,
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

  async fn refill_cart(pool: &PgPool, cart_id: Uuid, variant_id: Uuid) {
    sqlx::query(
      "INSERT INTO cart_items (id, cart_id, variant_id, quantity)
       VALUES ($1, $2, $3, 1)",
    )
    .bind(Uuid::now_v7())
    .bind(cart_id)
    .bind(variant_id)
    .execute(pool)
    .await
    .expect("refill cart");
  }

  #[test]
  fn honeypot_trips_only_when_filled() {
    let mut form = checkout_input("hp@example.com", "");
    assert!(!honeypot_tripped(&form));
    form.website = Some("   ".to_string());
    assert!(!honeypot_tripped(&form));
    form.website = Some("http://spam.example".to_string());
    assert!(honeypot_tripped(&form));
    assert_eq!(HONEYPOT_FIELD, "website");
  }

  #[tokio::test]
  async fn pending_orders_are_capped_per_email_and_cart() {
    let pool = crate::test_support::fresh_pool().await;
    ensure_pix_settings(&pool).await;
    let (_, variant_id) = seed_variant(&pool, UNIT_PRICE_CENTS, 20).await;
    let local = format!("cap.{}", Uuid::now_v7().simple());
    let email = format!("{local}@gmail.com");
    let cart_id = seed_cart(&pool, variant_id, 1).await;

    let mut created = Vec::new();
    for _ in 0..MAX_PENDING_ORDERS {
      created.push(
        create_order(&pool, cart_id, checkout_input(&email, ""), None)
          .await
          .expect("under the cap"),
      );
      refill_cart(&pool, cart_id, variant_id).await;
    }

    // Same cart, different email: still the same buyer.
    let other = format!("other-{}@example.com", Uuid::now_v7().simple());
    let err = create_order(&pool, cart_id, checkout_input(&other, ""), None)
      .await
      .expect_err("cart at the cap");
    let latest = crate::mail::templates::order_code(created[2].id);
    assert!(
      matches!(&err, CheckoutError::Form(message)
        if message.contains(&format!("#{latest}"))
          && message.contains("aguardando pagamento")),
      "{err}"
    );

    // Fresh cart, Gmail alias of the same address: still capped.
    let alias = format!("{}+promo@googlemail.com", local.replace('.', ""));
    let second_cart = seed_cart(&pool, variant_id, 1).await;
    let err =
      create_order(&pool, second_cart, checkout_input(&alias, ""), None)
        .await
        .expect_err("email at the cap");
    assert!(matches!(err, CheckoutError::Form(_)));
    let reserved: i32 = sqlx::query_scalar(
      "SELECT reserved FROM inventory WHERE variant_id = $1",
    )
    .bind(variant_id)
    .fetch_one(&pool)
    .await
    .expect("reserved");
    assert_eq!(reserved, 3, "rejected attempts hold no stock");

    // Once one pending order leaves the pending state, a new one is allowed.
    sqlx::query("UPDATE orders SET status = 'cancelled' WHERE id = $1")
      .bind(created[0].id)
      .execute(&pool)
      .await
      .expect("cancel one");
    create_order(&pool, second_cart, checkout_input(&alias, ""), None)
      .await
      .expect("back under the cap");
  }
}
