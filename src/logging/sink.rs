use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, OnceLock, PoisonError};

use super::log_entry::RequestLogEntry;

pub const DEFAULT_LOG_FILE_MAX_BYTES: u64 = 10 * 1024 * 1024;
pub const DEFAULT_LOG_FILE_MAX_FILES: u32 = 5;

pub type LogSink = Arc<dyn Fn(RequestLogEntry) + Send + Sync>;

const LOG_EXTENSION: &str = ".log";

type FailureScope = &'static str;

fn reported_scopes() -> &'static Mutex<Vec<FailureScope>> {
  static SCOPES: OnceLock<Mutex<Vec<FailureScope>>> = OnceLock::new();
  SCOPES.get_or_init(|| Mutex::new(Vec::new()))
}

fn warn_once(scope: FailureScope, err: &dyn std::fmt::Display) {
  let mut reported = reported_scopes()
    .lock()
    .unwrap_or_else(PoisonError::into_inner);
  if reported.contains(&scope) {
    return;
  }
  reported.push(scope);
  tracing::warn!(
    "[logging] {scope} sink failed; further failures suppressed: {err}"
  );
}

struct FileTee {
  path: PathBuf,
  max_bytes: u64,
  max_files: u32,
  disabled: bool,
}

fn archive_path(file_path: &Path, index: u32) -> PathBuf {
  let path_str = file_path.to_string_lossy();
  let stem = path_str.strip_suffix(LOG_EXTENSION).unwrap_or(&path_str);
  PathBuf::from(format!("{stem}.{index}{LOG_EXTENSION}"))
}

fn shift_archives(file_path: &Path, max_files: u32) -> io::Result<()> {
  let oldest = archive_path(file_path, max_files);
  if oldest.exists() {
    fs::remove_file(&oldest)?;
  }
  if max_files == 0 {
    return Ok(());
  }
  for index in (1..max_files).rev() {
    let source = archive_path(file_path, index);
    if source.exists() {
      fs::rename(&source, archive_path(file_path, index + 1))?;
    }
  }
  Ok(())
}

fn rotate_file(file_path: &Path, max_files: u32) -> io::Result<()> {
  if !file_path.exists() {
    return Ok(());
  }
  shift_archives(file_path, max_files)?;
  fs::rename(file_path, archive_path(file_path, 1))
}

fn current_file_size(file_path: &Path) -> u64 {
  fs::metadata(file_path).map(|m| m.len()).unwrap_or(0)
}

fn write_to_file(tee: &FileTee, line: &str) -> io::Result<()> {
  let line_bytes = line.len() as u64;
  let size = current_file_size(&tee.path);
  if size > 0 && size.saturating_add(line_bytes) > tee.max_bytes {
    rotate_file(&tee.path, tee.max_files)?;
  }
  let mut file = OpenOptions::new()
    .create(true)
    .append(true)
    .open(&tee.path)?;
  file.write_all(line.as_bytes())
}

pub fn create_log_sink(
  file_path: Option<PathBuf>,
  max_bytes: u64,
  max_files: u32,
) -> LogSink {
  let max_bytes = if max_bytes == 0 {
    DEFAULT_LOG_FILE_MAX_BYTES
  } else {
    max_bytes
  };
  let max_files = if max_files == 0 {
    DEFAULT_LOG_FILE_MAX_FILES
  } else {
    max_files
  };

  let file_tee = file_path.map(|path| {
    let mut tee = FileTee {
      path,
      max_bytes,
      max_files,
      disabled: false,
    };
    if let Some(parent) = tee.path.parent()
      && !parent.as_os_str().is_empty()
      && let Err(err) = fs::create_dir_all(parent)
    {
      warn_once("file", &err);
      tee.disabled = true;
    }
    Mutex::new(tee)
  });

  Arc::new(move |entry: RequestLogEntry| {
    let line = match serde_json::to_string(&entry) {
      Ok(json) => {
        let mut line = json;
        line.push('\n');
        line
      }
      Err(err) => {
        warn_once("serialize", &err);
        return;
      }
    };

    if let Err(err) = std::io::stdout().write_all(line.as_bytes()) {
      warn_once("stdout", &err);
    }

    if let Some(tee) = &file_tee {
      let tee = tee.lock().unwrap_or_else(PoisonError::into_inner);
      if tee.disabled {
        return;
      }
      if let Err(err) = write_to_file(&tee, &line) {
        warn_once("file", &err);
      }
    }
  })
}
