use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct ProductSummary {
  pub id: Uuid,
  pub name: String,
  pub slug: String,

  pub price_in_cents: i64,
  pub compare_at_price_in_cents: Option<i64>,
  pub category: String,

  pub image_url: Option<String>,
  pub secondary_image_url: Option<String>,
  pub available_total: i32,
  pub colors: Vec<(String, Option<String>)>,
}

#[derive(Clone)]
pub struct ProductVariant {
  pub id: Uuid,
  pub sku: String,
  pub size: String,
  pub color: String,
  pub price_in_cents: Option<i64>,
  pub available: i32,
}

#[derive(Clone)]
pub struct ProductDetail {
  pub id: Uuid,
  pub name: String,
  pub slug: String,
  pub description: Option<String>,
  pub price_in_cents: i64,
  pub compare_at_price_in_cents: Option<i64>,
  pub category: String,

  pub images: Vec<ProductImage>,
  pub variants: Vec<ProductVariant>,
  pub colors: Vec<(String, Option<String>)>,
}

#[derive(Clone)]
pub struct ProductImage {
  pub id: Uuid,
  pub url: String,
  pub alt: Option<String>,
}

#[derive(Clone)]
pub struct SetSummary {
  pub id: Uuid,
  pub name: String,
  pub slug: String,
  pub description: Option<String>,
  pub active: bool,
  pub cover_url: Option<String>,
  pub product_count: i64,
  pub price_in_cents: Option<i64>,
  pub compare_at_price_in_cents: Option<i64>,
}

#[derive(Clone)]
pub struct SetImage {
  pub id: Uuid,
  pub url: String,
  pub alt: Option<String>,
}

#[derive(Clone)]
pub struct SetPriceSummary {
  pub price_in_cents: Option<i64>,
  pub compare_at_price_in_cents: Option<i64>,
}

#[derive(Clone)]
pub struct StoreSettings {
  pub whatsapp_number: Option<String>,

  pub whatsapp_message_template: Option<String>,
  pub pix_key: Option<String>,
  pub pix_key_type: Option<String>,
  pub pix_merchant_name: Option<String>,
  pub pix_merchant_city: Option<String>,
  pub free_shipping_threshold_cents: i64,
  pub pix_discount_percent: u32,
  pub announcement_text: Option<String>,
  pub first_purchase_coupon_code: Option<String>,
}

/// Used when no settings row exists yet.
impl Default for StoreSettings {
  fn default() -> Self {
    Self {
      whatsapp_number: None,
      whatsapp_message_template: None,
      pix_key: None,
      pix_key_type: None,
      pix_merchant_name: None,
      pix_merchant_city: None,
      free_shipping_threshold_cents: DEFAULT_FREE_SHIPPING_THRESHOLD_CENTS,
      pix_discount_percent: 5,
      announcement_text: None,
      first_purchase_coupon_code: None,
    }
  }
}

pub const DEFAULT_FREE_SHIPPING_THRESHOLD_CENTS: i64 = 29_900;

pub fn render_whatsapp_template(
  template: Option<&str>,
  vars: &[(&str, &str)],
  fallback: &str,
) -> String {
  let template = template.map(str::trim).filter(|t| !t.is_empty());
  let Some(template) = template else {
    return fallback.to_string();
  };
  let mut rendered = template.to_string();
  for (key, value) in vars {
    rendered = rendered.replace(&format!("{{{{{}}}}}", key), value);
  }
  if rendered.contains("{{") {
    return fallback.to_string();
  }
  rendered
}

pub fn whatsapp_link(number: &str, text: &str) -> String {
  let mut url = reqwest::Url::parse("https://wa.me/").expect("wa.me base URL");
  url
    .path_segments_mut()
    .expect("wa.me can take a path")
    .pop_if_empty()
    .push(number);
  url.query_pairs_mut().append_pair("text", text);
  url.to_string()
}

pub const STOREFRONT_PAGE_SIZE: i64 = 24;
pub const HOME_FEATURED_LIMIT: i64 = 8;

pub fn clamp_catalog_page(page: i64) -> i64 {
  page.clamp(1, 10_000)
}

pub fn format_price(cents: impl Into<i64>) -> String {
  let cents = cents.into();
  let reais = cents / 100;
  let centavos = (cents % 100).abs();
  let reais_str = reais.to_string();
  let bytes = reais_str.as_bytes();
  let mut result = String::new();
  for (i, b) in bytes.iter().enumerate() {
    if i > 0 && (bytes.len() - i).is_multiple_of(3) {
      result.push('.');
    }
    result.push(*b as char);
  }
  result.push_str(&format!(",{:02}", centavos));
  format!("R$ {}", result)
}

