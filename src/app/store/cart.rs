use serde::{Deserialize, Serialize};
use sqlx::{PgPool, Postgres, Transaction};
use topcoat::{
  context::{Cx, app_context},
  cookie::{Cookie, Cookies, SameSite, cookies},
  runtime::procedure,
};
use uuid::Uuid;

use crate::app::store::inventory::lock_cart_inventory;
use crate::app::store::queries::size_label;
use crate::auth::user::current_user_owned;
use crate::config::app_env;

pub const CART_COOKIE: &str = "lindaflor_cart";
/// `__Host-` variant used outside development. `__Host-` requires
/// `Secure` + `Path=/` + no `Domain`, all of which the cart cookie already
/// sets (see [`write_cart_token`]); the prefix additionally keeps a
/// network attacker from planting a sibling-domain cart token. Safe to
/// rename per env: only the server reads this cookie (`HttpOnly`, never
/// touched by JS), following the `pending_2fa` pattern.
pub const CART_COOKIE_HOST: &str = "__Host-lindaflor_cart";
const CART_COOKIE_MAX_AGE_DAYS: i64 = 30;

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct CartItem {
  pub variant_id: String,
  pub product_id: String,
  pub product_slug: String,
  pub product_name: String,
  pub variant_label: String,
  pub image_url: Option<String>,
  pub unit_price_cents: i64,
  pub quantity: i32,
  pub max_quantity: i32,
  /// Owning set when this line is part of a fixed-composition bundle
  /// (`cart_items.set_id`, migration 0015). `None` for single-variant lines.
  /// `#[serde(default)]` keeps previously serialized shapes loadable.
  #[serde(default)]
  pub set_id: Option<String>,
  /// Snapshot of the set name for display/order history.
  #[serde(default)]
  pub set_name: Option<String>,
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

fn cart_cookie_secure() -> bool {
  !is_development()
}

fn cart_cookie_name() -> &'static str {
  if cart_cookie_secure() {
    CART_COOKIE_HOST
  } else {
    CART_COOKIE
  }
}

pub fn write_cart_token(cx: &Cx, token: Uuid) {
  cookies(cx)
    .override_same_site(SameSite::Lax)
    .override_http_only(true)
    .override_secure(cart_cookie_secure())
    .override_path("/")
    .override_max_age(topcoat::cookie::time::Duration::days(
      CART_COOKIE_MAX_AGE_DAYS,
    ))
    .add(Cookie::new(cart_cookie_name(), token.to_string()));
}

/// Expire the cart cookie (e.g. on logout) so the header badge falls back
/// to the guest state instead of stalling on the previous owner's count.
/// The user's cart rows stay in the DB and are re-attached on next login.
/// Attributes mirror [`write_cart_token`] (same name/path/`Secure`) so the
/// expiry overwrites the cookie in every env.
pub fn clear_cart_token(cx: &Cx) {
  cookies(cx)
    .override_same_site(SameSite::Lax)
    .override_http_only(true)
    .override_secure(cart_cookie_secure())
    .override_path("/")
    .remove(Cookie::new(cart_cookie_name(), ""));
}

fn read_cookie_value(cx: &Cx) -> Option<String> {
  cookies(cx)
    .get(cart_cookie_name())
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
  product_price: Option<i64>,
  size: Option<String>,
  color: Option<String>,
  variant_price: Option<i64>,
  available: Option<i32>,
  image_url: Option<String>,
  set_id: Option<Uuid>,
  set_name: Option<String>,
}

