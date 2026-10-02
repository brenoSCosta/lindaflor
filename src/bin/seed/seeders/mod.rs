pub mod admin;
pub mod commerce;

use sqlx::PgPool;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum SeedError {
  #[error("database error: {0}")]
  Database(#[from] sqlx::Error),
  #[error("environment error: {0}")]
  Env(String),
}

pub async fn run(pool: &PgPool) -> Result<(), SeedError> {
  admin::seed(pool).await?;
  commerce::seed(pool).await?;
  tracing::info!("Seed completed.");
  Ok(())
}