pub fn category_label(category: &str) -> &'static str {
  match category {
    "biquini" => "Biquíni",
    "biquini_top" => "Top de biquíni",
    "biquini_bottom" => "Calcinha de biquíni",
    "maio" => "Maiô",
    "saida_praia" => "Saída de praia",
    "acessorio" => "Acessório",
    _ => "Peça",
  }
}

pub fn size_label(size: &str) -> &'static str {
  match size {
    "pp" => "PP",
    "p" => "P",
    "m" => "M",
    "g" => "G",
    "gg" => "GG",
    _ => "Único",
  }
}

fn append_product_filters(
  sql: &mut String,
  param_index: &mut i32,
  category: bool,
  search: bool,
  featured_only: bool,
  include_out_of_stock: bool,
) {
  if category {
    sql.push_str(&format!(
      " AND p.category = ${}::product_category",
      param_index
    ));
    *param_index += 1;
  }
  if search {
    sql.push_str(&format!(
            " AND (p.name ILIKE ${} OR p.description ILIKE ${} OR EXISTS (SELECT 1 FROM product_variants pv WHERE pv.product_id = p.id AND pv.sku ILIKE ${}))",
            param_index,
            *param_index + 1,
            *param_index + 2,
        ));
    *param_index += 3;
  }
  if featured_only {
    sql.push_str(" AND p.featured = true");
  }
  if !include_out_of_stock {
    sql.push_str(" AND COALESCE((SELECT SUM(GREATEST(i.quantity - i.reserved, 0)) FROM inventory i INNER JOIN warehouses w ON w.id = i.warehouse_id WHERE i.variant_id IN (SELECT pv.id FROM product_variants pv WHERE pv.product_id = p.id) AND w.active = true), 0) > 0");
  }
}

#[derive(Clone)]
pub struct CatalogFilters {
  pub sizes: Vec<String>,
  pub colors: Vec<String>,
  pub price_min_cents: Option<i64>,
  pub price_max_cents: Option<i64>,
  pub available_only: bool,
  pub sale_only: bool,
}

/// Appends the faceted filters; binds must follow in the same order
/// (price min, price max, colors, sizes).
fn append_catalog_filters(
  sql: &mut String,
  param_index: &mut i32,
  filters: &CatalogFilters,
) {
  if filters.price_min_cents.is_some() {
    sql.push_str(&format!(" AND p.price_in_cents >= ${param_index}"));
    *param_index += 1;
  }
  if filters.price_max_cents.is_some() {
    sql.push_str(&format!(" AND p.price_in_cents <= ${param_index}"));
    *param_index += 1;
  }
  if !filters.colors.is_empty() {
    sql.push_str(&format!(
      " AND EXISTS (SELECT 1 FROM product_variants pv WHERE pv.product_id = p.id AND pv.color = ANY(${param_index}::text[]))"
    ));
    *param_index += 1;
  }
  if !filters.sizes.is_empty() {
    sql.push_str(&format!(
      " AND EXISTS (SELECT 1 FROM product_variants pv WHERE pv.product_id = p.id AND pv.size::text = ANY(${param_index}::text[]))"
    ));
    *param_index += 1;
  }
  if filters.sale_only {
    sql.push_str(" AND p.compare_at_price_in_cents IS NOT NULL");
  }
}

fn bind_product_filters<'q>(
  mut query: sqlx::query::QueryAs<
    'q,
    sqlx::Postgres,
    ProductSummaryRow,
    sqlx::postgres::PgArguments,
  >,
  category: Option<&'q str>,
  search: Option<String>,
) -> sqlx::query::QueryAs<
  'q,
  sqlx::Postgres,
  ProductSummaryRow,
  sqlx::postgres::PgArguments,
> {
  if let Some(c) = category {
    query = query.bind(c);
  }
  if let Some(term) = search {
    query = query.bind(term.clone()).bind(term.clone()).bind(term);
  }
  query
}

const PRODUCT_SUMMARY_COLUMNS: &str = "p.id, p.name, p.slug, p.price_in_cents, p.compare_at_price_in_cents, p.category::text AS category,
            (SELECT pi.url FROM product_images pi WHERE pi.product_id = p.id ORDER BY pi.sort_order ASC LIMIT 1) AS image_url,
            (SELECT pi.url FROM product_images pi WHERE pi.product_id = p.id ORDER BY pi.sort_order ASC LIMIT 1 OFFSET 1) AS secondary_image_url,
            COALESCE((SELECT SUM(GREATEST(i.quantity - i.reserved, 0))::int
                FROM inventory i
                INNER JOIN warehouses w ON w.id = i.warehouse_id
                WHERE i.variant_id IN (SELECT pv.id FROM product_variants pv WHERE pv.product_id = p.id)
                  AND w.active = true), 0) AS available_total,
            COALESCE(
              (SELECT json_agg(json_build_array(pv.color, pv.color_hex) ORDER BY pv.color ASC)
               FROM (SELECT DISTINCT ON (pv.color) pv.color, pv.color_hex
                     FROM product_variants pv
                     WHERE pv.product_id = p.id
                     ORDER BY pv.color ASC, pv.color_hex ASC) pv),
              '[]'::json
            )::text AS colors_json";

