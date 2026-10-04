use sqlx::PgPool;
use uuid::Uuid;

use lindaflor::auth::{CREDENTIAL_PROVIDER_ID, hash_password};

use super::SeedError;

pub async fn seed(pool: &PgPool) -> Result<(), SeedError> {
  let email = std::env::var("SEED_ADMIN_EMAIL")
    .unwrap_or_else(|_| "admin@lindaflor.com".to_string());
  let password = std::env::var("SEED_ADMIN_PASSWORD")
    .unwrap_or_else(|_| "admin123".to_string());
  let name = std::env::var("SEED_ADMIN_NAME")
    .unwrap_or_else(|_| "Admin User".to_string());

  let existing = sqlx::query!("SELECT id FROM users WHERE email = $1", email)
    .fetch_optional(pool)
    .await?;

  if existing.is_some() {
    tracing::info!("Admin user already exists: {}", email);
    return Ok(());
  }

  let user_id = Uuid::now_v7();
  let account_id = Uuid::now_v7();
  let password_hash =
    hash_password(&password).map_err(|e| SeedError::Env(e.to_string()))?;

  sqlx::query!(
        "INSERT INTO users (id, name, email, email_verified, role, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, now(), now())",
        user_id,
        name,
        email,
        true,
        "admin",
    )
    .execute(pool)
    .await?;

  // Use provider_id "credential" (singular) for email/password.
  sqlx::query!(
        "INSERT INTO accounts (id, account_id, provider_id, user_id, password, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, now(), now())",
        account_id,
        user_id.to_string(),
        CREDENTIAL_PROVIDER_ID,
        user_id,
        password_hash,
    )
    .execute(pool)
    .await?;

  tracing::info!("Admin user created: {}", email);
  Ok(())
}
