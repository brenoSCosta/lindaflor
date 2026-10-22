//! Admin sales reports (`/admin/relatorios`) and the orders CSV export.
//!
//! Every figure covers a range of Brasília calendar days `[from, to]`.
//! A sale counts on `COALESCE(paid_at, created_at)` (the day the money
//! came in) for orders in `paid | processing | shipped | delivered`;
//! refunded and cancelled orders are not revenue. Partial refunds
//! recorded on return requests (see `store::returns`) are reported on the
//! day they were resolved and subtracted for the net figure. Abandoned /
//! expired orders and the CSV export are keyed on `created_at`.
//!
//! Each block is one aggregated query over the range, served by the
//! `orders_sale_at_idx` / `orders_created_at_idx` indexes (migration 0030).

use sqlx::PgPool;
use time::{Date, Duration, OffsetDateTime, UtcOffset};
use uuid::Uuid;

/// Statuses that count as revenue.
pub const SALE_STATUSES: &str =
  "('paid', 'processing', 'shipped', 'delivered')";
/// Longest range a report or export may cover.
pub const MAX_RANGE_DAYS: i64 = 366;
/// Default range: the last 30 days, today included.
pub const DEFAULT_RANGE_DAYS: i64 = 30;
/// Most orders one CSV export may hold (the response is built in memory).
pub const CSV_MAX_ORDERS: i64 = 20_000;

/// Brazil has had no DST since 2019: Brasília is UTC−3 all year.
fn brasilia_today() -> Date {
  let offset = UtcOffset::from_hms(-3, 0, 0).unwrap_or(UtcOffset::UTC);
  OffsetDateTime::now_utc().to_offset(offset).date()
}

fn parse_date(raw: Option<&str>) -> Option<Date> {
  let mut parts = raw?.trim().splitn(3, '-');
  let year: i32 = parts.next()?.parse().ok()?;
  let month: u8 = parts.next()?.parse().ok()?;
  let day: u8 = parts.next()?.parse().ok()?;
  if !(2000..=9999).contains(&year) {
    return None;
  }
  Date::from_calendar_date(year, time::Month::try_from(month).ok()?, day).ok()
}

pub fn format_date_iso(date: Date) -> String {
  format!(
    "{:04}-{:02}-{:02}",
    date.year(),
    u8::from(date.month()),
    date.day()
  )
}

pub fn format_date_br(date: Date) -> String {
  format!(
    "{:02}/{:02}/{:04}",
    date.day(),
    u8::from(date.month()),
    date.year()
  )
}

/// Inclusive range of Brasília calendar days.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct DateRange {
  pub from: Date,
  pub to: Date,
}

impl DateRange {
  /// Parse `de`/`ate` (`YYYY-MM-DD`). Missing or invalid values fall back
  /// to the last [`DEFAULT_RANGE_DAYS`] days ending `today`; a reversed
  /// range is swapped, and an over-long one is cut to
  /// [`MAX_RANGE_DAYS`] ending at `to`.
  pub fn parse_with_today(
    from: Option<&str>,
    to: Option<&str>,
    today: Date,
  ) -> Self {
    let to_date = parse_date(to).unwrap_or(today);
    let from_date = parse_date(from)
      .unwrap_or(to_date - Duration::days(DEFAULT_RANGE_DAYS - 1));
    let (from_date, to_date) = if from_date > to_date {
      (to_date, from_date)
    } else {
      (from_date, to_date)
    };
    let earliest = to_date - Duration::days(MAX_RANGE_DAYS - 1);
    Self {
      from: from_date.max(earliest),
      to: to_date,
    }
  }

  pub fn parse(from: Option<&str>, to: Option<&str>) -> Self {
    Self::parse_with_today(from, to, brasilia_today())
  }

  pub fn days(self) -> i64 {
    (self.to - self.from).whole_days() + 1
  }
}

/// SQL bounds for a [`DateRange`] bound as `$1::date` / `$2::date`.
const RANGE_START: &str =
  "(($1::date)::timestamp AT TIME ZONE 'America/Sao_Paulo')";
const RANGE_END: &str =
  "((($2::date) + 1)::timestamp AT TIME ZONE 'America/Sao_Paulo')";

#[derive(Debug, Clone, Default, PartialEq, Eq, sqlx::FromRow)]
pub struct Summary {
  pub revenue_cents: i64,
  pub orders: i64,
  pub discount_cents: i64,
  pub pix_discount_cents: i64,
  pub shipping_cents: i64,
  /// Orders created in range still waiting for payment.
  pub pending_orders: i64,
  /// Orders created in range cancelled without ever being paid (expired
  /// holds and abandoned checkouts).
  pub abandoned_orders: i64,
  /// Orders created in range that ended refunded.
  pub refunded_orders: i64,
  /// Partial refunds of return requests resolved in range (orders that
  /// stayed sold).
  pub partial_refund_cents: i64,
}

