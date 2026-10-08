//! Test Postgres that dies with the test process.
//!
//! `postgresql_embedded`'s `start()` goes through `pg_ctl`, which daemonizes
//! the server. The test harness then leaves via `process::exit` or a signal,
//! so `Drop` never runs `pg_ctl stop` and every run orphans a cluster.
//!
//! This starts `postgres` in the foreground from a thread that lives as long
//! as the server should. That thread sets the parent-death signal to SIGTERM
//! before exec, which the kernel preserves. When the test process is killed,
//! the postmaster is asked to shut down and takes its workers with it.

use std::fs::{self, File};
use std::net::TcpListener;
use std::path::{Path, PathBuf};
use std::process::{Child, Command, Stdio};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};
use std::thread::JoinHandle;
use std::time::{Duration, Instant};

use postgresql_embedded::{PostgreSQL, Settings, SettingsBuilder};
use std::os::unix::net::UnixStream;
use std::os::unix::process::CommandExt;

const PARENT_DEATH_SIGNAL: i32 = 15;

unsafe extern "C" {
  fn prctl(option: i32, arg2: u64) -> i32;
  fn getppid() -> i32;
  fn kill(pid: i32, sig: i32) -> i32;
  fn _exit(code: i32) -> !;
}

pub struct RunningPostgres {
  stop: Arc<AtomicBool>,
  thread: Mutex<Option<JoinHandle<()>>>,
  server: PostgreSQL,
}

impl RunningPostgres {
  pub fn server(&self) -> &PostgreSQL {
    &self.server
  }
}

impl Drop for RunningPostgres {
  fn drop(&mut self) {
    self.stop.store(true, Ordering::SeqCst);
    if let Some(thread) =
      self.thread.lock().ok().and_then(|mut slot| slot.take())
    {
      let _ = thread.join();
    }
  }
}

pub async fn start() -> RunningPostgres {
  reap_orphaned_test_servers();

  let settings = build_settings();
  let mut server = PostgreSQL::new(settings);
  server.setup().await.expect("postgres setup");

  if let Some(socket_dir) = server.settings().socket_dir.clone() {
    let _ = fs::write(
      socket_dir.join("datadir"),
      server.settings().data_dir.display().to_string(),
    );
  }

  let (child_ready, ready_rx) = std::sync::mpsc::channel();
  let stop = Arc::new(AtomicBool::new(false));
  let stop_thread = Arc::clone(&stop);
  let data_dir = server.settings().data_dir.clone();
  let port = server.settings().port;
  let socket_dir = server.settings().socket_dir.clone();
  let binary = postgres_binary(server.settings());
  let log_path = data_dir.join("start.log");
  let thread_log = log_path.clone();

  let thread = std::thread::spawn(move || {
    let mut child = match spawn_postgres(
      &binary,
      &data_dir,
      port,
      socket_dir.as_deref(),
      &thread_log,
    ) {
      Ok(child) => child,
      Err(error) => {
        let _ = child_ready.send(Err(error));
        return;
      }
    };
    if child_ready.send(Ok(())).is_err() {
      let _ = signal(child.id(), PARENT_DEATH_SIGNAL);
      let _ = child.wait();
      return;
    }
    supervise(&mut child, &stop_thread);
  });

  ready_rx
    .recv()
    .expect("postgres thread")
    .expect("postgres spawn");
  wait_until_ready(server.settings(), &log_path).await;

  RunningPostgres {
    stop,
    thread: Mutex::new(Some(thread)),
    server,
  }
}

fn build_settings() -> Settings {
  let port = free_port();
  let mut builder = SettingsBuilder::new()
    .port(port)
    .config("max_connections", "200")
    .timeout(Some(Duration::from_secs(30)));

  if let Some(installation_dir) = pg_installation_dir_from_path() {
    let socket_dir = std::env::temp_dir()
      .join(format!("lindaflor-pg-sock-{}", std::process::id()));
    let _ = fs::create_dir_all(&socket_dir);
    builder = builder
      .installation_dir(installation_dir)
      .trust_installation_dir(true)
      .socket_dir(socket_dir);
  }

  builder.build()
}

fn pg_installation_dir_from_path() -> Option<PathBuf> {
  let path = std::env::var_os("PATH")?;
  for dir in std::env::split_paths(&path) {
    if dir.join("postgres").is_file() && dir.join("initdb").is_file() {
      return dir.parent().map(PathBuf::from);
    }
  }
  None
}

fn free_port() -> u16 {
  let listener =
    TcpListener::bind(("127.0.0.1", 0)).expect("bind ephemeral port");
  listener.local_addr().expect("ephemeral port").port()
}

fn postgres_binary(settings: &Settings) -> PathBuf {
  let candidate = settings.installation_dir.join("bin").join("postgres");
  if candidate.is_file() {
    candidate
  } else {
    PathBuf::from("postgres")
  }
}

