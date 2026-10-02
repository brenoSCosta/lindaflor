use sqlx::PgPool;
use uuid::Uuid;

#[derive(Clone)]
pub struct ProductSummary {
  pub id: Uuid,
  pub name: String,
  pub slug: String,
  #[allow(dead_code)]
  pub description: Option<String>,
  pub price_in_cents: i32,
  pub category: String,
  #[allow(dead_code)]
  pub featured: bool,
  pub image_url: Option<String>,
  pub available_total: i32,
}

#[derive(Clone)]
pub struct ProductVariant {
  pub id: Uuid,
  pub sku: String,
  pub size: String,
  pub color: String,
  pub price_in_cents: Option<i32>,
  pub available: i32,
}

#[derive(Clone)]
pub struct ProductDetail {
  pub id: Uuid,
  pub name: String,
  pub slug: String,
  pub description: Option<String>,
  pub price_in_cents: i32,
  pub category: String,
  #[allow(dead_code)]
  pub featured: bool,
  #[allow(dead_code)]
  pub available_total: i32,
  pub images: Vec<ProductImage>,
  pub variants: Vec<ProductVariant>,
  #[allow(dead_code)]
  pub collection: Option<CollectionBrief>,
}

#[derive(Clone)]
pub struct ProductImage {
  pub id: Uuid,
  pub url: String,
  pub alt: Option<String>,
  #[allow(dead_code)]
  pub sort_order: i32,
}

#[derive(Clone)]
pub struct CollectionBrief {
  #[allow(dead_code)]
  pub id: Uuid,
  #[allow(dead_code)]
  pub name: String,
  #[allow(dead_code)]
  pub slug: String,
}

#[derive(Clone)]
pub struct CollectionSummary {
  pub id: Uuid,
  pub name: String,
  pub slug: String,
  pub description: Option<String>,
  pub product_count: i64,
}

pub struct StoreSettings {
  pub whatsapp_number: Option<String>,
  #[allow(dead_code)]
  pub whatsapp_message_template: Option<String>,
  pub pix_key: Option<String>,
  pub pix_key_type: Option<String>,
  pub pix_merchant_name: Option<String>,
  pub pix_merchant_city: Option<String>,
}