impl Summary {
  /// Average order value, in cents (0 without orders).
  pub fn aov_cents(&self) -> i64 {
    if self.orders == 0 {
      0
    } else {
      self.revenue_cents / self.orders
    }
  }

  pub fn net_revenue_cents(&self) -> i64 {
    self.revenue_cents - self.partial_refund_cents
  }
}

pub async fn summary(
  pool: &PgPool,
  range: DateRange,
) -> Result<Summary, sqlx::Error> {
  sqlx::query_as::<_, Summary>(&format!(
    "WITH sales AS (
         SELECT COALESCE(SUM(o.total_cents), 0)::bigint AS revenue_cents,
                COUNT(*) AS orders,
                COALESCE(SUM(o.discount_cents), 0)::bigint AS discount_cents,
                COALESCE(SUM(o.pix_discount_cents), 0)::bigint AS pix_discount_cents,
                COALESCE(SUM(o.shipping_cents), 0)::bigint AS shipping_cents
         FROM orders o
         WHERE o.status IN {SALE_STATUSES}
           AND COALESCE(o.paid_at, o.created_at) >= {RANGE_START}
           AND COALESCE(o.paid_at, o.created_at) < {RANGE_END}
     ),
     created AS (
         SELECT COUNT(*) FILTER (WHERE o.status = 'pending_payment') AS pending_orders,
                COUNT(*) FILTER (WHERE o.status = 'cancelled' AND o.paid_at IS NULL)
                  AS abandoned_orders,
                COUNT(*) FILTER (WHERE o.status = 'refunded') AS refunded_orders
         FROM orders o
         WHERE o.created_at >= {RANGE_START} AND o.created_at < {RANGE_END}
     ),
     partial AS (
         SELECT COALESCE(SUM(r.refund_cents), 0)::bigint AS partial_refund_cents
         FROM return_requests r
         JOIN orders o ON o.id = r.order_id
         WHERE r.status = 'resolved' AND r.resolution = 'refunded'
           AND o.status <> 'refunded'
           AND r.resolved_at >= {RANGE_START} AND r.resolved_at < {RANGE_END}
     )
     SELECT sales.*, created.*, partial.* FROM sales, created, partial"
  ))
  .bind(range.from)
  .bind(range.to)
  .fetch_one(pool)
  .await
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct DayRow {
  pub day: Date,
  pub orders: i64,
  pub revenue_cents: i64,
}

/// One row per day of the range (zero-filled).
pub async fn revenue_by_day(
  pool: &PgPool,
  range: DateRange,
) -> Result<Vec<DayRow>, sqlx::Error> {
  sqlx::query_as::<_, DayRow>(&format!(
    "WITH days AS (
         SELECT generate_series($1::date, $2::date, interval '1 day')::date AS day
     ),
     sales AS (
         SELECT (COALESCE(o.paid_at, o.created_at) AT TIME ZONE 'America/Sao_Paulo')::date AS day,
                COUNT(*) AS orders,
                SUM(o.total_cents)::bigint AS revenue_cents
         FROM orders o
         WHERE o.status IN {SALE_STATUSES}
           AND COALESCE(o.paid_at, o.created_at) >= {RANGE_START}
           AND COALESCE(o.paid_at, o.created_at) < {RANGE_END}
         GROUP BY 1
     )
     SELECT d.day, COALESCE(s.orders, 0) AS orders,
            COALESCE(s.revenue_cents, 0)::bigint AS revenue_cents
     FROM days d LEFT JOIN sales s ON s.day = d.day
     ORDER BY d.day"
  ))
  .bind(range.from)
  .bind(range.to)
  .fetch_all(pool)
  .await
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct TopRow {
  pub name: String,
  /// Variant label ("M / azul"); empty for the per-product ranking.
  pub detail: String,
  pub units: i64,
  pub revenue_cents: i64,
}

/// Which ranking [`top_sellers`] returns.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TopBy {
  /// Variants ordered by units sold.
  VariantUnits,
  /// Products (all variants) ordered by item revenue.
  ProductRevenue,
}