/// Active products in a user's wishlist, most recently added first.
pub async fn list_wishlist_products(
  pool: &PgPool,
  user_id: Uuid,
) -> Result<Vec<ProductSummary>, sqlx::Error> {
  let sql = format!(
    "SELECT {PRODUCT_SUMMARY_COLUMNS}
     FROM wishlist_items wl
     INNER JOIN products p ON p.id = wl.product_id
     WHERE wl.user_id = $1 AND p.active = true
     ORDER BY wl.created_at DESC"
  );
  let rows = sqlx::query_as::<_, ProductSummaryRow>(&sql)
    .bind(user_id)
    .fetch_all(pool)
    .await?;
  Ok(rows.into_iter().map(ProductSummary::from).collect())
}

/// One page of storefront products: base filters, facets, sort, and window.
pub struct CatalogQuery<'a> {
  pub category: Option<&'a str>,
  pub search: Option<&'a str>,
  pub featured_only: bool,
  pub include_out_of_stock: bool,
  pub filters: &'a CatalogFilters,
  pub sort: &'a str,
  pub limit: i64,
  pub offset: i64,
}

pub async fn list_products_with_filters(
  pool: &PgPool,
  query: CatalogQuery<'_>,
) -> Result<Vec<ProductSummary>, sqlx::Error> {
  let CatalogQuery {
    category,
    search,
    featured_only,
    include_out_of_stock,
    filters,
    sort,
    limit,
    offset,
  } = query;
  let mut sql = String::from(
    "SELECT p.id, p.name, p.slug, p.price_in_cents, p.compare_at_price_in_cents, p.category::text AS category,
            (SELECT pi.url FROM product_images pi WHERE pi.product_id = p.id ORDER BY pi.sort_order ASC LIMIT 1) AS image_url,
            (SELECT pi.url FROM product_images pi WHERE pi.product_id = p.id ORDER BY pi.sort_order ASC LIMIT 1 OFFSET 1) AS secondary_image_url,
            COALESCE((SELECT SUM(GREATEST(i.quantity - i.reserved, 0))::int
                FROM inventory i
                INNER JOIN warehouses w ON w.id = i.warehouse_id
                WHERE i.variant_id IN (SELECT pv.id FROM product_variants pv WHERE pv.product_id = p.id)
                  AND w.active = true), 0) AS available_total,
            COALESCE(
              (SELECT json_agg(json_build_array(pv.color, pv.color_hex) ORDER BY pv.color ASC)
               FROM (SELECT DISTINCT ON (pv.color) pv.color, pv.color_hex
                     FROM product_variants pv
                     WHERE pv.product_id = p.id
                     ORDER BY pv.color ASC, pv.color_hex ASC) pv),
              '[]'::json
            )::text AS colors_json
     FROM products p
     WHERE p.active = true"
  );

  let mut param_index = 1;
  append_product_filters(
    &mut sql,
    &mut param_index,
    category.is_some(),
    search.is_some(),
    featured_only,
    include_out_of_stock,
  );

  append_catalog_filters(&mut sql, &mut param_index, filters);

  // Add sorting
  match sort {
    "relevantes" => {
      // For now, relevance is same as featured + newest when no search
      // In a real implementation, this could use full-text search ranking
      sql.push_str(" ORDER BY p.featured DESC, p.created_at DESC");
    }
    "mais_vendidos" => {
      sql.push_str(
        " ORDER BY COALESCE((SELECT SUM(oi.quantity)::int FROM order_items oi
                  INNER JOIN orders o ON o.id = oi.order_id
                  WHERE oi.variant_id IN (SELECT pv.id FROM product_variants pv WHERE pv.product_id = p.id)
                  AND o.status IN ('paid', 'processing', 'shipped', 'delivered')), 0) DESC,
                  p.featured DESC, p.created_at DESC"
      );
    }
    "maior_desconto" => {
      sql.push_str(
        " ORDER BY CASE WHEN p.compare_at_price_in_cents IS NOT NULL
                  THEN CAST(p.compare_at_price_in_cents - p.price_in_cents AS FLOAT) / p.compare_at_price_in_cents
                  ELSE 0 END DESC,
                  p.featured DESC, p.created_at DESC"
      );
    }
    "menor_preco" => {
      sql.push_str(
        " ORDER BY p.price_in_cents ASC, p.featured DESC, p.created_at DESC",
      );
    }
    "maior_preco" => {
      sql.push_str(
        " ORDER BY p.price_in_cents DESC, p.featured DESC, p.created_at DESC",
      );
    }
    "novos" => {
      sql.push_str(" ORDER BY p.created_at DESC, p.featured DESC");
    }
    "antigos" => {
      sql.push_str(" ORDER BY p.created_at ASC, p.featured DESC");
    }
    _ => {
      sql.push_str(" ORDER BY p.featured DESC, p.created_at DESC");
    }
  }

  sql.push_str(&format!(
    " LIMIT ${} OFFSET ${}",
    param_index,
    param_index + 1
  ));

  let search_term = search.map(|s| format!("%{}%", s.trim()));
  let mut query = sqlx::query_as::<_, ProductSummaryRow>(&sql);
  query = bind_product_filters(query, category, search_term);

  if let Some(min_cents) = filters.price_min_cents {
    query = query.bind(min_cents);
  }
  if let Some(max_cents) = filters.price_max_cents {
    query = query.bind(max_cents);
  }
  if !filters.colors.is_empty() {
    query = query.bind(filters.colors.clone());
  }
  if !filters.sizes.is_empty() {
    query = query.bind(filters.sizes.clone());
  }

  query = query.bind(limit).bind(offset);

  let rows = query.fetch_all(pool).await?;
  Ok(rows.into_iter().map(|r| r.into()).collect())
}

