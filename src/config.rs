use std::env;

/// Read `KEY`, falling back to Docker-secret style `KEY_FILE` (a path whose
/// contents are the value, trailing newlines trimmed). Direct env wins; empty
/// values are treated as unset.
fn read_env_or_file(key: &str) -> Option<String> {
  if let Ok(value) = env::var(key)
    && !value.is_empty()
  {
    return Some(value);
  }
  let file_key = format!("{key}_FILE");
  let path = env::var(&file_key).ok().filter(|p| !p.is_empty())?;
  std::fs::read_to_string(&path)
    .ok()
    .map(|contents| contents.trim_end_matches(['\n', '\r']).to_string())
    .filter(|value| !value.is_empty())
}

#[derive(Clone)]
pub struct S3Config {
  pub endpoint: String,
  pub region: String,
  pub access_key_id: String,
  pub secret_access_key: String,
  pub bucket: String,
}

// Manual `Debug` so a `{:?}` log/panic never leaks the secret key.
impl std::fmt::Debug for S3Config {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("S3Config")
      .field("endpoint", &self.endpoint)
      .field("region", &self.region)
      .field("access_key_id", &"***")
      .field("secret_access_key", &"***")
      .field("bucket", &self.bucket)
      .finish()
  }
}

#[derive(Clone)]
pub struct Config {
  pub port: u16,
  pub database_url: String,
  pub valkey_url: String,
  pub s3: Option<S3Config>,
}

// Manual `Debug` so connection strings (which embed passwords) are
// never emitted by `{:?}` logs/panics.
impl std::fmt::Debug for Config {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("Config")
      .field("port", &self.port)
      .field("database_url", &"***")
      .field("valkey_url", &"***")
      .field("s3", &self.s3)
      .finish()
  }
}

impl Config {
  pub fn from_env() -> Result<Self, ConfigError> {
    // Never auto-load a `.env` file in production images — env (or
    // mounted `*_FILE` secrets) is the only source there.
    if app_env() != "production" {
      dotenvy::dotenv().ok();
    }
    let port = read_env_or_file("PORT")
      .unwrap_or_else(|| "4200".to_string())
      .parse()
      .map_err(|_| ConfigError::InvalidPort)?;
    let database_url = read_env_or_file("DATABASE_URL")
      .ok_or_else(|| ConfigError::MissingVar("DATABASE_URL".to_string()))?;
    let valkey_url = read_env_or_file("VALKEY_URL")
      .unwrap_or_else(|| "redis://127.0.0.1:4202".to_string());
    // Fail boot in production without a valid public origin. The
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
  read_env_or_file(key)
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
  read_env_or_file(key)
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

/// Postgres pool size from `PG_MAX_CONN`. Explicit value wins;
/// otherwise 20 in production, 5 elsewhere.
pub fn pg_max_connections() -> u32 {
  if let Some(value) = read_env_or_file("PG_MAX_CONN")
    && let Ok(parsed) = value.parse::<u32>()
    && parsed > 0
  {
    return parsed;
  }
  if app_env() == "production" { 20 } else { 5 }
}

/// Pool acquire timeout in seconds from `PG_ACQUIRE_TIMEOUT_SECS`.
pub fn pg_acquire_timeout_secs() -> u64 {
  read_env_or_file("PG_ACQUIRE_TIMEOUT_SECS")
    .and_then(|value| value.parse::<u64>().ok())
    .filter(|value| *value > 0)
    .unwrap_or(5)
}

/// Pool idle timeout in seconds from `PG_IDLE_TIMEOUT_SECS`.
pub fn pg_idle_timeout_secs() -> u64 {
  read_env_or_file("PG_IDLE_TIMEOUT_SECS")
    .and_then(|value| value.parse::<u64>().ok())
    .filter(|value| *value > 0)
    .unwrap_or(600)
}

/// Whether the web/seed boot applies pending migrations itself.
/// Explicit `MIGRATE_ON_BOOT=1/0` wins; otherwise on outside production
/// (dev convenience) and off in production, where the migrate job
/// (`sqlx migrate run` pre-deploy) owns the schema.
pub fn migrate_on_boot() -> bool {
  if let Some(value) = read_env_or_file("MIGRATE_ON_BOOT") {
    let normalized = value.trim().to_ascii_lowercase();
    return matches!(normalized.as_str(), "1" | "true" | "yes" | "on");
  }
  app_env() != "production"
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

/// Require `APP_ORIGIN`: the public origin used for `base_url` and
/// `OriginPolicy`. Callers in non-production paths should keep using the
/// optional [`crate::auth::google::app_origin`] so dev/test stay unset-friendly.
pub fn require_app_origin() -> Result<String, ConfigError> {
  match read_env_or_file("APP_ORIGIN") {
    Some(raw) => normalize_app_origin(&raw),
    None => Err(ConfigError::MissingAppOrigin),
  }
}

/// Passwords that must never seed/Admin-create in production.
const WEAK_SEED_PASSWORDS: &[&str] = &["", "admin123", "password", "changeme"];

/// Fail closed when the seed/admin-create path would use a weak default in
/// production. The web server does not call this (it never reads `SEED_*`);
/// the seed binary calls it before inserting anything.
pub fn reject_weak_seed_password_in_production(
  password: &str,
) -> Result<(), ConfigError> {
  if app_env() != "production" {
    return Ok(());
  }
  let weak =
    password.len() < 12 || WEAK_SEED_PASSWORDS.contains(&password.trim());
  if weak {
    return Err(ConfigError::WeakSeedPassword);
  }
  Ok(())
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
  #[error(
    "refusing to seed with a weak SEED_ADMIN_PASSWORD in production: set a strong (12+ char) password or generate one per deploy"
  )]
  WeakSeedPassword,
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

  #[test]
  fn debug_redacts_secrets() {
    let cfg = Config {
      port: 4200,
      database_url: "postgres://user:s3cret@localhost/db".to_string(),
      valkey_url: "redis://:s3cret@localhost:6379".to_string(),
      s3: Some(S3Config {
        endpoint: "http://localhost:9000".to_string(),
        region: "us-east-1".to_string(),
        access_key_id: "AKIAEXAMPLE".to_string(),
        secret_access_key: "supersecret".to_string(),
        bucket: "lindaflor".to_string(),
      }),
    };
    let rendered = format!("{cfg:?}");
    assert!(!rendered.contains("s3cret"), "leaked secret: {rendered}");
    assert!(
      !rendered.contains("supersecret"),
      "leaked secret: {rendered}"
    );
    assert!(
      !rendered.contains("AKIAEXAMPLE"),
      "leaked key id: {rendered}"
    );
    assert!(rendered.contains("***"), "expected redaction marker");
    // Non-secret fields stay visible for debugging.
    assert!(rendered.contains("lindaflor"));
  }

  #[test]
  fn read_env_or_file_prefers_direct_env_over_file() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    let dir = std::env::temp_dir();
    let path =
      dir.join(format!("lindaflor-test-secret-{}", std::process::id()));
    std::fs::write(&path, "file-value\n").expect("write secret file");
    let _direct = set_env("LINDAFLOR_TEST_SECRET_XYZ", "direct-value");
    unsafe {
      std::env::set_var(
        "LINDAFLOR_TEST_SECRET_XYZ_FILE",
        path.to_str().expect("utf8"),
      );
    }
    let restore_file = EnvRestore {
      key: "LINDAFLOR_TEST_SECRET_XYZ_FILE",
      previous: None,
    };
    // Direct env wins.
    assert_eq!(
      read_env_or_file("LINDAFLOR_TEST_SECRET_XYZ").as_deref(),
      Some("direct-value")
    );
    drop(restore_file);
    unsafe { std::env::remove_var("LINDAFLOR_TEST_SECRET_XYZ_FILE") };
    let _ = std::fs::remove_file(&path);
  }

