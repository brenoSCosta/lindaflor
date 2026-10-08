use std::sync::Arc;
use std::time::Duration;

use sqlx::PgPool;
use sqlx::postgres::PgPoolOptions;
use tokio::sync::OnceCell;
use uuid::Uuid;

#[path = "../crates/test-support/src/embedded_postgres.rs"]
mod embedded_postgres;

struct TestDb {
  pool: PgPool,
  database_url: String,
  /// Kept alive for the process lifetime so the server is not shut down.
  _postgres: embedded_postgres::RunningPostgres,
}

static DB: OnceCell<Arc<TestDb>> = OnceCell::const_new();

/// One pool per process, against a database migrated from `./migrations`.
pub async fn pool() -> PgPool {
  DB.get_or_init(|| async {
    let postgres = embedded_postgres::start().await;
    let database_name = format!("lindaflor_test_{}", Uuid::now_v7().simple());
    postgres
      .server()
      .create_database(&database_name)
      .await
      .expect("create database");
    let database_url = postgres.server().settings().url(&database_name);

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
      database_url,
      _postgres: postgres,
    })
  })
  .await
  .pool
  .clone()
}

/// Pool bound to the current Tokio runtime. Use in `#[tokio::test]` when the
/// shared [`pool`] connections may have been created on a previous runtime.
pub async fn fresh_pool() -> PgPool {
  let _ = pool().await;
  let url = DB.get().expect("test db").database_url.clone();
  PgPoolOptions::new()
    .max_connections(3)
    .acquire_timeout(Duration::from_secs(30))
    .connect(&url)
    .await
    .expect("connect fresh pool")
}

#[cfg(test)]
mod tests {
  use std::process::{Command, Stdio};
  use std::time::{Duration, Instant};

  #[test]
  fn embedded_postgres_dies_when_parent_is_killed() {
    if std::env::var_os("LINDAFLOR_PG_HOLD").is_some() {
      let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .expect("runtime");
      runtime.block_on(async {
        let running = super::embedded_postgres::start().await;
        let ready =
          std::env::var("LINDAFLOR_PG_READY_FILE").expect("ready file");
        std::fs::write(
          ready,
          running.server().settings().data_dir.display().to_string(),
        )
        .expect("write ready");
        std::thread::sleep(Duration::from_secs(120));
      });
      return;
    }

    let ready = std::env::temp_dir()
      .join(format!("lindaflor-pg-ready-{}", std::process::id()));
    let stderr_path = std::env::temp_dir()
      .join(format!("lindaflor-pg-hold-{}-err", std::process::id()));
    let _ = std::fs::remove_file(&ready);
    let stderr = std::fs::File::create(&stderr_path).expect("stderr file");
    let mut child = Command::new(std::env::current_exe().expect("current exe"))
      .arg("--exact")
      .arg("test_support::tests::embedded_postgres_dies_when_parent_is_killed")
      .env("LINDAFLOR_PG_HOLD", "1")
      .env("LINDAFLOR_PG_READY_FILE", &ready)
      .stdout(Stdio::null())
      .stderr(Stdio::from(stderr))
      .spawn()
      .expect("spawn holder");

    let deadline = Instant::now() + Duration::from_secs(90);
    while !ready.exists() {
      if child.try_wait().ok().flatten().is_some() || Instant::now() >= deadline
      {
        let err = std::fs::read_to_string(&stderr_path).unwrap_or_default();
        let _ = child.kill();
        panic!("embedded postgres did not become ready\n{err}");
      }
      std::thread::sleep(Duration::from_millis(50));
    }

    let data_dir = std::fs::read_to_string(&ready).expect("read ready");
    let socket =
      std::env::temp_dir().join(format!("lindaflor-pg-sock-{}", child.id()));
    child.kill().expect("kill holder");
    let _ = child.wait();

    let deadline = Instant::now() + Duration::from_secs(5);
    while Instant::now() < deadline && postgres_still_using(&data_dir, &socket)
    {
      std::thread::sleep(Duration::from_millis(50));
    }
    let leaked = postgres_still_using(&data_dir, &socket);
    let _ = std::fs::remove_file(&ready);
    let _ = std::fs::remove_file(&stderr_path);
    assert!(!leaked, "postgres survived the test process");
  }

  fn postgres_still_using(data_dir: &str, socket: &std::path::Path) -> bool {
    let data_dir = data_dir.trim();
    let socket = socket.to_string_lossy();
    let Ok(proc) = std::fs::read_dir("/proc") else {
      return false;
    };
    for entry in proc.flatten() {
      let pid = entry.file_name();
      let pid = pid.to_string_lossy();
      if pid.parse::<u32>().is_err() {
        continue;
      }
      let cmdline =
        std::fs::read(format!("/proc/{pid}/cmdline")).unwrap_or_default();
      let cmdline = String::from_utf8_lossy(&cmdline);
      if cmdline.contains(data_dir) || cmdline.contains(socket.as_ref()) {
        return true;
      }
      if std::fs::read_link(format!("/proc/{pid}/cwd"))
        .is_ok_and(|cwd| cwd == std::path::Path::new(data_dir))
      {
        return true;
      }
    }
    false
  }
}
