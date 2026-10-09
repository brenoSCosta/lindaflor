use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use topcoat::{
  context::{Cx, app_context},
  cookie::{Cookie, Cookies, SameSite, cookies},
  runtime::procedure,
};
use uuid::Uuid;

use crate::app::store::inventory::{
  lock_cart_inventory, release_expired_reservations,
};
use crate::app::store::queries::size_label;
use crate::auth::user::current_user_owned;
use crate::config::app_env;

pub const CART_COOKIE: &str = "lindaflor_cart";
const CART_COOKIE_MAX_AGE_DAYS: i64 = 30;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CartItem {
  pub variant_id: String,
  pub product_id: String,
  pub product_slug: String,
  pub product_name: String,
  pub variant_label: String,
  pub image_url: Option<String>,
  pub unit_price_cents: i32,
  pub quantity: i32,
  pub max_quantity: i32,
}

#[derive(Clone, Debug)]
pub enum CartNotice {
  QtyAdjusted { product_name: String, quantity: i32 },
  ItemRemoved { product_name: String },
}

impl CartNotice {
  pub fn message(&self) -> String {
    match self {
      Self::QtyAdjusted {
        product_name,
        quantity,
      } => format!(
        "{product_name}: quantidade ajustada para {quantity} (estoque)."
      ),
      Self::ItemRemoved { product_name } => {
        format!("{product_name} foi removido do carrinho (indisponível).")
      }
    }
  }
}

#[derive(Clone, Debug)]
pub struct HydratedCart {
  pub cart_id: Uuid,
  pub token: Uuid,
  pub items: Vec<CartItem>,
  pub notices: Vec<CartNotice>,
}

#[derive(Clone, Copy, Debug)]
pub struct CartRef {
  pub id: Uuid,
  pub token: Uuid,
}

#[derive(Debug)]
pub enum CartError {
  InvalidVariant,
  OutOfStock,
  Db(sqlx::Error),
}

#[derive(Debug)]
pub enum QuoteError {
  Empty,
  Insufficient,
  Db(sqlx::Error),
}

impl From<sqlx::Error> for QuoteError {
  fn from(error: sqlx::Error) -> Self {
    Self::Db(error)
  }
}

impl std::fmt::Display for QuoteError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::Empty => write!(f, "Seu carrinho está vazio."),
      Self::Insufficient => {
        write!(f, "Estoque insuficiente para concluir o pedido.")
      }
      Self::Db(error) => write!(f, "{error}"),
    }
  }
}

impl From<sqlx::Error> for CartError {
  fn from(error: sqlx::Error) -> Self {
    Self::Db(error)
  }
}

impl std::fmt::Display for CartError {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    match self {
      Self::InvalidVariant => write!(f, "Variação inválida."),
      Self::OutOfStock => write!(f, "Esta peça está esgotada."),
      Self::Db(error) => write!(f, "{error}"),
    }
  }
}

fn is_development() -> bool {
  matches!(app_env().as_str(), "development" | "dev")
}

pub fn write_cart_token(cx: &Cx, token: Uuid) {
  cookies(cx)
    .override_same_site(SameSite::Lax)
    .override_http_only(true)
    .override_secure(!is_development())
    .override_path("/")
    .override_max_age(topcoat::cookie::time::Duration::days(
      CART_COOKIE_MAX_AGE_DAYS,
    ))
    .add(Cookie::new(CART_COOKIE, token.to_string()));
}

/// Expire the cart cookie (e.g. on logout) so the header badge falls back
/// to the guest state instead of stalling on the previous owner's count.
/// The user's cart rows stay in the DB and are re-attached on next login.
pub fn clear_cart_token(cx: &Cx) {
  cookies(cx)
    .override_same_site(SameSite::Lax)
    .override_http_only(true)
    .override_secure(!is_development())
    .override_path("/")
    .remove(Cookie::new(CART_COOKIE, ""));
}

fn read_cookie_value(cx: &Cx) -> Option<String> {
  cookies(cx)
    .get(CART_COOKIE)
    .map(|cookie| cookie.value().to_string())
    .filter(|value| !value.is_empty())
}

fn parse_token(value: &str) -> Option<Uuid> {
  Uuid::parse_str(value.trim()).ok()
}

#[derive(sqlx::FromRow)]
struct CartRow {
  id: Uuid,
  token: Uuid,
  user_id: Option<Uuid>,
}

