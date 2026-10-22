//! Production admin bootstrap (`create-admin` binary): create an admin with
//! a verified email, or promote an existing user. Uses the same email/name
//! validation, password policy and Argon2id hashing as sign-up.

use sqlx::PgPool;
use uuid::Uuid;

use crate::audit::{AuditEvent, record_audit_event};
use crate::auth::CREDENTIAL_PROVIDER_ID;
use crate::auth::password::{PasswordError, hash_password, validate_password};
use crate::auth::validation::{validate_email, validate_name};

/// `actor_name` on the audit trail for bootstrap writes (no session actor).
pub const BOOTSTRAP_ACTOR: &str = "create-admin";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminBootstrapOutcome {
  Created(Uuid),
  Promoted(Uuid),
}

impl AdminBootstrapOutcome {
  pub fn user_id(self) -> Uuid {
    match self {
      Self::Created(id) | Self::Promoted(id) => id,
    }
  }
}

#[derive(Debug, thiserror::Error)]
pub enum AdminBootstrapError {
  #[error("invalid email: {0}")]
  Email(String),
  #[error("invalid name: {0}")]
  Name(String),
  #[error(transparent)]
  Password(#[from] PasswordError),
  #[error("no user with this email: a password is required to create one")]
  PasswordRequired,
  #[error("database error: {0}")]
  Db(#[from] sqlx::Error),
}

/// Create `email` as a verified admin, or promote the existing user with that
/// email (role `admin`, `email_verified = true`).
///
/// `password`, when given, must pass [`validate_password`]; it becomes the
/// credential password (replacing an existing one on promotion, so this
/// doubles as admin password recovery). Creating a new user requires it.
/// `name` is only used for new users. Recorded in `audit_events`.
pub async fn create_or_promote_admin(
  pool: &PgPool,
  email: &str,
  name: &str,
  password: Option<&str>,
) -> Result<AdminBootstrapOutcome, AdminBootstrapError> {
  let email = validate_email(email)
    .map_err(|e| AdminBootstrapError::Email(e.to_string()))?;
  let name = validate_name(name)
    .map_err(|e| AdminBootstrapError::Name(e.to_string()))?;
  let password_hash = match password {
    Some(password) => {
      validate_password(password, &[&email, &name, "lindaflor"])?;
      Some(hash_password(password)?)
    }
    None => None,
  };

  let mut tx = pool.begin().await?;
  let existing: Option<Uuid> =
    sqlx::query_scalar("SELECT id FROM users WHERE email = $1 FOR UPDATE")
      .bind(&email)
      .fetch_optional(&mut *tx)
      .await?;

  let outcome = match existing {
    Some(user_id) => {
      sqlx::query(
        "UPDATE users SET role = 'admin', email_verified = true, updated_at = now()
         WHERE id = $1",
      )
      .bind(user_id)
      .execute(&mut *tx)
      .await?;
      AdminBootstrapOutcome::Promoted(user_id)
    }
    None => {
      if password_hash.is_none() {
        return Err(AdminBootstrapError::PasswordRequired);
      }
      let user_id = Uuid::now_v7();
      sqlx::query(
        "INSERT INTO users (id, name, email, email_verified, role, created_at, updated_at)
         VALUES ($1, $2, $3, true, 'admin', now(), now())",
      )
      .bind(user_id)
      .bind(&name)
      .bind(&email)
      .execute(&mut *tx)
      .await?;
      AdminBootstrapOutcome::Created(user_id)
    }
  };
  let user_id = outcome.user_id();

  if let Some(password_hash) = &password_hash {
    let updated = sqlx::query(
      "UPDATE accounts SET password = $2, updated_at = now()
       WHERE user_id = $1 AND provider_id = $3",
    )
    .bind(user_id)
    .bind(password_hash)
    .bind(CREDENTIAL_PROVIDER_ID)
    .execute(&mut *tx)
    .await?
    .rows_affected();
    if updated == 0 {
      sqlx::query(
        "INSERT INTO accounts (id, account_id, provider_id, user_id, password, created_at, updated_at)
         VALUES ($1, $2, $3, $4, $5, now(), now())",
      )
      .bind(Uuid::now_v7())
      .bind(user_id.to_string())
      .bind(CREDENTIAL_PROVIDER_ID)
      .bind(user_id)
      .bind(password_hash)
      .execute(&mut *tx)
      .await?;
    }
  }

  record_audit_event(
    &mut *tx,
    AuditEvent {
      aggregate_type: "user",
      aggregate_id: &user_id.to_string(),
      entity_type: "user",
      entity_id: user_id,
      actor_id: None,
      actor_name: Some(BOOTSTRAP_ACTOR),
      metadata: serde_json::json!({
        "action": "bootstrap_admin",
        "created": matches!(outcome, AdminBootstrapOutcome::Created(_)),
        "password_set": password_hash.is_some(),
      }),
    },
  )
  .await?;

  tx.commit().await?;
  Ok(outcome)
}

#[cfg(test)]
mod tests {
  use super::*;
  use crate::auth::verify_password;

  const STRONG: &str = "correct horse battery staple 42!";

  fn unique_email(prefix: &str) -> String {
    format!("{prefix}-{}@example.com", Uuid::now_v7().simple())
  }

  async fn user_row(pool: &PgPool, id: Uuid) -> (Option<String>, bool) {
    sqlx::query_as("SELECT role, email_verified FROM users WHERE id = $1")
      .bind(id)
      .fetch_one(pool)
      .await
      .expect("user row")
  }

  async fn credential_hash(pool: &PgPool, id: Uuid) -> Option<String> {
    sqlx::query_scalar(
      "SELECT password FROM accounts WHERE user_id = $1 AND provider_id = $2",
    )
    .bind(id)
    .bind(CREDENTIAL_PROVIDER_ID)
    .fetch_optional(pool)
    .await
    .expect("account row")
    .flatten()
  }

  #[tokio::test]
  async fn creates_verified_admin_with_credential() {
    let pool = crate::test_support::fresh_pool().await;
    let email = unique_email("Boot");

    let outcome = create_or_promote_admin(
      &pool,
      &email.to_uppercase(),
      "Dona",
      Some(STRONG),
    )
    .await
    .expect("create");
    let AdminBootstrapOutcome::Created(id) = outcome else {
      panic!("expected Created, got {outcome:?}");
    };
    assert_eq!(user_row(&pool, id).await, (Some("admin".to_string()), true));
    let stored_email: String =
      sqlx::query_scalar("SELECT email FROM users WHERE id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(stored_email, email.to_lowercase());
    let hash = credential_hash(&pool, id).await.expect("credential");
    assert!(verify_password(STRONG, &hash).unwrap());

    let audited: i64 = sqlx::query_scalar(
      "SELECT COUNT(*) FROM audit_events WHERE entity_id = $1 AND actor_name = $2",
    )
    .bind(id)
    .bind(BOOTSTRAP_ACTOR)
    .fetch_one(&pool)
    .await
    .unwrap();
    assert_eq!(audited, 1);
  }

  #[tokio::test]
  async fn promotes_existing_user_and_optionally_resets_password() {
    let pool = crate::test_support::fresh_pool().await;
    let email = unique_email("promote");
    let id = Uuid::now_v7();
    sqlx::query(
      "INSERT INTO users (id, name, email, email_verified, role) VALUES ($1, 'Cliente', $2, false, 'user')",
    )
    .bind(id)
    .bind(&email)
    .execute(&pool)
    .await
    .unwrap();

    // Promotion without a password leaves credentials alone.
    let outcome = create_or_promote_admin(&pool, &email, "Ignorado", None)
      .await
      .expect("promote");
    assert_eq!(outcome, AdminBootstrapOutcome::Promoted(id));
    assert_eq!(user_row(&pool, id).await, (Some("admin".to_string()), true));
    assert!(credential_hash(&pool, id).await.is_none());

    // With a password it adds (then replaces) the credential.
    create_or_promote_admin(&pool, &email, "Ignorado", Some(STRONG))
      .await
      .expect("set password");
    let first = credential_hash(&pool, id).await.expect("credential");
    assert!(verify_password(STRONG, &first).unwrap());
    let other = "another quite long passphrase 99?";
    create_or_promote_admin(&pool, &email, "Ignorado", Some(other))
      .await
      .expect("reset password");
    let second = credential_hash(&pool, id).await.expect("credential");
    assert!(verify_password(other, &second).unwrap());
    let accounts: i64 =
      sqlx::query_scalar("SELECT COUNT(*) FROM accounts WHERE user_id = $1")
        .bind(id)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(accounts, 1);
  }

  #[tokio::test]
  async fn rejects_weak_password_bad_email_and_missing_password() {
    let pool = crate::test_support::fresh_pool().await;
    let email = unique_email("reject");

    assert!(matches!(
      create_or_promote_admin(&pool, &email, "Dona", Some("admin123")).await,
      Err(AdminBootstrapError::Password(_))
    ));
    assert!(matches!(
      create_or_promote_admin(&pool, "not-an-email", "Dona", Some(STRONG))
        .await,
      Err(AdminBootstrapError::Email(_))
    ));
    assert!(matches!(
      create_or_promote_admin(&pool, &email, "Dona", None).await,
      Err(AdminBootstrapError::PasswordRequired)
    ));
    let exists: bool =
      sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM users WHERE email = $1)")
        .bind(&email)
        .fetch_one(&pool)
        .await
        .unwrap();
    assert!(!exists);
  }
}