pub fn format_price(cents: i32) -> String {
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

pub async fn list_products(
  pool: &PgPool,
  category: Option<&str>,
  search: Option<&str>,
  featured_only: bool,
  include_out_of_stock: bool,
  collection_slug: Option<&str>,
) -> Result<Vec<ProductSummary>, sqlx::Error> {
  let mut sql = String::from(
        "SELECT p.id, p.name, p.slug, p.description, p.price_in_cents, p.category::text AS category, p.featured,
            (SELECT pi.url FROM product_images pi WHERE pi.product_id = p.id ORDER BY pi.sort_order ASC LIMIT 1) AS image_url,
            COALESCE((SELECT SUM(GREATEST(i.quantity - i.reserved, 0))::int
                FROM inventory i
                INNER JOIN warehouses w ON w.id = i.warehouse_id
                WHERE i.variant_id IN (SELECT pv.id FROM product_variants pv WHERE pv.product_id = p.id)
                  AND w.active = true), 0) AS available_total
         FROM products p
         WHERE p.active = true",
    );

  let mut param_index = 1;

  if category.is_some() {
    sql.push_str(&format!(
      " AND p.category = ${}::product_category",
      param_index
    ));
    param_index += 1;
  }
  if search.is_some() {
    sql.push_str(&format!(
            " AND (p.name ILIKE ${} OR p.description ILIKE ${} OR EXISTS (SELECT 1 FROM product_variants pv WHERE pv.product_id = p.id AND pv.sku ILIKE ${}))",
            param_index,
            param_index + 1,
            param_index + 2,
        ));
    param_index += 3;
  }
  if featured_only {
    sql.push_str(" AND p.featured = true");
  }
  if collection_slug.is_some() {
    sql.push_str(&format!(
            " AND EXISTS (SELECT 1 FROM collections c WHERE c.id = p.collection_id AND c.slug = ${} AND c.active = true)",
            param_index
        ));
  }
  if !include_out_of_stock {
    sql.push_str(" AND COALESCE((SELECT SUM(GREATEST(i.quantity - i.reserved, 0)) FROM inventory i INNER JOIN warehouses w ON w.id = i.warehouse_id WHERE i.variant_id IN (SELECT pv.id FROM product_variants pv WHERE pv.product_id = p.id) AND w.active = true), 0) > 0");
  }

  sql.push_str(" ORDER BY p.featured DESC, p.created_at DESC");

  let mut query = sqlx::query_as::<_, ProductSummaryRow>(&sql);

  if let Some(c) = category {
    query = query.bind(c);
  }
  if let Some(s) = search {
    let term = format!("%{}%", s.trim());
    query = query.bind(term.clone()).bind(term.clone()).bind(term);
  }
  if let Some(slug) = collection_slug {
    query = query.bind(slug);
  }

  let rows = query.fetch_all(pool).await?;
  Ok(rows.into_iter().map(|r| r.into()).collect())
}

pub async fn get_product_by_slug(
  pool: &PgPool,
  slug: &str,
) -> Result<Option<ProductDetail>, sqlx::Error> {
  let row = sqlx::query!(
        "SELECT id, name, slug, description, price_in_cents, category::text AS \"category!\", featured, collection_id
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
        "SELECT id, url, alt, sort_order FROM product_images WHERE product_id = $1 ORDER BY sort_order ASC",
        product.id
    )
    .fetch_all(pool)
    .await?;

  let variants = sqlx::query!(
        "SELECT id, sku, size::text AS \"size!\", color, price_in_cents,
            COALESCE((SELECT SUM(GREATEST(i.quantity - i.reserved, 0))::int
                FROM inventory i
                INNER JOIN warehouses w ON w.id = i.warehouse_id
                WHERE i.variant_id = product_variants.id AND w.active = true), 0) AS available
         FROM product_variants WHERE product_id = $1 ORDER BY size ASC, color ASC",
        product.id
    )
    .fetch_all(pool)
    .await?;

  let available_total: i32 =
    variants.iter().map(|v| v.available.unwrap_or(0)).sum();

  let collection = match &product.collection_id {
    Some(cid) => sqlx::query_as!(
      CollectionBriefRow,
      "SELECT id, name, slug FROM collections WHERE id = $1",
      cid
    )
    .fetch_optional(pool)
    .await?
    .map(|c| c.into()),
    None => None,
  };

  Ok(Some(ProductDetail {
    id: product.id,
    name: product.name,
    slug: product.slug,
    description: product.description,
    price_in_cents: product.price_in_cents,
    category: product.category,
    featured: product.featured,
    available_total,
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
    collection,
  }))
}

pub async fn list_collections(
  pool: &PgPool,
) -> Result<Vec<CollectionSummary>, sqlx::Error> {
  let rows = sqlx::query!(
        "SELECT c.id, c.name, c.slug, c.description,
            (SELECT COUNT(*) FROM products p WHERE p.collection_id = c.id AND p.active = true) AS product_count
         FROM collections c WHERE c.active = true ORDER BY c.name ASC"
    )
    .fetch_all(pool)
    .await?;

  Ok(
    rows
      .into_iter()
      .map(|r| CollectionSummary {
        id: r.id,
        name: r.name,
        slug: r.slug,
        description: r.description,
        product_count: r.product_count.unwrap_or(0),
      })
      .collect(),
  )
}

pub async fn get_collection_by_slug(
  pool: &PgPool,
  slug: &str,
) -> Result<Option<CollectionSummary>, sqlx::Error> {
  let row = sqlx::query!(
        "SELECT id, name, slug, description FROM collections WHERE slug = $1 AND active = true",
        slug
    )
    .fetch_optional(pool)
    .await?;

  Ok(row.map(|r| CollectionSummary {
    id: r.id,
    name: r.name,
    slug: r.slug,
    description: r.description,
    product_count: 0,
  }))
}

pub async fn count_collection_products(
  pool: &PgPool,
  collection_id: &Uuid,
) -> Result<i64, sqlx::Error> {
  let count: i64 = sqlx::query_scalar!(
    "SELECT COUNT(*) FROM products WHERE collection_id = $1 AND active = true",
    collection_id
  )
  .fetch_one(pool)
  .await?
  .unwrap_or(0);
  Ok(count)
}

pub async fn get_store_settings(
  pool: &PgPool,
) -> Result<StoreSettings, sqlx::Error> {
  let row = sqlx::query!(
        "SELECT whatsapp_number, whatsapp_message_template, pix_key, pix_key_type::text AS pix_key_type, pix_merchant_name, pix_merchant_city
         FROM store_settings ORDER BY created_at ASC LIMIT 1"
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
    },
    None => StoreSettings {
      whatsapp_number: None,
      whatsapp_message_template: None,
      pix_key: None,
      pix_key_type: None,
      pix_merchant_name: None,
      pix_merchant_city: None,
    },
  })
}

#[derive(sqlx::FromRow)]
struct ProductSummaryRow {
  id: Uuid,
  name: String,
  slug: String,
  description: Option<String>,
  price_in_cents: i32,
  category: String,
  featured: bool,
  image_url: Option<String>,
  available_total: Option<i32>,
}

impl From<ProductSummaryRow> for ProductSummary {
  fn from(r: ProductSummaryRow) -> Self {
    ProductSummary {
      id: r.id,
      name: r.name,
      slug: r.slug,
      description: r.description,
      price_in_cents: r.price_in_cents,
      category: r.category,
      featured: r.featured,
      image_url: r.image_url,
      available_total: r.available_total.unwrap_or(0),
    }
  }
}

#[derive(sqlx::FromRow)]
struct ProductImageRow {
  id: Uuid,
  url: String,
  alt: Option<String>,
  sort_order: i32,
}

impl From<ProductImageRow> for ProductImage {
  fn from(r: ProductImageRow) -> Self {
    ProductImage {
      id: r.id,
      url: r.url,
      alt: r.alt,
      sort_order: r.sort_order,
    }
  }
}

#[derive(sqlx::FromRow)]
struct CollectionBriefRow {
  id: Uuid,
  name: String,
  slug: String,
}

impl From<CollectionBriefRow> for CollectionBrief {
  fn from(r: CollectionBriefRow) -> Self {
    CollectionBrief {
      id: r.id,
      name: r.name,
      slug: r.slug,
    }
  }
}