#[derive(sqlx::FromRow)]
struct HydrateRow {
  cart_item_id: Uuid,
  variant_id: Uuid,
  quantity: i32,
  product_id: Option<Uuid>,
  product_slug: Option<String>,
  product_name: Option<String>,
  product_active: Option<bool>,
  product_price: Option<i32>,
  size: Option<String>,
  color: Option<String>,
  variant_price: Option<i32>,
  available: Option<i32>,
  image_url: Option<String>,
}

async fn fetch_cart_by_token(
  pool: &PgPool,
  token: Uuid,
) -> Result<Option<CartRow>, sqlx::Error> {
  sqlx::query_as::<_, CartRow>(
    "SELECT id, token, user_id FROM carts WHERE token = $1",
  )
  .bind(token)
  .fetch_optional(pool)
  .await
}

async fn fetch_cart_by_user(
  pool: &PgPool,
  user_id: Uuid,
) -> Result<Option<CartRow>, sqlx::Error> {
  sqlx::query_as::<_, CartRow>(
    "SELECT id, token, user_id FROM carts WHERE user_id = $1",
  )
  .bind(user_id)
  .fetch_optional(pool)
  .await
}

async fn insert_cart(
  pool: &PgPool,
  user_id: Option<Uuid>,
) -> Result<CartRow, sqlx::Error> {
  let id = Uuid::now_v7();
  let token = Uuid::now_v7();
  sqlx::query("INSERT INTO carts (id, token, user_id) VALUES ($1, $2, $3)")
    .bind(id)
    .bind(token)
    .bind(user_id)
    .execute(pool)
    .await?;
  Ok(CartRow { id, token, user_id })
}

async fn get_or_create_user_or_guest(
  pool: &PgPool,
  user_id: Option<Uuid>,
) -> Result<CartRow, sqlx::Error> {
  if let Some(user_id) = user_id
    && let Some(existing) = fetch_cart_by_user(pool, user_id).await?
  {
    return Ok(existing);
  }
  insert_cart(pool, user_id).await
}

/// Read the cart token cookie, create a guest/user cart if needed, and
/// import a legacy JSON cookie once.
pub async fn load_or_create_cart(
  cx: &Cx,
  pool: &PgPool,
  user_id: Option<Uuid>,
) -> Result<CartRef, sqlx::Error> {
  let cookie = read_cookie_value(cx);

  if let Some(ref value) = cookie
    && let Some(token) = parse_token(value)
    && let Some(cart) = fetch_cart_by_token(pool, token).await?
  {
    let belongs_to_other = cart
      .user_id
      .is_some_and(|owner| user_id.is_none_or(|uid| uid != owner));
    if !belongs_to_other {
      return Ok(CartRef {
        id: cart.id,
        token: cart.token,
      });
    }
  }

  if let Some(ref value) = cookie
    && parse_token(value).is_none()
    && let Ok(legacy) = serde_json::from_str::<Vec<CartItem>>(value)
  {
    let cart = get_or_create_user_or_guest(pool, user_id).await?;
    import_legacy_items(pool, cart.id, &legacy).await?;
    write_cart_token(cx, cart.token);
    return Ok(CartRef {
      id: cart.id,
      token: cart.token,
    });
  }

  let cart = get_or_create_user_or_guest(pool, user_id).await?;
  write_cart_token(cx, cart.token);
  Ok(CartRef {
    id: cart.id,
    token: cart.token,
  })
}

async fn import_legacy_items(
  pool: &PgPool,
  cart_id: Uuid,
  items: &[CartItem],
) -> Result<(), sqlx::Error> {
  for item in items {
    let Ok(variant_id) = Uuid::parse_str(&item.variant_id) else {
      continue;
    };
    let quantity = item.quantity.max(1);
    match add_item_to_cart(pool, cart_id, variant_id, quantity).await {
      Ok(()) | Err(CartError::InvalidVariant | CartError::OutOfStock) => {}
      Err(CartError::Db(error)) => return Err(error),
    }
  }
  Ok(())
}

async fn variant_availability(
  pool: &PgPool,
  variant_id: Uuid,
) -> Result<Option<(bool, i32)>, sqlx::Error> {
  let row = sqlx::query_as::<_, (bool, i32)>(
    r#"
        SELECT p.active,
            COALESCE((
                SELECT SUM(GREATEST(i.quantity - i.reserved, 0))::int
                FROM inventory i
                INNER JOIN warehouses w ON w.id = i.warehouse_id
                WHERE i.variant_id = pv.id AND w.active = true
            ), 0) AS available
        FROM product_variants pv
        INNER JOIN products p ON p.id = pv.product_id
        WHERE pv.id = $1
        "#,
  )
  .bind(variant_id)
  .fetch_optional(pool)
  .await?;
  Ok(row)
}