pub async fn count_products_with_filters(
  pool: &PgPool,
  category: Option<&str>,
  search: Option<&str>,
  featured_only: bool,
  include_out_of_stock: bool,
  filters: &CatalogFilters,
) -> Result<i64, sqlx::Error> {
  let mut sql =
    String::from("SELECT COUNT(*) FROM products p WHERE p.active = true");
  let mut param_index = 1;
  append_product_filters(
    &mut sql,
    &mut param_index,
    category.is_some(),
    search.is_some(),
    featured_only,
    include_out_of_stock,
  );

  append_catalog_filters(&mut sql, &mut param_index, filters);

  let mut query = sqlx::query_scalar::<_, i64>(&sql);
  if let Some(c) = category {
    query = query.bind(c);
  }
  if let Some(s) = search {
    let term = format!("%{}%", s.trim());
    query = query.bind(term.clone()).bind(term.clone()).bind(term);
  }

  if let Some(min_cents) = filters.price_min_cents {
    query = query.bind(min_cents);
  }
  if let Some(max_cents) = filters.price_max_cents {
    query = query.bind(max_cents);
  }
  if !filters.colors.is_empty() {
    query = query.bind(filters.colors.clone());
  }
  if !filters.sizes.is_empty() {
    query = query.bind(filters.sizes.clone());
  }

  query.fetch_one(pool).await
}

pub async fn list_products(
  pool: &PgPool,
  category: Option<&str>,
  search: Option<&str>,
  featured_only: bool,
  include_out_of_stock: bool,
  limit: i64,
  offset: i64,
) -> Result<Vec<ProductSummary>, sqlx::Error> {
  let mut sql = String::from(
        "SELECT p.id, p.name, p.slug, p.price_in_cents, p.compare_at_price_in_cents, p.category::text AS category,
            (SELECT pi.url FROM product_images pi WHERE pi.product_id = p.id ORDER BY pi.sort_order ASC LIMIT 1) AS image_url,
            (SELECT pi.url FROM product_images pi WHERE pi.product_id = p.id ORDER BY pi.sort_order ASC LIMIT 1 OFFSET 1) AS secondary_image_url,
            COALESCE((SELECT SUM(GREATEST(i.quantity - i.reserved, 0))::int
                FROM inventory i
                INNER JOIN warehouses w ON w.id = i.warehouse_id
                WHERE i.variant_id IN (SELECT pv.id FROM product_variants pv WHERE pv.product_id = p.id)
                  AND w.active = true), 0) AS available_total,
            COALESCE(
              (SELECT json_agg(json_build_array(pv.color, pv.color_hex) ORDER BY pv.color ASC)
               FROM (SELECT DISTINCT ON (pv.color) pv.color, pv.color_hex
                     FROM product_variants pv
                     WHERE pv.product_id = p.id
                     ORDER BY pv.color ASC, pv.color_hex ASC) pv),
              '[]'::json
            )::text AS colors_json
         FROM products p
         WHERE p.active = true",
    );

  let mut param_index = 1;
  append_product_filters(
    &mut sql,
    &mut param_index,
    category.is_some(),
    search.is_some(),
    featured_only,
    include_out_of_stock,
  );
  sql.push_str(&format!(
    " ORDER BY p.featured DESC, p.created_at DESC LIMIT ${} OFFSET ${}",
    param_index,
    param_index + 1
  ));

  let search_term = search.map(|s| format!("%{}%", s.trim()));
  let mut query = sqlx::query_as::<_, ProductSummaryRow>(&sql);
  query = bind_product_filters(query, category, search_term);
  query = query.bind(limit).bind(offset);

  let rows = query.fetch_all(pool).await?;
  Ok(rows.into_iter().map(|r| r.into()).collect())
}

