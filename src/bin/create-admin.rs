//! Create or promote a Lindaflor admin (production bootstrap).
//!
//! ```text
//! create-admin --email EMAIL [--name NAME] [--no-password]
//! ```
//!
//! The password comes from the file named by `ADMIN_PASSWORD_FILE` (Docker
//! secret style) or, failing that, the first line of stdin — never from
//! argv, so it stays out of shell history and `ps`. It must satisfy the
//! sign-up password policy. `--no-password` promotes an existing user
//! without touching their credentials. Needs the same env as the server
//! (`DATABASE_URL`, and in production `APP_ORIGIN` + `TOTP_ENCRYPTION_KEY`);
//! refuses to run against a database with pending migrations.

use std::io::{BufRead, IsTerminal, Write};
use std::process::ExitCode;

use lindaflor::auth::bootstrap::{
  AdminBootstrapOutcome, create_or_promote_admin,
};
use lindaflor::config::Config;
use lindaflor::db;
use zeroize::Zeroizing;

const USAGE: &str = "usage: create-admin --email EMAIL [--name NAME] [--no-password]\n\
  password: ADMIN_PASSWORD_FILE=<path> or one line on stdin";

struct Args {
  email: String,
  name: String,
  no_password: bool,
}

fn parse_args() -> Result<Args, String> {
  let mut email = None;
  let mut name = None;
  let mut no_password = false;
  let mut args = std::env::args().skip(1);
  while let Some(arg) = args.next() {
    match arg.as_str() {
      "--email" => email = args.next(),
      "--name" => name = args.next(),
      "--no-password" => no_password = true,
      "-h" | "--help" => return Err(USAGE.to_string()),
      other => {
        if let Some(value) = other.strip_prefix("--email=") {
          email = Some(value.to_string());
        } else if let Some(value) = other.strip_prefix("--name=") {
          name = Some(value.to_string());
        } else {
          return Err(format!("unknown argument `{other}`\n{USAGE}"));
        }
      }
    }
  }
  Ok(Args {
    email: email.ok_or_else(|| format!("--email is required\n{USAGE}"))?,
    name: name.unwrap_or_else(|| "Admin".to_string()),
    no_password,
  })
}

fn read_password() -> Result<Zeroizing<String>, String> {
  if let Some(path) = std::env::var("ADMIN_PASSWORD_FILE")
    .ok()
    .filter(|path| !path.is_empty())
  {
    let contents = std::fs::read_to_string(&path)
      .map_err(|e| format!("cannot read ADMIN_PASSWORD_FILE: {e}"))?;
    return Ok(Zeroizing::new(
      contents.trim_end_matches(['\n', '\r']).to_string(),
    ));
  }
  let stdin = std::io::stdin();
  if stdin.is_terminal() {
    eprint!("Password (input is visible; prefer ADMIN_PASSWORD_FILE): ");
    let _ = std::io::stderr().flush();
  }
  let mut line = Zeroizing::new(String::new());
  stdin
    .lock()
    .read_line(&mut line)
    .map_err(|e| format!("cannot read password from stdin: {e}"))?;
  Ok(Zeroizing::new(
    line.trim_end_matches(['\n', '\r']).to_string(),
  ))
}

#[tokio::main]
async fn main() -> ExitCode {
  match run().await {
    Ok(()) => ExitCode::SUCCESS,
    Err(message) => {
      eprintln!("create-admin: {message}");
      ExitCode::FAILURE
    }
  }
}

async fn run() -> Result<(), String> {
  let args = parse_args()?;
  let password = if args.no_password {
    None
  } else {
    let password = read_password()?;
    if password.is_empty() {
      return Err(
        "empty password (use --no-password to promote without one)".to_string(),
      );
    }
    Some(password)
  };

  let cfg = Config::from_env().map_err(|e| e.to_string())?;
  let pool = db::create_pool(&cfg.database_url)
    .await
    .map_err(|e| e.to_string())?;

  let outcome = create_or_promote_admin(
    &pool,
    &args.email,
    &args.name,
    password.as_deref().map(String::as_str),
  )
  .await
  .map_err(|e| e.to_string())?;

  match outcome {
    AdminBootstrapOutcome::Created(id) => {
      println!("created admin {id} ({})", args.email.trim());
    }
    AdminBootstrapOutcome::Promoted(id) => {
      println!("promoted {id} ({}) to admin", args.email.trim());
    }
  }
  Ok(())
}