/// Item revenue is `quantity × unit_price` (before order-level discounts).
pub async fn top_sellers(
  pool: &PgPool,
  range: DateRange,
  by: TopBy,
  limit: i64,
) -> Result<Vec<TopRow>, sqlx::Error> {
  let (group, select, order) = match by {
    TopBy::VariantUnits => (
      "oi.variant_id",
      "MAX(oi.product_name) AS name, MAX(oi.variant_label) AS detail",
      "units DESC, revenue_cents DESC",
    ),
    TopBy::ProductRevenue => (
      "v.product_id",
      "MAX(oi.product_name) AS name, '' AS detail",
      "revenue_cents DESC, units DESC",
    ),
  };
  sqlx::query_as::<_, TopRow>(&format!(
    "SELECT {select},
            SUM(oi.quantity)::bigint AS units,
            SUM(oi.quantity * oi.unit_price_cents)::bigint AS revenue_cents
     FROM orders o
     JOIN order_items oi ON oi.order_id = o.id
     JOIN product_variants v ON v.id = oi.variant_id
     WHERE o.status IN {SALE_STATUSES}
       AND COALESCE(o.paid_at, o.created_at) >= {RANGE_START}
       AND COALESCE(o.paid_at, o.created_at) < {RANGE_END}
     GROUP BY {group}
     ORDER BY {order}, name
     LIMIT $3"
  ))
  .bind(range.from)
  .bind(range.to)
  .bind(limit)
  .fetch_all(pool)
  .await
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct CouponRow {
  pub code: String,
  pub orders: i64,
  pub discount_cents: i64,
  pub revenue_cents: i64,
}

/// Coupon usage on sales in range (orders with a coupon only).
pub async fn coupon_usage(
  pool: &PgPool,
  range: DateRange,
) -> Result<Vec<CouponRow>, sqlx::Error> {
  sqlx::query_as::<_, CouponRow>(&format!(
    "SELECT upper(COALESCE(c.code, o.payment_meta->>'coupon_code')) AS code,
            COUNT(*) AS orders,
            SUM(o.discount_cents)::bigint AS discount_cents,
            SUM(o.total_cents)::bigint AS revenue_cents
     FROM orders o
     LEFT JOIN coupons c ON c.id = o.coupon_id
     WHERE o.status IN {SALE_STATUSES}
       AND COALESCE(o.paid_at, o.created_at) >= {RANGE_START}
       AND COALESCE(o.paid_at, o.created_at) < {RANGE_END}
       AND COALESCE(c.code, NULLIF(o.payment_meta->>'coupon_code', '')) IS NOT NULL
     GROUP BY 1
     ORDER BY orders DESC, discount_cents DESC, code"
  ))
  .bind(range.from)
  .bind(range.to)
  .fetch_all(pool)
  .await
}

#[derive(Debug, Clone, PartialEq, Eq, sqlx::FromRow)]
pub struct ProviderRow {
  pub provider: String,
  pub orders: i64,
  pub revenue_cents: i64,
}

pub fn provider_label(provider: &str) -> &'static str {
  match provider {
    "mercadopago" => "Mercado Pago",
    "static_pix" => "PIX (chave da loja)",
    _ => "Outro",
  }
}

pub async fn provider_split(
  pool: &PgPool,
  range: DateRange,
) -> Result<Vec<ProviderRow>, sqlx::Error> {
  sqlx::query_as::<_, ProviderRow>(&format!(
    "SELECT o.payment_provider AS provider, COUNT(*) AS orders,
            SUM(o.total_cents)::bigint AS revenue_cents
     FROM orders o
     WHERE o.status IN {SALE_STATUSES}
       AND COALESCE(o.paid_at, o.created_at) >= {RANGE_START}
       AND COALESCE(o.paid_at, o.created_at) < {RANGE_END}
     GROUP BY 1
     ORDER BY revenue_cents DESC"
  ))
  .bind(range.from)
  .bind(range.to)
  .fetch_all(pool)
  .await
}

// --- CSV export ---

/// One exported order (all statuses), created in range.
#[derive(Debug, Clone, sqlx::FromRow)]
pub struct CsvOrder {
  pub id: Uuid,
  pub created_at: String,
  pub paid_at: Option<String>,
  pub status: String,
  pub email: Option<String>,
  pub uf: Option<String>,
  pub city: Option<String>,
  pub items: Option<String>,
  pub units: i64,
  pub subtotal_cents: i64,
  pub shipping_cents: i64,
  pub discount_cents: i64,
  pub pix_discount_cents: i64,
  pub total_cents: i64,
  pub provider: String,
  pub coupon: Option<String>,
  pub carrier: String,
  pub tracking_code: Option<String>,
  pub shipped_at: Option<String>,
  pub delivered_at: Option<String>,
}