pub async fn add_item(
  pool: &PgPool,
  cart_id: Uuid,
  variant_id: Uuid,
  quantity: i32,
) -> Result<(), CartError> {
  add_item_to_cart(pool, cart_id, variant_id, quantity).await
}

async fn add_item_to_cart(
  pool: &PgPool,
  cart_id: Uuid,
  variant_id: Uuid,
  quantity: i32,
) -> Result<(), CartError> {
  let Some((active, available)) =
    variant_availability(pool, variant_id).await?
  else {
    return Err(CartError::InvalidVariant);
  };
  if !active {
    return Err(CartError::InvalidVariant);
  }
  if available <= 0 {
    return Err(CartError::OutOfStock);
  }
  let add_qty = quantity.max(1).min(available);

  sqlx::query(
    r#"
        INSERT INTO cart_items (id, cart_id, variant_id, quantity)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (cart_id, variant_id)
        DO UPDATE SET
            quantity = LEAST(cart_items.quantity + EXCLUDED.quantity, $5),
            updated_at = now()
        "#,
  )
  .bind(Uuid::now_v7())
  .bind(cart_id)
  .bind(variant_id)
  .bind(add_qty)
  .bind(available)
  .execute(pool)
  .await?;

  sqlx::query("UPDATE carts SET updated_at = now() WHERE id = $1")
    .bind(cart_id)
    .execute(pool)
    .await?;

  Ok(())
}

pub async fn set_quantity(
  pool: &PgPool,
  cart_id: Uuid,
  variant_id: Uuid,
  quantity: i32,
) -> Result<(), CartError> {
  if quantity <= 0 {
    remove_item(pool, cart_id, variant_id).await?;
    return Ok(());
  }
  let Some((active, available)) =
    variant_availability(pool, variant_id).await?
  else {
    remove_item(pool, cart_id, variant_id).await?;
    return Err(CartError::InvalidVariant);
  };
  if !active || available <= 0 {
    remove_item(pool, cart_id, variant_id).await?;
    return Err(CartError::OutOfStock);
  }
  let qty = quantity.min(available);
  let updated = sqlx::query(
    "UPDATE cart_items SET quantity = $3, updated_at = now()
     WHERE cart_id = $1 AND variant_id = $2",
  )
  .bind(cart_id)
  .bind(variant_id)
  .bind(qty)
  .execute(pool)
  .await?;
  if updated.rows_affected() == 0 {
    add_item_to_cart(pool, cart_id, variant_id, qty).await?;
  }
  Ok(())
}

pub async fn remove_item(
  pool: &PgPool,
  cart_id: Uuid,
  variant_id: Uuid,
) -> Result<(), sqlx::Error> {
  sqlx::query("DELETE FROM cart_items WHERE cart_id = $1 AND variant_id = $2")
    .bind(cart_id)
    .bind(variant_id)
    .execute(pool)
    .await?;
  Ok(())
}

pub async fn clear_items(
  pool: &PgPool,
  cart_id: Uuid,
) -> Result<(), sqlx::Error> {
  sqlx::query("DELETE FROM cart_items WHERE cart_id = $1")
    .bind(cart_id)
    .execute(pool)
    .await?;
  Ok(())
}

pub async fn clear_items_in_tx(
  tx: &mut Transaction<'_, Postgres>,
  cart_id: Uuid,
) -> Result<(), sqlx::Error> {
  sqlx::query("DELETE FROM cart_items WHERE cart_id = $1")
    .bind(cart_id)
    .execute(&mut **tx)
    .await?;
  Ok(())
}

/// JS fast-path for the product-page add form. Returns the new item count as a
/// decimal string. Shopper-facing failures use the POST form fallback.
#[procedure("/carrinho/adicionar")]
pub async fn add_to_cart_proc(
  cx: &Cx,
  variant_id: String,
) -> topcoat::Result<String> {
  let pool = app_context::<PgPool>(cx);
  let user_id = current_user_owned(cx).await?.map(|session| session.user.id);
  let variant_id = Uuid::parse_str(variant_id.trim())
    .map_err(|_| topcoat::Error::msg("Variação inválida."))?;
  let cart = load_or_create_cart(cx, pool, user_id).await?;
  match add_item(pool, cart.id, variant_id, 1).await {
    Ok(()) => {
      let n =
        cart_item_count_for_token(pool, cart.token).await?.max(0) as usize;
      Ok(n.to_string())
    }
    Err(CartError::InvalidVariant) => {
      Err(topcoat::Error::msg("Esta variação não está disponível."))
    }
    Err(CartError::OutOfStock) => {
      Err(topcoat::Error::msg("Esta peça está esgotada."))
    }
    Err(CartError::Db(error)) => Err(error.into()),
  }
}

