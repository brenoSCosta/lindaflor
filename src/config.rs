use std::env;

#[derive(Clone, Debug)]
pub struct S3Config {
  pub endpoint: String,
  pub region: String,
  pub access_key_id: String,
  pub secret_access_key: String,
  pub bucket: String,
}

#[derive(Clone, Debug)]
pub struct Config {
  #[allow(dead_code)]
  pub port: u16,
  pub database_url: String,
  pub valkey_url: String,
  pub s3: Option<S3Config>,
}

impl Config {
  pub fn from_env() -> Result<Self, ConfigError> {
    dotenvy::dotenv().ok();
    let port = env::var("PORT")
      .unwrap_or_else(|_| "4200".to_string())
      .parse()
      .map_err(|_| ConfigError::InvalidPort)?;
    let database_url = env::var("DATABASE_URL")
      .map_err(|_| ConfigError::MissingVar("DATABASE_URL".to_string()))?;
    let valkey_url = env::var("VALKEY_URL")
      .unwrap_or_else(|_| "redis://127.0.0.1:4202".to_string());
    Ok(Self {
      port,
      database_url,
      valkey_url,
      s3: s3_from_env(),
    })
  }
}

fn env_non_empty(key: &str) -> Option<String> {
  env::var(key).ok().filter(|value| !value.is_empty())
}

fn s3_from_env() -> Option<S3Config> {
  let endpoint = env_non_empty("S3_ENDPOINT")?;
  let access_key_id = env_non_empty("S3_ACCESS_KEY_ID")?;
  let secret_access_key = env_non_empty("S3_SECRET_ACCESS_KEY")?;
  Some(S3Config {
    endpoint,
    region: env_non_empty("S3_REGION")
      .unwrap_or_else(|| "us-east-1".to_string()),
    access_key_id,
    secret_access_key,
    bucket: env_non_empty("S3_BUCKET")
      .unwrap_or_else(|| "lindaflor".to_string()),
  })
}

/// Current deployment environment from `APP_ENV` (defaults to `production`).
pub fn app_env() -> String {
  env::var("APP_ENV")
    .unwrap_or_else(|_| "production".to_string())
    .trim()
    .to_ascii_lowercase()
}

/// Whether OpenAPI UI/spec routes should respond (vs 404).
/// Enabled only when `APP_ENV` is `development` or `dev`.
pub fn openapi_docs_enabled() -> bool {
  matches!(app_env().as_str(), "development" | "dev")
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
  #[error("invalid PORT value")]
  InvalidPort,
  #[error("missing environment variable: {0}")]
  MissingVar(String),
}
