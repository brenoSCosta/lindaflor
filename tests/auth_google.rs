//! End-to-end Google OAuth callback tests (security review A1, A2, A4)
//! against a local stub of Google's token/userinfo endpoints
//! (`GOOGLE_OAUTH_TEST_BASE_URL`, honored only outside production).

use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::{Mutex, OnceLock};

use http::{Method, Request, StatusCode, header};
use sqlx::PgPool;
use test_support::{
  json_get, json_post, named_cookie_from, pool_and_router, session_cookie_from,
  sign_in, sign_up, totp_code, unique_email,
};
use topcoat::router::{Body, Router};
use uuid::Uuid;

const PASSWORD: &str = "Harbor-Violet-Tiger-4821-Lantern!";
const BIND_COOKIE: &str = "oauth_bind";
const PENDING_COOKIE: &str = "lindaflor_pending_2fa";

/// `code` → userinfo JSON served by the stub.
fn profiles() -> &'static Mutex<HashMap<String, serde_json::Value>> {
  static PROFILES: OnceLock<Mutex<HashMap<String, serde_json::Value>>> =
    OnceLock::new();
  PROFILES.get_or_init(|| Mutex::new(HashMap::new()))
}

/// Blocking stub on its own thread so it outlives every test runtime.
/// Replies `Connection: close` so the app's pooled HTTP client never reuses
/// a connection across test runtimes.
fn stub_base_url() -> &'static str {
  static BASE: OnceLock<String> = OnceLock::new();
  BASE.get_or_init(|| {
    let listener = TcpListener::bind("127.0.0.1:0").expect("bind stub");
    let base = format!("http://{}", listener.local_addr().unwrap());
    std::thread::spawn(move || {
      for stream in listener.incoming().flatten() {
        std::thread::spawn(move || serve(stream));
      }
    });
    base
  })
}

fn serve(stream: TcpStream) {
  let mut reader = BufReader::new(stream.try_clone().expect("clone"));
  let mut request_line = String::new();
  if reader.read_line(&mut request_line).is_err() {
    return;
  }
  let mut content_length = 0usize;
  let mut bearer = String::new();
  loop {
    let mut line = String::new();
    if reader.read_line(&mut line).is_err() || line == "\r\n" || line.is_empty()
    {
      break;
    }
    let lower = line.to_ascii_lowercase();
    if let Some(v) = lower.strip_prefix("content-length:") {
      content_length = v.trim().parse().unwrap_or(0);
    }
    if lower.starts_with("authorization:") {
      bearer = line
        .split_once("Bearer ")
        .map(|(_, t)| t.trim().to_owned())
        .unwrap_or_default();
    }
  }
  let mut body = vec![0u8; content_length];
  let _ = reader.read_exact(&mut body);
  let body = String::from_utf8_lossy(&body).into_owned();

  let (status, payload) = if request_line.starts_with("POST /token") {
    let code = body
      .split('&')
      .find_map(|kv| kv.strip_prefix("code="))
      .unwrap_or("")
      .to_owned();
    (
      "200 OK",
      serde_json::json!({
          "access_token": format!("at-{code}"),
          "refresh_token": format!("rt-{code}"),
          "id_token": format!("it-{code}"),
          "expires_in": 3600,
          "token_type": "Bearer",
      }),
    )
  } else if request_line.starts_with("GET /userinfo") {
    let code = bearer.strip_prefix("at-").unwrap_or("");
    match profiles().lock().unwrap().get(code) {
      Some(profile) => ("200 OK", profile.clone()),
      None => ("401 Unauthorized", serde_json::json!({})),
    }
  } else {
    ("404 Not Found", serde_json::json!({}))
  };
  let payload = payload.to_string();
  let mut stream = stream;
  let _ = write!(
    stream,
    "HTTP/1.1 {status}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{payload}",
    payload.len()
  );
}

/// Pool + router with Google configured against the stub. Every test in
/// this binary uses the same env values, so setting them is race-free.
async fn setup() -> (PgPool, Router) {
  let base = stub_base_url();
  unsafe {
    std::env::set_var(
      "GOOGLE_CLIENT_ID",
      "test-client.apps.googleusercontent.com",
    );
    std::env::set_var("GOOGLE_CLIENT_SECRET", "test-secret");
    std::env::set_var("APP_ORIGIN", "http://127.0.0.1:4200");
    std::env::set_var("GOOGLE_OAUTH_TEST_BASE_URL", base);
  }
  pool_and_router().await
}