/// JS fast-path for the cart-page remove form. Returns the new item count.
#[procedure("/carrinho/remover")]
pub async fn remove_from_cart_proc(
  cx: &Cx,
  variant_id: String,
) -> topcoat::Result<String> {
  let pool = app_context::<PgPool>(cx);
  let user_id = current_user_owned(cx).await?.map(|session| session.user.id);
  let variant_id = Uuid::parse_str(variant_id.trim())
    .map_err(|_| topcoat::Error::msg("Variação inválida."))?;
  let cart = load_or_create_cart(cx, pool, user_id).await?;
  remove_item(pool, cart.id, variant_id).await?;
  let n = cart_item_count_for_token(pool, cart.token).await?.max(0) as usize;
  Ok(n.to_string())
}

/// JS fast-path for the cart-page − / + forms. Returns the new item count.
#[procedure("/carrinho/quantidade")]
pub async fn set_cart_quantity_proc(
  cx: &Cx,
  variant_id: String,
  quantity: String,
) -> topcoat::Result<String> {
  let pool = app_context::<PgPool>(cx);
  let user_id = current_user_owned(cx).await?.map(|session| session.user.id);
  let variant_id = Uuid::parse_str(variant_id.trim())
    .map_err(|_| topcoat::Error::msg("Variação inválida."))?;
  let quantity: i32 = quantity
    .trim()
    .parse()
    .map_err(|_| topcoat::Error::msg("Quantidade inválida."))?;
  let cart = load_or_create_cart(cx, pool, user_id).await?;
  match set_quantity(pool, cart.id, variant_id, quantity).await {
    Ok(()) | Err(CartError::InvalidVariant | CartError::OutOfStock) => {
      let n =
        cart_item_count_for_token(pool, cart.token).await?.max(0) as usize;
      Ok(n.to_string())
    }
    Err(CartError::Db(error)) => Err(error.into()),
  }
}

/// Sheet fast-path for the coupon form. Returns the domain outcome as data
/// so the browser can show an inline error without re-rendering the shard:
/// empty string applies/clears, non-empty is the message to show inline.
/// Outer `Err` is a transport failure. Empty code clears the coupon.
#[procedure("/carrinho/cupom")]
pub async fn set_cart_coupon_proc(
  cx: &Cx,
  code: String,
) -> topcoat::Result<String> {
  use crate::app::store::coupons::{CouponReject, resolve_coupon};

  let pool = app_context::<PgPool>(cx);
  let user_id = current_user_owned(cx).await?.map(|session| session.user.id);
  let cart = load_or_create_cart(cx, pool, user_id).await?;
  let raw: String = code.trim().chars().take(64).collect();
  if raw.is_empty() {
    set_cart_coupon(pool, cart.id, None).await?;
    return Ok(String::new());
  }
  let subtotal = {
    let hydrated = hydrate_cart(pool, cart.id).await?;
    cart_subtotal_cents(&hydrated.items)
  };
  let mut tx = pool.begin().await?;
  let outcome = resolve_coupon(&mut tx, &raw, subtotal, user_id, false).await;
  tx.rollback().await?;
  match outcome {
    Ok(Some(coupon)) => {
      set_cart_coupon(pool, cart.id, Some(&coupon.code)).await?;
      Ok(String::new())
    }
    Ok(None) => {
      set_cart_coupon(pool, cart.id, None).await?;
      Ok(String::new())
    }
    Err(CouponReject::Invalid(message)) => {
      set_cart_coupon(pool, cart.id, None).await?;
      Ok(message)
    }
    Err(CouponReject::Db(error)) => Err(error.into()),
  }
}

/// Header badge: sum of stored quantities for the cookie token (no create).
pub async fn cart_item_count(
  cx: &Cx,
  pool: &PgPool,
) -> Result<i32, sqlx::Error> {
  let Some(value) = read_cookie_value(cx) else {
    return Ok(0);
  };
  let Some(token) = parse_token(&value) else {
    return Ok(0);
  };
  cart_item_count_for_token(pool, token).await
}

async fn cart_item_count_for_token(
  pool: &PgPool,
  token: Uuid,
) -> Result<i32, sqlx::Error> {
  let count: Option<i32> = sqlx::query_scalar(
    r#"
        SELECT COALESCE(SUM(ci.quantity), 0)::int
        FROM cart_items ci
        INNER JOIN carts c ON c.id = ci.cart_id
        WHERE c.token = $1
        "#,
  )
  .bind(token)
  .fetch_one(pool)
  .await?;
  Ok(count.unwrap_or(0))
}