pub async fn count_products(
  pool: &PgPool,
  category: Option<&str>,
  search: Option<&str>,
  featured_only: bool,
  include_out_of_stock: bool,
) -> Result<i64, sqlx::Error> {
  let mut sql =
    String::from("SELECT COUNT(*) FROM products p WHERE p.active = true");
  let mut param_index = 1;
  append_product_filters(
    &mut sql,
    &mut param_index,
    category.is_some(),
    search.is_some(),
    featured_only,
    include_out_of_stock,
  );

  let mut query = sqlx::query_scalar::<_, i64>(&sql);
  if let Some(c) = category {
    query = query.bind(c);
  }
  if let Some(s) = search {
    let term = format!("%{}%", s.trim());
    query = query.bind(term.clone()).bind(term.clone()).bind(term);
  }
  query.fetch_one(pool).await
}

pub async fn list_related_products(
  pool: &PgPool,
  product_id: Uuid,
  category: &str,
) -> Result<Vec<ProductSummary>, sqlx::Error> {
  let rows = sqlx::query_as::<_, ProductSummaryRow>(
    "SELECT p.id, p.name, p.slug, p.price_in_cents, p.compare_at_price_in_cents, p.category::text AS category,
            (SELECT pi.url FROM product_images pi WHERE pi.product_id = p.id ORDER BY pi.sort_order ASC LIMIT 1) AS image_url,
            (SELECT pi.url FROM product_images pi WHERE pi.product_id = p.id ORDER BY pi.sort_order ASC LIMIT 1 OFFSET 1) AS secondary_image_url,
            COALESCE((SELECT SUM(GREATEST(i.quantity - i.reserved, 0))::int
                FROM inventory i
                INNER JOIN warehouses w ON w.id = i.warehouse_id
                WHERE i.variant_id IN (SELECT pv.id FROM product_variants pv WHERE pv.product_id = p.id)
                  AND w.active = true), 0) AS available_total,
            COALESCE(
              (SELECT json_agg(json_build_array(pv.color, pv.color_hex) ORDER BY pv.color ASC)
               FROM (SELECT DISTINCT ON (pv.color) pv.color, pv.color_hex
                     FROM product_variants pv
                     WHERE pv.product_id = p.id
                     ORDER BY pv.color ASC, pv.color_hex ASC) pv),
              '[]'::json
            )::text AS colors_json
     FROM products p
     WHERE p.active = true
       AND p.category = $1::product_category
       AND p.id != $2
     ORDER BY p.featured DESC, p.created_at DESC
     LIMIT 4",
  )
  .bind(category)
  .bind(product_id)
  .fetch_all(pool)
  .await?;
  Ok(rows.into_iter().map(|r| r.into()).collect())
}

#[cfg(test)]
mod tests {
  use super::*;

  #[tokio::test]
  async fn list_products_respects_limit() {
    let pool = crate::test_support::fresh_pool().await;
    for i in 0..3 {
      let id = Uuid::now_v7();
      sqlx::query(
        "INSERT INTO products (id, name, slug, price_in_cents, active)
         VALUES ($1, $2, $3, 1000, true)",
      )
      .bind(id)
      .bind(format!("P{i}"))
      .bind(format!("p-{id}"))
      .execute(&pool)
      .await
      .unwrap();
    }
    let page = list_products(&pool, None, None, false, true, 2, 0)
      .await
      .unwrap();
    assert_eq!(page.len(), 2);
    let total = count_products(&pool, None, None, false, true)
      .await
      .unwrap();
    assert!(total >= 3);
  }

  #[test]
  fn render_whatsapp_template_replaces_known_vars() {
    let rendered = render_whatsapp_template(
      Some("Pedido {{order_id}} — {{total}}"),
      &[("order_id", "abc123"), ("total", "R$ 10,00")],
      "fallback",
    );
    assert_eq!(rendered, "Pedido abc123 — R$ 10,00");
  }

