use rand::Rng;
use serde::Deserialize;
use sqlx::PgPool;
use std::collections::{HashMap, HashSet};
use uuid::Uuid;

use super::SeedError;

const CATALOG_JSON: &str = include_str!("../data/catalog.json");

const VALID_CATEGORIES: [&str; 6] = [
  "biquini",
  "biquini_top",
  "biquini_bottom",
  "maio",
  "saida_praia",
  "acessorio",
];

const VALID_SIZES: [&str; 5] = ["pp", "p", "m", "g", "gg"];

#[derive(Debug, Deserialize)]
struct Catalog {
  products: Vec<CatalogProduct>,
  sets: Vec<CatalogSet>,
}

#[derive(Debug, Deserialize, Clone)]
struct CatalogProduct {
  name: String,
  slug: String,
  description: String,
  category: String,
  price_in_cents: i64,
  compare_at_price_in_cents: Option<i64>,
  featured: bool,
  images: Vec<CatalogImage>,
  colors: Vec<CatalogColor>,
  sizes: Vec<String>,
}

#[derive(Debug, Deserialize, Clone)]
struct CatalogImage {
  url: String,
  alt: String,
}

#[derive(Debug, Deserialize, Clone)]
struct CatalogColor {
  name: String,
  hex: String,
}

#[derive(Debug, Deserialize, Clone)]
struct CatalogSet {
  name: String,
  slug: String,
  description: String,
  top_slug: String,
  bottom_slug: String,
}

fn env_str(key: &str, default: &str) -> String {
  std::env::var(key).unwrap_or_else(|_| default.to_string())
}

fn env_num<T: std::str::FromStr>(
  key: &str,
  default: T,
) -> Result<T, SeedError> {
  match std::env::var(key) {
    Ok(value) => value
      .parse()
      .map_err(|_| SeedError::Env(format!("invalid value for {key}"))),
    Err(_) => Ok(default),
  }
}

fn slugify(input: &str) -> String {
  let mut slug = String::new();
  for c in input.chars() {
    let c = match c {
      'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' | 'Ä' => {
        'a'
      }
      'é' | 'ê' | 'ë' | 'É' | 'Ê' | 'Ë' => 'e',
      'í' | 'ï' | 'Í' | 'Ï' => 'i',
      'ó' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ô' | 'Õ' | 'Ö' => 'o',
      'ú' | 'ü' | 'Ú' | 'Ü' => 'u',
      'ç' | 'Ç' => 'c',
      'ñ' | 'Ñ' => 'n',
      _ => c,
    };
    if c.is_alphanumeric() {
      slug.extend(c.to_lowercase());
    } else if !slug.is_empty() && !slug.ends_with('-') {
      slug.push('-');
    }
  }
  slug.trim_matches('-').to_string()
}

fn clamp_price(price: i64, min: i64, max: i64) -> i64 {
  price.max(min).min(max)
}

/// Round-robin across categories so a small `SEED_PRODUCT_COUNT` still
/// fills every nav bucket instead of taking only the first JSON block.
fn take_products(
  products: &[CatalogProduct],
  count: usize,
) -> Vec<&CatalogProduct> {
  let mut buckets: HashMap<&str, Vec<&CatalogProduct>> = HashMap::new();
  let mut order: Vec<&str> = Vec::new();
  for product in products {
    let cat = product.category.as_str();
    if !buckets.contains_key(cat) {
      order.push(cat);
    }
    buckets.entry(cat).or_default().push(product);
  }
  let mut indexes: HashMap<&str, usize> = HashMap::new();
  let mut chosen = Vec::new();
  while chosen.len() < count {
    let mut progressed = false;
    for cat in &order {
      if chosen.len() >= count {
        break;
      }
      let bucket = buckets.get(cat).map(Vec::as_slice).unwrap_or(&[]);
      let idx = indexes.get(cat).copied().unwrap_or(0);
      if idx < bucket.len() {
        chosen.push(bucket[idx]);
        indexes.insert(cat, idx + 1);
        progressed = true;
      }
    }
    if !progressed {
      break;
    }
  }
  chosen
}

async fn seed_warehouse(pool: &PgPool) -> Result<Uuid, SeedError> {
  let existing =
    sqlx::query!("SELECT id FROM warehouses WHERE code = 'PRINCIPAL'")
      .fetch_optional(pool)
      .await?;

  if let Some(row) = existing {
    return Ok(row.id);
  }

  let id = Uuid::now_v7();
  sqlx::query!(
    "INSERT INTO warehouses (id, code, name, is_default, active, created_at)
         VALUES ($1, 'PRINCIPAL', 'Depósito Principal', true, true, now())",
    id,
  )
  .execute(pool)
  .await?;

  tracing::info!("Warehouse created: PRINCIPAL (Depósito Principal)");
  Ok(id)
}