pub fn cart_subtotal_cents(items: &[CartItem]) -> i32 {
  items.iter().map(|i| i.unit_price_cents * i.quantity).sum()
}

pub async fn hydrate_cart(
  pool: &PgPool,
  cart_id: Uuid,
) -> Result<HydratedCart, sqlx::Error> {
  let _ = release_expired_reservations(pool).await;
  let mut tx = pool.begin().await?;
  match hydrate_cart_in_tx(&mut tx, cart_id).await {
    Ok(hydrated) => {
      tx.commit().await?;
      Ok(hydrated)
    }
    Err(error) => {
      let _ = tx.rollback().await;
      Err(error)
    }
  }
}

pub async fn hydrate_cart_in_tx(
  tx: &mut Transaction<'_, Postgres>,
  cart_id: Uuid,
) -> Result<HydratedCart, sqlx::Error> {
  let token: Uuid = sqlx::query_scalar("SELECT token FROM carts WHERE id = $1")
    .bind(cart_id)
    .fetch_one(&mut **tx)
    .await?;

  let rows = sqlx::query_as::<_, HydrateRow>(
    r#"
        SELECT
            ci.id AS cart_item_id,
            ci.variant_id,
            ci.quantity,
            p.id AS product_id,
            p.slug AS product_slug,
            p.name AS product_name,
            p.active AS product_active,
            p.price_in_cents AS product_price,
            pv.size::text AS size,
            pv.color,
            pv.price_in_cents AS variant_price,
            COALESCE((
                SELECT SUM(GREATEST(i.quantity - i.reserved, 0))::int
                FROM inventory i
                INNER JOIN warehouses w ON w.id = i.warehouse_id
                WHERE i.variant_id = pv.id AND w.active = true
            ), 0) AS available,
            (
                SELECT pi.url FROM product_images pi
                WHERE pi.product_id = p.id
                ORDER BY pi.sort_order ASC
                LIMIT 1
            ) AS image_url
        FROM cart_items ci
        LEFT JOIN product_variants pv ON pv.id = ci.variant_id
        LEFT JOIN products p ON p.id = pv.product_id
        WHERE ci.cart_id = $1
        FOR UPDATE OF ci
        "#,
  )
  .bind(cart_id)
  .fetch_all(&mut **tx)
  .await?;

  let mut items = Vec::new();
  let mut notices = Vec::new();

  for row in rows {
    let product_name = row
      .product_name
      .clone()
      .unwrap_or_else(|| "Peça".to_string());
    let active = row.product_active.unwrap_or(false);
    let available = row.available.unwrap_or(0);
    let Some(product_id) = row.product_id else {
      sqlx::query("DELETE FROM cart_items WHERE id = $1")
        .bind(row.cart_item_id)
        .execute(&mut **tx)
        .await?;
      notices.push(CartNotice::ItemRemoved { product_name });
      continue;
    };
    if !active || available <= 0 {
      sqlx::query("DELETE FROM cart_items WHERE id = $1")
        .bind(row.cart_item_id)
        .execute(&mut **tx)
        .await?;
      notices.push(CartNotice::ItemRemoved { product_name });
      continue;
    }

    let mut quantity = row.quantity;
    if quantity > available {
      quantity = available;
      sqlx::query(
        "UPDATE cart_items SET quantity = $2, updated_at = now() WHERE id = $1",
      )
      .bind(row.cart_item_id)
      .bind(quantity)
      .execute(&mut **tx)
      .await?;
      notices.push(CartNotice::QtyAdjusted {
        product_name: product_name.clone(),
        quantity,
      });
    }

    let unit_price_cents = row.variant_price.or(row.product_price).unwrap_or(0);
    let size = row.size.as_deref().unwrap_or("");
    let color = row.color.as_deref().unwrap_or("");
    items.push(CartItem {
      variant_id: row.variant_id.to_string(),
      product_id: product_id.to_string(),
      product_slug: row.product_slug.unwrap_or_default(),
      product_name,
      variant_label: format!("{} · {}", size_label(size), color),
      image_url: row.image_url,
      unit_price_cents,
      quantity,
      max_quantity: available,
    });
  }

  Ok(HydratedCart {
    cart_id,
    token,
    items,
    notices,
  })
}