  #[test]
  fn render_whatsapp_template_falls_back_on_leftover_placeholders() {
    let rendered = render_whatsapp_template(
      Some("Fiz o pedido {{order_id}} no valor de {{total}}"),
      &[("product", "Maiô")],
      "Olá! Tenho interesse no Maiô.",
    );
    assert_eq!(rendered, "Olá! Tenho interesse no Maiô.");
  }
}

pub async fn get_product_by_slug(
  pool: &PgPool,
  slug: &str,
) -> Result<Option<ProductDetail>, sqlx::Error> {
  let row = sqlx::query!(
        "SELECT id, name, slug, description, price_in_cents, compare_at_price_in_cents, category::text AS \"category!\"
         FROM products WHERE slug = $1 AND active = true",
        slug
    )
    .fetch_optional(pool)
    .await?;

  let product = match row {
    Some(p) => p,
    None => return Ok(None),
  };

  let images = sqlx::query_as!(
        ProductImageRow,
        "SELECT id, url, alt FROM product_images WHERE product_id = $1 ORDER BY sort_order ASC",
        product.id
    )
    .fetch_all(pool)
    .await?;

  // Inline copy of `inventory::LIVE_AVAILABLE_SQL` (`query!` needs a literal).
  let variants = sqlx::query!(
        "SELECT id, sku, size::text AS \"size!\", color, price_in_cents,
            COALESCE((SELECT SUM(GREATEST(i.quantity - GREATEST(i.reserved - COALESCE((
                    SELECT SUM(r.quantity) FROM inventory_reservations r
                    INNER JOIN orders o ON o.id = r.order_id
                    WHERE r.inventory_id = i.id
                      AND o.status = 'pending_payment'
                      AND o.reservation_expires_at < now()), 0), 0), 0))::int
                FROM inventory i
                INNER JOIN warehouses w ON w.id = i.warehouse_id
                WHERE i.variant_id = product_variants.id AND w.active = true), 0) AS available
         FROM product_variants WHERE product_id = $1 ORDER BY size ASC, color ASC",
        product.id
    )
    .fetch_all(pool)
    .await?;

  let colors_row = sqlx::query_scalar::<_, String>(
    "SELECT COALESCE(
              (SELECT json_agg(json_build_array(pv.color, pv.color_hex) ORDER BY pv.color ASC)
               FROM (SELECT DISTINCT ON (pv.color) pv.color, pv.color_hex
                     FROM product_variants pv
                     WHERE pv.product_id = $1
                     ORDER BY pv.color ASC, pv.color_hex ASC) pv),
              '[]'::json
            )::text"
  )
  .bind(product.id)
  .fetch_one(pool)
  .await?;

  let colors: Vec<(String, Option<String>)> =
    serde_json::from_str(&colors_row).unwrap_or_default();

  Ok(Some(ProductDetail {
    id: product.id,
    name: product.name,
    slug: product.slug,
    description: product.description,
    price_in_cents: product.price_in_cents,
    compare_at_price_in_cents: product.compare_at_price_in_cents,
    category: product.category,
    images: images.into_iter().map(|i| i.into()).collect(),
    variants: variants
      .into_iter()
      .map(|v| ProductVariant {
        id: v.id,
        sku: v.sku,
        size: v.size,
        color: v.color,
        price_in_cents: v.price_in_cents,
        available: v.available.unwrap_or(0),
      })
      .collect(),
    colors,
  }))
}

/// Active sets with cover image + member count (+ bundle price when set).
pub async fn list_sets(pool: &PgPool) -> Result<Vec<SetSummary>, sqlx::Error> {
  let rows = sqlx::query_as::<_, SetSummaryRow>(
    "SELECT s.id, s.name, s.slug, s.description, s.active,
            (SELECT si.url FROM set_images si WHERE si.set_id = s.id ORDER BY si.sort_order ASC LIMIT 1) AS cover_url,
            (SELECT COUNT(*) FROM set_members sm INNER JOIN products p ON p.id = sm.product_id WHERE sm.set_id = s.id AND p.active = true) AS product_count,
            s.price_in_cents, s.compare_at_price_in_cents
         FROM sets s WHERE s.active = true ORDER BY s.name ASC",
  )
  .fetch_all(pool)
  .await?;

  Ok(rows.into_iter().map(SetSummary::from).collect())
}

