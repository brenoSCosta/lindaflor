use uuid::Uuid;

use super::queries::format_price;

/// Row mirroring the `coupons` table (see `migrations/0004_coupons.sql`).
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CouponRow {
  pub id: Uuid,
  pub code: String,
  pub discount_type: String,
  pub discount_value: i64,
  pub min_subtotal_cents: i64,
  pub max_discount_cents: Option<i64>,
  pub active: bool,
  pub starts_at: Option<time::OffsetDateTime>,
  pub expires_at: Option<time::OffsetDateTime>,
  pub usage_type: String,
  pub max_uses: Option<i32>,
  pub per_user_limit: i32,
}

impl CouponRow {
  pub fn is_unique(&self) -> bool {
    self.usage_type == "unique"
  }

  /// Effective total-use cap. `unique` coupons are single-use even when
  /// `max_uses` is NULL; `unlimited` coupons use `max_uses` (NULL = no cap).
  pub fn effective_max_uses(&self) -> Option<i32> {
    if self.is_unique() {
      Some(1)
    } else {
      self.max_uses
    }
  }
}

/// Compute the discount in cents for `coupon` given `subtotal_cents`.
///
/// - Returns `0` when `subtotal_cents < min_subtotal_cents`.
/// - `fixed` => `min(value, subtotal).max(0)`.
/// - `percent` => `floor(subtotal * value / 100)`, then
///   `min(max_discount_cents if Some, subtotal).max(0)`.
/// - Unknown `discount_type` => `0`.
/// - Uses `i64` intermediates to avoid overflow.
pub fn calc_coupon_discount(coupon: &CouponRow, subtotal_cents: i64) -> i64 {
  if subtotal_cents <= 0 {
    return 0;
  }
  if subtotal_cents < coupon.min_subtotal_cents {
    return 0;
  }
  match coupon.discount_type.as_str() {
    "fixed" => coupon.discount_value.min(subtotal_cents).max(0),
    "percent" => {
      let mut discount =
        subtotal_cents.saturating_mul(coupon.discount_value) / 100;
      if let Some(cap) = coupon.max_discount_cents {
        discount = discount.min(cap);
      }
      discount.min(subtotal_cents).max(0)
    }
    _ => 0,
  }
}

/// Normalize an email for per-person coupon limits: trim + lowercase, drop a
/// `+tag` from the local part, and fold Gmail aliases (dots in the local part,
/// `googlemail.com`) onto one `@gmail.com` address. Returns `None` for blank
/// input. Keep in sync with the backfill in `0018_coupon_identity.sql`.
pub fn normalize_email(raw: &str) -> Option<String> {
  let lower = raw.trim().to_lowercase();
  if lower.is_empty() {
    return None;
  }
  let Some((local, domain)) = lower.rsplit_once('@') else {
    return Some(lower);
  };
  let local = local.split_once('+').map_or(local, |(base, _)| base);
  if local.is_empty() || domain.is_empty() {
    return Some(lower);
  }
  if domain == "gmail.com" || domain == "googlemail.com" {
    return Some(format!("{}@gmail.com", local.replace('.', "")));
  }
  Some(format!("{local}@{domain}"))
}

pub struct ResolvedCoupon {
  pub id: Uuid,
  pub code: String,
  pub discount_cents: i64,
  /// Normalized identity email the per-user limit was checked against; stored
  /// on the redemption by [`record_redemption`].
  pub email_norm: Option<String>,
}

/// User-facing message for [`CouponReject::Gone`].
pub const COUPON_GONE_MESSAGE: &str = "Cupom inválido.";

pub enum CouponReject {
  /// The code doesn't exist or the coupon was deactivated. Checkout drops it
  /// from the cart instead of blocking the order.
  Gone,
  Invalid(String),
  Db(sqlx::Error),
}

impl From<sqlx::Error> for CouponReject {
  fn from(error: sqlx::Error) -> Self {
    Self::Db(error)
  }
}