fn display_item_from_row(row: &HydrateRow) -> Option<CartItem> {
  let product_id = row.product_id?;
  let product_name = row
    .product_name
    .clone()
    .unwrap_or_else(|| "Peça".to_string());
  let size = row.size.as_deref().unwrap_or("");
  let color = row.color.as_deref().unwrap_or("");
  Some(CartItem {
    variant_id: row.variant_id.to_string(),
    product_id: product_id.to_string(),
    product_slug: row.product_slug.clone().unwrap_or_default(),
    product_name,
    variant_label: format!("{} · {}", size_label(size), color),
    image_url: row.image_url.clone(),
    unit_price_cents: row.variant_price.or(row.product_price).unwrap_or(0),
    quantity: row.quantity,
    max_quantity: row.available.unwrap_or(0),
  })
}

/// Checkout quote: lock inventory then cart lines. Never delete or clamp.
pub async fn quote_cart_for_checkout(
  tx: &mut Transaction<'_, Postgres>,
  cart_id: Uuid,
) -> Result<Vec<CartItem>, QuoteError> {
  lock_cart_inventory(tx, cart_id).await?;

  let rows = sqlx::query_as::<_, HydrateRow>(
    r#"
        SELECT
            ci.id AS cart_item_id,
            ci.variant_id,
            ci.quantity,
            p.id AS product_id,
            p.slug AS product_slug,
            p.name AS product_name,
            p.active AS product_active,
            p.price_in_cents AS product_price,
            pv.size::text AS size,
            pv.color,
            pv.price_in_cents AS variant_price,
            COALESCE((
                SELECT SUM(GREATEST(i.quantity - i.reserved, 0))::int
                FROM inventory i
                INNER JOIN warehouses w ON w.id = i.warehouse_id
                WHERE i.variant_id = pv.id AND w.active = true
            ), 0) AS available,
            (
                SELECT pi.url FROM product_images pi
                WHERE pi.product_id = p.id
                ORDER BY pi.sort_order ASC
                LIMIT 1
            ) AS image_url
        FROM cart_items ci
        LEFT JOIN product_variants pv ON pv.id = ci.variant_id
        LEFT JOIN products p ON p.id = pv.product_id
        WHERE ci.cart_id = $1
        FOR UPDATE OF ci
        "#,
  )
  .bind(cart_id)
  .fetch_all(&mut **tx)
  .await?;

  if rows.is_empty() {
    return Err(QuoteError::Empty);
  }

  let mut items = Vec::new();
  for row in rows {
    let active = row.product_active.unwrap_or(false);
    let available = row.available.unwrap_or(0);
    if row.product_id.is_none() || !active || available < row.quantity {
      return Err(QuoteError::Insufficient);
    }
    let Some(item) = display_item_from_row(&row) else {
      return Err(QuoteError::Insufficient);
    };
    items.push(item);
  }
  Ok(items)
}

pub async fn cart_coupon_code(
  pool: &PgPool,
  cart_id: Uuid,
) -> Result<Option<String>, sqlx::Error> {
  let code: Option<Option<String>> =
    sqlx::query_scalar("SELECT coupon_code FROM carts WHERE id = $1")
      .bind(cart_id)
      .fetch_optional(pool)
      .await?;
  Ok(code.flatten().filter(|value| !value.trim().is_empty()))
}

pub async fn set_cart_coupon(
  pool: &PgPool,
  cart_id: Uuid,
  coupon_code: Option<&str>,
) -> Result<(), sqlx::Error> {
  sqlx::query(
    "UPDATE carts SET coupon_code = $2, updated_at = now() WHERE id = $1",
  )
  .bind(cart_id)
  .bind(coupon_code)
  .execute(pool)
  .await?;
  Ok(())
}

/// Merge a guest cart into the signed-in user's cart and point the cookie
/// at the surviving token.
pub async fn merge_guest_cart_on_login(
  cx: &Cx,
  pool: &PgPool,
  user_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
  let cookie = read_cookie_value(cx);
  let guest = if let Some(ref value) = cookie
    && let Some(token) = parse_token(value)
  {
    fetch_cart_by_token(pool, token).await?
  } else {
    None
  };

  let token =
    merge_carts_for_user(pool, guest.as_ref().map(|c| c.id), user_id).await?;
  write_cart_token(cx, token);
  Ok(token)
}