fn spawn_postgres(
  binary: &Path,
  data_dir: &Path,
  port: u16,
  socket_dir: Option<&Path>,
  log_path: &Path,
) -> std::io::Result<Child> {
  let log = File::create(log_path)?;
  let mut command = Command::new(binary);
  command
    .arg("-D")
    .arg(data_dir)
    .arg("-F")
    .arg("-p")
    .arg(port.to_string())
    .env("PGDATABASE", "")
    .stdin(Stdio::null())
    .stdout(Stdio::from(log.try_clone()?))
    .stderr(Stdio::from(log));
  if let Some(socket_dir) = socket_dir {
    command.arg("-k").arg(socket_dir);
  }
  unsafe {
    command.pre_exec(|| {
      arm_parent_death_signal();
      Ok(())
    });
  }
  command.spawn()
}

fn arm_parent_death_signal() {
  const PR_SET_PDEATHSIG: i32 = 1;
  unsafe {
    if prctl(PR_SET_PDEATHSIG, PARENT_DEATH_SIGNAL as u64) != 0
      || getppid() == 1
    {
      _exit(1);
    }
  }
}

fn supervise(child: &mut Child, stop: &AtomicBool) {
  loop {
    if stop.load(Ordering::SeqCst) {
      let _ = signal(child.id(), PARENT_DEATH_SIGNAL);
      let _ = child.wait();
      break;
    }
    match child.try_wait() {
      Ok(Some(_)) => break,
      Ok(None) => std::thread::sleep(Duration::from_millis(50)),
      Err(_) => break,
    }
  }
}

fn signal(pid: u32, sig: i32) -> std::io::Result<()> {
  let result = unsafe { kill(pid as i32, sig) };
  if result == 0 {
    Ok(())
  } else {
    Err(std::io::Error::last_os_error())
  }
}

async fn wait_until_ready(settings: &Settings, log_path: &Path) {
  let timeout = settings.timeout.unwrap_or(Duration::from_secs(30));
  let deadline = Instant::now() + timeout;
  let socket = settings
    .socket_dir
    .as_ref()
    .map(|dir| dir.join(format!(".s.PGSQL.{}", settings.port)));

  loop {
    if socket
      .as_ref()
      .is_some_and(|path| UnixStream::connect(path).is_ok())
    {
      return;
    }
    if Instant::now() >= deadline {
      let log = fs::read_to_string(log_path).unwrap_or_default();
      panic!("postgres did not accept connections\n{log}");
    }
    tokio::time::sleep(Duration::from_millis(50)).await;
  }
}

fn reap_orphaned_test_servers() {
  let temp = std::env::temp_dir();
  let entries = match fs::read_dir(&temp) {
    Ok(entries) => entries,
    Err(_) => return,
  };
  for entry in entries.flatten() {
    let name = entry.file_name();
    let Some(name) = name.to_str() else {
      continue;
    };
    let Some(pid) = name.strip_prefix("lindaflor-pg-sock-") else {
      continue;
    };
    let Ok(pid) = pid.parse::<u32>() else {
      continue;
    };
    if pid_alive(pid) {
      continue;
    }
    let socket_dir = entry.path();
    stop_servers_using(&socket_dir);
    if let Ok(data_dir) = fs::read(socket_dir.join("datadir")) {
      let data_dir = PathBuf::from(String::from_utf8_lossy(&data_dir).as_ref());
      if data_dir.starts_with(&temp) {
        let _ = fs::remove_dir_all(data_dir);
      }
    }
    let _ = fs::remove_dir_all(socket_dir);
  }
}

fn stop_servers_using(socket_dir: &Path) {
  let marker = socket_dir.to_string_lossy().into_owned();
  let mut pids = Vec::new();
  let Ok(proc) = fs::read_dir("/proc") else {
    return;
  };
  for entry in proc.flatten() {
    let Ok(pid) = entry.file_name().to_string_lossy().parse::<u32>() else {
      continue;
    };
    let Ok(cmdline) = fs::read(format!("/proc/{pid}/cmdline")) else {
      continue;
    };
    let cmdline = String::from_utf8_lossy(&cmdline);
    if cmdline.contains(&marker) && cmdline.contains(" -D ") {
      pids.push(pid);
    }
  }
  for pid in &pids {
    let _ = signal(*pid, PARENT_DEATH_SIGNAL);
  }
  let deadline = Instant::now() + Duration::from_secs(2);
  while Instant::now() < deadline && pids.iter().any(|pid| pid_alive(*pid)) {
    std::thread::sleep(Duration::from_millis(50));
  }
  for pid in pids {
    if pid_alive(pid) {
      let _ = signal(pid, 9);
    }
  }
}

fn pid_alive(pid: u32) -> bool {
  match signal(pid, 0) {
    Ok(()) => true,
    // ESRCH: nothing has this pid. Any other error means it exists.
    Err(error) => error.raw_os_error() != Some(3),
  }
}
