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
  /// Requires `GOOGLE_CLIENT_ID`, `GOOGLE_CLIENT_SECRET` (each also as a
  /// Docker-secret style `*_FILE`) and `APP_ORIGIN`
  pub fn from_env() -> Option<Self> {
    let client_id = env_or_file_nonempty("GOOGLE_CLIENT_ID")?;
    let client_secret = env_or_file_nonempty("GOOGLE_CLIENT_SECRET")?;
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
    self.authorize_url_with(state, None)
  }

  /// Authorize URL with an optional PKCE S256 `code_challenge`.
  ///
  /// Callers must store the matching `code_verifier` server-side; only the
  /// challenge travels on this URL. No OIDC `nonce` is sent: the profile
  /// comes from `userinfo` over the server-side, PKCE-bound code exchange,
  /// and the `id_token` is never parsed, so a nonce would imply a check
  /// that does not exist.
  pub fn authorize_url_with(
    &self,
    state: &str,
    code_challenge: Option<&str>,
  ) -> String {
    let redirect_uri = self.redirect_uri();
    let mut pairs: Vec<(&str, &str)> = vec![
      ("client_id", self.client_id.as_str()),
      ("redirect_uri", redirect_uri.as_str()),
      ("response_type", "code"),
      ("scope", OAUTH_SCOPES),
      ("state", state),
      ("access_type", "online"),
      ("prompt", "select_account"),
    ];
    if let Some(challenge) = code_challenge {
      pairs.push(("code_challenge", challenge));
      pairs.push(("code_challenge_method", "S256"));
    }
    reqwest::Url::parse_with_params(GOOGLE_AUTH_URL, &pairs)
      .expect("Google auth URL is valid")
      .to_string()
  }
}

fn env_or_file_nonempty(key: &str) -> Option<String> {
  crate::config::read_env_or_file(key)
    .map(|v| v.trim().to_string())
    .filter(|v| !v.is_empty())
}

/// Token endpoint, honoring [`test_endpoint_base`].
pub fn token_url() -> String {
  test_endpoint_base()
    .map(|base| format!("{base}/token"))
    .unwrap_or_else(|| GOOGLE_TOKEN_URL.to_owned())
}

fn userinfo_url() -> String {
  test_endpoint_base()
    .map(|base| format!("{base}/userinfo"))
    .unwrap_or_else(|| GOOGLE_USERINFO_URL.to_owned())
}

/// `GOOGLE_OAUTH_TEST_BASE_URL` points the token/userinfo calls at a local
/// stub so integration tests can drive the real callback. Ignored unless
/// `APP_ENV` is development/test: production always talks to Google.
fn test_endpoint_base() -> Option<String> {
  if !matches!(
    crate::config::app_env().as_str(),
    "development" | "dev" | "test"
  ) {
    return None;
  }
  env_nonempty("GOOGLE_OAUTH_TEST_BASE_URL")
    .map(|base| base.trim_end_matches('/').to_owned())
}

fn env_nonempty(key: &str) -> Option<String> {
  std::env::var(key)
    .ok()
    .map(|v| v.trim().to_string())
    .filter(|v| !v.is_empty())
}

/// Optional public origin (`APP_ORIGIN`) for OAuth callbacks and
/// `OriginPolicy::trust_origins`. Invalid values are treated as unset;
/// production boot still fails via [`crate::config::require_app_origin`].
pub fn app_origin() -> Option<String> {
  env_nonempty("APP_ORIGIN")
    .and_then(|raw| crate::config::normalize_app_origin(&raw).ok())
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
  let response = crate::http_client::shared_http_client()
    .post(token_url())
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
    tracing::error!(status = %status, "google token exchange failed");
    return Err(GoogleOAuthError::Token(format!("status {status}")));
  }

  serde_json::from_str(&body)
    .map_err(|e| GoogleOAuthError::Token(e.to_string()))
}

/// Fetch the Google OpenID userinfo profile.
pub async fn fetch_userinfo(
  access_token: &str,
) -> Result<GoogleUserInfo, GoogleOAuthError> {
  let response = crate::http_client::shared_http_client()
    .get(userinfo_url())
    .bearer_auth(access_token)
    .header("Accept", "application/json")
    .send()
    .await?;

  let status = response.status();
  let body = response.text().await?;
  if !status.is_success() {
    tracing::error!(status = %status, "google userinfo failed");
    return Err(GoogleOAuthError::UserInfo(format!("status {status}")));
  }

  serde_json::from_str(&body)
    .map_err(|e| GoogleOAuthError::UserInfo(e.to_string()))
}

/// Normalize a post-login callback path (relative only; blocks open redirects).
pub fn safe_callback_path(callback: Option<&str>) -> String {
  callback
    .and_then(crate::redirect::safe_local_redirect)
    .unwrap_or_else(|| "/dashboard".to_string())
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
    assert!(!url.contains("code_challenge"));
    assert!(!url.contains("nonce="));
  }

  #[test]
  fn authorize_url_with_pkce_has_no_nonce() {
    let config = GoogleOAuthConfig {
      client_id: "fake-client.apps.googleusercontent.com".into(),
      client_secret: "GOCSPX-fake".into(),
      app_origin: "http://localhost:4200".into(),
    };
    let url = config.authorize_url_with(
      "csrf-state-1",
      Some("E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"),
    );
    assert!(
      url
        .contains("code_challenge=E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM")
    );
    assert!(url.contains("code_challenge_method=S256"));
    assert!(!url.contains("nonce="));
    assert!(url.contains("state=csrf-state-1"));
  }

  #[test]
  fn safe_callback_rejects_open_redirect() {
    assert_eq!(safe_callback_path(Some("/conta")), "/conta");
    assert_eq!(safe_callback_path(Some("https://evil.test")), "/dashboard");
    assert_eq!(safe_callback_path(Some("//evil.test")), "/dashboard");
    assert_eq!(safe_callback_path(None), "/dashboard");
    assert_eq!(safe_callback_path(Some("/\\evil.test")), "/dashboard");
    assert_eq!(safe_callback_path(Some("/\t/evil.test")), "/dashboard");
  }
}
