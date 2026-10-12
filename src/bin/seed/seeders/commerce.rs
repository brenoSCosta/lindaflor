use rand::Rng;
use rand::seq::SliceRandom;
use sqlx::PgPool;
use uuid::Uuid;

use super::SeedError;

const PRODUCT_NAMES: [&str; 12] = [
  "Biquíni Triângulo",
  "Biquíni Corta-Vento",
  "Maiô Estampado",
  "Maiô Liso",
  "Saída de Praia Midi",
  "Saída de Praia Longa",
  "Chapéu de Palha",
  "Canga Estampada",
  "Pareô Estampado",
  "Biquíni Fio Dental",
  "Maiô com Amarração",
  "Biquíni Manga Longa",
];

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "product_category", rename_all = "lowercase")]
enum ProductCategory {
  Biquini,
  #[sqlx(rename = "top_biquini")]
  TopBiquini,
  #[sqlx(rename = "calcinha_biquini")]
  CalchinhaBiquini,
  Maio,
  #[sqlx(rename = "saida_praia")]
  SaidaPraia,
  Acessorio,
}

#[derive(Debug, Clone, Copy, sqlx::Type)]
#[sqlx(type_name = "product_size", rename_all = "lowercase")]
enum ProductSize {
  Pp,
  P,
  M,
  G,
  Gg,
}

impl ProductSize {
  fn as_str(self) -> &'static str {
    match self {
      ProductSize::Pp => "pp",
      ProductSize::P => "p",
      ProductSize::M => "m",
      ProductSize::G => "g",
      ProductSize::Gg => "gg",
    }
  }
}

const CATEGORIES: [ProductCategory; 6] = [
  ProductCategory::Biquini,
  ProductCategory::TopBiquini,
  ProductCategory::CalchinhaBiquini,
  ProductCategory::Maio,
  ProductCategory::SaidaPraia,
  ProductCategory::Acessorio,
];

const SIZES: [ProductSize; 5] = [
  ProductSize::Pp,
  ProductSize::P,
  ProductSize::M,
  ProductSize::G,
  ProductSize::Gg,
];

const COLORS: [(&str, &str); 12] = [
  ("Verde Tropical", "#2D5016"),
  ("Floral", "#E8A0BF"),
  ("Azul Escandinavo", "#4A90E2"),
  ("Branco", "#F5F5F5"),
  ("Preto", "#111111"),
  ("Vermelho", "#E63946"),
  ("Rosa", "#FFB4D1"),
  ("Amarelo", "#FFD60A"),
  ("Terracota", "#C84E00"),
  ("Maré", "#0A9FB5"),
  ("Areia", "#D4B896"),
  ("Geométrico", "#7B68EE"),
];

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
    // Leave an existing row alone: admins may have cleared or changed these.
    tracing::info!("Store settings already present, leaving unchanged");
  } else {
    sqlx::query(
        "INSERT INTO store_settings (id, pix_key, pix_key_type, pix_merchant_name, pix_merchant_city, pix_discount_percent, announcement_text, first_purchase_coupon_code, whatsapp_number, whatsapp_message_template, created_at, updated_at)
         VALUES ($1, $2, 'phone', 'Linda Flor Moda Praia', 'Aracaju', 5, NULL, $5, $3, $4, now(), now())",
    )
    .bind(Uuid::now_v7())
    .bind(env_str("SEED_PIX_KEY", "79998165115"))
    .bind(env_str("SEED_WHATSAPP_NUMBER", "557998165115"))
    .bind(env_str(
        "SEED_WHATSAPP_MESSAGE_TEMPLATE",
        "Olá! Fiz o pedido {{order_id}} no valor de {{total}} e quero confirmar o pagamento via PIX.",
    ))
    .bind(FIRST_PURCHASE_COUPON)
    .execute(pool)
    .await?;

    tracing::info!("Store settings created");
  }

  // The storefront advertises this code, so it must exist as a real coupon:
  // 10% off, once per customer.
  sqlx::query(
    "INSERT INTO coupons (id, code, discount_type, discount_value, min_subtotal_cents, active, usage_type, per_user_limit, created_at, updated_at)
     VALUES ($1, $2, 'percent', 10, 0, true, 'unlimited', 1, now(), now())
     ON CONFLICT (code) DO NOTHING",
  )
  .bind(Uuid::now_v7())
  .bind(FIRST_PURCHASE_COUPON)
  .execute(pool)
  .await?;

  // Insert home banners if empty
  let banner_count: i64 =
    sqlx::query_scalar("SELECT COUNT(*) FROM home_banners")
      .fetch_one(pool)
      .await?;

  if banner_count == 0 {
    sqlx::query(
        "INSERT INTO home_banners (id, title, subtitle, cta_label, link_url, image_url, sort_order, active, created_at)
         VALUES
         ($1, 'Verão 2027', 'Coleção Nova', 'Explorar', '/produtos?sort=novos', 'https://images.unsplash.com/photo-1519046904884-53103b34b206?w=1200&h=600&fit=crop', 0, true, now()),
         ($2, 'Liquida', '20% OFF', 'Ver Liquida', '/produtos?sale=true', 'https://images.unsplash.com/photo-1505252585461-04db1267ae5b?w=1200&h=600&fit=crop', 1, true, now())",
    )
    .bind(Uuid::now_v7())
    .bind(Uuid::now_v7())
    .execute(pool)
    .await?;
    tracing::info!("Home banners created");
  }

  Ok(())
}

