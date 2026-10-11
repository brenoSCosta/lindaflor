use sqlx::PgPool;
use topcoat::{Result, context::Cx, context::app_context};

/// Price after the PIX discount; the discount is rounded down to the cent.
pub fn pix_price(cents: i64, discount_percent: u32) -> i64 {
  cents - pix_discount_cents(cents, discount_percent)
}

/// PIX discount in cents, rounded down so the customer is never overcharged
/// relative to the advertised percentage. Negative amounts yield no discount.
pub fn pix_discount_cents(cents: i64, discount_percent: u32) -> i64 {
  if cents <= 0 {
    return 0;
  }
  cents.saturating_mul(i64::from(discount_percent.min(100))) / 100
}

/// Percentage discount: (compare_at - price) / compare_at * 100. None if no compare_at or compare_at <= price.
pub fn discount_percent(compare_at: Option<i64>, price: i64) -> Option<u8> {
  compare_at.and_then(|ca| {
    if ca > price {
      Some(((ca - price) as f64 / ca as f64 * 100.0).round() as u8)
    } else {
      None
    }
  })
}

/// Load PIX %, free shipping threshold, announcement, first-purchase coupon from store_settings.
pub async fn store_pricing(cx: &Cx) -> Result<StorePricing> {
  let pool = app_context::<PgPool>(cx);
  let settings = sqlx::query!(
    "SELECT pix_discount_percent, free_shipping_threshold_cents, announcement_text, first_purchase_coupon_code FROM store_settings LIMIT 1"
  )
    .fetch_optional(pool)
    .await?;

  Ok(StorePricing {
    pix_discount_percent: settings
      .as_ref()
      .map(|s| s.pix_discount_percent as u32)
      .unwrap_or(5),
    free_shipping_threshold_cents: settings
      .as_ref()
      .map(|s| s.free_shipping_threshold_cents)
      .unwrap_or(29900),
    announcement: settings
      .as_ref()
      .and_then(|s| s.announcement_text.clone())
      .unwrap_or_default(),
    first_purchase_coupon_code: settings
      .and_then(|s| s.first_purchase_coupon_code),
  })
}

pub struct StorePricing {
  pub pix_discount_percent: u32,
  pub free_shipping_threshold_cents: i64,
  pub announcement: String,
  pub first_purchase_coupon_code: Option<String>,
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn test_pix_price() {
    assert_eq!(pix_price(10000, 5), 9500); // 100 - 5% = 95
    assert_eq!(pix_price(10000, 10), 9000);
    assert_eq!(pix_price(10001, 5), 9501); // discount 500.05 rounds down to 500
  }

  #[test]
  fn test_pix_discount_cents() {
    assert_eq!(pix_discount_cents(10000, 5), 500);
    assert_eq!(pix_discount_cents(10000, 10), 1000);
    assert_eq!(pix_discount_cents(10001, 5), 500); // rounds down
    assert_eq!(pix_discount_cents(-100, 5), 0);
    assert_eq!(pix_discount_cents(10000, 0), 0);
  }

  #[test]
  fn test_discount_percent() {
    assert_eq!(discount_percent(Some(10000), 9000), Some(10));
    assert_eq!(discount_percent(Some(10000), 10000), None);
    assert_eq!(discount_percent(None, 5000), None);
  }
}