const FIRST_PURCHASE_COUPON: &str = "BEMVINDA10";

async fn seed_settings(pool: &PgPool) -> Result<(), SeedError> {
  let existing = sqlx::query!("SELECT id FROM store_settings LIMIT 1")
    .fetch_optional(pool)
    .await?;

  if existing.is_some() {
    tracing::info!("Store settings already present, leaving unchanged");
  } else {
    sqlx::query(
        "INSERT INTO store_settings (id, pix_key, pix_key_type, pix_merchant_name, pix_merchant_city, pix_discount_percent, announcement_text, first_purchase_coupon_code, whatsapp_number, whatsapp_message_template, created_at, updated_at)
         VALUES ($1, $2, 'phone', 'Linda Flor Moda Praia', 'Aracaju', 5, NULL, $5, $3, $4, now(), now())",
    )
    .bind(Uuid::now_v7())
    .bind(env_str("SEED_PIX_KEY", "79998165115"))
    .bind(env_str("SEED_WHATSAPP_NUMBER", "5579998165115"))
    .bind(env_str(
        "SEED_WHATSAPP_MESSAGE_TEMPLATE",
        "Olá! Fiz o pedido {{order_id}} no valor de {{total}} e quero confirmar o pagamento via PIX.",
    ))
    .bind(FIRST_PURCHASE_COUPON)
    .execute(pool)
    .await?;

    tracing::info!("Store settings created");
  }

  sqlx::query(
    "INSERT INTO coupons (id, code, discount_type, discount_value, min_subtotal_cents, active, usage_type, per_user_limit, created_at, updated_at)
     VALUES ($1, $2, 'percent', 10, 0, true, 'unlimited', 1, now(), now())
     ON CONFLICT (code) DO NOTHING",
  )
  .bind(Uuid::now_v7())
  .bind(FIRST_PURCHASE_COUPON)
  .execute(pool)
  .await?;

  let banner_count: i64 =
    sqlx::query_scalar("SELECT COUNT(*) FROM home_banners")
      .fetch_one(pool)
      .await?;

  if banner_count == 0 {
    sqlx::query(
        "INSERT INTO home_banners (id, title, subtitle, cta_label, link_url, image_url, sort_order, active, created_at)
         VALUES
         ($1, 'Verão 2027', 'Coleção Nova', 'Explorar', '/produtos?sort=novos', 'https://images.unsplash.com/photo-1519046904884-53103b34b206?w=1200&h=600&fit=crop', 0, true, now()),
         ($2, 'Liquida', '20% OFF', 'Ver Liquida', '/produtos?sale=true', 'https://images.unsplash.com/photo-1473496169904-658ba7c44d8a?w=1200&h=600&fit=crop', 1, true, now())",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(pool)
    .await?;
    tracing::info!("Home banners created");
  }

  Ok(())
}

async fn seed_looks(pool: &PgPool) -> Result<(), SeedError> {
  let rows: Vec<(Uuid, String, String)> = sqlx::query_as(
    "SELECT p.id, p.category::text, pv.color
     FROM products p
     INNER JOIN product_variants pv ON pv.product_id = p.id
     WHERE p.active = true
       AND p.category IN ('biquini_top'::product_category, 'biquini_bottom'::product_category)",
  )
  .fetch_all(pool)
  .await?;

  let mut colors_by_product: HashMap<Uuid, HashSet<String>> = HashMap::new();
  let mut tops: HashSet<Uuid> = HashSet::new();
  let mut bottoms: HashSet<Uuid> = HashSet::new();
  for (id, category, color) in rows {
    colors_by_product.entry(id).or_default().insert(color);
    if category == "biquini_top" {
      tops.insert(id);
    } else {
      bottoms.insert(id);
    }
  }

  const MAX_LOOKS: usize = 8;
  for top_id in &tops {
    let top_colors = colors_by_product.get(top_id);
    let mut sharing: Vec<Uuid> = bottoms
      .iter()
      .copied()
      .filter(|bottom_id| {
        let Some(top_colors) = top_colors else {
          return false;
        };
        colors_by_product
          .get(bottom_id)
          .is_some_and(|bottom_colors| !top_colors.is_disjoint(bottom_colors))
      })
      .collect();
    if sharing.is_empty() {
      sharing = bottoms.iter().copied().collect();
    }
    sharing.sort();
    for (sort, bottom_id) in sharing.into_iter().take(MAX_LOOKS).enumerate() {
      sqlx::query(
        "INSERT INTO product_looks (product_id, related_product_id, sort_order)
         VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING",
      )
      .bind(top_id)
      .bind(bottom_id)
      .bind(sort as i32)
      .execute(pool)
      .await?;
      sqlx::query(
        "INSERT INTO product_looks (product_id, related_product_id, sort_order)
         VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING",
      )
      .bind(bottom_id)
      .bind(top_id)
      .bind(sort as i32)
      .execute(pool)
      .await?;
    }
  }

  Ok(())
}

async fn seed_sets(pool: &PgPool, catalog: &Catalog) -> Result<(), SeedError> {
  let max_sets = env_num("SEED_SET_COUNT", 12)?;

  for set in catalog.sets.iter().take(max_sets) {
    let top: Option<(Uuid, Uuid, i64)> = sqlx::query_as(
      "SELECT p.id, pv.id, p.price_in_cents
       FROM products p
       INNER JOIN LATERAL (
         SELECT pv.id FROM product_variants pv
         WHERE pv.product_id = p.id
         ORDER BY pv.created_at ASC LIMIT 1
       ) pv ON true
       WHERE p.slug = $1 AND p.active = true",
    )
    .bind(&set.top_slug)
    .fetch_optional(pool)
    .await?;

    let bottom: Option<(Uuid, Uuid, i64)> = sqlx::query_as(
      "SELECT p.id, pv.id, p.price_in_cents
       FROM products p
       INNER JOIN LATERAL (
         SELECT pv.id FROM product_variants pv
         WHERE pv.product_id = p.id
         ORDER BY pv.created_at ASC LIMIT 1
       ) pv ON true
       WHERE p.slug = $1 AND p.active = true",
    )
    .bind(&set.bottom_slug)
    .fetch_optional(pool)
    .await?;

    let (
      Some((top_id, top_variant, top_price)),
      Some((bottom_id, bottom_variant, bottom_price)),
    ) = (top, bottom)
    else {
      tracing::info!(
        slug = set.slug.as_str(),
        "Skipping set seed: missing top or bottom product"
      );
      continue;
    };

    let sum = top_price + bottom_price;
    let price = (sum * 90) / 100;

    let existing: Option<Uuid> =
      sqlx::query_scalar("SELECT id FROM sets WHERE slug = $1")
        .bind(&set.slug)
        .fetch_optional(pool)
        .await?;

    let set_id = match existing {
      Some(id) => id,
      None => {
        let id = Uuid::now_v7();
        sqlx::query(
          "INSERT INTO sets (id, name, slug, description, active, price_in_cents, compare_at_price_in_cents, created_at, updated_at)
           VALUES ($1, $2, $3, $4, true, $5, $6, now(), now())",
        )
        .bind(id)
        .bind(&set.name)
        .bind(&set.slug)
        .bind(&set.description)
        .bind(price)
        .bind(sum)
        .execute(pool)
        .await?;
        tracing::info!("Set created: {}", set.name);
        id
      }
    };

    let image_count: i64 =
      sqlx::query_scalar("SELECT COUNT(*) FROM set_images WHERE set_id = $1")
        .bind(set_id)
        .fetch_one(pool)
        .await?;
    if image_count == 0 {
      let member_urls: Vec<(String, String)> = sqlx::query_as(
        "SELECT url, alt FROM product_images
         WHERE product_id = ANY($1)
         ORDER BY sort_order ASC
         LIMIT 3",
      )
      .bind(&[top_id, bottom_id][..])
      .fetch_all(pool)
      .await?;
      for (sort, (url, alt)) in member_urls.into_iter().enumerate() {
        sqlx::query(
          "INSERT INTO set_images (id, set_id, url, alt, sort_order, created_at)
           VALUES ($1, $2, $3, $4, $5, now())",
        )
        .bind(Uuid::now_v7())
        .bind(set_id)
        .bind(url)
        .bind(alt)
        .bind(sort as i32)
        .execute(pool)
        .await?;
      }
    }

    for (order, product_id) in [top_id, bottom_id].into_iter().enumerate() {
      sqlx::query(
        "INSERT INTO set_members (set_id, product_id, sort_order)
         VALUES ($1, $2, $3)
         ON CONFLICT DO NOTHING",
      )
      .bind(set_id)
      .bind(product_id)
      .bind(order as i32)
      .execute(pool)
      .await?;
    }

    let slot_count: i64 =
      sqlx::query_scalar("SELECT COUNT(*) FROM set_slots WHERE set_id = $1")
        .bind(set_id)
        .fetch_one(pool)
        .await?;
    if slot_count == 0 {
      for (order, (label, variant_id)) in
        [("Top", top_variant), ("Calcinha", bottom_variant)]
          .into_iter()
          .enumerate()
      {
        let slot_id = Uuid::now_v7();
        sqlx::query(
          "INSERT INTO set_slots (id, set_id, label, sort_order)
           VALUES ($1, $2, $3, $4)",
        )
        .bind(slot_id)
        .bind(set_id)
        .bind(label)
        .bind(order as i32)
        .execute(pool)
        .await?;
        sqlx::query(
          "INSERT INTO set_slot_options (slot_id, variant_id, price_override_cents)
           VALUES ($1, $2, NULL)
           ON CONFLICT DO NOTHING",
        )
        .bind(slot_id)
        .bind(variant_id)
        .execute(pool)
        .await?;
      }
    }
  }

  Ok(())
}

async fn seed_products(
  pool: &PgPool,
  warehouse_id: Uuid,
  catalog: &Catalog,
) -> Result<(), SeedError> {
  let count = env_num("SEED_PRODUCT_COUNT", 108)?;
  let price_min = env_num("SEED_PRICE_MIN", 3990)?;
  let price_max = env_num("SEED_PRICE_MAX", 29990)?;
  let stock_min = env_num("SEED_STOCK_MIN", 5)?;
  let stock_max = env_num("SEED_STOCK_MAX", 30)?;
  let mut rng = rand::thread_rng();

  let selected = take_products(&catalog.products, count as usize);

  for (product_index, product) in selected.into_iter().enumerate() {
    if !VALID_CATEGORIES.contains(&product.category.as_str()) {
      tracing::warn!(
        slug = product.slug.as_str(),
        category = product.category.as_str(),
        "Skipping product with invalid category"
      );
      continue;
    }

    let existing =
      sqlx::query!("SELECT id FROM products WHERE slug = $1", product.slug)
        .fetch_optional(pool)
        .await?;

    if existing.is_some() {
      continue;
    }

    let id = Uuid::now_v7();
    let price = clamp_price(product.price_in_cents, price_min, price_max);
    let compare_at_price = product.compare_at_price_in_cents.and_then(|raw| {
      let compare = clamp_price(raw, price_min, price_max.max(raw));
      if compare > price { Some(compare) } else { None }
    });

    sqlx::query(
            "INSERT INTO products (id, name, slug, description, price_in_cents, compare_at_price_in_cents, category, active, featured, created_at, updated_at)
             VALUES ($1, $2, $3, $4, $5, $6, $7::product_category, true, $8, now(), now())",
        )
        .bind(id)
        .bind(&product.name)
        .bind(&product.slug)
        .bind(&product.description)
        .bind(price)
        .bind(compare_at_price)
        .bind(&product.category)
        .bind(product.featured)
        .execute(pool)
        .await?;

    let sizes: Vec<&str> = product
      .sizes
      .iter()
      .map(String::as_str)
      .filter(|size| VALID_SIZES.contains(size))
      .collect();
    let sizes = if sizes.is_empty() { vec!["m"] } else { sizes };

    let mut variant_index = 0usize;
    for size in &sizes {
      for color in &product.colors {
        let variant_id = Uuid::now_v7();
        let sku = format!("{}-{}-{}", product.slug, size, slugify(&color.name));

        sqlx::query(
                "INSERT INTO product_variants (id, product_id, sku, size, color, color_hex, price_in_cents, low_stock_threshold, created_at, updated_at)
                 VALUES ($1, $2, $3, $4::product_size, $5, $6, NULL, 5, now(), now())",
            )
            .bind(variant_id)
            .bind(id)
            .bind(&sku)
            .bind(*size)
            .bind(&color.name)
            .bind(&color.hex)
            .execute(pool)
            .await?;

        let quantity = if (product_index + variant_index) % 23 == 0 {
          0
        } else if stock_max > stock_min {
          rng.gen_range(stock_min..=stock_max)
        } else {
          stock_min
        };
        variant_index += 1;

        sqlx::query(
                "INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved, updated_at)
                 VALUES ($1, $2, $3, $4, 0, now())",
            )
            .bind(Uuid::now_v7())
            .bind(variant_id)
            .bind(warehouse_id)
            .bind(quantity)
            .execute(pool)
            .await?;
      }
    }

    for (sort, image) in product.images.iter().take(8).enumerate() {
      sqlx::query(
            "INSERT INTO product_images (id, product_id, url, alt, sort_order, created_at)
             VALUES ($1, $2, $3, $4, $5, now())",
        )
        .bind(Uuid::now_v7())
        .bind(id)
        .bind(&image.url)
        .bind(&image.alt)
        .bind(sort as i32)
        .execute(pool)
        .await?;
    }

    tracing::info!("Product created: {} ({})", product.name, product.slug);
  }

  seed_looks(pool).await?;
  Ok(())
}

pub async fn seed(pool: &PgPool) -> Result<(), SeedError> {
  let catalog: Catalog = serde_json::from_str(CATALOG_JSON)?;
  let warehouse_id = seed_warehouse(pool).await?;
  seed_settings(pool).await?;
  seed_products(pool, warehouse_id, &catalog).await?;
  seed_sets(pool, &catalog).await?;
  Ok(())
}
