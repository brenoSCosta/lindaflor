// Link library API + auth modules (routes register via inventory discover).
use lindaflor::api;
use lindaflor::auth;
use lindaflor::auth::routes;
use lindaflor::openapi;

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
  let _ = api::readyz;
  let _ = lindaflor::logging::metrics_route::metrics;
  let _ = openapi::openapi_json;
  let _ = openapi::scalar_docs;
  let _ = openapi::swagger_docs;

  let cfg = lindaflor::config::Config::from_env()?;
  let pool = lindaflor::db::create_pool(&cfg.database_url).await?;
  let valkey_conn = lindaflor::valkey::create_client(&cfg.valkey_url).await?;

  let storage = match cfg.s3.clone() {
    Some(s3) => {
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

  // `topcoat::serve` already drains in-flight requests on Ctrl+C / SIGTERM
  // (30s shutdown timeout). Close the pool afterwards so in-flight queries
  // finish before the process exits.
  let pool_for_shutdown = pool.clone();
  let sweep_pool = pool.clone();
  let sweep_secs = lindaflor::config::reservation_sweep_secs();
  tokio::spawn(async move {
    let mut interval =
      tokio::time::interval(std::time::Duration::from_secs(sweep_secs));
    loop {
      interval.tick().await;
      if let Err(error) =
        lindaflor::app::store::order::expire_pending_reservations(&sweep_pool)
          .await
      {
        tracing::error!(error = %error, "reservation sweep failed");
      }
    }
  });
  topcoat::serve(listener, lindaflor::app::router(pool, valkey_conn, storage))
    .await?;
  pool_for_shutdown.close().await;

  Ok(())
}
