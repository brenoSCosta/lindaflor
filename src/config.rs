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
    // §3.4: fail boot in production without a valid public origin. The
    // origin feeds `base_url` + `OriginPolicy` (see `src/app.rs`).
    if app_env() == "production" {
      require_app_origin()?;
    }
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

/// HMAC pepper for hashing one-time verification tokens (`verifications.value_hash`).
///
/// Read from `TOKEN_PEPPER`. Empty when unset: callers fall back to plain
/// SHA-256. Rotate by setting a new value — outstanding tokens are
/// invalidated (they are short-lived).
pub fn token_pepper() -> String {
  env_non_empty("TOKEN_PEPPER").unwrap_or_default()
}

fn env_parse<T: std::str::FromStr>(key: &str, default: T) -> T {
  env::var(key)
    .ok()
    .filter(|value| !value.is_empty())
    .and_then(|value| value.parse().ok())
    .unwrap_or(default)
}

/// Request-log sampling fraction from `LOG_SAMPLE_RATE` (default `1.0`,
/// clamped to `0.0..=1.0`). Empty or unset uses the default.
pub fn log_sample_rate() -> f64 {
  let rate: f64 = env_parse("LOG_SAMPLE_RATE", 1.0);
  rate.clamp(0.0, 1.0)
}

/// Slow-request threshold in milliseconds from `LOG_SLOW_THRESHOLD_MS`
/// (default `1000`). Empty or unset uses the default.
pub fn log_slow_threshold_ms() -> u64 {
  env_parse("LOG_SLOW_THRESHOLD_MS", 1000)
}

/// Optional file sink path from `LOG_FILE_PATH`. Empty or unset is `None`.
pub fn log_file_path() -> Option<String> {
  env_non_empty("LOG_FILE_PATH")
}

/// Rotating log file size cap from `LOG_FILE_MAX_BYTES` (default 10 MiB).
/// Empty or unset uses the default.
pub fn log_file_max_bytes() -> u64 {
  env_parse("LOG_FILE_MAX_BYTES", 10 * 1024 * 1024)
}

/// Rotating log file count from `LOG_FILE_MAX_FILES` (default `5`).
/// Empty or unset uses the default.
pub fn log_file_max_files() -> u32 {
  env_parse("LOG_FILE_MAX_FILES", 5)
}