  #[test]
  fn read_env_or_file_reads_file_when_env_unset() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    let dir = std::env::temp_dir();
    let path =
      dir.join(format!("lindaflor-test-secret2-{}", std::process::id()));
    std::fs::write(&path, "file-value\n").expect("write secret file");
    let _unset = remove_env("LINDAFLOR_TEST_SECRET2_XYZ");
    unsafe {
      std::env::set_var(
        "LINDAFLOR_TEST_SECRET2_XYZ_FILE",
        path.to_str().expect("utf8"),
      );
    }
    let result = read_env_or_file("LINDAFLOR_TEST_SECRET2_XYZ");
    unsafe { std::env::remove_var("LINDAFLOR_TEST_SECRET2_XYZ_FILE") };
    let _ = std::fs::remove_file(&path);
    assert_eq!(result.as_deref(), Some("file-value"));
  }

  #[test]
  fn weak_seed_password_rejected_in_production_only() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    let _env = set_env("APP_ENV", "production");
    assert!(reject_weak_seed_password_in_production("admin123").is_err());
    assert!(reject_weak_seed_password_in_production("short").is_err());
    assert!(
      reject_weak_seed_password_in_production("a-strong-password-1").is_ok()
    );
    let _dev = set_env("APP_ENV", "development");
    assert!(reject_weak_seed_password_in_production("admin123").is_ok());
  }

  #[test]
  fn pg_max_connections_env_and_env_defaults() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    let _max = remove_env("PG_MAX_CONN");
    let _prod = set_env("APP_ENV", "production");
    assert_eq!(pg_max_connections(), 20);
    let _dev = set_env("APP_ENV", "development");
    assert_eq!(pg_max_connections(), 5);
    let _explicit = set_env("PG_MAX_CONN", "12");
    assert_eq!(pg_max_connections(), 12);
    drop(_explicit);
    let _zero = set_env("PG_MAX_CONN", "0");
    assert_eq!(pg_max_connections(), 5);
  }

  #[test]
  fn pg_timeouts_use_defaults_and_env() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    let _a = remove_env("PG_ACQUIRE_TIMEOUT_SECS");
    let _i = remove_env("PG_IDLE_TIMEOUT_SECS");
    assert_eq!(pg_acquire_timeout_secs(), 5);
    assert_eq!(pg_idle_timeout_secs(), 600);
    let _a2 = set_env("PG_ACQUIRE_TIMEOUT_SECS", "9");
    let _i2 = set_env("PG_IDLE_TIMEOUT_SECS", "30");
    assert_eq!(pg_acquire_timeout_secs(), 9);
    assert_eq!(pg_idle_timeout_secs(), 30);
  }

  #[test]
  fn migrate_on_boot_defaults_off_in_production() {
    let _lock = ENV_LOCK.lock().expect("env lock");
    let _flag = remove_env("MIGRATE_ON_BOOT");
    let _prod = set_env("APP_ENV", "production");
    assert!(!migrate_on_boot());
    let _dev = set_env("APP_ENV", "development");
    assert!(migrate_on_boot());
    let _off = set_env("MIGRATE_ON_BOOT", "0");
    assert!(!migrate_on_boot());
    let _on = set_env("MIGRATE_ON_BOOT", "true");
    assert!(migrate_on_boot());
  }
}