/// Display label for a set-member line: base variant label plus the bundle
/// marker so shoppers see the line belongs to a conjunto.
fn set_member_label(base: &str, set_name: Option<&str>) -> String {
  match set_name {
    Some(name) => format!("{base} — parte do conjunto {name}"),
    None => format!("{base} — parte do conjunto"),
  }
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

/// Read the cart token cookie, creating a guest/user cart if needed.
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

  let cart = get_or_create_user_or_guest(pool, user_id).await?;
  write_cart_token(cx, cart.token);
  Ok(CartRef {
    id: cart.id,
    token: cart.token,
  })
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

  // Loose line only (`set_id IS NULL`): never touches set-member rows, so a
  // variant that is also in a set gets a separate full-price line.
  sqlx::query(
    r#"
        INSERT INTO cart_items (id, cart_id, variant_id, quantity)
        VALUES ($1, $2, $3, $4)
        ON CONFLICT (cart_id, variant_id) WHERE set_id IS NULL
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

/// Add a whole fixed-composition set (conjunto) to the cart: one
/// `cart_items` row per slot variant, all stamped with `set_id`, all sharing
/// one clamped quantity (`max(1, min(qty, min_available))`).
/// Errors follow the single-variant patterns: missing/inactive set, set
/// without a price ("conjunto sem preço"), empty composition, or an inactive
/// member variant all map to [`CartError::InvalidVariant`]; zero availability
/// on any member maps to [`CartError::OutOfStock`].
pub async fn add_set_to_cart(
  pool: &PgPool,
  cart_id: Uuid,
  set_id: Uuid,
  quantity: i32,
) -> Result<(), CartError> {
  let set_row: Option<(bool, Option<i64>)> =
    sqlx::query_as("SELECT active, price_in_cents FROM sets WHERE id = $1")
      .bind(set_id)
      .fetch_optional(pool)
      .await?;
  let Some((active, price)) = set_row else {
    return Err(CartError::InvalidVariant);
  };
  if !active {
    return Err(CartError::InvalidVariant);
  }
  if price.is_none() {
    return Err(CartError::InvalidVariant);
  }

  let variants: Vec<Uuid> = sqlx::query_scalar(
    r#"
        SELECT o.variant_id
        FROM set_slot_options o
        INNER JOIN set_slots s ON s.id = o.slot_id
        WHERE s.set_id = $1
        ORDER BY s.sort_order ASC, o.variant_id ASC
        "#,
  )
  .bind(set_id)
  .fetch_all(pool)
  .await?;
  if variants.is_empty() {
    return Err(CartError::InvalidVariant);
  }

  let mut lines: Vec<(Uuid, i32)> = Vec::with_capacity(variants.len());
  for variant_id in variants {
    let Some((variant_active, available)) =
      variant_availability(pool, variant_id).await?
    else {
      return Err(CartError::InvalidVariant);
    };
    if !variant_active {
      return Err(CartError::InvalidVariant);
    }
    lines.push((variant_id, available));
  }
  let min_available = lines.iter().map(|(_, a)| *a).min().unwrap_or(0);
  if min_available <= 0 {
    return Err(CartError::OutOfStock);
  }
  let add_qty = quantity.max(1).min(min_available);

  // One row per member, all sharing the same added quantity capped by the
  // minimum availability: members of a set group always move together, so a
  // repeated add can never overfill a single member. The conflict target only
  // matches this set's member lines; loose lines of the same variant (and
  // other sets' lines) are separate rows.
  for (variant_id, _) in &lines {
    sqlx::query(
      r#"
        INSERT INTO cart_items (id, cart_id, variant_id, quantity, set_id)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (cart_id, set_id, variant_id) WHERE set_id IS NOT NULL
        DO UPDATE SET
            quantity = LEAST(cart_items.quantity + EXCLUDED.quantity, $6),
            updated_at = now()
        "#,
    )
    .bind(Uuid::now_v7())
    .bind(cart_id)
    .bind(variant_id)
    .bind(add_qty)
    .bind(set_id)
    .bind(min_available)
    .execute(pool)
    .await?;
  }

  sqlx::query("UPDATE carts SET updated_at = now() WHERE id = $1")
    .bind(cart_id)
    .execute(pool)
    .await?;

  Ok(())
}

/// Display stock for a set: MIN over its slots. Thin wrapper — no duplicate
/// logic; see [`crate::app::store::queries::set_available_quantity`].
pub async fn set_available_for_display(
  pool: &PgPool,
  set_id: Uuid,
) -> Result<i64, sqlx::Error> {
  crate::app::store::queries::set_available_quantity(pool, set_id).await
}

/// Loose-line quantity only (`set_id IS NULL`): never changes set-member
/// rows. Use [`set_set_quantity`] for a whole set group.
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
     WHERE cart_id = $1 AND variant_id = $2 AND set_id IS NULL",
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

/// Set the whole set group's quantity: every member line gets the same
/// quantity, clamped by the minimum member availability. All members of a
/// set group must always share one quantity (see §1.2); per-member controls
/// are not offered in the UI.
pub async fn set_set_quantity(
  pool: &PgPool,
  cart_id: Uuid,
  set_id: Uuid,
  quantity: i32,
) -> Result<(), CartError> {
  if quantity <= 0 {
    remove_set(pool, cart_id, set_id).await?;
    return Ok(());
  }
  let member_variants: Vec<Uuid> = sqlx::query_scalar(
    "SELECT variant_id FROM cart_items WHERE cart_id = $1 AND set_id = $2",
  )
  .bind(cart_id)
  .bind(set_id)
  .fetch_all(pool)
  .await?;
  if member_variants.is_empty() {
    return Ok(());
  }
  let mut min_available = i32::MAX;
  for variant_id in &member_variants {
    let Some((active, available)) =
      variant_availability(pool, *variant_id).await?
    else {
      remove_set(pool, cart_id, set_id).await?;
      return Err(CartError::InvalidVariant);
    };
    if !active || available <= 0 {
      remove_set(pool, cart_id, set_id).await?;
      return Err(CartError::OutOfStock);
    }
    min_available = min_available.min(available);
  }
  let qty = quantity.max(1).min(min_available);
  sqlx::query(
    "UPDATE cart_items SET quantity = $3, updated_at = now()
     WHERE cart_id = $1 AND set_id = $2",
  )
  .bind(cart_id)
  .bind(set_id)
  .bind(qty)
  .execute(pool)
  .await?;
  Ok(())
}

/// Loose-line remove only (`set_id IS NULL`): set-member rows survive. Use
/// [`remove_set`] to drop a whole set group.
pub async fn remove_item(
  pool: &PgPool,
  cart_id: Uuid,
  variant_id: Uuid,
) -> Result<(), sqlx::Error> {
  sqlx::query(
    "DELETE FROM cart_items WHERE cart_id = $1 AND variant_id = $2 AND set_id IS NULL",
  )
  .bind(cart_id)
  .bind(variant_id)
  .execute(pool)
  .await?;
  Ok(())
}

/// Remove every member line of one set group.
pub async fn remove_set(
  pool: &PgPool,
  cart_id: Uuid,
  set_id: Uuid,
) -> Result<(), sqlx::Error> {
  sqlx::query("DELETE FROM cart_items WHERE cart_id = $1 AND set_id = $2")
    .bind(cart_id)
    .bind(set_id)
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

/// JS fast-path for the set-detail add form. Adds the whole
/// fixed-composition bundle (one cart line per slot variant, stamped with
/// `set_id`) and returns the new item count as a decimal string.
/// Shopper-facing failures use the POST form fallback.
/// No `#[route(POST "/sets/:slug/add")]` variant: every `#[route]` in this
/// repo uses a static path with `path_param!` for `#[page]`s, so the
/// multi-arg `#[procedure]` pattern (like `/carrinho/quantidade`) is the one
/// that compiles. The set-detail page POSTs back to itself and calls
/// [`add_set_to_cart`] directly; its `@submit` handler calls this procedure
/// with the set id (+ `quantity`) for the inline update.
#[procedure("/conjuntos/adicionar")]
pub async fn add_set_to_cart_proc(
  cx: &Cx,
  set_id: String,
  quantity: String,
) -> topcoat::Result<String> {
  let pool = app_context::<PgPool>(cx);
  let user_id = current_user_owned(cx).await?.map(|session| session.user.id);
  let set_id = Uuid::parse_str(set_id.trim())
    .map_err(|_| topcoat::Error::msg("Conjunto inválido."))?;
  let quantity: i32 = quantity
    .trim()
    .parse()
    .map_err(|_| topcoat::Error::msg("Quantidade inválida."))?;
  let cart = load_or_create_cart(cx, pool, user_id).await?;
  match add_set_to_cart(pool, cart.id, set_id, quantity).await {
    Ok(()) => {
      let n =
        cart_item_count_for_token(pool, cart.token).await?.max(0) as usize;
      Ok(n.to_string())
    }
    Err(CartError::InvalidVariant) => {
      Err(topcoat::Error::msg("Este conjunto não está disponível."))
    }
    Err(CartError::OutOfStock) => {
      Err(topcoat::Error::msg("Este conjunto está esgotado."))
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

/// JS fast-path for the cart-page set-group − / + control. Sets the whole
/// group's quantity (every member moves together). Returns the new item
/// count. Argument types mirror [`set_cart_quantity_proc`] (`String` only).
#[procedure("/carrinho/conjunto/quantidade")]
pub async fn set_cart_set_quantity_proc(
  cx: &Cx,
  set_id: String,
  quantity: String,
) -> topcoat::Result<String> {
  let pool = app_context::<PgPool>(cx);
  let user_id = current_user_owned(cx).await?.map(|session| session.user.id);
  let set_id = Uuid::parse_str(set_id.trim())
    .map_err(|_| topcoat::Error::msg("Conjunto inválido."))?;
  let quantity: i32 = quantity
    .trim()
    .parse()
    .map_err(|_| topcoat::Error::msg("Quantidade inválida."))?;
  let cart = load_or_create_cart(cx, pool, user_id).await?;
  match set_set_quantity(pool, cart.id, set_id, quantity).await {
    Ok(()) | Err(CartError::InvalidVariant | CartError::OutOfStock) => {
      let n =
        cart_item_count_for_token(pool, cart.token).await?.max(0) as usize;
      Ok(n.to_string())
    }
    Err(CartError::Db(error)) => Err(error.into()),
  }
}

/// JS fast-path for the cart-page set-group remove control. Removes every
/// member line of the set. Returns the new item count.
#[procedure("/carrinho/conjunto/remover")]
pub async fn remove_set_from_cart_proc(
  cx: &Cx,
  set_id: String,
) -> topcoat::Result<String> {
  let pool = app_context::<PgPool>(cx);
  let user_id = current_user_owned(cx).await?.map(|session| session.user.id);
  let set_id = Uuid::parse_str(set_id.trim())
    .map_err(|_| topcoat::Error::msg("Conjunto inválido."))?;
  let cart = load_or_create_cart(cx, pool, user_id).await?;
  remove_set(pool, cart.id, set_id).await?;
  let n = cart_item_count_for_token(pool, cart.token).await?.max(0) as usize;
  Ok(n.to_string())
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
  use crate::app::store::coupons::{
    COUPON_GONE_MESSAGE, CouponReject, resolve_coupon,
  };

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
    cart_subtotal_cents(&hydrated.items).ok_or_else(|| {
      topcoat::Error::msg("Carrinho inválido. Atualize a página.")
    })?
  };
  let mut tx = pool.begin().await?;
  let outcome =
    resolve_coupon(&mut tx, &raw, subtotal, user_id, None, false).await;
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
    Err(CouponReject::Gone) => {
      set_cart_coupon(pool, cart.id, None).await?;
      Ok(COUPON_GONE_MESSAGE.to_string())
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

pub fn cart_subtotal_cents(items: &[CartItem]) -> Option<i64> {
  items.iter().try_fold(0i64, |acc, item| {
    let line = item
      .unit_price_cents
      .checked_mul(i64::from(item.quantity))?;
    acc.checked_add(line)
  })
}

pub async fn hydrate_cart(
  pool: &PgPool,
  cart_id: Uuid,
) -> Result<HydratedCart, sqlx::Error> {
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
            ) AS image_url,
            ci.set_id AS set_id,
            s.name AS set_name
        FROM cart_items ci
        LEFT JOIN product_variants pv ON pv.id = ci.variant_id
        LEFT JOIN products p ON p.id = pv.product_id
        LEFT JOIN sets s ON s.id = ci.set_id
        WHERE ci.cart_id = $1
        FOR UPDATE OF ci
        "#,
  )
  .bind(cart_id)
  .fetch_all(&mut **tx)
  .await?;

  let mut items = Vec::new();
  let mut notices = Vec::new();
  // Set groups that lost a member below: the rest of the group is dropped
  // after the loop, since a partial set can never be quoted.
  let mut broken_sets: std::collections::HashSet<Uuid> =
    std::collections::HashSet::new();

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
      broken_sets.extend(row.set_id);
      notices.push(CartNotice::ItemRemoved { product_name });
      continue;
    };
    if !active || available <= 0 {
      sqlx::query("DELETE FROM cart_items WHERE id = $1")
        .bind(row.cart_item_id)
        .execute(&mut **tx)
        .await?;
      broken_sets.extend(row.set_id);
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
    let base_label = format!("{} · {}", size_label(size), color);
    let (variant_label, set_id, set_name) = match row.set_id {
      Some(set_id) => (
        set_member_label(&base_label, row.set_name.as_deref()),
        Some(set_id.to_string()),
        row.set_name.clone(),
      ),
      None => (base_label, None, None),
    };
    items.push(CartItem {
      variant_id: row.variant_id.to_string(),
      product_id: product_id.to_string(),
      product_slug: row.product_slug.unwrap_or_default(),
      product_name,
      variant_label,
      image_url: row.image_url,
      unit_price_cents,
      quantity,
      max_quantity: available,
      set_id,
      set_name,
    });
  }

  normalize_set_groups(tx, cart_id, &mut items, &mut notices, &broken_sets)
    .await?;

  Ok(HydratedCart {
    cart_id,
    token,
    items,
    notices,
  })
}

/// Keep every set group whole and uniform after per-row stock clamping:
/// a group that lost a member is removed entirely, and a group whose
/// members were clamped to different quantities is lowered to its smallest
/// member quantity. Without this a partial or uneven set (one member sold
/// out, or capped alone on login merge) would sit in the cart and fail
/// every checkout quote.
async fn normalize_set_groups(
  tx: &mut Transaction<'_, Postgres>,
  cart_id: Uuid,
  items: &mut Vec<CartItem>,
  notices: &mut Vec<CartNotice>,
  broken_sets: &std::collections::HashSet<Uuid>,
) -> Result<(), sqlx::Error> {
  for set_id in broken_sets {
    sqlx::query("DELETE FROM cart_items WHERE cart_id = $1 AND set_id = $2")
      .bind(cart_id)
      .bind(set_id)
      .execute(&mut **tx)
      .await?;
    let key = set_id.to_string();
    let set_name = items
      .iter()
      .find(|item| item.set_id.as_deref() == Some(key.as_str()))
      .and_then(|item| item.set_name.clone());
    items.retain(|item| item.set_id.as_deref() != Some(key.as_str()));
    if let Some(set_name) = set_name {
      notices.push(CartNotice::ItemRemoved {
        product_name: set_name,
      });
    }
  }

  let mut group_min: std::collections::BTreeMap<String, i32> =
    std::collections::BTreeMap::new();
  for item in items.iter() {
    if let Some(set_id) = &item.set_id {
      let min = group_min.entry(set_id.clone()).or_insert(item.quantity);
      *min = (*min).min(item.quantity);
    }
  }
  for (set_id, min) in group_min {
    let uneven = items.iter().any(|item| {
      item.set_id.as_deref() == Some(set_id.as_str()) && item.quantity != min
    });
    if !uneven {
      continue;
    }
    let Ok(set_uuid) = Uuid::parse_str(&set_id) else {
      continue;
    };
    sqlx::query(
      "UPDATE cart_items SET quantity = $3, updated_at = now()
       WHERE cart_id = $1 AND set_id = $2",
    )
    .bind(cart_id)
    .bind(set_uuid)
    .bind(min)
    .execute(&mut **tx)
    .await?;
    let mut set_name = None;
    for item in items.iter_mut() {
      if item.set_id.as_deref() == Some(set_id.as_str()) {
        item.quantity = min;
        set_name = set_name.or_else(|| item.set_name.clone());
      }
    }
    notices.push(CartNotice::QtyAdjusted {
      product_name: set_name.unwrap_or_else(|| "Conjunto".to_string()),
      quantity: min,
    });
  }
  Ok(())
}

fn display_item_from_row(row: &HydrateRow) -> Option<CartItem> {
  let product_id = row.product_id?;
  let product_name = row
    .product_name
    .clone()
    .unwrap_or_else(|| "Peça".to_string());
  let size = row.size.as_deref().unwrap_or("");
  let color = row.color.as_deref().unwrap_or("");
  let base_label = format!("{} · {}", size_label(size), color);
  let (variant_label, set_id, set_name) = match row.set_id {
    Some(set_id) => (
      set_member_label(&base_label, row.set_name.as_deref()),
      Some(set_id.to_string()),
      row.set_name.clone(),
    ),
    None => (base_label, None, None),
  };
  Some(CartItem {
    variant_id: row.variant_id.to_string(),
    product_id: product_id.to_string(),
    product_slug: row.product_slug.clone().unwrap_or_default(),
    product_name,
    variant_label,
    image_url: row.image_url.clone(),
    unit_price_cents: row.variant_price.or(row.product_price).unwrap_or(0),
    quantity: row.quantity,
    max_quantity: row.available.unwrap_or(0),
    set_id,
    set_name,
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
            ) AS image_url,
            ci.set_id AS set_id,
            s.name AS set_name
        FROM cart_items ci
        LEFT JOIN product_variants pv ON pv.id = ci.variant_id
        LEFT JOIN products p ON p.id = pv.product_id
        LEFT JOIN sets s ON s.id = ci.set_id
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
  let mut set_groups: std::collections::BTreeMap<Uuid, Vec<usize>> =
    std::collections::BTreeMap::new();
  // The same variant can appear on several lines (one loose line plus one
  // member line per set): availability covers the summed quantity, so split
  // lines can never oversell stock the single-line check would catch.
  let mut totals: std::collections::HashMap<Uuid, i32> =
    std::collections::HashMap::new();
  for row in &rows {
    *totals.entry(row.variant_id).or_default() += row.quantity;
  }
  for row in &rows {
    let active = row.product_active.unwrap_or(false);
    let available = row.available.unwrap_or(0);
    let total = totals.get(&row.variant_id).copied().unwrap_or(0);
    if row.product_id.is_none() || !active || available < total {
      return Err(QuoteError::Insufficient);
    }
    let Some(item) = display_item_from_row(row) else {
      return Err(QuoteError::Insufficient);
    };
    if let Some(set_id) = row.set_id {
      set_groups.entry(set_id).or_default().push(items.len());
    }
    items.push(item);
  }

  // Bundle atomicity + bundle pricing. A set is never partially quoted: if
  // any member of a `set_id` group is missing (partial set after a single
  // member remove, or admin recomposition) or the set lost its price /
  // active flag, the whole quote fails with `Insufficient`. Member
  // quantities must also be equal: each member is priced at `set_price / n`
  // per unit, so unequal quantities would hand extra units the cheaper set
  // share (§1.2).
  for (set_id, member_indices) in &set_groups {
    let first_qty = rows[member_indices[0]].quantity;
    if member_indices
      .iter()
      .any(|&i| rows[i].quantity != first_qty)
    {
      return Err(QuoteError::Insufficient);
    }
    let set_row: Option<(String, bool, Option<i64>)> = sqlx::query_as(
      "SELECT name, active, price_in_cents FROM sets WHERE id = $1",
    )
    .bind(set_id)
    .fetch_optional(&mut **tx)
    .await?;
    let Some((set_name, set_active, set_price)) = set_row else {
      return Err(QuoteError::Insufficient);
    };
    let Some(set_price) = set_price else {
      return Err(QuoteError::Insufficient);
    };
    if !set_active {
      return Err(QuoteError::Insufficient);
    }

    let mut expected: Vec<Uuid> = sqlx::query_scalar(
      r#"
        SELECT o.variant_id
        FROM set_slot_options o
        INNER JOIN set_slots s ON s.id = o.slot_id
        WHERE s.set_id = $1
        "#,
    )
    .bind(set_id)
    .fetch_all(&mut **tx)
    .await?;
    if expected.is_empty() {
      return Err(QuoteError::Insufficient);
    }
    expected.sort();
    let mut group_variants: Vec<Uuid> =
      member_indices.iter().map(|&i| rows[i].variant_id).collect();
    group_variants.sort();
    if group_variants != expected {
      return Err(QuoteError::Insufficient);
    }

    // Bundle pricing: the sum of member unit prices is replaced by a
    // prorated per-unit split of the set price. First slot (deterministic
    // variant-id order) absorbs the integer remainder, so the shares sum to
    // exactly `set_price` and — for uniform member quantities `qty` — the
    // group total equals `set_price * qty`. Per-line `unit_price_cents` is
    // kept, so `cart_subtotal_cents`/coupon/PIX math is unchanged.
    let n = member_indices.len() as i64;
    let base_share = set_price / n;
    let first_share = set_price - base_share * (n - 1);
    let mut ordered = member_indices.clone();
    ordered.sort_by_key(|&i| rows[i].variant_id);
    for (position, &i) in ordered.iter().enumerate() {
      items[i].unit_price_cents = if position == 0 {
        first_share
      } else {
        base_share
      };
      items[i].set_name = Some(set_name.clone());
    }
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

/// Merge one guest set-member line into the same set's member line of the
/// user's cart, capped at live stock. Single-line upsert (no group
/// equalization here): [`clamp_cart_to_stock`] caps rows afterwards and the
/// quote rejects unequal groups.
async fn merge_set_member_line(
  pool: &PgPool,
  cart_id: Uuid,
  set_id: Uuid,
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
        INSERT INTO cart_items (id, cart_id, variant_id, quantity, set_id)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (cart_id, set_id, variant_id) WHERE set_id IS NOT NULL
        DO UPDATE SET
            quantity = LEAST(cart_items.quantity + EXCLUDED.quantity, $6),
            updated_at = now()
        "#,
  )
  .bind(Uuid::now_v7())
  .bind(cart_id)
  .bind(variant_id)
  .bind(add_qty)
  .bind(set_id)
  .bind(available)
  .execute(pool)
  .await?;
  sqlx::query("UPDATE carts SET updated_at = now() WHERE id = $1")
    .bind(cart_id)
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

      let guest_items = sqlx::query_as::<_, (Uuid, i32, Option<Uuid>)>(
        "SELECT variant_id, quantity, set_id FROM cart_items WHERE cart_id = $1",
      )
      .bind(guest_id)
      .fetch_all(pool)
      .await?;

      // Loose lines merge into loose lines; set-member lines merge into the
      // same set's lines, so neither side rewrites the other's `set_id`.
      for (variant_id, quantity, set_id) in guest_items {
        let outcome = match set_id {
          None => {
            add_item_to_cart(pool, user_cart.id, variant_id, quantity).await
          }
          Some(set_id) => {
            merge_set_member_line(
              pool,
              user_cart.id,
              set_id,
              variant_id,
              quantity,
            )
            .await
          }
        };
        match outcome {
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

  async fn seed_set_variant(
    pool: &PgPool,
    price_cents: i64,
    stock: i32,
  ) -> Uuid {
    let product_id = Uuid::now_v7();
    let variant_id = Uuid::now_v7();
    let warehouse_id = Uuid::now_v7();

    sqlx::query(
      "INSERT INTO products (id, name, slug, price_in_cents, active)
       VALUES ($1, 'Peça conjunto', $2, $3, true)",
    )
    .bind(product_id)
    .bind(format!("conj-peca-{product_id}"))
    .bind(price_cents)
    .execute(pool)
    .await
    .expect("product");

    sqlx::query(
      "INSERT INTO product_variants (id, product_id, sku, size, color)
       VALUES ($1, $2, $3, 'm', 'branco')",
    )
    .bind(variant_id)
    .bind(product_id)
    .bind(format!("conj-sku-{variant_id}"))
    .execute(pool)
    .await
    .expect("variant");

    sqlx::query(
      "INSERT INTO warehouses (id, code, name, is_default, active)
       VALUES ($1, $2, 'Principal', true, true)",
    )
    .bind(warehouse_id)
    .bind(format!("conj-wh-{}", warehouse_id.simple()))
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

    variant_id
  }

  async fn seed_set(
    pool: &PgPool,
    price_cents: Option<i64>,
    member_prices: &[i64],
    member_stocks: &[i32],
  ) -> (Uuid, Vec<Uuid>) {
    assert_eq!(member_prices.len(), member_stocks.len());
    let set_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO sets (id, name, slug, active, price_in_cents)
       VALUES ($1, 'Conjunto teste', $2, true, $3)",
    )
    .bind(set_id)
    .bind(format!("conj-{set_id}"))
    .bind(price_cents)
    .execute(pool)
    .await
    .expect("set");

    let mut variants = Vec::new();
    for (index, (&price, &stock)) in
      member_prices.iter().zip(member_stocks.iter()).enumerate()
    {
      let variant_id = seed_set_variant(pool, price, stock).await;
      let slot_id = Uuid::now_v7();
      sqlx::query(
        "INSERT INTO set_slots (id, set_id, sort_order) VALUES ($1, $2, $3)",
      )
      .bind(slot_id)
      .bind(set_id)
      .bind(index as i32)
      .execute(pool)
      .await
      .expect("slot");
      sqlx::query(
        "INSERT INTO set_slot_options (slot_id, variant_id) VALUES ($1, $2)",
      )
      .bind(slot_id)
      .bind(variant_id)
      .execute(pool)
      .await
      .expect("slot option");
      variants.push(variant_id);
    }
    (set_id, variants)
  }

  #[tokio::test]
  async fn add_set_inserts_one_row_per_slot_clamped_to_min_stock() {
    let pool = crate::test_support::fresh_pool().await;
    let (set_id, variants) =
      seed_set(&pool, Some(10_000), &[6_000, 7_000], &[2, 5]).await;
    let cart_id = new_cart(&pool).await;

    add_set_to_cart(&pool, cart_id, set_id, 9)
      .await
      .expect("add set");

    let rows: Vec<(Uuid, i32, Option<Uuid>)> = sqlx::query_as(
      "SELECT variant_id, quantity, set_id FROM cart_items WHERE cart_id = $1",
    )
    .bind(cart_id)
    .fetch_all(&pool)
    .await
    .expect("rows");
    assert_eq!(rows.len(), 2);
    for (variant_id, quantity, row_set_id) in &rows {
      assert!(variants.contains(variant_id));
      // min(2, 5) = 2 clamps both members.
      assert_eq!(*quantity, 2);
      assert_eq!(*row_set_id, Some(set_id));
    }
  }

  #[tokio::test]
  async fn add_set_rejects_missing_price_and_inactive() {
    let pool = crate::test_support::fresh_pool().await;
    let (priceless_id, _) = seed_set(&pool, None, &[6_000], &[3]).await;
    let cart_id = new_cart(&pool).await;
    assert!(matches!(
      add_set_to_cart(&pool, cart_id, priceless_id, 1).await,
      Err(CartError::InvalidVariant)
    ));

    let (set_id, _) = seed_set(&pool, Some(9_000), &[6_000], &[3]).await;
    sqlx::query("UPDATE sets SET active = false WHERE id = $1")
      .bind(set_id)
      .execute(&pool)
      .await
      .expect("deactivate set");
    assert!(matches!(
      add_set_to_cart(&pool, cart_id, set_id, 1).await,
      Err(CartError::InvalidVariant)
    ));

    let (empty_id, _) = seed_set(&pool, Some(9_000), &[6_000], &[0]).await;
    assert!(matches!(
      add_set_to_cart(&pool, cart_id, empty_id, 1).await,
      Err(CartError::OutOfStock)
    ));
  }

  #[tokio::test]
  async fn quote_splits_set_price_with_first_slot_remainder() {
    let pool = crate::test_support::fresh_pool().await;
    // Member list prices (6000 + 7000) are replaced by the 10001 bundle split.
    let (set_id, _) =
      seed_set(&pool, Some(10_001), &[6_000, 7_000], &[4, 4]).await;
    let cart_id = new_cart(&pool).await;
    add_set_to_cart(&pool, cart_id, set_id, 1)
      .await
      .expect("add set");

    let mut tx = pool.begin().await.expect("tx");
    let items = quote_cart_for_checkout(&mut tx, cart_id)
      .await
      .expect("quote");
    tx.rollback().await.expect("rollback");
    assert_eq!(items.len(), 2);

    let mut units: Vec<i64> =
      items.iter().map(|item| item.unit_price_cents).collect();
    units.sort_unstable();
    assert_eq!(units, vec![5_000, 5_001]);
    let total: i64 = items
      .iter()
      .map(|item| item.unit_price_cents * i64::from(item.quantity))
      .sum();
    assert_eq!(total, 10_001);
    for item in &items {
      assert_eq!(item.set_id.as_deref(), Some(set_id.to_string()).as_deref());
      assert!(
        item
          .variant_label
          .contains("parte do conjunto Conjunto teste"),
        "unexpected label: {}",
        item.variant_label
      );
    }
  }

  #[tokio::test]
  async fn quote_partial_set_after_member_remove_is_insufficient() {
    let pool = crate::test_support::fresh_pool().await;
    let (set_id, variants) =
      seed_set(&pool, Some(10_000), &[6_000, 7_000], &[4, 4]).await;
    let cart_id = new_cart(&pool).await;
    add_set_to_cart(&pool, cart_id, set_id, 1)
      .await
      .expect("add set");

    // Simulate a legacy partial group (e.g. admin recomposition): one member
    // row deleted straight in SQL, bypassing the group-level `remove_set`.
    sqlx::query(
      "DELETE FROM cart_items WHERE cart_id = $1 AND set_id = $2 AND variant_id = $3",
    )
    .bind(cart_id)
    .bind(set_id)
    .bind(variants[0])
    .execute(&pool)
    .await
    .expect("remove member");

    let mut tx = pool.begin().await.expect("tx");
    let outcome = quote_cart_for_checkout(&mut tx, cart_id).await;
    tx.rollback().await.expect("rollback");
    assert!(matches!(outcome, Err(QuoteError::Insufficient)));
  }

  #[tokio::test]
  async fn set_group_quantity_change_applies_to_all_members() {
    let pool = crate::test_support::fresh_pool().await;
    let (set_id, variants) =
      seed_set(&pool, Some(10_000), &[6_000, 7_000], &[5, 5]).await;
    let cart_id = new_cart(&pool).await;
    add_set_to_cart(&pool, cart_id, set_id, 1)
      .await
      .expect("add set");

    set_set_quantity(&pool, cart_id, set_id, 3)
      .await
      .expect("group quantity");

    let rows: Vec<(Uuid, i32)> = sqlx::query_as(
      "SELECT variant_id, quantity FROM cart_items WHERE cart_id = $1 AND set_id = $2",
    )
    .bind(cart_id)
    .bind(set_id)
    .fetch_all(&pool)
    .await
    .expect("rows");
    assert_eq!(rows.len(), 2);
    for (variant_id, quantity) in &rows {
      assert!(variants.contains(variant_id));
      assert_eq!(*quantity, 3);
    }

    let mut tx = pool.begin().await.expect("tx");
    let items = quote_cart_for_checkout(&mut tx, cart_id)
      .await
      .expect("quote");
    tx.rollback().await.expect("rollback");
    let total: i64 = items
      .iter()
      .map(|item| item.unit_price_cents * i64::from(item.quantity))
      .sum();
    assert_eq!(total, 10_000 * 3);
  }

  async fn set_variant_stock(pool: &PgPool, variant_id: Uuid, quantity: i32) {
    sqlx::query(
      "UPDATE inventory SET quantity = $2, reserved = 0 WHERE variant_id = $1",
    )
    .bind(variant_id)
    .bind(quantity)
    .execute(pool)
    .await
    .expect("set stock");
  }

  #[tokio::test]
  async fn hydrate_drops_whole_set_when_a_member_sells_out() {
    let pool = crate::test_support::fresh_pool().await;
    let (set_id, variants) =
      seed_set(&pool, Some(10_000), &[6_000, 7_000], &[5, 5]).await;
    let cart_id = new_cart(&pool).await;
    add_set_to_cart(&pool, cart_id, set_id, 2)
      .await
      .expect("add set");
    set_variant_stock(&pool, variants[0], 0).await;

    let hydrated = hydrate_cart(&pool, cart_id).await.expect("hydrate");
    assert!(hydrated.items.is_empty());
    let left: i64 = sqlx::query_scalar(
      "SELECT COUNT(*) FROM cart_items WHERE cart_id = $1 AND set_id = $2",
    )
    .bind(cart_id)
    .bind(set_id)
    .fetch_one(&pool)
    .await
    .expect("count");
    assert_eq!(left, 0);
  }

  #[tokio::test]
  async fn hydrate_lowers_set_group_to_smallest_member() {
    let pool = crate::test_support::fresh_pool().await;
    let (set_id, variants) =
      seed_set(&pool, Some(10_000), &[6_000, 7_000], &[5, 5]).await;
    let cart_id = new_cart(&pool).await;
    add_set_to_cart(&pool, cart_id, set_id, 3)
      .await
      .expect("add set");
    set_variant_stock(&pool, variants[1], 1).await;

    let hydrated = hydrate_cart(&pool, cart_id).await.expect("hydrate");
    assert_eq!(hydrated.items.len(), 2);
    assert!(hydrated.items.iter().all(|item| item.quantity == 1));

    let mut tx = pool.begin().await.expect("tx");
    let items = quote_cart_for_checkout(&mut tx, cart_id)
      .await
      .expect("uniform group quotes");
    tx.rollback().await.expect("rollback");
    let total: i64 = items
      .iter()
      .map(|item| item.unit_price_cents * i64::from(item.quantity))
      .sum();
    assert_eq!(total, 10_000);
  }

  #[tokio::test]
  async fn loose_unit_of_set_variant_is_a_separate_full_price_line() {
    let pool = crate::test_support::fresh_pool().await;
    // Member list prices 8000 + 7000; set price 10000 (cheaper per unit).
    let (set_id, variants) =
      seed_set(&pool, Some(10_000), &[8_000, 7_000], &[5, 5]).await;
    let cart_id = new_cart(&pool).await;
    add_set_to_cart(&pool, cart_id, set_id, 1)
      .await
      .expect("add set");
    // Extra unit of a member variant through the normal add path.
    add_item(&pool, cart_id, variants[0], 1)
      .await
      .expect("loose");

    let rows: Vec<(Uuid, i32, Option<Uuid>)> = sqlx::query_as(
      "SELECT variant_id, quantity, set_id FROM cart_items WHERE cart_id = $1",
    )
    .bind(cart_id)
    .fetch_all(&pool)
    .await
    .expect("rows");
    assert_eq!(rows.len(), 3);
    let loose: Vec<_> = rows
      .iter()
      .filter(|(_, _, set_id)| set_id.is_none())
      .collect();
    assert_eq!(loose.len(), 1);
    assert_eq!(loose[0].0, variants[0]);

    let mut tx = pool.begin().await.expect("tx");
    let items = quote_cart_for_checkout(&mut tx, cart_id)
      .await
      .expect("quote");
    tx.rollback().await.expect("rollback");
    assert_eq!(items.len(), 3);
    // Set group totals 10000; loose unit keeps its 8000 list price.
    let loose_item = items
      .iter()
      .find(|item| item.set_id.is_none())
      .expect("loose item");
    assert_eq!(loose_item.unit_price_cents, 8_000);
    let total: i64 = items
      .iter()
      .map(|item| item.unit_price_cents * i64::from(item.quantity))
      .sum();
    assert_eq!(total, 10_000 + 8_000);
  }

  #[tokio::test]
  async fn quote_rejects_mismatched_member_quantities() {
    let pool = crate::test_support::fresh_pool().await;
    let (set_id, variants) =
      seed_set(&pool, Some(10_000), &[6_000, 7_000], &[5, 5]).await;
    let cart_id = new_cart(&pool).await;
    add_set_to_cart(&pool, cart_id, set_id, 1)
      .await
      .expect("add set");

    // Simulate the old exploit: extra units on one member only.
    sqlx::query(
      "UPDATE cart_items SET quantity = 4
       WHERE cart_id = $1 AND set_id = $2 AND variant_id = $3",
    )
    .bind(cart_id)
    .bind(set_id)
    .bind(variants[0])
    .execute(&pool)
    .await
    .expect("skew member");

    let mut tx = pool.begin().await.expect("tx");
    let outcome = quote_cart_for_checkout(&mut tx, cart_id).await;
    tx.rollback().await.expect("rollback");
    assert!(matches!(outcome, Err(QuoteError::Insufficient)));
  }

  #[tokio::test]
  async fn quote_sums_shared_variant_across_loose_and_set_lines() {
    let pool = crate::test_support::fresh_pool().await;
    let (set_id, variants) =
      seed_set(&pool, Some(10_000), &[8_000, 7_000], &[3, 5]).await;
    let cart_id = new_cart(&pool).await;
    add_set_to_cart(&pool, cart_id, set_id, 2)
      .await
      .expect("add set");
    // Shared variant has 3 in stock: 2 via the set + 2 loose = 4 > 3.
    add_item(&pool, cart_id, variants[0], 2)
      .await
      .expect("loose");

    let mut tx = pool.begin().await.expect("tx");
    let outcome = quote_cart_for_checkout(&mut tx, cart_id).await;
    tx.rollback().await.expect("rollback");
    assert!(matches!(outcome, Err(QuoteError::Insufficient)));
  }

  async fn seed_set_with_variants(
    pool: &PgPool,
    name: &str,
    price_cents: i64,
    member_variants: &[Uuid],
  ) -> Uuid {
    let set_id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO sets (id, name, slug, active, price_in_cents)
       VALUES ($1, $2, $3, true, $4)",
    )
    .bind(set_id)
    .bind(name)
    .bind(format!("conj-{set_id}"))
    .bind(price_cents)
    .execute(pool)
    .await
    .expect("set");
    for (index, variant_id) in member_variants.iter().enumerate() {
      let slot_id = Uuid::now_v7();
      sqlx::query(
        "INSERT INTO set_slots (id, set_id, sort_order) VALUES ($1, $2, $3)",
      )
      .bind(slot_id)
      .bind(set_id)
      .bind(index as i32)
      .execute(pool)
      .await
      .expect("slot");
      sqlx::query(
        "INSERT INTO set_slot_options (slot_id, variant_id) VALUES ($1, $2)",
      )
      .bind(slot_id)
      .bind(variant_id)
      .execute(pool)
      .await
      .expect("slot option");
    }
    set_id
  }

  #[tokio::test]
  async fn two_sets_sharing_a_variant_coexist_in_cart() {
    let pool = crate::test_support::fresh_pool().await;
    let shared = seed_set_variant(&pool, 8_000, 6).await;
    let other_a = seed_set_variant(&pool, 7_000, 6).await;
    let other_b = seed_set_variant(&pool, 6_000, 6).await;
    let set_a =
      seed_set_with_variants(&pool, "Conjunto A", 10_000, &[shared, other_a])
        .await;
    let set_b =
      seed_set_with_variants(&pool, "Conjunto B", 12_000, &[shared, other_b])
        .await;
    let cart_id = new_cart(&pool).await;

    add_set_to_cart(&pool, cart_id, set_a, 1)
      .await
      .expect("add set a");
    add_set_to_cart(&pool, cart_id, set_b, 1)
      .await
      .expect("add set b");

    let lines: i64 =
      sqlx::query_scalar("SELECT COUNT(*) FROM cart_items WHERE cart_id = $1")
        .bind(cart_id)
        .fetch_one(&pool)
        .await
        .expect("lines");
    // Two member rows per set; the shared variant has one row per set.
    assert_eq!(lines, 4);

    let mut tx = pool.begin().await.expect("tx");
    let items = quote_cart_for_checkout(&mut tx, cart_id)
      .await
      .expect("quote");
    tx.rollback().await.expect("rollback");
    assert_eq!(items.len(), 4);
    let total: i64 = items
      .iter()
      .map(|item| item.unit_price_cents * i64::from(item.quantity))
      .sum();
    assert_eq!(total, 10_000 + 12_000);
  }
}
