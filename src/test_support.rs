use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use postgresql_embedded::{PostgreSQL, SettingsBuilder};
use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::OnceCell;
use uuid::Uuid;

struct TestDb {
  pool: PgPool,
  /// Kept alive for the process lifetime so the server is not shut down.
  _postgres: PostgreSQL,
}

static DB: OnceCell<Arc<TestDb>> = OnceCell::const_new();

/// Prefer Postgres from PATH (devenv/nix) so we avoid theseus glibc/ABI issues.
fn pg_installation_dir_from_path() -> Option<PathBuf> {
  let path = std::env::var_os("PATH")?;
  for dir in std::env::split_paths(&path) {
    if dir.join("postgres").is_file() && dir.join("initdb").is_file() {
      return dir.parent().map(PathBuf::from);
    }
  }
  None
}

/// One pool per process, against a database migrated from `./migrations`.
pub async fn pool() -> PgPool {
  DB.get_or_init(|| async {
    let settings =
      if let Some(installation_dir) = pg_installation_dir_from_path() {
        let socket_dir = std::env::temp_dir()
          .join(format!("lindaflor-pg-sock-{}", std::process::id()));
        let _ = std::fs::create_dir_all(&socket_dir);
        SettingsBuilder::new()
          .installation_dir(installation_dir)
          .trust_installation_dir(true)
          .socket_dir(socket_dir)
          .timeout(Some(Duration::from_secs(30)))
          .build()
      } else {
        SettingsBuilder::new()
          .timeout(Some(Duration::from_secs(30)))
          .build()
      };

    let mut postgres = PostgreSQL::new(settings);
    postgres.setup().await.expect("postgres setup");
    postgres.start().await.expect("postgres start");

    let database_name = format!("lindaflor_test_{}", Uuid::now_v7().simple());
    postgres
      .create_database(&database_name)
      .await
      .expect("create database");
    let database_url = postgres.settings().url(&database_name);

    let pool = PgPoolOptions::new()
      .max_connections(16)
      .acquire_timeout(Duration::from_secs(30))
      .connect(&database_url)
      .await
      .expect("connect pool");
    sqlx::migrate!("./migrations")
      .run(&pool)
      .await
      .expect("migrate");

    Arc::new(TestDb {
      pool,
      _postgres: postgres,
    })
  })
  .await
  .pool
  .clone()
}