/// Register the profile Google "returns" for `code`.
fn google_profile(sub: &str, email: &str, verified: bool) -> String {
  let code = format!("code{}", Uuid::now_v7().simple());
  profiles().lock().unwrap().insert(
    code.clone(),
    serde_json::json!({
        "sub": sub,
        "email": email,
        "email_verified": verified,
        "name": "Google Person",
    }),
  );
  code
}

struct Started {
  state: String,
  bind_cookie: Option<String>,
}

async fn send(
  router: &Router,
  method: Method,
  uri: &str,
  cookie: Option<&str>,
  json: Option<serde_json::Value>,
) -> http::Response<Body> {
  let mut builder = Request::builder().method(method).uri(uri);
  if let Some(cookie) = cookie {
    builder = builder.header(header::COOKIE, cookie);
  }
  let req = match json {
    Some(json) => builder
      .header(header::CONTENT_TYPE, "application/json")
      .body(Body::from(serde_json::to_vec(&json).unwrap())),
    None => builder.body(Body::empty()),
  }
  .expect("request");
  router.handle(req).await
}

fn state_from_url(url: &str) -> String {
  url
    .split(['?', '&'])
    .find_map(|kv| kv.strip_prefix("state="))
    .expect("state param")
    .to_owned()
}

async fn start_login(router: &Router) -> Started {
  let res = send(
    router,
    Method::POST,
    "/api/auth/sign-in/social",
    None,
    Some(serde_json::json!({ "provider": "google" })),
  )
  .await;
  assert_eq!(res.status(), StatusCode::OK);
  let bind_cookie = named_cookie_from(&res, BIND_COOKIE);
  let json = test_support::body_json(res).await;
  Started {
    state: state_from_url(json["url"].as_str().unwrap()),
    bind_cookie,
  }
}

async fn start_link(router: &Router, session_cookie: &str) -> Started {
  let res = send(
    router,
    Method::POST,
    "/api/auth/link-social",
    Some(session_cookie),
    Some(serde_json::json!({ "provider": "google", "password": PASSWORD })),
  )
  .await;
  assert_eq!(res.status(), StatusCode::OK);
  let bind_cookie = named_cookie_from(&res, BIND_COOKIE);
  let json = test_support::body_json(res).await;
  Started {
    state: state_from_url(json["url"].as_str().unwrap()),
    bind_cookie,
  }
}

async fn callback(
  router: &Router,
  code: &str,
  state: &str,
  cookie: Option<&str>,
) -> http::Response<Body> {
  send(
    router,
    Method::GET,
    &format!("/api/auth/callback/google?code={code}&state={state}"),
    cookie,
    None,
  )
  .await
}

fn location(res: &http::Response<Body>) -> String {
  res
    .headers()
    .get(header::LOCATION)
    .and_then(|v| v.to_str().ok())
    .unwrap_or("")
    .to_owned()
}

async fn user_id_by_email(pool: &PgPool, email: &str) -> Option<Uuid> {
  sqlx::query_scalar("SELECT id FROM users WHERE email = $1")
    .bind(email)
    .fetch_optional(pool)
    .await
    .unwrap()
}

#[tokio::test]
async fn google_login_creates_session_in_the_starting_browser() {
  let (pool, router) = setup().await;
  let email = unique_email("google-new");
  let code = google_profile(&format!("sub-{}", Uuid::now_v7()), &email, true);

  let started = start_login(&router).await;
  let bind = started.bind_cookie.expect("oauth_bind cookie on start");
  let res = callback(&router, &code, &started.state, Some(&bind)).await;
  assert!(res.status().is_redirection(), "{}", res.status());
  assert!(session_cookie_from(&res).is_some(), "expected a session");
  assert!(user_id_by_email(&pool, &email).await.is_some());
}

