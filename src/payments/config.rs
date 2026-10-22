use crate::config::{app_env, read_env_or_file};

/// Default Mercado Pago API origin. Overridable with `MP_API_BASE_URL` so
/// tests (and a sandbox proxy) can point the client at a mock server.
pub const MP_DEFAULT_BASE_URL: &str = "https://api.mercadopago.com";

/// Which payment strategy new orders use (`PAYMENT_PROVIDER`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ProviderKind {
  /// Locally generated static PIX BR Code + manual confirmation. Kept for
  /// dev and as a fallback; never the production default.
  StaticPix,
  MercadoPago,
}

impl ProviderKind {
  pub fn as_str(self) -> &'static str {
    match self {
      Self::StaticPix => "static_pix",
      Self::MercadoPago => "mercadopago",
    }
  }
}

#[derive(Clone)]
pub struct MercadoPagoConfig {
  pub access_token: String,
  pub webhook_secret: String,
  pub base_url: String,
  /// `https://<APP_ORIGIN>/api/webhooks/mercadopago` when `APP_ORIGIN` is
  /// set; otherwise the webhook must be configured in the MP dashboard.
  pub notification_url: Option<String>,
}

// Manual `Debug` so a `{:?}` log/panic never leaks the access token or the
// webhook signing secret.
impl std::fmt::Debug for MercadoPagoConfig {
  fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
    f.debug_struct("MercadoPagoConfig")
      .field("access_token", &"***")
      .field("webhook_secret", &"***")
      .field("base_url", &self.base_url)
      .field("notification_url", &self.notification_url)
      .finish()
  }
}

#[derive(Debug, Clone)]
pub struct PaymentConfig {
  pub kind: ProviderKind,
  pub mercadopago: Option<MercadoPagoConfig>,
}

#[derive(Debug, thiserror::Error)]
pub enum PaymentConfigError {
  #[error("invalid PAYMENT_PROVIDER {0:?}: expected static_pix or mercadopago")]
  InvalidProvider(String),
  #[error("PAYMENT_PROVIDER=mercadopago requires {0} (or {0}_FILE) to be set")]
  MissingVar(&'static str),
  #[error(
    "refusing to boot in production with the static PIX provider: set PAYMENT_PROVIDER=mercadopago, or ALLOW_STATIC_PIX=1 to accept manual confirmation"
  )]
  StaticPixInProduction,
  #[error("MP_API_BASE_URL must be an https:// URL in production")]
  InsecureBaseUrl,
}

fn truthy(raw: &str) -> bool {
  matches!(
    raw.trim().to_ascii_lowercase().as_str(),
    "1" | "true" | "yes" | "on"
  )
}

impl PaymentConfig {
  pub fn static_pix() -> Self {
    Self {
      kind: ProviderKind::StaticPix,
      mercadopago: None,
    }
  }

  /// Read `PAYMENT_PROVIDER`, `MP_ACCESS_TOKEN`, `MP_WEBHOOK_SECRET`
  /// (each with `*_FILE` support) and `MP_API_BASE_URL`.
  ///
  /// Unset `PAYMENT_PROVIDER` means `mercadopago` when `MP_ACCESS_TOKEN` is
  /// present, else `static_pix`. Production guard: Mercado Pago needs both
  /// secrets and an https base URL; static PIX needs an explicit
  /// `ALLOW_STATIC_PIX=1` (manual reconciliation is an opt-in, not a
  /// silent default).
  pub fn from_env() -> Result<Self, PaymentConfigError> {
    let production = app_env() == "production";
    let access_token = read_env_or_file("MP_ACCESS_TOKEN");
    let kind = match read_env_or_file("PAYMENT_PROVIDER") {
      Some(raw) => match raw.trim().to_ascii_lowercase().as_str() {
        "static_pix" | "static" | "pix" | "manual_pix" => {
          ProviderKind::StaticPix
        }
        "mercadopago" | "mercado_pago" | "mp" => ProviderKind::MercadoPago,
        _ => return Err(PaymentConfigError::InvalidProvider(raw)),
      },
      None if access_token.is_some() => ProviderKind::MercadoPago,
      None => ProviderKind::StaticPix,
    };

    match kind {
      ProviderKind::StaticPix => {
        if production
          && !read_env_or_file("ALLOW_STATIC_PIX").is_some_and(|v| truthy(&v))
        {
          return Err(PaymentConfigError::StaticPixInProduction);
        }
        Ok(Self::static_pix())
      }
      ProviderKind::MercadoPago => {
        let access_token = access_token
          .ok_or(PaymentConfigError::MissingVar("MP_ACCESS_TOKEN"))?;
        let webhook_secret = read_env_or_file("MP_WEBHOOK_SECRET")
          .ok_or(PaymentConfigError::MissingVar("MP_WEBHOOK_SECRET"))?;
        let base_url = read_env_or_file("MP_API_BASE_URL")
          .map(|url| url.trim().trim_end_matches('/').to_string())
          .unwrap_or_else(|| MP_DEFAULT_BASE_URL.to_string());
        if production && !base_url.starts_with("https://") {
          return Err(PaymentConfigError::InsecureBaseUrl);
        }
        let notification_url = crate::auth::google::app_origin()
          .map(|origin| format!("{origin}/api/webhooks/mercadopago"));
        Ok(Self {
          kind,
          mercadopago: Some(MercadoPagoConfig {
            access_token,
            webhook_secret,
            base_url,
            notification_url,
          }),
        })
      }
    }
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  #[test]
  fn debug_redacts_secrets() {
    let cfg = MercadoPagoConfig {
      access_token: "APP_USR-secret-token".to_string(),
      webhook_secret: "whsec-very-secret".to_string(),
      base_url: MP_DEFAULT_BASE_URL.to_string(),
      notification_url: None,
    };
    let rendered = format!(
      "{:?}",
      PaymentConfig {
        kind: ProviderKind::MercadoPago,
        mercadopago: Some(cfg),
      }
    );
    assert!(!rendered.contains("secret-token"), "{rendered}");
    assert!(!rendered.contains("very-secret"), "{rendered}");
    assert!(rendered.contains("***"));
    assert!(rendered.contains("api.mercadopago.com"));
  }
}
