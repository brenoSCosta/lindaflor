//! Session cookie configuration for local HTTP vs production HTTPS.

use std::borrow::Cow;
use std::time::Duration;

use topcoat::{
  context::Cx,
  cookie::{Cookie, Cookies, SameSite},
  session::{
    SessionConfig, Token, TokenStore, TokenStoreFuture,
    cookie::{CookieTokenStore, SESSION_COOKIE_NAME},
  },
};

use crate::config::app_env;

/// Prefer daily sliding refresh, matching Better Auth `updateAge` (24h).
pub const SESSION_UPDATE_AGE: Duration = Duration::from_secs(60 * 60 * 24);

fn is_development() -> bool {
  matches!(app_env().as_str(), "development" | "dev")
}

/// Build [`SessionConfig`]: hardened `__Host-` cookie in production; plain
/// `session` cookie (Secure=false, SameSite=Lax) when `APP_ENV` is development.
pub fn session_config() -> SessionConfig {
  if is_development() {
    // Local HTTP: `__Host-` + Secure would never stick on http://localhost.
    SessionConfig::builder()
      .token_store(LocalHttpCookieTokenStore::new())
      .build()
  } else {
    SessionConfig::builder()
      .token_store(CookieTokenStore::new().name(SESSION_COOKIE_NAME))
      .build()
  }
}

/// Cookie token store for non-TLS localhost: no `__Host-` prefix, `Secure=false`.
struct LocalHttpCookieTokenStore {
  name: Cow<'static, str>,
}

impl LocalHttpCookieTokenStore {
  fn new() -> Self {
    Self {
      name: Cow::Borrowed(SESSION_COOKIE_NAME),
    }
  }
}

impl TokenStore for LocalHttpCookieTokenStore {
  fn read<'a>(&'a self, cx: &'a Cx) -> TokenStoreFuture<'a, Option<Token>> {
    Box::pin(async move {
      let cookies = topcoat::cookie::cookies(cx)
        .override_same_site(SameSite::Lax)
        .override_http_only(true)
        .override_secure(false)
        .override_path("/");
      let Some(cookie) = cookies.get(&self.name) else {
        return Ok(None);
      };
      Ok(Token::decode(cookie.value_trimmed()).ok())
    })
  }

  fn write<'a>(
    &'a self,
    cx: &'a Cx,
    token: Token,
    max_age: Duration,
  ) -> TokenStoreFuture<'a, ()> {
    Box::pin(async move {
      let max_age = topcoat::cookie::time::Duration::try_from(max_age)?;
      topcoat::cookie::cookies(cx)
        .override_same_site(SameSite::Lax)
        .override_http_only(true)
        .override_secure(false)
        .override_path("/")
        .override_max_age(max_age)
        .add(Cookie::new(self.name.clone(), token.encode()));
      Ok(())
    })
  }

  fn delete<'a>(&'a self, cx: &'a Cx) -> TokenStoreFuture<'a, ()> {
    Box::pin(async move {
      topcoat::cookie::cookies(cx)
        .override_same_site(SameSite::Lax)
        .override_http_only(true)
        .override_secure(false)
        .override_path("/")
        .remove(Cookie::new(self.name.clone(), ""));
      Ok(())
    })
  }
}