/// Validate a coupon code against the DB and compute its discount.
///
/// Checks, in order: code lookup (case-insensitive), `active` flag and
/// `starts_at`/`expires_at` window, `min_subtotal_cents`, assignment
/// restriction (when the coupon has any `coupon_assignments` rows only the
/// assigned logged-in user may use it — guests are rejected), global
/// `COUNT(*) vs effective_max_uses` (`unique` counts as 1 even when
/// `max_uses` is NULL), and per-identity `COUNT(*) vs per_user_limit`
/// matching `user_id = $uid OR email_norm = $email`, where the email is the
/// account email for logged-in users and the checkout email for guests, both
/// passed through [`normalize_email`] (so logging out, `+tag` or Gmail-dot
/// variants share one limit). Active redemptions only (`refunded_at IS
/// NULL`). Missing identity on a per-user coupon is a reject (login or
/// checkout email). Unknown or inactive codes are [`CouponReject::Gone`].
///
/// Discount math is shared via [`calc_coupon_discount`]. Shipping is computed
/// on the pre-discount subtotal by the caller. Pass `for_update = true`
/// inside the order transaction to lock the coupon row while re-validating.
pub async fn resolve_coupon(
  tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  raw_code: &str,
  subtotal_cents: i64,
  user_id: Option<Uuid>,
  guest_email: Option<&str>,
  for_update: bool,
) -> Result<Option<ResolvedCoupon>, CouponReject> {
  let code = raw_code.trim();
  if code.is_empty() {
    return Ok(None);
  }

  let lock = if for_update { " FOR UPDATE" } else { "" };
  let coupon = sqlx::query_as::<_, CouponRow>(&format!(
    "SELECT id, code, discount_type, discount_value, min_subtotal_cents,
        max_discount_cents, active, starts_at, expires_at, usage_type,
        max_uses, per_user_limit
     FROM coupons WHERE UPPER(code) = UPPER($1){lock}"
  ))
  .bind(code)
  .fetch_optional(&mut **tx)
  .await?;

  let Some(coupon) = coupon else {
    return Err(CouponReject::Gone);
  };

  let coupon_id = coupon.id;

  if !coupon.active {
    return Err(CouponReject::Gone);
  }
  let now = time::OffsetDateTime::now_utc();
  if coupon.starts_at.is_some_and(|starts_at| starts_at > now) {
    return Err(CouponReject::Invalid(
      "Este cupom ainda não está válido.".to_string(),
    ));
  }
  if coupon
    .expires_at
    .is_some_and(|expires_at| expires_at <= now)
  {
    return Err(CouponReject::Invalid("Cupom expirado.".to_string()));
  }
  if subtotal_cents < coupon.min_subtotal_cents {
    return Err(CouponReject::Invalid(format!(
      "Este cupom exige uma compra mínima de {}.",
      format_price(coupon.min_subtotal_cents)
    )));
  }

  let assignment_count: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM coupon_assignments WHERE coupon_id = $1",
  )
  .bind(coupon_id)
  .fetch_one(&mut **tx)
  .await?;

  if assignment_count > 0 {
    let Some(uid) = user_id else {
      return Err(CouponReject::Invalid(
        "Este cupom é restrito a contas específicas. Entre para usá-lo."
          .to_string(),
      ));
    };
    let is_member: bool = sqlx::query_scalar(
      "SELECT EXISTS(SELECT 1 FROM coupon_assignments WHERE coupon_id = $1 AND user_id = $2)",
    )
    .bind(coupon_id)
    .bind(uid)
    .fetch_one(&mut **tx)
    .await?;
    if !is_member {
      return Err(CouponReject::Invalid(
        "Este cupom não está disponível para a sua conta.".to_string(),
      ));
    }
  }

  if let Some(cap) = coupon.effective_max_uses() {
    let global_redemptions: i64 = sqlx::query_scalar(
      "SELECT COUNT(*) FROM coupon_redemptions
       WHERE coupon_id = $1 AND refunded_at IS NULL",
    )
    .bind(coupon_id)
    .fetch_one(&mut **tx)
    .await?;
    if global_redemptions >= i64::from(cap) {
      return Err(CouponReject::Invalid(
        "Este cupom atingiu o limite de utilizações.".to_string(),
      ));
    }
  }

  let account_email: Option<String> = match user_id {
    Some(uid) => {
      sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(uid)
        .fetch_optional(&mut **tx)
        .await?
    }
    None => None,
  };
  let email_norm = account_email
    .as_deref()
    .and_then(normalize_email)
    .or_else(|| guest_email.and_then(normalize_email));
  if user_id.is_none() && email_norm.is_none() {
    return Err(CouponReject::Invalid(
      "Entre para usar este cupom, ou informe seu e-mail no checkout."
        .to_string(),
    ));
  }
  let personal_redemptions: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM coupon_redemptions
     WHERE coupon_id = $1
       AND refunded_at IS NULL
       AND (user_id = $2 OR email_norm = $3)",
  )
  .bind(coupon_id)
  .bind(user_id)
  .bind(email_norm.as_deref())
  .fetch_one(&mut **tx)
  .await?;
  if personal_redemptions >= i64::from(coupon.per_user_limit) {
    return Err(CouponReject::Invalid(
      "Você já utilizou este cupom o máximo de vezes permitido.".to_string(),
    ));
  }

  Ok(Some(ResolvedCoupon {
    id: coupon.id,
    code: coupon.code.clone(),
    discount_cents: calc_coupon_discount(&coupon, subtotal_cents),
    email_norm,
  }))
}

