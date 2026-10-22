use super::*;
use time::Month;

fn date(year: i32, month: Month, day: u8) -> Date {
  Date::from_calendar_date(year, month, day).unwrap()
}

#[test]
fn range_parsing_defaults_swaps_and_clamps() {
  let today = date(2026, Month::October, 22);
  let default = DateRange::parse_with_today(None, None, today);
  assert_eq!(default.to, today);
  assert_eq!(default.days(), DEFAULT_RANGE_DAYS);
  assert_eq!(default.from, date(2026, Month::September, 23));

  let invalid =
    DateRange::parse_with_today(Some("2026-02-30"), Some("lixo"), today);
  assert_eq!(invalid, default);

  let swapped =
    DateRange::parse_with_today(Some("2026-03-10"), Some("2026-03-01"), today);
  assert_eq!(
    (swapped.from, swapped.to),
    (date(2026, Month::March, 1), date(2026, Month::March, 10))
  );

  let long =
    DateRange::parse_with_today(Some("2020-01-01"), Some("2026-01-31"), today);
  assert_eq!(long.to, date(2026, Month::January, 31));
  assert_eq!(long.days(), MAX_RANGE_DAYS);
}

#[test]
fn csv_cells_are_quoted_and_formula_safe() {
  assert_eq!(csv_field("plain"), "\"plain\"");
  assert_eq!(csv_field("a\"b,c"), "\"a\"\"b,c\"");
  for (raw, expected) in [
    ("=1+1", "\"'=1+1\""),
    ("+55 11", "\"'+55 11\""),
    ("-2", "\"'-2\""),
    ("@SUM(A1)", "\"'@SUM(A1)\""),
    ("\t=x", "\"'\t=x\""),
    ("\r=x", "\"'\r=x\""),
  ] {
    assert_eq!(csv_field(raw), expected, "{raw:?}");
  }
  // Only a leading trigger matters.
  assert_eq!(csv_field("a=b"), "\"a=b\"");
  assert_eq!(csv_money(123_450), "1234.50");
  assert_eq!(csv_money(5), "0.05");
}

#[test]
fn rendered_csv_neutralises_injected_fields() {
  let order = CsvOrder {
    id: Uuid::now_v7(),
    created_at: "2026-01-10 12:00".into(),
    paid_at: None,
    status: "paid".into(),
    email: Some("=HYPERLINK(\"http://evil\")@x.com".into()),
    uf: Some("SP".into()),
    city: Some("@cidade".into()),
    items: Some("-1x Biquíni (M / azul)".into()),
    units: 1,
    subtotal_cents: 1000,
    shipping_cents: 0,
    discount_cents: 0,
    pix_discount_cents: 0,
    total_cents: 1000,
    provider: "static_pix".into(),
    coupon: Some("+VERAO".into()),
    carrier: "Correios".into(),
    tracking_code: None,
    shipped_at: None,
    delivered_at: None,
  };
  let csv = render_csv(&[order]);
  assert!(csv.starts_with('\u{feff}'));
  let mut lines = csv.trim_start_matches('\u{feff}').split("\r\n");
  let header = lines.next().unwrap();
  assert_eq!(header.split(',').count(), CSV_HEADER.len());
  let row = lines.next().unwrap();
  assert!(
    row.contains("\"'=HYPERLINK(\"\"http://evil\"\")@x.com\""),
    "{row}"
  );
  assert!(row.contains("\"'@cidade\""), "{row}");
  assert!(row.contains("\"'-1x Biquíni (M / azul)\""), "{row}");
  assert!(row.contains("\"'+VERAO\""), "{row}");
  assert!(row.contains("\"10.00\""), "{row}");
  // Every cell is quoted: no unquoted formula can start a cell.
  assert!(!row.contains(",="), "{row}");
}

// --- Report math on a seeded fixture ---

struct Variant {
  id: Uuid,
}

async fn variant(pool: &PgPool, name: &str) -> Variant {
  let product_id = Uuid::now_v7();
  let id = Uuid::now_v7();
  sqlx::query(
    "INSERT INTO products (id, name, slug, price_in_cents) VALUES ($1, $2, $3, 1000)",
  )
  .bind(product_id)
  .bind(name)
  .bind(format!("p-{product_id}"))
  .execute(pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO product_variants (id, product_id, sku, size, color)
     VALUES ($1, $2, $3, 'm', 'azul')",
  )
  .bind(id)
  .bind(product_id)
  .bind(format!("sku-{id}"))
  .execute(pool)
  .await
  .unwrap();
  Variant { id }
}