pub async fn get_set_by_slug(
  pool: &PgPool,
  slug: &str,
) -> Result<Option<SetSummary>, sqlx::Error> {
  let row = sqlx::query_as::<_, SetSummaryRow>(
    "SELECT s.id, s.name, s.slug, s.description, s.active,
            (SELECT si.url FROM set_images si WHERE si.set_id = s.id ORDER BY si.sort_order ASC LIMIT 1) AS cover_url,
            (SELECT COUNT(*) FROM set_members sm INNER JOIN products p ON p.id = sm.product_id WHERE sm.set_id = s.id AND p.active = true) AS product_count,
            s.price_in_cents, s.compare_at_price_in_cents
         FROM sets s WHERE s.slug = $1 AND s.active = true",
  )
  .bind(slug)
  .fetch_optional(pool)
  .await?;

  Ok(row.map(SetSummary::from))
}

/// Images for a set, ordered for display.
pub async fn list_set_images(
  pool: &PgPool,
  set_id: Uuid,
) -> Result<Vec<SetImage>, sqlx::Error> {
  let rows = sqlx::query_as::<_, SetImageRow>(
    "SELECT id, url, alt FROM set_images WHERE set_id = $1 ORDER BY sort_order ASC",
  )
  .bind(set_id)
  .fetch_all(pool)
  .await?;
  Ok(rows.into_iter().map(SetImage::from).collect())
}

/// Active member products of a set, in `set_members.sort_order`.
pub async fn list_set_members(
  pool: &PgPool,
  set_id: Uuid,
) -> Result<Vec<ProductSummary>, sqlx::Error> {
  let rows = sqlx::query_as::<_, ProductSummaryRow>(&format!(
    "SELECT {PRODUCT_SUMMARY_COLUMNS}
       FROM set_members sm
       INNER JOIN products p ON p.id = sm.product_id
       WHERE sm.set_id = $1 AND p.active = true
       ORDER BY sm.sort_order ASC"
  ))
  .bind(set_id)
  .fetch_all(pool)
  .await?;
  Ok(rows.into_iter().map(|r| r.into()).collect())
}

/// Number of active member products in a set.
pub async fn count_set_members(
  pool: &PgPool,
  set_id: &Uuid,
) -> Result<i64, sqlx::Error> {
  sqlx::query_scalar::<_, i64>(
    "SELECT COUNT(*) FROM set_members sm
      INNER JOIN products p ON p.id = sm.product_id
      WHERE sm.set_id = $1 AND p.active = true",
  )
  .bind(set_id)
  .fetch_one(pool)
  .await
}

/// Sellable availability of a set: the MIN over its slots of the summed
/// available quantity of each slot's option variants (active warehouses
/// only). Returns 0 when the set has no slots.
pub async fn set_available_quantity(
  pool: &PgPool,
  set_id: Uuid,
) -> Result<i64, sqlx::Error> {
  use crate::app::store::inventory::LIVE_AVAILABLE_SQL;
  sqlx::query_scalar::<_, i64>(&format!(
    "SELECT COALESCE(MIN(slot_qty), 0)::bigint FROM (
       SELECT COALESCE(SUM(COALESCE((
         SELECT SUM({LIVE_AVAILABLE_SQL})
         FROM inventory i
         INNER JOIN warehouses w ON w.id = i.warehouse_id
         WHERE i.variant_id = o.variant_id AND w.active = true
       ), 0)), 0) AS slot_qty
       FROM set_slots s
       LEFT JOIN set_slot_options o ON o.slot_id = s.id
       WHERE s.set_id = $1
       GROUP BY s.id
     ) t",
  ))
  .bind(set_id)
  .fetch_one(pool)
  .await
}

/// Bundle price snapshot for a set (`sets.price_in_cents` /
/// `compare_at_price_in_cents` from migration 0015).
pub async fn set_price_summary(
  pool: &PgPool,
  set_id: Uuid,
) -> Result<Option<SetPriceSummary>, sqlx::Error> {
  let row = sqlx::query_as::<_, SetPriceSummaryRow>(
    "SELECT price_in_cents, compare_at_price_in_cents FROM sets WHERE id = $1",
  )
  .bind(set_id)
  .fetch_optional(pool)
  .await?;
  Ok(row.map(|r| SetPriceSummary {
    price_in_cents: r.price_in_cents,
    compare_at_price_in_cents: r.compare_at_price_in_cents,
  }))
}

pub async fn get_store_settings(
  pool: &PgPool,
) -> Result<StoreSettings, sqlx::Error> {
  let row = sqlx::query!(
        "SELECT whatsapp_number, whatsapp_message_template, pix_key, pix_key_type::text AS pix_key_type, pix_merchant_name, pix_merchant_city, free_shipping_threshold_cents, pix_discount_percent, announcement_text, first_purchase_coupon_code
         FROM store_settings WHERE id"
    )
    .fetch_optional(pool)
    .await?;

  Ok(match row {
    Some(r) => StoreSettings {
      whatsapp_number: r.whatsapp_number,
      whatsapp_message_template: r.whatsapp_message_template,
      pix_key: r.pix_key,
      pix_key_type: r.pix_key_type,
      pix_merchant_name: r.pix_merchant_name,
      pix_merchant_city: r.pix_merchant_city,
      free_shipping_threshold_cents: r.free_shipping_threshold_cents,
      pix_discount_percent: r.pix_discount_percent as u32,
      announcement_text: r.announcement_text,
      first_purchase_coupon_code: r.first_purchase_coupon_code,
    },
    None => StoreSettings::default(),
  })
}