/// Record that `order_id` consumed `coupon`. `guest_email` is the raw
/// checkout email, kept for guests only; the normalized identity email is
/// always stored so later per-user checks match across login state.
pub async fn record_redemption(
  tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  coupon: &ResolvedCoupon,
  user_id: Option<Uuid>,
  order_id: Uuid,
  guest_email: Option<&str>,
) -> Result<(), sqlx::Error> {
  sqlx::query(
    "INSERT INTO coupon_redemptions
       (id, coupon_id, user_id, order_id, guest_email, email_norm)
     VALUES ($1, $2, $3, $4, $5, $6)",
  )
  .bind(Uuid::now_v7())
  .bind(coupon.id)
  .bind(user_id)
  .bind(order_id)
  .bind(guest_email)
  .bind(coupon.email_norm.as_deref())
  .execute(&mut **tx)
  .await?;
  Ok(())
}

pub async fn restore_redemption_for_order(
  tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  order_id: Uuid,
) -> Result<(), sqlx::Error> {
  sqlx::query(
    "UPDATE coupon_redemptions SET refunded_at = now()
     WHERE order_id = $1 AND refunded_at IS NULL",
  )
  .bind(order_id)
  .execute(&mut **tx)
  .await?;
  Ok(())
}

/// Re-apply the coupon redemption after an order reopen: a cancelled order
/// had its redemption marked `refunded_at`, and reopening consumes the
/// coupon again.
pub async fn reclaim_redemption_for_order(
  tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
  order_id: Uuid,
) -> Result<(), sqlx::Error> {
  sqlx::query(
    "UPDATE coupon_redemptions SET refunded_at = NULL
     WHERE order_id = $1 AND refunded_at IS NOT NULL",
  )
  .bind(order_id)
  .execute(&mut **tx)
  .await?;
  Ok(())
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::app::store::queries::format_price;

  fn coupon(
    discount_type: &str,
    discount_value: i64,
    min_subtotal_cents: i64,
    max_discount_cents: Option<i64>,
  ) -> CouponRow {
    CouponRow {
      id: Uuid::now_v7(),
      code: "TEST".to_string(),
      discount_type: discount_type.to_string(),
      discount_value,
      min_subtotal_cents,
      max_discount_cents,
      active: true,
      starts_at: None,
      expires_at: None,
      usage_type: "unlimited".to_string(),
      max_uses: None,
      per_user_limit: 1,
    }
  }

  #[test]
  fn fixed_is_capped_at_subtotal() {
    let c = coupon("fixed", 500, 0, None);
    assert_eq!(calc_coupon_discount(&c, 300), 300);
    assert_eq!(calc_coupon_discount(&c, 500), 500);
    assert_eq!(calc_coupon_discount(&c, 1000), 500);
    assert_eq!(calc_coupon_discount(&c, 0), 0);
  }

  #[test]
  fn percent_floors_and_caps() {
    let c = coupon("percent", 10, 0, None);
    assert_eq!(calc_coupon_discount(&c, 999), 99);
    assert_eq!(calc_coupon_discount(&c, 1000), 100);

    let capped = coupon("percent", 50, 0, Some(200));
    assert_eq!(calc_coupon_discount(&c, 10_000), 1000);
    assert_eq!(calc_coupon_discount(&capped, 10_000), 200);
    // Never exceeds subtotal.
    let full = coupon("percent", 50, 0, None);
    assert_eq!(calc_coupon_discount(&full, 100), 50);
  }

  #[test]
  fn respects_min_subtotal() {
    let c = coupon("fixed", 100, 1000, None);
    assert_eq!(calc_coupon_discount(&c, 999), 0);
    assert_eq!(calc_coupon_discount(&c, 1000), 100);
  }

  #[test]
  fn unique_implies_single_use() {
    let mut c = coupon("fixed", 100, 0, None);
    c.usage_type = "unique".to_string();
    assert_eq!(c.effective_max_uses(), Some(1));
    c.usage_type = "unlimited".to_string();
    c.max_uses = Some(5);
    assert_eq!(c.effective_max_uses(), Some(5));
    c.max_uses = None;
    assert_eq!(c.effective_max_uses(), None);
  }

  fn unique_code(prefix: &str) -> String {
    format!("{prefix}-{}", Uuid::now_v7().simple())
  }

  struct NewCoupon {
    code: String,
    discount_type: String,
    discount_value: i64,
    min_subtotal_cents: i64,
    max_discount_cents: Option<i64>,
    active: bool,
    starts_at: Option<time::OffsetDateTime>,
    expires_at: Option<time::OffsetDateTime>,
    usage_type: String,
    max_uses: Option<i32>,
    per_user_limit: i32,
  }

  fn new_coupon(code: &str) -> NewCoupon {
    NewCoupon {
      code: code.to_string(),
      discount_type: "fixed".to_string(),
      discount_value: 1_000,
      min_subtotal_cents: 0,
      max_discount_cents: None,
      active: true,
      starts_at: None,
      expires_at: None,
      usage_type: "unlimited".to_string(),
      max_uses: None,
      per_user_limit: 1,
    }
  }

  fn as_row(id: Uuid, coupon: &NewCoupon) -> CouponRow {
    CouponRow {
      id,
      code: coupon.code.clone(),
      discount_type: coupon.discount_type.clone(),
      discount_value: coupon.discount_value,
      min_subtotal_cents: coupon.min_subtotal_cents,
      max_discount_cents: coupon.max_discount_cents,
      active: coupon.active,
      starts_at: coupon.starts_at,
      expires_at: coupon.expires_at,
      usage_type: coupon.usage_type.clone(),
      max_uses: coupon.max_uses,
      per_user_limit: coupon.per_user_limit,
    }
  }

  async fn insert_coupon(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    coupon: &NewCoupon,
  ) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO coupons (
         id, code, discount_type, discount_value, min_subtotal_cents,
         max_discount_cents, active, starts_at, expires_at, usage_type,
         max_uses, per_user_limit
       ) VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)",
    )
    .bind(id)
    .bind(&coupon.code)
    .bind(&coupon.discount_type)
    .bind(coupon.discount_value)
    .bind(coupon.min_subtotal_cents)
    .bind(coupon.max_discount_cents)
    .bind(coupon.active)
    .bind(coupon.starts_at)
    .bind(coupon.expires_at)
    .bind(&coupon.usage_type)
    .bind(coupon.max_uses)
    .bind(coupon.per_user_limit)
    .execute(&mut **tx)
    .await
    .expect("insert coupon");
    id
  }

  async fn insert_user(tx: &mut sqlx::Transaction<'_, sqlx::Postgres>) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, name, email) VALUES ($1, $2, $3)")
      .bind(id)
      .bind("Test User")
      .bind(format!("{id}@example.com"))
      .execute(&mut **tx)
      .await
      .expect("insert user");
    id
  }

  async fn insert_order(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    user_id: Option<Uuid>,
  ) -> Uuid {
    let id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO orders (id, user_id, access_token) VALUES ($1, $2, $3)",
    )
    .bind(id)
    .bind(user_id)
    .bind(Uuid::now_v7())
    .execute(&mut **tx)
    .await
    .expect("insert order");
    id
  }

  async fn insert_redemption(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    coupon_id: Uuid,
    user_id: Option<Uuid>,
    order_id: Uuid,
    guest_email: Option<&str>,
  ) {
    sqlx::query(
      "INSERT INTO coupon_redemptions
         (id, coupon_id, user_id, order_id, guest_email, email_norm)
       VALUES ($1, $2, $3, $4, $5, $6)",
    )
    .bind(Uuid::now_v7())
    .bind(coupon_id)
    .bind(user_id)
    .bind(order_id)
    .bind(guest_email)
    .bind(guest_email.and_then(normalize_email))
    .execute(&mut **tx)
    .await
    .expect("insert redemption");
  }

  fn unwrap_coupon(
    result: Result<Option<ResolvedCoupon>, CouponReject>,
  ) -> ResolvedCoupon {
    match result {
      Ok(Some(coupon)) => coupon,
      Ok(None) => panic!("expected a coupon"),
      Err(CouponReject::Gone) => panic!("rejected: gone"),
      Err(CouponReject::Invalid(message)) => panic!("rejected: {message}"),
      Err(CouponReject::Db(error)) => panic!("db error: {error}"),
    }
  }

  fn unwrap_invalid(
    result: Result<Option<ResolvedCoupon>, CouponReject>,
  ) -> String {
    match result {
      Err(CouponReject::Invalid(message)) => message,
      Ok(Some(coupon)) => {
        panic!("expected rejection, resolved {}", coupon.code)
      }
      Ok(None) => panic!("expected rejection, got none"),
      Err(CouponReject::Gone) => panic!("expected invalid, got gone"),
      Err(CouponReject::Db(error)) => panic!("db error: {error}"),
    }
  }

  #[test]
  fn normalize_email_folds_aliases() {
    assert_eq!(normalize_email("  "), None);
    assert_eq!(
      normalize_email(" Maria@Example.COM ").as_deref(),
      Some("maria@example.com")
    );
    assert_eq!(
      normalize_email("maria+promo@example.com").as_deref(),
      Some("maria@example.com")
    );
    // Dots only fold on Gmail.
    assert_eq!(
      normalize_email("ma.ria@example.com").as_deref(),
      Some("ma.ria@example.com")
    );
    assert_eq!(
      normalize_email("Ma.Ria+x@Gmail.com").as_deref(),
      Some("maria@gmail.com")
    );
    assert_eq!(
      normalize_email("m.a.r.i.a@googlemail.com").as_deref(),
      Some("maria@gmail.com")
    );
    // Degenerate inputs stay as lowercased raw text.
    assert_eq!(
      normalize_email("+tag@example.com").as_deref(),
      Some("+tag@example.com")
    );
    assert_eq!(normalize_email("NoAt").as_deref(), Some("noat"));
  }

  #[tokio::test]
  async fn blank_code_returns_none() {
    let pool = crate::test_support::fresh_pool().await;
    let mut tx = pool.begin().await.expect("begin tx");

    let result =
      resolve_coupon(&mut tx, "   ", 5_000, None, Some("g@x.co"), false).await;
    assert!(matches!(result, Ok(None)));

    tx.rollback().await.expect("rollback");
  }

  #[tokio::test]
  async fn unknown_or_inactive_code_is_invalid() {
    let pool = crate::test_support::fresh_pool().await;
    let mut tx = pool.begin().await.expect("begin tx");

    let unknown = resolve_coupon(
      &mut tx,
      &unique_code("missing"),
      5_000,
      None,
      Some("g@x.co"),
      false,
    )
    .await;
    assert!(matches!(unknown, Err(CouponReject::Gone)));

    let code = unique_code("off");
    let mut coupon = new_coupon(&code);
    coupon.active = false;
    insert_coupon(&mut tx, &coupon).await;
    let inactive =
      resolve_coupon(&mut tx, &code, 5_000, None, Some("g@x.co"), false).await;
    assert!(matches!(inactive, Err(CouponReject::Gone)));

    tx.rollback().await.expect("rollback");
  }

  #[tokio::test]
  async fn future_start_and_past_expiry_messages() {
    let pool = crate::test_support::fresh_pool().await;
    let mut tx = pool.begin().await.expect("begin tx");
    let now = time::OffsetDateTime::now_utc();

    let future_code = unique_code("soon");
    let mut future = new_coupon(&future_code);
    future.starts_at = Some(now + time::Duration::hours(1));
    insert_coupon(&mut tx, &future).await;
    let message = unwrap_invalid(
      resolve_coupon(&mut tx, &future_code, 5_000, None, Some("g@x.co"), false)
        .await,
    );
    assert_eq!(message, "Este cupom ainda não está válido.");

    let expired_code = unique_code("old");
    let mut expired = new_coupon(&expired_code);
    expired.expires_at = Some(now - time::Duration::hours(1));
    insert_coupon(&mut tx, &expired).await;
    let message = unwrap_invalid(
      resolve_coupon(
        &mut tx,
        &expired_code,
        5_000,
        None,
        Some("g@x.co"),
        false,
      )
      .await,
    );
    assert_eq!(message, "Cupom expirado.");

    tx.rollback().await.expect("rollback");
  }

  #[tokio::test]
  async fn subtotal_under_minimum_mentions_price() {
    let pool = crate::test_support::fresh_pool().await;
    let mut tx = pool.begin().await.expect("begin tx");

    let code = unique_code("min");
    let mut coupon = new_coupon(&code);
    coupon.min_subtotal_cents = 15_000;
    insert_coupon(&mut tx, &coupon).await;

    let message = unwrap_invalid(
      resolve_coupon(&mut tx, &code, 1_000, None, Some("g@x.co"), false).await,
    );
    assert_eq!(
      message,
      format!(
        "Este cupom exige uma compra mínima de {}.",
        format_price(coupon.min_subtotal_cents)
      )
    );

    tx.rollback().await.expect("rollback");
  }

  #[tokio::test]
  async fn fixed_and_percent_return_stored_code_and_discount() {
    let pool = crate::test_support::fresh_pool().await;
    let mut tx = pool.begin().await.expect("begin tx");
    let subtotal = 10_000;

    let fixed_code = unique_code("Fixo");
    let mut fixed = new_coupon(&fixed_code);
    fixed.discount_type = "fixed".to_string();
    fixed.discount_value = 2_500;
    let fixed_id = insert_coupon(&mut tx, &fixed).await;
    let resolved = unwrap_coupon(
      resolve_coupon(
        &mut tx,
        &fixed_code.to_lowercase(),
        subtotal,
        None,
        Some("g@x.co"),
        false,
      )
      .await,
    );
    assert_eq!(resolved.code, fixed_code);
    assert_eq!(
      resolved.discount_cents,
      calc_coupon_discount(&as_row(fixed_id, &fixed), subtotal)
    );

    let percent_code = unique_code("Pct");
    let mut percent = new_coupon(&percent_code);
    percent.discount_type = "percent".to_string();
    percent.discount_value = 40;
    percent.max_discount_cents = Some(500);
    let percent_id = insert_coupon(&mut tx, &percent).await;
    let resolved = unwrap_coupon(
      resolve_coupon(
        &mut tx,
        &percent_code.to_lowercase(),
        subtotal,
        None,
        Some("g@x.co"),
        false,
      )
      .await,
    );
    assert_eq!(resolved.code, percent_code);
    assert_eq!(
      resolved.discount_cents,
      calc_coupon_discount(&as_row(percent_id, &percent), subtotal)
    );

    tx.rollback().await.expect("rollback");
  }

  #[tokio::test]
  async fn assignments_open_for_guest_and_restricted_to_member() {
    let pool = crate::test_support::fresh_pool().await;
    let mut tx = pool.begin().await.expect("begin tx");

    let open_code = unique_code("open");
    insert_coupon(&mut tx, &new_coupon(&open_code)).await;
    let resolved = unwrap_coupon(
      resolve_coupon(&mut tx, &open_code, 5_000, None, Some("g@x.co"), false)
        .await,
    );
    assert_eq!(resolved.code, open_code);

    let restricted_code = unique_code("vip");
    let coupon_id = insert_coupon(&mut tx, &new_coupon(&restricted_code)).await;
    let member = insert_user(&mut tx).await;
    let outsider = insert_user(&mut tx).await;
    sqlx::query(
      "INSERT INTO coupon_assignments (coupon_id, user_id) VALUES ($1, $2)",
    )
    .bind(coupon_id)
    .bind(member)
    .execute(&mut *tx)
    .await
    .expect("assign coupon");

    let guest = unwrap_invalid(
      resolve_coupon(
        &mut tx,
        &restricted_code,
        5_000,
        None,
        Some("g@x.co"),
        false,
      )
      .await,
    );
    assert_eq!(
      guest,
      "Este cupom é restrito a contas específicas. Entre para usá-lo."
    );

    let rejected = unwrap_invalid(
      resolve_coupon(
        &mut tx,
        &restricted_code,
        5_000,
        Some(outsider),
        None,
        false,
      )
      .await,
    );
    assert_eq!(rejected, "Este cupom não está disponível para a sua conta.");

    let accepted = unwrap_coupon(
      resolve_coupon(
        &mut tx,
        &restricted_code,
        5_000,
        Some(member),
        None,
        false,
      )
      .await,
    );
    assert_eq!(accepted.code, restricted_code);

    tx.rollback().await.expect("rollback");
  }

  #[tokio::test]
  async fn global_cap_unique_unlimited_and_null() {
    let pool = crate::test_support::fresh_pool().await;
    let mut tx = pool.begin().await.expect("begin tx");

    let once_code = unique_code("once");
    let mut unique = new_coupon(&once_code);
    unique.usage_type = "unique".to_string();
    unique.max_uses = None;
    let unique_id = insert_coupon(&mut tx, &unique).await;
    let order_id = insert_order(&mut tx, None).await;
    insert_redemption(&mut tx, unique_id, None, order_id, Some("g@x.co")).await;
    let message = unwrap_invalid(
      resolve_coupon(&mut tx, &once_code, 5_000, None, Some("g@x.co"), false)
        .await,
    );
    assert_eq!(message, "Este cupom atingiu o limite de utilizações.");

    let capped_code = unique_code("cap");
    let mut capped = new_coupon(&capped_code);
    capped.usage_type = "unlimited".to_string();
    capped.max_uses = Some(2);
    let capped_id = insert_coupon(&mut tx, &capped).await;
    for _ in 0..2 {
      let order_id = insert_order(&mut tx, None).await;
      insert_redemption(&mut tx, capped_id, None, order_id, Some("g@x.co"))
        .await;
    }
    let message = unwrap_invalid(
      resolve_coupon(&mut tx, &capped_code, 5_000, None, Some("g@x.co"), false)
        .await,
    );
    assert_eq!(message, "Este cupom atingiu o limite de utilizações.");

    let open_code = unique_code("nocap");
    let mut open = new_coupon(&open_code);
    open.usage_type = "unlimited".to_string();
    open.max_uses = None;
    open.per_user_limit = 10;
    let open_id = insert_coupon(&mut tx, &open).await;
    for _ in 0..3 {
      let order_id = insert_order(&mut tx, None).await;
      insert_redemption(&mut tx, open_id, None, order_id, Some("g@x.co")).await;
    }
    let resolved = unwrap_coupon(
      resolve_coupon(&mut tx, &open_code, 5_000, None, Some("g@x.co"), false)
        .await,
    );
    assert_eq!(resolved.code, open_code);

    tx.rollback().await.expect("rollback");
  }

  #[tokio::test]
  async fn per_user_cap_keys_guest_email_and_requires_identity() {
    let pool = crate::test_support::fresh_pool().await;
    let mut tx = pool.begin().await.expect("begin tx");

    let code = unique_code("personal");
    let mut coupon = new_coupon(&code);
    coupon.per_user_limit = 1;
    coupon.max_uses = None;
    let coupon_id = insert_coupon(&mut tx, &coupon).await;

    let user = insert_user(&mut tx).await;
    let order_id = insert_order(&mut tx, Some(user)).await;
    insert_redemption(&mut tx, coupon_id, Some(user), order_id, None).await;

    let message = unwrap_invalid(
      resolve_coupon(&mut tx, &code, 5_000, Some(user), None, false).await,
    );
    assert_eq!(
      message,
      "Você já utilizou este cupom o máximo de vezes permitido."
    );

    let no_id = unwrap_invalid(
      resolve_coupon(&mut tx, &code, 5_000, None, None, false).await,
    );
    assert_eq!(
      no_id,
      "Entre para usar este cupom, ou informe seu e-mail no checkout."
    );

    let guest_order = insert_order(&mut tx, None).await;
    insert_redemption(
      &mut tx,
      coupon_id,
      None,
      guest_order,
      Some("guest@x.co"),
    )
    .await;
    let same_guest = unwrap_invalid(
      resolve_coupon(&mut tx, &code, 5_000, None, Some("guest@x.co"), false)
        .await,
    );
    assert_eq!(
      same_guest,
      "Você já utilizou este cupom o máximo de vezes permitido."
    );

    let other = unwrap_coupon(
      resolve_coupon(&mut tx, &code, 5_000, None, Some("other@x.co"), false)
        .await,
    );
    assert_eq!(other.code, code);

    restore_redemption_for_order(&mut tx, guest_order)
      .await
      .expect("restore");
    let after_restore = unwrap_coupon(
      resolve_coupon(&mut tx, &code, 5_000, None, Some("guest@x.co"), false)
        .await,
    );
    assert_eq!(after_restore.code, code);

    tx.rollback().await.expect("rollback");
  }

  #[tokio::test]
  async fn per_user_cap_follows_email_across_login_and_aliases() {
    let pool = crate::test_support::fresh_pool().await;
    let mut tx = pool.begin().await.expect("begin tx");

    let code = unique_code("Primeira");
    let mut coupon = new_coupon(&code);
    coupon.per_user_limit = 1;
    insert_coupon(&mut tx, &coupon).await;

    let local = format!("maria.{}", Uuid::now_v7().simple());
    let user = Uuid::now_v7();
    sqlx::query("INSERT INTO users (id, name, email) VALUES ($1, $2, $3)")
      .bind(user)
      .bind("Maria")
      .bind(format!("{local}@Gmail.com"))
      .execute(&mut *tx)
      .await
      .expect("insert user");

    // Logged-in checkout with a different typed email: the account email is
    // what gets recorded.
    let resolved = unwrap_coupon(
      resolve_coupon(
        &mut tx,
        &code,
        5_000,
        Some(user),
        Some("whatever@example.com"),
        true,
      )
      .await,
    );
    let order_id = insert_order(&mut tx, Some(user)).await;
    record_redemption(&mut tx, &resolved, Some(user), order_id, None)
      .await
      .expect("record redemption");
    let stored: Option<String> = sqlx::query_scalar(
      "SELECT email_norm FROM coupon_redemptions WHERE order_id = $1",
    )
    .bind(order_id)
    .fetch_one(&mut *tx)
    .await
    .expect("stored email");
    let canonical = format!("{}@gmail.com", local.replace('.', ""));
    assert_eq!(stored.as_deref(), Some(canonical.as_str()));

    let limit = "Você já utilizou este cupom o máximo de vezes permitido.";
    let (dotless, rest) = local.split_once('.').expect("dotted local");
    for guest in [
      format!("{local}@gmail.com"),
      format!("{}@GMAIL.com", local.to_uppercase()),
      format!("{local}+promo@gmail.com"),
      format!("{dotless}{rest}@googlemail.com"),
      format!("{dotless}.{rest}+x@googlemail.com"),
    ] {
      let message = unwrap_invalid(
        resolve_coupon(&mut tx, &code, 5_000, None, Some(&guest), false).await,
      );
      assert_eq!(message, limit, "guest {guest}");
    }

    // A different person is unaffected.
    let other = unwrap_coupon(
      resolve_coupon(
        &mut tx,
        &code,
        5_000,
        None,
        Some(&format!("outra.{}@gmail.com", Uuid::now_v7().simple())),
        false,
      )
      .await,
    );
    assert_eq!(other.code, code);

    tx.rollback().await.expect("rollback");
  }

  #[tokio::test]
  async fn code_is_unique_case_insensitively() {
    let pool = crate::test_support::fresh_pool().await;
    let mut tx = pool.begin().await.expect("begin tx");

    let code = unique_code("Dup");
    insert_coupon(&mut tx, &new_coupon(&code)).await;
    let duplicate = sqlx::query(
      "INSERT INTO coupons (id, code, discount_type, discount_value)
       VALUES ($1, $2, 'fixed', 100)",
    )
    .bind(Uuid::now_v7())
    .bind(code.to_lowercase())
    .execute(&mut *tx)
    .await;
    let error = duplicate.expect_err("case-variant code must collide");
    assert!(
      error
        .as_database_error()
        .is_some_and(|db| db.is_unique_violation()),
      "unexpected error: {error}"
    );

    tx.rollback().await.expect("rollback");
  }

  #[tokio::test]
  async fn for_update_resolves_inside_transaction() {
    let pool = crate::test_support::fresh_pool().await;
    let mut tx = pool.begin().await.expect("begin tx");

    let code = unique_code("lock");
    let coupon = new_coupon(&code);
    let id = insert_coupon(&mut tx, &coupon).await;
    let subtotal = 8_000;
    let resolved = unwrap_coupon(
      resolve_coupon(&mut tx, &code, subtotal, None, Some("g@x.co"), true)
        .await,
    );
    assert_eq!(resolved.code, code);
    assert_eq!(
      resolved.discount_cents,
      calc_coupon_discount(&as_row(id, &coupon), subtotal)
    );

    tx.rollback().await.expect("rollback");
  }
}