/// One fixture order. `at` is a Brasília wall-clock timestamp
/// (`"2026-01-10 12:00"`) used as `created_at`, and as `paid_at` when
/// `paid`.
struct Seed<'a> {
  status: &'a str,
  provider: &'a str,
  at: &'a str,
  paid: bool,
  total: i64,
  discount: i64,
  coupon_id: Option<Uuid>,
  /// `(variant, product name, quantity, unit price)`.
  items: &'a [(&'a Variant, &'a str, i32, i64)],
}

impl Default for Seed<'_> {
  fn default() -> Self {
    Self {
      status: "paid",
      provider: "static_pix",
      at: "2026-01-10 12:00",
      paid: true,
      total: 0,
      discount: 0,
      coupon_id: None,
      items: &[],
    }
  }
}

async fn order(pool: &PgPool, seed: Seed<'_>) -> Uuid {
  let Seed {
    status,
    provider,
    at,
    paid,
    total,
    discount,
    coupon_id,
    items,
  } = seed;
  let id = Uuid::now_v7();
  sqlx::query(
    "INSERT INTO orders (id, guest_email, status, access_token, payment_provider,
                         subtotal_cents, discount_cents, total_cents, coupon_id,
                         shipping_address, created_at, paid_at)
     VALUES ($1, 'r@example.com', $2::order_status, $3, $4, $5, $6, $7, $8,
             jsonb_build_object('state', 'SP', 'city', 'Santos'),
             ($9::timestamp AT TIME ZONE 'America/Sao_Paulo'),
             CASE WHEN $10 THEN ($9::timestamp AT TIME ZONE 'America/Sao_Paulo') END)",
  )
  .bind(id)
  .bind(status)
  .bind(Uuid::new_v4())
  .bind(provider)
  .bind(total + discount)
  .bind(discount)
  .bind(total)
  .bind(coupon_id)
  .bind(at)
  .bind(paid)
  .execute(pool)
  .await
  .unwrap();
  for &(variant, name, quantity, price) in items {
    sqlx::query(
      "INSERT INTO order_items (id, order_id, variant_id, product_name, variant_label, quantity, unit_price_cents)
       VALUES ($1, $2, $3, $4, 'M / azul', $5, $6)",
    )
    .bind(Uuid::now_v7())
    .bind(id)
    .bind(variant.id)
    .bind(name)
    .bind(quantity)
    .bind(price)
    .execute(pool)
    .await
    .unwrap();
  }
  id
}

