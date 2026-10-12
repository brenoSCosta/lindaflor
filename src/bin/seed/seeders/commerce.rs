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
  #[sqlx(rename = "biquini_top")]
  BiquiniTop,
  #[sqlx(rename = "biquini_bottom")]
  BiquiniBottom,
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
  ProductCategory::BiquiniTop,
  ProductCategory::BiquiniBottom,
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

async fn seed_sets(pool: &PgPool) -> Result<(), SeedError> {
  let max_sets = env_num("SEED_SET_COUNT", 4)?;

  // Pair tops with bottoms so each conjunto is a realistic top + calcinha
  // bundle. Falls back to any active products when categories are missing.
  struct SeedCandidate {
    product_id: Uuid,
    variant_id: Uuid,
    price_in_cents: i64,
  }

  async fn candidates_for(
    pool: &PgPool,
    category: &str,
  ) -> Result<Vec<SeedCandidate>, SeedError> {
    let rows: Vec<(Uuid, Uuid, i64)> = sqlx::query_as(
      "SELECT p.id, pv.id, p.price_in_cents
       FROM products p
       INNER JOIN LATERAL (
         SELECT pv.id FROM product_variants pv
         WHERE pv.product_id = p.id
         ORDER BY pv.created_at ASC LIMIT 1
       ) pv ON true
       WHERE p.active = true AND p.category = $1::product_category
       ORDER BY p.created_at ASC",
    )
    .bind(category)
    .fetch_all(pool)
    .await?;
    Ok(
      rows
        .into_iter()
        .map(|(product_id, variant_id, price_in_cents)| SeedCandidate {
          product_id,
          variant_id,
          price_in_cents,
        })
        .collect(),
    )
  }

  let tops = candidates_for(pool, "biquini_top").await?;
  let bottoms = candidates_for(pool, "biquini_bottom").await?;

  // (name, slug, description) pool; only as many as we can pair.
  let demo = [
    (
      "Conjunto Verão Tropical",
      "conjunto-verao-tropical",
      "Top + calcinha estampados para os dias de sol.",
    ),
    (
      "Conjunto Pôr do Sol",
      "conjunto-por-do-sol",
      "Peças lisas em tons quentes para combinar com tudo.",
    ),
    (
      "Conjunto Maré Alta",
      "conjunto-mare-alta",
      "Top de amarração + calcinha fio dental.",
    ),
    (
      "Conjunto Areia Dourada",
      "conjunto-areia-dourada",
      "Clássico neutro para o verão inteiro.",
    ),
  ];

  // Fixed composition needs one top and one bottom per set.
  let pairable = tops.len().min(bottoms.len());
  if pairable == 0 {
    tracing::info!(
      "Skipping set seed: need biquini_top + biquini_bottom products"
    );
    return Ok(());
  }
  let count = (max_sets as usize).min(pairable).min(demo.len());

  for (index, (name, slug, description)) in demo.iter().take(count).enumerate()
  {
    let top = &tops[index % tops.len()];
    let bottom = &bottoms[index % bottoms.len()];
    // Bundle price: ~10% off the summed member prices; compare_at = sum.
    let sum = top.price_in_cents + bottom.price_in_cents;
    let price = (sum * 90) / 100;

    let existing: Option<Uuid> =
      sqlx::query_scalar("SELECT id FROM sets WHERE slug = $1")
        .bind(*slug)
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
        .bind(*name)
        .bind(*slug)
        .bind(*description)
        .bind(price)
        .bind(sum)
        .execute(pool)
        .await?;
        tracing::info!("Set created: {}", name);
        id
      }
    };

    let image_count: i64 =
      sqlx::query_scalar("SELECT COUNT(*) FROM set_images WHERE set_id = $1")
        .bind(set_id)
        .fetch_one(pool)
        .await?;
    if image_count == 0 {
      for sort in 0..3 {
        sqlx::query(
          "INSERT INTO set_images (id, set_id, url, alt, sort_order, created_at)
           VALUES ($1, $2, $3, $4, $5, now())",
        )
        .bind(Uuid::now_v7())
        .bind(set_id)
        .bind(format!("https://picsum.photos/seed/{slug}-{sort}/1200/1200"))
        .bind(*name)
        .bind(sort)
        .execute(pool)
        .await?;
      }
    }

    for (order, product_id) in
      [top.product_id, bottom.product_id].into_iter().enumerate()
    {
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

    // Fixed composition: one slot per piece, one variant per slot.
    let slot_count: i64 =
      sqlx::query_scalar("SELECT COUNT(*) FROM set_slots WHERE set_id = $1")
        .bind(set_id)
        .fetch_one(pool)
        .await?;
    if slot_count == 0 {
      for (order, (label, variant_id)) in
        [("Top", top.variant_id), ("Calcinha", bottom.variant_id)]
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
) -> Result<(), SeedError> {
  let count = env_num("SEED_PRODUCT_COUNT", 12)?;
  let price_min = env_num("SEED_PRICE_MIN", 6990)?;
  let price_max = env_num("SEED_PRICE_MAX", 29990)?;
  let stock_min = env_num("SEED_STOCK_MIN", 5)?;
  let stock_max = env_num("SEED_STOCK_MAX", 30)?;
  let mut rng = rand::thread_rng();
  let mut biquini_top_ids = Vec::new();
  let mut biquini_bottom_ids = Vec::new();

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

    sqlx::query(
            "INSERT INTO products (id, name, slug, description, price_in_cents, compare_at_price_in_cents, category, active, featured, created_at, updated_at)
             VALUES ($1, $2, $3, NULL, $4, $5, $6::product_category, true, $7, now(), now())",
        )
        .bind(id)
        .bind(name)
        .bind(&slug)
        .bind(price)
        .bind(compare_at_price)
        .bind(match category {
          ProductCategory::Biquini => "biquini",
          ProductCategory::BiquiniTop => "biquini_top",
          ProductCategory::BiquiniBottom => "biquini_bottom",
          ProductCategory::Maio => "maio",
          ProductCategory::SaidaPraia => "saida_praia",
          ProductCategory::Acessorio => "acessorio",
        })
        .bind(featured)
        .execute(pool)
        .await?;

    // Track biquini_top and biquini_bottom IDs for linking
    match category {
      ProductCategory::BiquiniTop => biquini_top_ids.push(id),
      ProductCategory::BiquiniBottom => biquini_bottom_ids.push(id),
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

  // Link biquini_top and biquini_bottom products (both directions)
  for top_id in &biquini_top_ids {
    for calcinha_id in &biquini_bottom_ids {
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
  seed_products(pool, warehouse_id).await?;
  seed_sets(pool).await?;
  Ok(())
}
