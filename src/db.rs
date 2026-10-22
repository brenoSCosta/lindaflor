use std::time::Duration;

use sqlx::migrate::{AppliedMigration, Migrate, Migrator};
use sqlx::postgres::{PgPool, PgPoolOptions};

use crate::config;

/// Build a Postgres pool from env (`PG_MAX_CONN`, acquire/idle timeouts).
///
/// Schema changes are **not** applied by the web process in production.
/// Set `MIGRATE_ON_BOOT=1` in development (the default outside
/// production) or run `scripts/migrate.sh` / `sqlx migrate run` as a
/// separate migrate job. When boot-migrate is off, pending (or missing)
/// migrations abort startup so replicas cannot race `sqlx::migrate!().run()`.
pub async fn create_pool(database_url: &str) -> Result<PgPool, DbError> {
  let pool = PgPoolOptions::new()
    .max_connections(config::pg_max_connections())
    .acquire_timeout(Duration::from_secs(config::pg_acquire_timeout_secs()))
    .idle_timeout(Duration::from_secs(config::pg_idle_timeout_secs()))
    .connect(database_url)
    .await?;

  let migrator = sqlx::migrate!("./migrations");
  if config::migrate_on_boot() {
    migrator.run(&pool).await?;
  } else {
    require_schema_current(&pool, &migrator).await?;
  }
  Ok(pool)
}

async fn require_schema_current(
  pool: &PgPool,
  migrator: &Migrator,
) -> Result<(), DbError> {
  let mut conn = pool.acquire().await?;
  let applied = match conn.list_applied_migrations().await {
    Ok(applied) => applied,
    Err(err) if is_undefined_table(&err) => {
      return Err(DbError::PendingMigrations {
        versions: pending_up_versions(migrator, &[]),
      });
    }
    Err(err) => return Err(err.into()),
  };

  if let Some(version) = conn.dirty_version().await? {
    return Err(sqlx::migrate::MigrateError::Dirty(version).into());
  }

  for row in &applied {
    match migrator
      .iter()
      .find(|m| m.version == row.version && m.migration_type.is_up_migration())
    {
      None => {
        return Err(
          sqlx::migrate::MigrateError::VersionMissing(row.version).into(),
        );
      }
      Some(migration)
        if migration.checksum.as_ref() != row.checksum.as_ref() =>
      {
        return Err(
          sqlx::migrate::MigrateError::VersionMismatch(row.version).into(),
        );
      }
      Some(_) => {}
    }
  }

  let versions = pending_up_versions(migrator, &applied);
  if !versions.is_empty() {
    return Err(DbError::PendingMigrations { versions });
  }
  Ok(())
}

fn pending_up_versions(
  migrator: &Migrator,
  applied: &[AppliedMigration],
) -> Vec<i64> {
  migrator
    .iter()
    .filter(|m| m.migration_type.is_up_migration())
    .filter(|m| !applied.iter().any(|row| row.version == m.version))
    .map(|m| m.version)
    .collect()
}

fn is_undefined_table(err: &sqlx::migrate::MigrateError) -> bool {
  match err {
    sqlx::migrate::MigrateError::Execute(sqlx::Error::Database(db)) => {
      db.code().as_deref() == Some("42P01")
    }
    _ => false,
  }
}

#[derive(Debug, thiserror::Error)]
pub enum DbError {
  #[error(transparent)]
  Sqlx(#[from] sqlx::Error),
  #[error(transparent)]
  Migrate(#[from] sqlx::migrate::MigrateError),
  #[error(
    "pending migrations {versions:?}; run the migrate job (`scripts/migrate.sh` or `sqlx migrate run`) before starting the web process"
  )]
  PendingMigrations { versions: Vec<i64> },
}

/// Escape `%`, `_` and `\` so user input matches literally inside a
/// `LIKE`/`ILIKE` pattern. Every caller pairs it with an explicit
/// `ESCAPE '\'` in SQL so the escape character never depends on server
/// defaults.
pub fn escape_like(input: &str) -> String {
  let mut out = String::with_capacity(input.len());
  for c in input.chars() {
    if matches!(c, '%' | '_' | '\\') {
      out.push('\\');
    }
    out.push(c);
  }
  out
}

