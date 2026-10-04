use serde::Deserialize;
use thiserror::Error;

pub const GOOGLE_PROVIDER_ID: &str = "google";

const GOOGLE_AUTH_URL: &str = "https://accounts.google.com/o/oauth2/v2/auth";
const GOOGLE_TOKEN_URL: &str = "https://oauth2.googleapis.com/token";
const GOOGLE_USERINFO_URL: &str =
  "https://openidconnect.googleapis.com/v1/userinfo";
const OAUTH_SCOPES: &str = "openid email profile";

#[derive(Debug, Clone)]
pub struct GoogleOAuthConfig {
  pub client_id: String,
  pub client_secret: String,
  pub app_origin: String,
}

impl GoogleOAuthConfig {
  /// Load from env when Google social sign-in is configured.
  ///
  /// Requires `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` and `APP_ORIGIN`
  pub fn from_env() -> Option<Self> {
    let client_id = env_nonempty("GOOGLE_CLIENT_ID")?;
    let client_secret = env_nonempty("GOOGLE_CLIENT_SECRET")?;
    let app_origin = app_origin()?;
    Some(Self {
      client_id,
      client_secret,
      app_origin,
    })
  }

  pub fn redirect_uri(&self) -> String {
    format!("{}/api/auth/callback/google", self.app_origin)
  }

  /// Build the Google authorize URL the client should navigate to.
  pub fn authorize_url(&self, state: &str) -> String {
    let redirect_uri = self.redirect_uri();
    reqwest::Url::parse_with_params(
      GOOGLE_AUTH_URL,
      [
        ("client_id", self.client_id.as_str()),
        ("redirect_uri", redirect_uri.as_str()),
        ("response_type", "code"),
        ("scope", OAUTH_SCOPES),
        ("state", state),
        ("access_type", "online"),
        ("prompt", "select_account"),
      ],
    )
    .expect("Google auth URL is valid")
    .to_string()
  }
}

fn env_nonempty(key: &str) -> Option<String> {
  std::env::var(key)
    .ok()
    .map(|v| v.trim().to_string())
    .filter(|v| !v.is_empty())
}

/// Public origin for OAuth callback construction.
pub fn app_origin() -> Option<String> {
  env_nonempty("APP_ORIGIN").map(|s| s.trim_end_matches('/').to_string())
}

#[derive(Debug, Error)]
pub enum GoogleOAuthError {
  #[error("HTTP client error: {0}")]
  Http(#[from] reqwest::Error),
  #[error("token exchange failed: {0}")]
  Token(String),
  #[error("userinfo failed: {0}")]
  UserInfo(String),
}

#[derive(Debug, Deserialize)]
pub struct GoogleTokenResponse {
  pub access_token: String,
  pub expires_in: Option<i64>,
  pub refresh_token: Option<String>,
  pub id_token: Option<String>,
  pub scope: Option<String>,
  pub token_type: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct GoogleUserInfo {
  pub sub: String,
  pub email: Option<String>,
  pub email_verified: Option<bool>,
  pub name: Option<String>,
  pub picture: Option<String>,
}

/// Exchange an authorization code for tokens.
pub async fn exchange_code(
  config: &GoogleOAuthConfig,
  code: &str,
) -> Result<GoogleTokenResponse, GoogleOAuthError> {
  let redirect_uri = config.redirect_uri();
  let client = reqwest::Client::new();
  let response = client
    .post(GOOGLE_TOKEN_URL)
    .header("Accept", "application/json")
    .form(&[
      ("code", code),
      ("client_id", config.client_id.as_str()),
      ("client_secret", config.client_secret.as_str()),
      ("redirect_uri", redirect_uri.as_str()),
      ("grant_type", "authorization_code"),
    ])
    .send()
    .await?;

  let status = response.status();
  let body = response.text().await?;
  if !status.is_success() {
    return Err(GoogleOAuthError::Token(format!("status {status}: {body}")));
  }

  serde_json::from_str(&body)
    .map_err(|e| GoogleOAuthError::Token(e.to_string()))
}

/// Fetch the Google OpenID userinfo profile.
pub async fn fetch_userinfo(
  access_token: &str,
) -> Result<GoogleUserInfo, GoogleOAuthError> {
  let client = reqwest::Client::new();
  let response = client
    .get(GOOGLE_USERINFO_URL)
    .bearer_auth(access_token)
    .header("Accept", "application/json")
    .send()
    .await?;

  let status = response.status();
  let body = response.text().await?;
  if !status.is_success() {
    return Err(GoogleOAuthError::UserInfo(format!(
      "status {status}: {body}"
    )));
  }

  serde_json::from_str(&body)
    .map_err(|e| GoogleOAuthError::UserInfo(e.to_string()))
}

/// Normalize a post-login callback path (relative only; blocks open redirects).
pub fn safe_callback_path(callback: Option<&str>) -> String {
  match callback.map(str::trim).filter(|s| !s.is_empty()) {
    Some(path) if path.starts_with('/') && !path.starts_with("//") => {
      path.to_string()
    }
    _ => "/dashboard".to_string(),
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn authorize_url_includes_callback_and_state() {
    let config = GoogleOAuthConfig {
      client_id: "fake-client.apps.googleusercontent.com".into(),
      client_secret: "GOCSPX-fake".into(),
      app_origin: "http://localhost:4200".into(),
    };
    let url = config.authorize_url("csrf-state-1");
    assert!(url.starts_with("https://accounts.google.com/o/oauth2/v2/auth?"));
    assert!(url.contains("client_id=fake-client.apps.googleusercontent.com"));
    assert!(url.contains("state=csrf-state-1"));
    assert!(url.contains(
            "redirect_uri=http%3A%2F%2Flocalhost%3A4200%2Fapi%2Fauth%2Fcallback%2Fgoogle"
        ));
    assert!(url.contains("scope=openid+email+profile"));
  }

  #[test]
  fn safe_callback_rejects_open_redirect() {
    assert_eq!(safe_callback_path(Some("/conta")), "/conta");
    assert_eq!(safe_callback_path(Some("https://evil.test")), "/dashboard");
    assert_eq!(safe_callback_path(Some("//evil.test")), "/dashboard");
    assert_eq!(safe_callback_path(None), "/dashboard");
  }
}