/// Orders created in range, oldest first, at most `limit` rows.
pub async fn csv_orders(
  pool: &PgPool,
  range: DateRange,
  limit: i64,
) -> Result<Vec<CsvOrder>, sqlx::Error> {
  const TS: &str = "'YYYY-MM-DD HH24:MI'";
  sqlx::query_as::<_, CsvOrder>(&format!(
    "SELECT o.id,
            to_char(o.created_at AT TIME ZONE 'America/Sao_Paulo', {TS}) AS created_at,
            to_char(o.paid_at AT TIME ZONE 'America/Sao_Paulo', {TS}) AS paid_at,
            o.status::text AS status,
            COALESCE(NULLIF(btrim(o.guest_email), ''), u.email) AS email,
            o.shipping_address->>'state' AS uf,
            o.shipping_address->>'city' AS city,
            items.summary AS items,
            COALESCE(items.units, 0)::bigint AS units,
            o.subtotal_cents, o.shipping_cents, o.discount_cents,
            o.pix_discount_cents, o.total_cents,
            o.payment_provider AS provider,
            COALESCE(c.code, NULLIF(o.payment_meta->>'coupon_code', '')) AS coupon,
            o.carrier, o.tracking_code,
            to_char(o.shipped_at AT TIME ZONE 'America/Sao_Paulo', {TS}) AS shipped_at,
            to_char(o.delivered_at AT TIME ZONE 'America/Sao_Paulo', {TS}) AS delivered_at
     FROM orders o
     LEFT JOIN users u ON u.id = o.user_id
     LEFT JOIN coupons c ON c.id = o.coupon_id
     LEFT JOIN LATERAL (
         SELECT string_agg(
                    oi.quantity || 'x ' || oi.product_name || ' (' || oi.variant_label || ')',
                    '; ' ORDER BY oi.created_at, oi.id) AS summary,
                SUM(oi.quantity) AS units
         FROM order_items oi WHERE oi.order_id = o.id
     ) items ON true
     WHERE o.created_at >= {RANGE_START} AND o.created_at < {RANGE_END}
     ORDER BY o.created_at, o.id
     LIMIT $3"
  ))
  .bind(range.from)
  .bind(range.to)
  .bind(limit)
  .fetch_all(pool)
  .await
}

/// Quote one CSV field (RFC 4180) and neutralise spreadsheet formulas: a
/// cell starting with `=`, `+`, `-`, `@` (or a tab / carriage return, which
/// some spreadsheets strip before evaluating) gets a leading `'`.
pub fn csv_field(raw: &str) -> String {
  let value = if raw.starts_with(['=', '+', '-', '@', '\t', '\r']) {
    format!("'{raw}")
  } else {
    raw.to_string()
  };
  format!("\"{}\"", value.replace('"', "\"\""))
}

/// Cents as a plain decimal (`1234.50`), never negative here.
fn csv_money(cents: i64) -> String {
  format!("{}.{:02}", cents / 100, (cents % 100).abs())
}

pub const CSV_HEADER: [&str; 21] = [
  "pedido",
  "id",
  "criado_em",
  "pago_em",
  "status",
  "email",
  "uf",
  "cidade",
  "itens",
  "unidades",
  "subtotal",
  "frete",
  "desconto_cupom",
  "desconto_pix",
  "total",
  "pagamento",
  "cupom",
  "transportadora",
  "rastreio",
  "enviado_em",
  "entregue_em",
];

/// Render the export. Starts with a UTF-8 BOM so spreadsheet apps read the
/// accents right; every field is quoted through [`csv_field`].
pub fn render_csv(orders: &[CsvOrder]) -> String {
  let mut out = String::from("\u{feff}");
  let header: Vec<String> = CSV_HEADER.iter().map(|h| csv_field(h)).collect();
  out.push_str(&header.join(","));
  out.push_str("\r\n");
  for order in orders {
    let fields = [
      crate::mail::templates::order_code(order.id),
      order.id.to_string(),
      order.created_at.clone(),
      order.paid_at.clone().unwrap_or_default(),
      order.status.clone(),
      order.email.clone().unwrap_or_default(),
      order.uf.clone().unwrap_or_default(),
      order.city.clone().unwrap_or_default(),
      order.items.clone().unwrap_or_default(),
      order.units.to_string(),
      csv_money(order.subtotal_cents),
      csv_money(order.shipping_cents),
      csv_money(order.discount_cents),
      csv_money(order.pix_discount_cents),
      csv_money(order.total_cents),
      order.provider.clone(),
      order.coupon.clone().unwrap_or_default(),
      order.carrier.clone(),
      order.tracking_code.clone().unwrap_or_default(),
      order.shipped_at.clone().unwrap_or_default(),
      order.delivered_at.clone().unwrap_or_default(),
    ];
    let line: Vec<String> = fields.iter().map(|f| csv_field(f)).collect();
    out.push_str(&line.join(","));
    out.push_str("\r\n");
  }
  out
}

#[cfg(test)]
mod tests;