/// Only the Google identity is kept: the OAuth tokens returned by the
/// exchange never reach `accounts`, and no unchecked OIDC nonce is sent.
#[tokio::test]
async fn google_login_stores_no_provider_tokens() {
  let (pool, router) = setup().await;
  let email = unique_email("google-notokens");
  let sub = format!("sub-{}", Uuid::now_v7());
  let code = google_profile(&sub, &email, true);

  let res = send(
    &router,
    Method::POST,
    "/api/auth/sign-in/social",
    None,
    Some(serde_json::json!({ "provider": "google" })),
  )
  .await;
  assert_eq!(res.status(), StatusCode::OK);
  let bind = named_cookie_from(&res, BIND_COOKIE).expect("bind cookie");
  let json = test_support::body_json(res).await;
  let url = json["url"].as_str().unwrap().to_owned();
  assert!(url.contains("code_challenge="), "{url}");
  assert!(!url.contains("nonce="), "{url}");

  let res = callback(&router, &code, &state_from_url(&url), Some(&bind)).await;
  assert!(res.status().is_redirection(), "{}", res.status());

  // Second login updates the existing row: still nothing stored.
  let again = start_login(&router).await;
  let res =
    callback(&router, &code, &again.state, again.bind_cookie.as_deref()).await;
  assert!(res.status().is_redirection(), "{}", res.status());

  let row: (Option<String>, Option<String>, Option<String>, bool) =
    sqlx::query_as(
      "SELECT access_token, refresh_token, id_token,
              access_token_expires_at IS NULL
       FROM accounts WHERE provider_id = 'google' AND account_id = $1",
    )
    .bind(&sub)
    .fetch_one(&pool)
    .await
    .unwrap();
  assert_eq!(row, (None, None, None, true));
}

/// Migration 0026 scrubs tokens stored before the change (idempotent, so
/// it can be replayed here against the already-migrated test database).
#[tokio::test]
async fn migration_0026_clears_stored_google_tokens() {
  let (pool, _router) = setup().await;
  let user_id = Uuid::now_v7();
  let sub = format!("sub-{}", Uuid::now_v7());
  sqlx::query(
    "INSERT INTO users (id, name, email, email_verified, role, created_at, updated_at)
     VALUES ($1, 'Legacy', $2, true, 'user', now(), now())",
  )
  .bind(user_id)
  .bind(unique_email("google-legacy"))
  .execute(&pool)
  .await
  .unwrap();
  sqlx::query(
    "INSERT INTO accounts (id, account_id, provider_id, user_id, access_token,
                           refresh_token, id_token, access_token_expires_at,
                           created_at, updated_at)
     VALUES ($1, $2, 'google', $3, 'at', 'rt', 'it', now(), now(), now())",
  )
  .bind(Uuid::now_v7())
  .bind(&sub)
  .bind(user_id)
  .execute(&pool)
  .await
  .unwrap();

  sqlx::raw_sql(include_str!("../migrations/0026_polish.sql"))
    .execute(&pool)
    .await
    .unwrap();

  let row: (Option<String>, Option<String>, Option<String>, bool) =
    sqlx::query_as(
      "SELECT access_token, refresh_token, id_token,
              access_token_expires_at IS NULL
       FROM accounts WHERE provider_id = 'google' AND account_id = $1",
    )
    .bind(&sub)
    .fetch_one(&pool)
    .await
    .unwrap();
  assert_eq!(row, (None, None, None, true));
}

// A4: login CSRF — the callback must run in the browser that started it.
#[tokio::test]
async fn google_callback_without_the_binding_cookie_is_rejected() {
  let (pool, router) = setup().await;
  let email = unique_email("google-csrf");
  let code = google_profile(&format!("sub-{}", Uuid::now_v7()), &email, true);

  // Attacker starts the flow; the victim's browser follows the callback URL.
  let attacker = start_login(&router).await;
  let res = callback(&router, &code, &attacker.state, None).await;
  assert_eq!(res.status(), StatusCode::BAD_REQUEST);
  assert!(session_cookie_from(&res).is_none());

  // A binding cookie from a different flow does not match either.
  let attacker = start_login(&router).await;
  let victim = start_login(&router).await;
  let res = callback(
    &router,
    &code,
    &attacker.state,
    victim.bind_cookie.as_deref(),
  )
  .await;
  assert_eq!(res.status(), StatusCode::BAD_REQUEST);
  assert!(session_cookie_from(&res).is_none());
  assert!(user_id_by_email(&pool, &email).await.is_none());
}

