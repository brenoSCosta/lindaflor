use sqlx::PgPool;
use topcoat::context::{app_context, memoize};

use super::queries::{StoreSettings, get_store_settings};

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

/// Store settings for storefront display (PIX %, free-shipping threshold,
/// announcement), loaded once per request so every product card shares one
/// query. Falls back to defaults if the query fails; checkout reads settings
/// directly so a failure there still surfaces.
#[memoize]
pub async fn storefront_settings(cx: &Cx) -> StoreSettings {
  match get_store_settings(app_context::<PgPool>(cx)).await {
    Ok(settings) => settings,
    Err(err) => {
      tracing::warn!(error = %err, "store settings unavailable, using defaults");
      StoreSettings::default()
    }
  }
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
