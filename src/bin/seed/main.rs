mod seeders;

use lindaflor::config::Config;
use lindaflor::db;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  tracing_subscriber::fmt()
    .with_env_filter(tracing_subscriber::EnvFilter::from_default_env())
    .init();

  let cfg = Config::from_env()?;
  tracing::debug!(port = cfg.port, "seed loaded config");
  let pool = db::create_pool(&cfg.database_url).await?;

  seeders::run(&pool).await?;

  Ok(())
}