/// Bearer token for `GET /metrics` from `METRICS_TOKEN`. Empty or unset
/// is `None` (open metrics in local/dev).
pub fn metrics_token() -> Option<String> {
  env_non_empty("METRICS_TOKEN")
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

/// Validate and normalize an `APP_ORIGIN` value: non-empty, `http(s)://`
/// plus a host, with any trailing `/` stripped.
pub fn normalize_app_origin(raw: &str) -> Result<String, ConfigError> {
  let trimmed = raw.trim();
  if trimmed.is_empty() {
    return Err(ConfigError::MissingAppOrigin);
  }
  let normalized = trimmed.trim_end_matches('/').to_string();
  let after_scheme = normalized
    .strip_prefix("https://")
    .or_else(|| normalized.strip_prefix("http://"));
  let valid = after_scheme.is_some_and(|rest| {
    !rest.is_empty()
      && !rest.contains(['/', '?', '#'])
      && !rest.chars().any(char::is_whitespace)
  });
  if !valid {
    return Err(ConfigError::InvalidAppOrigin(raw.to_string()));
  }
  Ok(normalized)
}

/// Require `APP_ORIGIN` (§3.4): the public origin used for `base_url` and
/// `OriginPolicy`. Callers in non-production paths should keep using the
/// optional [`crate::auth::google::app_origin`] so dev/test stay unset-friendly.
pub fn require_app_origin() -> Result<String, ConfigError> {
  match env::var("APP_ORIGIN") {
    Ok(raw) => normalize_app_origin(&raw),
    Err(_) => Err(ConfigError::MissingAppOrigin),
  }
}

#[derive(Debug, thiserror::Error)]
pub enum ConfigError {
  #[error("invalid PORT value")]
  InvalidPort,
  #[error("missing environment variable: {0}")]
  MissingVar(String),
  #[error(
    "missing APP_ORIGIN: set it to the public https origin in production"
  )]
  MissingAppOrigin,
  #[error("invalid APP_ORIGIN {0:?}: must be an http(s):// URL with a host")]
  InvalidAppOrigin(String),
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn normalize_app_origin_accepts_http_origins() {
    assert_eq!(
      normalize_app_origin("https://loja.example.com").unwrap(),
      "https://loja.example.com"
    );
    assert_eq!(
      normalize_app_origin("http://localhost:4200/").unwrap(),
      "http://localhost:4200"
    );
    assert_eq!(
      normalize_app_origin("  https://loja.example.com///  ").unwrap(),
      "https://loja.example.com"
    );
  }

  #[test]
  fn normalize_app_origin_rejects_bad_values() {
    for raw in [
      "",
      "   ",
      "loja.example.com",
      "ftp://loja.example.com",
      "https://",
      "https:///path",
      "https://example.com/app",
      "https://example.com?q=1",
      "https://exam ple.com",
    ] {
      assert!(
        normalize_app_origin(raw).is_err(),
        "expected error for {raw:?}"
      );
    }
    assert!(matches!(
      normalize_app_origin(""),
      Err(ConfigError::MissingAppOrigin)
    ));
    assert!(matches!(
      normalize_app_origin("notaurl"),
      Err(ConfigError::InvalidAppOrigin(_))
    ));
  }

  /// Save/restore env around each case: the lib test binary shares the
  /// process, so never leak `APP_ENV`/`APP_ORIGIN` changes. A mutex keeps
  /// these cases from racing each other under `cargo test`.
  static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

  struct EnvRestore {
    key: &'static str,
    previous: Option<String>,
  }

  impl Drop for EnvRestore {
    fn drop(&mut self) {
      match self.previous.take() {
        Some(value) => unsafe { std::env::set_var(self.key, value) },
        None => unsafe { std::env::remove_var(self.key) },
      }
    }
  }

  fn set_env(key: &'static str, value: &str) -> EnvRestore {
    let previous = std::env::var(key).ok();
    unsafe { std::env::set_var(key, value) };
    EnvRestore { key, previous }
  }

  fn remove_env(key: &'static str) -> EnvRestore {
    let previous = std::env::var(key).ok();
    unsafe { std::env::remove_var(key) };
    EnvRestore { key, previous }
  }

  #[test]
  fn from_env_fails_in_production_without_app_origin() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    let _env = set_env("APP_ENV", "production");
    let _origin = remove_env("APP_ORIGIN");
    let _db = set_env("DATABASE_URL", "postgres://localhost/test");
    let err = Config::from_env().expect_err("prod boot must fail");
    assert!(
      matches!(
        err,
        ConfigError::MissingAppOrigin | ConfigError::InvalidAppOrigin(_)
      ),
      "unexpected error: {err:?}"
    );
  }

  #[test]
  fn from_env_fails_in_production_with_invalid_app_origin() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    let _env = set_env("APP_ENV", "production");
    let _origin = set_env("APP_ORIGIN", "not-a-url");
    let _db = set_env("DATABASE_URL", "postgres://localhost/test");
    let err = Config::from_env().expect_err("prod boot must fail");
    assert!(
      matches!(err, ConfigError::InvalidAppOrigin(_)),
      "unexpected error: {err:?}"
    );
  }

  #[test]
  fn from_env_ok_in_dev_without_app_origin() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    let _env = set_env("APP_ENV", "development");
    let _origin = remove_env("APP_ORIGIN");
    let _db = set_env("DATABASE_URL", "postgres://localhost/test");
    Config::from_env().expect("dev boot must not require APP_ORIGIN");
  }

  #[test]
  fn from_env_ok_in_production_with_valid_app_origin() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    let _env = set_env("APP_ENV", "production");
    let _origin = set_env("APP_ORIGIN", "https://loja.example.com/");
    let _db = set_env("DATABASE_URL", "postgres://localhost/test");
    Config::from_env().expect("prod boot with valid APP_ORIGIN");
  }

  #[test]
  fn require_app_origin_rejects_empty() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    let _origin = set_env("APP_ORIGIN", "  ");
    assert!(matches!(
      require_app_origin(),
      Err(ConfigError::MissingAppOrigin)
    ));
  }
}