#[tokio::test]
async fn report_math_on_fixture() {
  // Own database: the report aggregates every order in the range.
  let pool = crate::test_support::isolated_pool().await;
  let x = variant(&pool, "Biquíni Flor").await;
  let y = variant(&pool, "Maiô Sol").await;
  let coupon_id = Uuid::now_v7();
  sqlx::query(
    "INSERT INTO coupons (id, code, discount_type, discount_value)
     VALUES ($1, 'verao', 'fixed', 1000)",
  )
  .bind(coupon_id)
  .execute(&pool)
  .await
  .unwrap();

  // In range (2026-01-10 .. 2026-01-12, Brasília):
  order(
    &pool,
    Seed {
      total: 10_000,
      discount: 1_000,
      coupon_id: Some(coupon_id),
      items: &[(&x, "Biquíni Flor", 2, 4_000), (&y, "Maiô Sol", 1, 3_000)],
      ..Seed::default()
    },
  )
  .await;
  // 23:30 in Brasília is already the 12th in UTC: counts on the 11th.
  let b = order(
    &pool,
    Seed {
      status: "delivered",
      provider: "mercadopago",
      at: "2026-01-11 23:30",
      total: 5_000,
      items: &[(&x, "Biquíni Flor", 1, 5_000)],
      ..Seed::default()
    },
  )
  .await;
  // No paid_at: falls back to created_at.
  order(
    &pool,
    Seed {
      status: "processing",
      at: "2026-01-12 10:00",
      paid: false,
      total: 2_000,
      items: &[(&y, "Maiô Sol", 1, 2_000)],
      ..Seed::default()
    },
  )
  .await;
  // Not revenue:
  order(
    &pool,
    Seed {
      status: "refunded",
      at: "2026-01-10 09:00",
      total: 7_000,
      items: &[(&x, "Biquíni Flor", 1, 7_000)],
      ..Seed::default()
    },
  )
  .await;
  order(
    &pool,
    Seed {
      status: "cancelled",
      at: "2026-01-10 10:00",
      paid: false,
      total: 3_000,
      items: &[(&y, "Maiô Sol", 1, 3_000)],
      ..Seed::default()
    },
  )
  .await;
  order(
    &pool,
    Seed {
      status: "pending_payment",
      provider: "mercadopago",
      at: "2026-01-11 08:00",
      paid: false,
      total: 4_000,
      items: &[(&y, "Maiô Sol", 1, 4_000)],
      ..Seed::default()
    },
  )
  .await;
  // Outside the range (both sides of the Brasília day boundary).
  for (at, total) in [("2026-01-13 00:00", 9_999), ("2026-01-09 23:59", 8_888)]
  {
    order(
      &pool,
      Seed {
        at,
        total,
        items: &[(&x, "Biquíni Flor", 9, 1_111)],
        ..Seed::default()
      },
    )
    .await;
  }
  // Partial refund (return request) on B, resolved on the 12th.
  sqlx::query(
    "INSERT INTO return_requests (id, order_id, kind, status, resolution, reason,
                                  refund_cents, resolved_at)
     VALUES ($1, $2, 'defect', 'resolved', 'refunded', 'Costura solta', 500,
             '2026-01-12 15:00'::timestamp AT TIME ZONE 'America/Sao_Paulo')",
  )
  .bind(Uuid::now_v7())
  .bind(b)
  .execute(&pool)
  .await
  .unwrap();

  let range = DateRange {
    from: date(2026, Month::January, 10),
    to: date(2026, Month::January, 12),
  };
  let totals = summary(&pool, range).await.unwrap();
  assert_eq!(
    totals,
    Summary {
      revenue_cents: 17_000,
      orders: 3,
      discount_cents: 1_000,
      pix_discount_cents: 0,
      shipping_cents: 0,
      pending_orders: 1,
      abandoned_orders: 1,
      refunded_orders: 1,
      partial_refund_cents: 500,
    }
  );
  assert_eq!(totals.aov_cents(), 5_666);
  assert_eq!(totals.net_revenue_cents(), 16_500);

  let days = revenue_by_day(&pool, range).await.unwrap();
  let by_day: Vec<(u8, i64, i64)> = days
    .iter()
    .map(|d| (d.day.day(), d.orders, d.revenue_cents))
    .collect();
  assert_eq!(by_day, [(10, 1, 10_000), (11, 1, 5_000), (12, 1, 2_000)]);

  let variants = top_sellers(&pool, range, TopBy::VariantUnits, 10)
    .await
    .unwrap();
  let variants: Vec<(&str, i64, i64)> = variants
    .iter()
    .map(|r| (r.name.as_str(), r.units, r.revenue_cents))
    .collect();
  assert_eq!(
    variants,
    [("Biquíni Flor", 3, 13_000), ("Maiô Sol", 2, 5_000)]
  );
  let products = top_sellers(&pool, range, TopBy::ProductRevenue, 1)
    .await
    .unwrap();
  assert_eq!(products.len(), 1);
  assert_eq!(products[0].name, "Biquíni Flor");
  assert_eq!(products[0].revenue_cents, 13_000);

  let coupons = coupon_usage(&pool, range).await.unwrap();
  assert_eq!(
    coupons,
    [CouponRow {
      code: "VERAO".into(),
      orders: 1,
      discount_cents: 1_000,
      revenue_cents: 10_000,
    }]
  );

  let providers = provider_split(&pool, range).await.unwrap();
  let providers: Vec<(&str, i64, i64)> = providers
    .iter()
    .map(|p| (p.provider.as_str(), p.orders, p.revenue_cents))
    .collect();
  assert_eq!(
    providers,
    [("static_pix", 2, 12_000), ("mercadopago", 1, 5_000)]
  );

  // Export: every order created in range, any status, oldest first.
  let rows = csv_orders(&pool, range, 100).await.unwrap();
  assert_eq!(rows.len(), 6);
  assert_eq!(rows[0].created_at, "2026-01-10 09:00");
  assert!(rows.iter().all(|r| r.uf.as_deref() == Some("SP")));
  let first_paid = rows.iter().find(|r| r.coupon.is_some()).unwrap();
  assert_eq!(
    first_paid.items.as_deref(),
    Some("2x Biquíni Flor (M / azul); 1x Maiô Sol (M / azul)")
  );
  assert_eq!(first_paid.units, 3);
  // The bound is honoured.
  assert_eq!(csv_orders(&pool, range, 2).await.unwrap().len(), 2);

  // An empty range is all zeros, one row per day.
  let empty = DateRange {
    from: date(2025, Month::June, 1),
    to: date(2025, Month::June, 2),
  };
  assert_eq!(summary(&pool, empty).await.unwrap(), Summary::default());
  assert_eq!(revenue_by_day(&pool, empty).await.unwrap().len(), 2);
}
