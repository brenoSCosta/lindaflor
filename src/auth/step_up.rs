//! Re-authentication ("step-up") for credential-changing actions, plus the
//! shared per-user TOTP guard (failure cap + replay reservation).
//!
//! A session cookie alone must not be enough to change the e-mail, enroll
//! or remove a second factor, or add/remove a login method: each of those
//! calls [`verify_step_up`] with the current password or a TOTP code. None
//! of them may run from an admin impersonation session
//! ([`forbid_impersonated`]).

use sqlx::PgPool;
use topcoat::{
  Result,
  context::Cx,
  router::error::{bad_request, forbidden, unauthorized},
};
use uuid::Uuid;

use crate::auth::CREDENTIAL_PROVIDER_ID;
use crate::auth::password::verify_password;
use crate::auth::totp::verify_code;
use crate::auth::totp_crypto::decrypt_row_secret;
use crate::auth::user::SessionUser;

/// pt-BR message for HTML callers refused by [`forbid_impersonated`].
pub const MSG_IMPERSONATING: &str =
  "Esta ação não está disponível enquanto você atua como outra pessoa.";

/// Credential-changing actions are never allowed from an impersonated
/// session: an admin acting as a customer must not be able to plant a login
/// method (link Google, enroll TOTP, change the e-mail) that outlives
/// "Encerrar".
pub fn forbid_impersonated(su: &SessionUser) -> Result<()> {
  if su.impersonated_by.is_some() {
    return Err(forbidden().into());
  }
  Ok(())
}

/// Per-IP budget ([`crate::rate_limit::SCOPE_STEP_UP`], 10/min by default)
/// for password/TOTP re-checks on authenticated actions. Each password
/// attempt is a full Argon2 verify, so an unthrottled session could use
/// these endpoints as a CPU amplifier or a TOTP guessing oracle.
pub fn check_step_up_rate(cx: &Cx) -> Result<()> {
  use crate::rate_limit::{
    DEFAULT_STEP_UP_PER_MIN, SCOPE_STEP_UP, check_ip_rate, limit_for,
  };
  if check_ip_rate(
    SCOPE_STEP_UP,
    &crate::valkey::client_ip(cx),
    limit_for(SCOPE_STEP_UP, DEFAULT_STEP_UP_PER_MIN),
  ) {
    Ok(())
  } else {
    Err(crate::rate_limit::rate_limit_error())
  }
}

/// Refuse when `user_id` already burned its second-factor budget
/// ([`crate::valkey::TOTP_FAIL_MAX`]). Call before any TOTP/backup-code work.
pub async fn ensure_totp_not_locked(cx: &Cx, user_id: Uuid) -> Result<()> {
  if crate::valkey::is_totp_locked_out(crate::valkey::conn_from(cx), &user_id)
    .await
  {
    return Err(crate::rate_limit::rate_limit_error());
  }
  Ok(())
}

/// Count one wrong TOTP/backup code against `user_id`.
pub async fn record_totp_failure(cx: &Cx, user_id: Uuid) {
  crate::valkey::record_totp_fail(crate::valkey::conn_from(cx), &user_id).await;
}

/// Verify `code` against `secret` for `user_id` with the per-user failure
/// cap and the replay guard.
///
/// `Ok(false)` for a wrong or already-used code, `Err(429)` while the user
/// is locked out. A successful code is reserved, so it cannot be presented
/// again inside its validity window on any path.
pub async fn check_totp_code(
  cx: &Cx,
  user_id: Uuid,
  secret: &str,
  code: &str,
) -> Result<bool> {
  ensure_totp_not_locked(cx, user_id).await?;
  if !verify_code(secret, code).map_err(bad_request)? {
    record_totp_failure(cx, user_id).await;
    return Ok(false);
  }
  Ok(
    crate::valkey::totp_reserve(crate::valkey::conn_from(cx), &user_id, code)
      .await,
  )
}

/// Which re-authentication factors an account can present.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct StepUpFactors {
  pub password: bool,
  pub totp: bool,
}

impl StepUpFactors {
  /// OAuth-only accounts without 2FA have nothing to re-check beyond the
  /// session.
  pub fn none(self) -> bool {
    !self.password && !self.totp
  }
}

pub async fn step_up_factors(
  pool: &PgPool,
  user_id: Uuid,
) -> Result<StepUpFactors> {
  let password = sqlx::query_scalar!(
    r#"SELECT password FROM accounts WHERE user_id = $1 AND provider_id = $2"#,
    user_id,
    CREDENTIAL_PROVIDER_ID,
  )
  .fetch_optional(pool)
  .await?
  .flatten()
  .is_some_and(|p| !p.is_empty());
  let totp = sqlx::query_scalar!(
    r#"SELECT verified FROM two_factor WHERE user_id = $1"#,
    user_id,
  )
  .fetch_optional(pool)
  .await?
  .unwrap_or(false);
  Ok(StepUpFactors { password, totp })
}

/// Require the current password (when the account has one) or a current
/// TOTP code (when 2FA is verified). Either factor is accepted when both
/// exist. Accounts with neither (OAuth-only, no 2FA) pass on the session.
///
/// Errors are `401` for a missing/wrong factor and `429` while TOTP is
/// locked out.
pub async fn verify_step_up(
  cx: &Cx,
  pool: &PgPool,
  user_id: Uuid,
  password: Option<&str>,
  code: Option<&str>,
) -> Result<()> {
  let factors = step_up_factors(pool, user_id).await?;
  if factors.none() {
    return Ok(());
  }
  check_step_up_rate(cx)?;

  if factors.password
    && let Some(password) = password.filter(|p| !p.is_empty())
  {
    let stored = sqlx::query_scalar!(
      r#"SELECT password FROM accounts WHERE user_id = $1 AND provider_id = $2"#,
      user_id,
      CREDENTIAL_PROVIDER_ID,
    )
    .fetch_optional(pool)
    .await?
    .flatten();
    if let Some(stored) = stored
      && verify_password(password, &stored)?
    {
      return Ok(());
    }
  }

  if factors.totp
    && let Some(code) = code.map(str::trim).filter(|c| !c.is_empty())
  {
    let tf = sqlx::query!(
      r#"SELECT secret_nonce, secret_ciphertext, secret_key_version FROM two_factor WHERE user_id = $1 AND verified = true"#,
      user_id,
    )
    .fetch_optional(pool)
    .await?
    .ok_or_else(unauthorized)?;
    // Fail closed on any decrypt error.
    let secret = decrypt_row_secret(
      tf.secret_nonce,
      tf.secret_ciphertext,
      tf.secret_key_version,
    )
    .map_err(|_| unauthorized())?;
    if check_totp_code(cx, user_id, secret.as_str(), code).await? {
      return Ok(());
    }
  }

  Err(unauthorized().into())
}