/// `%<escaped>%` substring pattern for `ILIKE $n ESCAPE '\'`.
pub fn like_contains(input: &str) -> String {
  format!("%{}%", escape_like(input))
}

#[cfg(test)]
mod tests {
  use super::*;
  use std::time::Duration;

  static ENV_LOCK: tokio::sync::Mutex<()> = tokio::sync::Mutex::const_new(());

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

  #[test]
  fn embedded_migrations_are_forward_only() {
    let migrator = sqlx::migrate!("./migrations");
    let downs: Vec<_> = migrator
      .iter()
      .filter(|m| m.migration_type.is_down_migration())
      .map(|m| m.version)
      .collect();
    assert!(
      downs.is_empty(),
      "down-migrations are forbidden (forward-fix + PITR): {downs:?}"
    );
    assert!(
      migrator.iter().any(|m| m.migration_type.is_up_migration()),
      "expected at least one up migration"
    );
  }

  #[tokio::test]
  async fn create_pool_uses_env_pool_settings() {
    let _lock = ENV_LOCK.lock().await;
    let url = crate::test_support::empty_database_url().await;
    let _migrate = set_env("MIGRATE_ON_BOOT", "1");
    let _max = set_env("PG_MAX_CONN", "7");
    let _acquire = set_env("PG_ACQUIRE_TIMEOUT_SECS", "8");
    let _idle = set_env("PG_IDLE_TIMEOUT_SECS", "90");
    let pool = create_pool(&url).await.expect("create_pool");
    assert_eq!(pool.options().get_max_connections(), 7);
    assert_eq!(pool.options().get_acquire_timeout(), Duration::from_secs(8));
    assert_eq!(
      pool.options().get_idle_timeout(),
      Some(Duration::from_secs(90))
    );
    pool.close().await;
  }

  #[tokio::test]
  async fn create_pool_aborts_when_migrations_pending() {
    let _lock = ENV_LOCK.lock().await;
    let url = crate::test_support::empty_database_url().await;
    let _migrate = set_env("MIGRATE_ON_BOOT", "0");
    let err = create_pool(&url).await.expect_err("pending must abort");
    match err {
      DbError::PendingMigrations { versions } => {
        assert!(!versions.is_empty(), "expected pending versions");
      }
      other => panic!("unexpected error: {other}"),
    }
  }

  #[tokio::test]
  async fn create_pool_accepts_already_migrated_without_boot_migrate() {
    let _lock = ENV_LOCK.lock().await;
    let url = crate::test_support::empty_database_url().await;
    let _on = set_env("MIGRATE_ON_BOOT", "1");
    let pool = create_pool(&url).await.expect("migrate on boot");
    pool.close().await;
    drop(_on);
    let _off = set_env("MIGRATE_ON_BOOT", "0");
    create_pool(&url)
      .await
      .expect("current schema must boot")
      .close()
      .await;
  }

  #[test]
  fn pending_up_versions_skips_applied() {
    let migrator = sqlx::migrate!("./migrations");
    let all = pending_up_versions(&migrator, &[]);
    assert!(!all.is_empty());
    let first = all[0];
    let applied = [AppliedMigration {
      version: first,
      checksum: std::borrow::Cow::Borrowed(&[]),
    }];
    let rest = pending_up_versions(&migrator, &applied);
    assert!(!rest.contains(&first));
    assert_eq!(rest.len(), all.len() - 1);
  }

  #[test]
  fn like_helpers_escape_wildcards() {
    assert_eq!(escape_like("plain"), "plain");
    assert_eq!(escape_like("50%_off\\x"), "50\\%\\_off\\\\x");
    assert_eq!(like_contains("a_b"), "%a\\_b%");
    assert_eq!(like_contains(""), "%%");
  }
}
