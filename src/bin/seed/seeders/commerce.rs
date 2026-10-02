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

const CATEGORIES: [ProductCategory; 4] = [
  ProductCategory::Biquini,
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

const COLORS: [&str; 12] = [
  "Verde Tropical",
  "Floral",
  "Azul Escandinavo",
  "Branco",
  "Preto",
  "Vermelho",
  "Rosa",
  "Amarelo",
  "Terracota",
  "Maré",
  "Areia",
  "Geométrico",
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

async fn seed_settings(pool: &PgPool) -> Result<(), SeedError> {
  let existing = sqlx::query!("SELECT id FROM store_settings LIMIT 1")
    .fetch_optional(pool)
    .await?;

  if existing.is_some() {
    return Ok(());
  }

  sqlx::query!(
        "INSERT INTO store_settings (id, pix_key, pix_key_type, pix_merchant_name, pix_merchant_city, whatsapp_number, whatsapp_message_template, created_at, updated_at)
         VALUES ($1, $2, 'phone', 'Linda Flor Moda Praia', 'Aracaju', $3, $4, now(), now())",
        Uuid::now_v7(),
        env_str("SEED_PIX_KEY", "79998165115"),
        env_str("SEED_WHATSAPP_NUMBER", "557998165115"),
        env_str(
            "SEED_WHATSAPP_MESSAGE_TEMPLATE",
            "Olá! Fiz o pedido {{order_id}} no valor de {{total}} e quero confirmar o pagamento via PIX.",
        ),
    )
    .execute(pool)
    .await?;

  tracing::info!("Store settings created");
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
    let featured = (i + 1) % 3 == 0;
    let collection_id = if collection_ids.is_empty() {
      None
    } else {
      Some(collection_ids[i as usize % collection_ids.len()])
    };

    sqlx::query!(
            "INSERT INTO products (id, name, slug, description, price_in_cents, category, collection_id, active, featured, created_at, updated_at)
             VALUES ($1, $2, $3, NULL, $4, $5, $6, true, $7, now(), now())",
            id,
            name,
            slug,
            price,
            category as _,
            collection_id,
            featured,
        )
        .execute(pool)
        .await?;

    let mut sizes = SIZES.to_vec();
    sizes.shuffle(&mut rng);
    let variant_count = rng.gen_range(1..=5);

    for size in sizes.iter().take(variant_count) {
      let color = COLORS.choose(&mut rng).copied().unwrap();
      let variant_id = Uuid::now_v7();
      let sku = format!("{}-{}", slug, size.as_str());

      sqlx::query!(
                "INSERT INTO product_variants (id, product_id, sku, size, color, price_in_cents, low_stock_threshold, created_at, updated_at)
                 VALUES ($1, $2, $3, $4, $5, NULL, 5, now(), now())",
                variant_id,
                id,
                sku,
                size as _,
                color,
            )
            .execute(pool)
            .await?;

      let quantity = rng.gen_range(stock_min..=stock_max);
      sqlx::query!(
                "INSERT INTO inventory (id, variant_id, warehouse_id, quantity, reserved, updated_at)
                 VALUES ($1, $2, $3, $4, 0, now())",
                Uuid::now_v7(),
                variant_id,
                warehouse_id,
                quantity,
            )
            .execute(pool)
            .await?;
    }

    sqlx::query!(
            "INSERT INTO product_images (id, product_id, url, alt, sort_order, created_at)
             VALUES ($1, $2, $3, $4, 0, now())",
            Uuid::now_v7(),
            id,
            format!("https://picsum.photos/seed/{}/1200/1200", slug),
            name,
        )
        .execute(pool)
        .await?;

    tracing::info!("Product created: {} ({})", name, slug);
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
