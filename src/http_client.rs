use std::sync::OnceLock;
use std::time::Duration;

static HTTP: OnceLock<reqwest::Client> = OnceLock::new();

/// Shared outbound HTTP client (10s timeout, connection pool).
pub fn shared_http_client() -> &'static reqwest::Client {
  HTTP.get_or_init(|| {
    reqwest::Client::builder()
      .timeout(Duration::from_secs(10))
      .pool_max_idle_per_host(8)
      .build()
      .expect("reqwest client")
  })
}