// A1: Google proves the first factor only.
#[tokio::test]
async fn google_login_for_a_2fa_user_goes_to_the_2fa_step() {
  let (pool, router) = setup().await;
  let email = unique_email("google-2fa");
  let signed_up = sign_up(&router, "Two Factor", &email, PASSWORD).await;
  let cookie = signed_up.next_cookie.clone().unwrap();
  let user_id =
    Uuid::parse_str(signed_up.json["user"]["id"].as_str().unwrap()).unwrap();
  // A verified address links normally (no takeover).
  sqlx::query("UPDATE users SET email_verified = true WHERE id = $1")
    .bind(user_id)
    .execute(&pool)
    .await
    .unwrap();
  let enable = json_post(
    &router,
    "/api/auth/two-factor/enable",
    Some(&cookie),
    serde_json::json!({ "password": PASSWORD }),
  )
  .await;
  let secret = enable.json["secret"].as_str().unwrap().to_owned();
  let verified = json_post(
    &router,
    "/api/auth/two-factor/verify-totp",
    Some(&cookie),
    serde_json::json!({ "code": totp_code(&secret) }),
  )
  .await;
  assert_eq!(verified.status, StatusCode::OK, "{:?}", verified.json);

  let code = google_profile(&format!("sub-{}", Uuid::now_v7()), &email, true);
  let started = start_login(&router).await;
  let res = callback(
    &router,
    &code,
    &started.state,
    started.bind_cookie.as_deref(),
  )
  .await;
  assert!(res.status().is_redirection(), "{}", res.status());
  assert!(
    session_cookie_from(&res).is_none(),
    "2FA user must not get a session from Google alone"
  );
  assert!(named_cookie_from(&res, PENDING_COOKIE).is_some());
  assert!(location(&res).contains("/two-factor"), "{}", location(&res));
}

// A2: account pre-hijacking.
#[tokio::test]
async fn google_login_takes_over_an_unverified_squatted_account() {
  let (pool, router) = setup().await;
  let victim_email = unique_email("google-victim");

  // Attacker registers the victim's address first (never verified).
  let squat = sign_up(&router, "Attacker", &victim_email, PASSWORD).await;
  assert_eq!(squat.status, StatusCode::OK);
  let attacker_cookie = squat.next_cookie.clone().unwrap();
  let user_id = user_id_by_email(&pool, &victim_email).await.unwrap();

  let code =
    google_profile(&format!("sub-{}", Uuid::now_v7()), &victim_email, true);
  let started = start_login(&router).await;
  let res = callback(
    &router,
    &code,
    &started.state,
    started.bind_cookie.as_deref(),
  )
  .await;
  assert!(res.status().is_redirection(), "{}", res.status());
  assert!(session_cookie_from(&res).is_some());
  assert_eq!(user_id_by_email(&pool, &victim_email).await, Some(user_id));

  // The attacker's session and password are gone.
  let old =
    json_get(&router, "/api/auth/get-session", Some(&attacker_cookie)).await;
  assert!(old.json.is_null(), "{:?}", old.json);
  let relogin = sign_in(&router, &victim_email, PASSWORD).await;
  assert_eq!(relogin.status, StatusCode::UNAUTHORIZED);

  let (verified, providers): (bool, Vec<String>) = (
    sqlx::query_scalar("SELECT email_verified FROM users WHERE id = $1")
      .bind(user_id)
      .fetch_one(&pool)
      .await
      .unwrap(),
    sqlx::query_scalar("SELECT provider_id FROM accounts WHERE user_id = $1")
      .bind(user_id)
      .fetch_all(&pool)
      .await
      .unwrap(),
  );
  assert!(verified);
  assert_eq!(providers, vec!["google".to_owned()]);
}

// A2 (second bug): linking a Google account with another address must not
// verify the local e-mail.
#[tokio::test]
async fn linking_google_with_a_different_email_does_not_verify_local_email() {
  let (pool, router) = setup().await;
  let local_email = unique_email("google-link-local");
  let signed_up = sign_up(&router, "Linker", &local_email, PASSWORD).await;
  let session = signed_up.next_cookie.clone().unwrap();
  let user_id = user_id_by_email(&pool, &local_email).await.unwrap();

  let code = google_profile(
    &format!("sub-{}", Uuid::now_v7()),
    &unique_email("google-link-other"),
    true,
  );
  let started = start_link(&router, &session).await;
  let cookie = format!(
    "{session}; {}",
    started
      .bind_cookie
      .expect("oauth_bind cookie on link start")
  );
  let res = callback(&router, &code, &started.state, Some(&cookie)).await;
  assert!(res.status().is_redirection(), "{}", res.status());

  let linked: i64 = sqlx::query_scalar(
    "SELECT COUNT(*) FROM accounts WHERE user_id = $1 AND provider_id = 'google'",
  )
  .bind(user_id)
  .fetch_one(&pool)
  .await
  .unwrap();
  assert_eq!(linked, 1);
  let verified: bool =
    sqlx::query_scalar("SELECT email_verified FROM users WHERE id = $1")
      .bind(user_id)
      .fetch_one(&pool)
      .await
      .unwrap();
  assert!(
    !verified,
    "a different Google address must not verify this one"
  );
}