/// Sum guest lines into the user's cart, cap to live stock, delete the guest
/// cart. If there is no guest cart, ensure a user cart exists.
pub async fn merge_carts_for_user(
  pool: &PgPool,
  guest_cart_id: Option<Uuid>,
  user_id: Uuid,
) -> Result<Uuid, sqlx::Error> {
  let user_cart = fetch_cart_by_user(pool, user_id).await?;

  match (guest_cart_id, user_cart) {
    (None, Some(user_cart)) => Ok(user_cart.token),
    (None, None) => Ok(insert_cart(pool, Some(user_id)).await?.token),
    (Some(guest_id), None) => {
      sqlx::query(
        "UPDATE carts SET user_id = $2, updated_at = now() WHERE id = $1",
      )
      .bind(guest_id)
      .bind(user_id)
      .execute(pool)
      .await?;
      let token: Uuid =
        sqlx::query_scalar("SELECT token FROM carts WHERE id = $1")
          .bind(guest_id)
          .fetch_one(pool)
          .await?;
      clamp_cart_to_stock(pool, guest_id).await?;
      Ok(token)
    }
    (Some(guest_id), Some(user_cart)) => {
      if guest_id == user_cart.id {
        return Ok(user_cart.token);
      }
      let guest_owner: Option<Uuid> =
        sqlx::query_scalar("SELECT user_id FROM carts WHERE id = $1")
          .bind(guest_id)
          .fetch_one(pool)
          .await?;
      if guest_owner.is_some_and(|owner| owner != user_id) {
        return Ok(user_cart.token);
      }

      let guest_items = sqlx::query_as::<_, (Uuid, i32)>(
        "SELECT variant_id, quantity FROM cart_items WHERE cart_id = $1",
      )
      .bind(guest_id)
      .fetch_all(pool)
      .await?;

      for (variant_id, quantity) in guest_items {
        match add_item_to_cart(pool, user_cart.id, variant_id, quantity).await {
          Ok(()) | Err(CartError::InvalidVariant | CartError::OutOfStock) => {}
          Err(CartError::Db(error)) => return Err(error),
        }
      }

      sqlx::query("DELETE FROM carts WHERE id = $1")
        .bind(guest_id)
        .execute(pool)
        .await?;
      clamp_cart_to_stock(pool, user_cart.id).await?;
      Ok(user_cart.token)
    }
  }
}