#[derive(sqlx::FromRow)]
struct ProductSummaryRow {
  id: Uuid,
  name: String,
  slug: String,
  price_in_cents: i64,
  compare_at_price_in_cents: Option<i64>,
  category: String,
  image_url: Option<String>,
  secondary_image_url: Option<String>,
  available_total: Option<i32>,
  colors_json: Option<String>,
}

impl From<ProductSummaryRow> for ProductSummary {
  fn from(r: ProductSummaryRow) -> Self {
    let colors = if let Some(json_str) = r.colors_json {
      serde_json::from_str(&json_str).unwrap_or_default()
    } else {
      Vec::new()
    };

    ProductSummary {
      id: r.id,
      name: r.name,
      slug: r.slug,
      price_in_cents: r.price_in_cents,
      compare_at_price_in_cents: r.compare_at_price_in_cents,
      category: r.category,
      image_url: r.image_url,
      secondary_image_url: r.secondary_image_url,
      available_total: r.available_total.unwrap_or(0),
      colors,
    }
  }
}

#[derive(sqlx::FromRow)]
struct ProductImageRow {
  id: Uuid,
  url: String,
  alt: Option<String>,
}

impl From<ProductImageRow> for ProductImage {
  fn from(r: ProductImageRow) -> Self {
    ProductImage {
      id: r.id,
      url: r.url,
      alt: r.alt,
    }
  }
}

#[derive(sqlx::FromRow)]
struct SetSummaryRow {
  id: Uuid,
  name: String,
  slug: String,
  description: Option<String>,
  active: bool,
  cover_url: Option<String>,
  product_count: Option<i64>,
  price_in_cents: Option<i64>,
  compare_at_price_in_cents: Option<i64>,
}

impl From<SetSummaryRow> for SetSummary {
  fn from(r: SetSummaryRow) -> Self {
    SetSummary {
      id: r.id,
      name: r.name,
      slug: r.slug,
      description: r.description,
      active: r.active,
      cover_url: r.cover_url,
      product_count: r.product_count.unwrap_or(0),
      price_in_cents: r.price_in_cents,
      compare_at_price_in_cents: r.compare_at_price_in_cents,
    }
  }
}

#[derive(sqlx::FromRow)]
struct SetImageRow {
  id: Uuid,
  url: String,
  alt: Option<String>,
}

impl From<SetImageRow> for SetImage {
  fn from(r: SetImageRow) -> Self {
    SetImage {
      id: r.id,
      url: r.url,
      alt: r.alt,
    }
  }
}

#[derive(sqlx::FromRow)]
struct SetPriceSummaryRow {
  price_in_cents: Option<i64>,
  compare_at_price_in_cents: Option<i64>,
}

/// Related products from the product_looks table, ordered by sort_order.
pub async fn list_product_looks(
  pool: &PgPool,
  product_id: Uuid,
) -> Result<Vec<ProductSummary>, sqlx::Error> {
  let rows = sqlx::query_as::<_, ProductSummaryRow>(&format!(
    "SELECT {PRODUCT_SUMMARY_COLUMNS}
       FROM product_looks pl
       INNER JOIN products p ON p.id = pl.related_product_id
       WHERE pl.product_id = $1 AND p.active = true
       ORDER BY pl.sort_order ASC"
  ))
  .bind(product_id)
  .fetch_all(pool)
  .await?;
  Ok(rows.into_iter().map(|r| r.into()).collect())
}

/// Distinct colors (one hex per name) among active products, for the filter
/// panel; optionally narrowed to a category.
pub async fn list_filter_colors(
  pool: &PgPool,
  category: Option<&str>,
) -> Result<Vec<(String, Option<String>)>, sqlx::Error> {
  sqlx::query_as::<_, (String, Option<String>)>(
    "SELECT DISTINCT ON (pv.color) pv.color, pv.color_hex
     FROM product_variants pv
     INNER JOIN products p ON p.id = pv.product_id
     WHERE p.active = true
       AND ($1::text IS NULL OR p.category::text = $1)
     ORDER BY pv.color ASC, pv.color_hex ASC NULLS LAST
     LIMIT 30",
  )
  .bind(category)
  .fetch_all(pool)
  .await
}