async fn seed_collections(pool: &PgPool) -> Result<Vec<Uuid>, SeedError> {
  let names = env_str("SEED_COLLECTIONS", "Verão 2026,Clássicos,Pôr do Sol");
  let mut ids = Vec::new();

  for name in names.split(',').map(|s| s.trim()).filter(|s| !s.is_empty()) {
    let slug = slugify(name);
    let existing =
      sqlx::query!("SELECT id FROM collections WHERE slug = $1", slug)
        .fetch_optional(pool)
        .await?;

    let id = match existing {
      Some(row) => row.id,
      None => {
        let id = Uuid::now_v7();
        sqlx::query!(
                    "INSERT INTO collections (id, name, slug, description, active, created_at, updated_at)
                     VALUES ($1, $2, $3, NULL, true, now(), now())",
                    id,
                    name,
                    slug,
                )
                .execute(pool)
                .await?;
        tracing::info!("Collection created: {}", name);
        id
      }
    };
    ids.push(id);
  }

  Ok(ids)
}

async fn seed_products(
  pool: &PgPool,
  warehouse_id: Uuid,
  collection_ids: &[Uuid],
) -> Result<(), SeedError> {
  let count = env_num("SEED_PRODUCT_COUNT", 12)?;
  let price_min = env_num("SEED_PRICE_MIN", 6990)?;
  let price_max = env_num("SEED_PRICE_MAX", 29990)?;
  let stock_min = env_num("SEED_STOCK_MIN", 5)?;
  let stock_max = env_num("SEED_STOCK_MAX", 30)?;
  let mut rng = rand::thread_rng();
  let mut top_biquini_ids = Vec::new();
  let mut calcinha_biquini_ids = Vec::new();

  for i in 0..count {
    let name = PRODUCT_NAMES[i as usize % PRODUCT_NAMES.len()];
    let mut slug = slugify(name);
    if i as usize >= PRODUCT_NAMES.len() {
      slug = format!("{}-{}", slug, i);
    }

    let existing =
      sqlx::query!("SELECT id FROM products WHERE slug = $1", slug)
        .fetch_optional(pool)
        .await?;

    if existing.is_some() {
      continue;
    }

    let id = Uuid::now_v7();
    let category = CATEGORIES[i as usize % CATEGORIES.len()];
    let price = if price_max > price_min {
      rng.gen_range(price_min..=price_max)
    } else {
      price_min
    };
    // Set compare_at_price_in_cents on ~1/3 of products (20-40% above price)
    let compare_at_price = if (i + 1) % 3 == 0 {
      let markup = (120 + rng.gen_range(0..=20)) as i64; // 120-140% of price
      Some((price as i64 * markup) / 100)
    } else {
      None
    };
    let featured = (i + 1) % 3 == 0;
    let collection_id = if collection_ids.is_empty() {
      None
    } else {
      Some(collection_ids[i as usize % collection_ids.len()])
    };

    sqlx::query(
            "INSERT INTO products (id, name, slug, description, price_in_cents, compare_at_price_in_cents, category, collection_id, active, featured, created_at, updated_at)
             VALUES ($1, $2, $3, NULL, $4, $5, $6::product_category, $7, true, $8, now(), now())",
        )
        .bind(id)
        .bind(name)
        .bind(&slug)
        .bind(price)
        .bind(compare_at_price)
        .bind(match category {
          ProductCategory::Biquini => "biquini",
          ProductCategory::TopBiquini => "top_biquini",
          ProductCategory::CalchinhaBiquini => "calcinha_biquini",
          ProductCategory::Maio => "maio",
          ProductCategory::SaidaPraia => "saida_praia",
          ProductCategory::Acessorio => "acessorio",
        })
        .bind(collection_id)
        .bind(featured)
        .execute(pool)
        .await?;

    // Track top_biquini and calcinha_biquini IDs for linking
    match category {
      ProductCategory::TopBiquini => top_biquini_ids.push(id),
      ProductCategory::CalchinhaBiquini => calcinha_biquini_ids.push(id),
      _ => {}
    }

    let mut sizes = SIZES.to_vec();
    sizes.shuffle(&mut rng);
    let variant_count = rng.gen_range(1..=5);

    for size in sizes.iter().take(variant_count) {
      let (color_name, color_hex) = COLORS.choose(&mut rng).copied().unwrap();
      let variant_id = Uuid::now_v7();
      let sku = format!("{}-{}", slug, size.as_str());

      sqlx::query(
                "INSERT INTO product_variants (id, product_id, sku, size, color, color_hex, price_in_cents, low_stock_threshold, created_at, updated_at)
                 VALUES ($1, $2, $3, $4::product_size, $5, $6, NULL, 5, now(), now())",
            )
            .bind(variant_id)
            .bind(id)
            .bind(&sku)
            .bind(size.as_str())
            .bind(color_name)
            .bind(color_hex)
            .execute(pool)
            .await?;

      let quantity = rng.gen_range(stock_min..=stock_max);
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

    sqlx::query(
            "INSERT INTO product_images (id, product_id, url, alt, sort_order, created_at)
             VALUES ($1, $2, $3, $4, 0, now())",
        )
        .bind(Uuid::now_v7())
        .bind(id)
        .bind(format!("https://picsum.photos/seed/{}/1200/1200", slug))
        .bind(name)
        .execute(pool)
        .await?;

    tracing::info!("Product created: {} ({})", name, slug);
  }

  // Link top_biquini and calcinha_biquini products (both directions)
  for top_id in &top_biquini_ids {
    for calcinha_id in &calcinha_biquini_ids {
      // Link top to calcinha
      sqlx::query(
        "INSERT INTO product_looks (product_id, related_product_id, sort_order)
         VALUES ($1, $2, 0)
         ON CONFLICT DO NOTHING",
      )
      .bind(top_id)
      .bind(calcinha_id)
      .execute(pool)
      .await?;

      // Link calcinha to top (both directions)
      sqlx::query(
        "INSERT INTO product_looks (product_id, related_product_id, sort_order)
         VALUES ($1, $2, 0)
         ON CONFLICT DO NOTHING",
      )
      .bind(calcinha_id)
      .bind(top_id)
      .execute(pool)
      .await?;
    }
  }

  Ok(())
}

pub async fn seed(pool: &PgPool) -> Result<(), SeedError> {
  let warehouse_id = seed_warehouse(pool).await?;
  seed_settings(pool).await?;
  let collection_ids = seed_collections(pool).await?;
  seed_products(pool, warehouse_id, &collection_ids).await?;
  Ok(())
}