async fn clamp_cart_to_stock(
  pool: &PgPool,
  cart_id: Uuid,
) -> Result<(), sqlx::Error> {
  let mut tx = pool.begin().await?;
  let _ = hydrate_cart_in_tx(&mut tx, cart_id).await?;
  tx.commit().await?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;

  async fn seed_variant(
    pool: &PgPool,
    active: bool,
    stock: i32,
  ) -> (Uuid, Uuid) {
    let product_id = Uuid::now_v7();
    let variant_id = Uuid::now_v7();
    let warehouse_id = Uuid::now_v7();

    sqlx::query(
      "INSERT INTO products (id, name, slug, price_in_cents, active)
       VALUES ($1, 'Peça teste', $2, 10000, $3)",
    )
    .bind(product_id)
    .bind(format!("peca-{product_id}"))
    .bind(active)
    .execute(pool)
    .await
    .expect("product");

    sqlx::query(
      "INSERT INTO product_variants (id, product_id, sku, size, color)
       VALUES ($1, $2, $3, 'm', 'azul')",
    )
    .bind(variant_id)
    .bind(product_id)
    .bind(format!("sku-{variant_id}"))
    .execute(pool)
    .await
    .expect("variant");

    sqlx::query(
      "INSERT INTO warehouses (id, code, name, is_default, active)
       VALUES ($1, $2, 'Principal', true, true)",
    )
    .bind(warehouse_id)
    .bind(format!("wh-{}", warehouse_id.simple()))
    .execute(pool)
    .await
    .expect("warehouse");

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
    .expect("inventory");

    (product_id, variant_id)
  }

  async fn new_cart(pool: &PgPool) -> Uuid {
    let cart_id = Uuid::now_v7();
    sqlx::query("INSERT INTO carts (id, token) VALUES ($1, $2)")
      .bind(cart_id)
      .bind(Uuid::now_v7())
      .execute(pool)
      .await
      .expect("cart");
    cart_id
  }

  #[tokio::test]
  async fn add_item_merges_same_variant_and_caps_stock() {
    let pool = crate::test_support::fresh_pool().await;
    let (_product_id, variant_id) = seed_variant(&pool, true, 3).await;
    let cart_id = new_cart(&pool).await;

    add_item(&pool, cart_id, variant_id, 1).await.expect("add");
    add_item(&pool, cart_id, variant_id, 1)
      .await
      .expect("merge");
    add_item(&pool, cart_id, variant_id, 5).await.expect("cap");

    let qty: i32 = sqlx::query_scalar(
      "SELECT quantity FROM cart_items WHERE cart_id = $1 AND variant_id = $2",
    )
    .bind(cart_id)
    .bind(variant_id)
    .fetch_one(&pool)
    .await
    .expect("qty");
    assert_eq!(qty, 3);

    let lines: i64 =
      sqlx::query_scalar("SELECT COUNT(*) FROM cart_items WHERE cart_id = $1")
        .bind(cart_id)
        .fetch_one(&pool)
        .await
        .expect("lines");
    assert_eq!(lines, 1);
  }

  #[tokio::test]
  async fn hydrate_drops_inactive_product() {
    let pool = crate::test_support::fresh_pool().await;
    let (product_id, variant_id) = seed_variant(&pool, true, 4).await;
    let cart_id = new_cart(&pool).await;
    add_item(&pool, cart_id, variant_id, 2).await.expect("add");

    sqlx::query("UPDATE products SET active = false WHERE id = $1")
      .bind(product_id)
      .execute(&pool)
      .await
      .expect("deactivate");

    let hydrated = hydrate_cart(&pool, cart_id).await.expect("hydrate");
    assert!(hydrated.items.is_empty());
    assert!(matches!(
      hydrated.notices.first(),
      Some(CartNotice::ItemRemoved { .. })
    ));
  }

  #[tokio::test]
  async fn carts_are_isolated_by_token() {
    let pool = crate::test_support::fresh_pool().await;
    let (_product_id, variant_id) = seed_variant(&pool, true, 4).await;
    let cart_a = new_cart(&pool).await;
    let cart_b = new_cart(&pool).await;
    add_item(&pool, cart_a, variant_id, 1).await.expect("add a");

    let hydrated_b = hydrate_cart(&pool, cart_b).await.expect("hydrate b");
    assert!(hydrated_b.items.is_empty());
    let hydrated_a = hydrate_cart(&pool, cart_a).await.expect("hydrate a");
    assert_eq!(hydrated_a.items.len(), 1);
  }

  #[tokio::test]
  async fn login_merge_sums_guest_into_user_cart_and_caps_stock() {
    let pool = crate::test_support::fresh_pool().await;
    let (_product_id, variant_id) = seed_variant(&pool, true, 3).await;
    let user_id = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, name, email) VALUES ($1, $2, $3)")
      .bind(user_id)
      .bind("Cliente")
      .bind(format!("cart-{}@example.com", user_id.simple()))
      .execute(&pool)
      .await
      .expect("user");

    let guest_id = new_cart(&pool).await;
    let user_cart_id = new_cart(&pool).await;
    sqlx::query("UPDATE carts SET user_id = $2 WHERE id = $1")
      .bind(user_cart_id)
      .bind(user_id)
      .execute(&pool)
      .await
      .expect("attach user cart");

    add_item(&pool, guest_id, variant_id, 2)
      .await
      .expect("guest");
    add_item(&pool, user_cart_id, variant_id, 2)
      .await
      .expect("user");

    let token = merge_carts_for_user(&pool, Some(guest_id), user_id)
      .await
      .expect("merge");

    let surviving: Uuid =
      sqlx::query_scalar("SELECT token FROM carts WHERE user_id = $1")
        .bind(user_id)
        .fetch_one(&pool)
        .await
        .expect("user cart token");
    assert_eq!(token, surviving);

    let guest_gone: Option<Uuid> =
      sqlx::query_scalar("SELECT id FROM carts WHERE id = $1")
        .bind(guest_id)
        .fetch_optional(&pool)
        .await
        .expect("guest lookup");
    assert!(guest_gone.is_none());

    let qty: i32 = sqlx::query_scalar(
      "SELECT quantity FROM cart_items WHERE cart_id = $1 AND variant_id = $2",
    )
    .bind(user_cart_id)
    .bind(variant_id)
    .fetch_one(&pool)
    .await
    .expect("merged qty");
    assert_eq!(qty, 3);
  }

  #[tokio::test]
  async fn cart_coupon_persists_until_cleared() {
    let pool = crate::test_support::fresh_pool().await;
    let cart_id = new_cart(&pool).await;
    set_cart_coupon(&pool, cart_id, Some("CUPOM10"))
      .await
      .expect("set");
    let stored = cart_coupon_code(&pool, cart_id).await.expect("load");
    assert_eq!(stored.as_deref(), Some("CUPOM10"));
    set_cart_coupon(&pool, cart_id, None).await.expect("clear");
    let stored = cart_coupon_code(&pool, cart_id).await.expect("load empty");
    assert_eq!(stored, None);
  }
}
