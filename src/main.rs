mod app;
pub mod components;
mod config;
mod db;
mod openapi;
mod theme;
mod valkey;

#[cfg(test)]
mod test_support;

// Link library API + auth modules (routes register via inventory discover).
use lindaflor::api;
use lindaflor::auth;
use lindaflor::auth::routes;

use tokio::net::TcpListener;
use tracing_subscriber::EnvFilter;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
  tracing_subscriber::fmt()
    .with_env_filter(EnvFilter::from_default_env())
    .init();

  // Ensure route handlers are linked into the binary for discover().
  let _ = auth::CREDENTIAL_PROVIDER_ID;
  routes::link_for_discover();
  let _ = api::health;

  let cfg = crate::config::Config::from_env()?;
  let pool = crate::db::create_pool(&cfg.database_url).await?;
  let valkey_conn = crate::valkey::create_client(&cfg.valkey_url).await?;

  let storage = match cfg.s3.clone() {
    Some(s3) => {
      // `src/config.rs` is compiled into the binary and the library, so these
      // are distinct types. Copy the fields into the library config.
      let s3 = lindaflor::config::S3Config {
        endpoint: s3.endpoint,
        region: s3.region,
        access_key_id: s3.access_key_id,
        secret_access_key: s3.secret_access_key,
        bucket: s3.bucket,
      };
      let storage = lindaflor::storage::ObjectStore::s3(&s3);
      if let Err(err) = storage.ensure_bucket().await {
        tracing::warn!(error = %err, "object storage bucket is not ready");
      }
      storage
    }
    None => lindaflor::storage::ObjectStore::unavailable(),
  };

  let host = std::env::var("HOST").unwrap_or_else(|_| "127.0.0.1".to_string());
  let listener = TcpListener::bind((host.as_str(), cfg.port)).await?;
  tracing::info!("listening on http://{}", listener.local_addr()?);

  topcoat::serve(listener, crate::app::router(pool, valkey_conn, storage))
    .await?;

  Ok(())
}
