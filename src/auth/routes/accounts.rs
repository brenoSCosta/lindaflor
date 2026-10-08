use http::StatusCode;
use serde::{Deserialize, Serialize};
use sqlx::PgPool;
use topcoat::{
  Result,
  context::{Cx, app_context},
  router::{
    content::Json,
    error::{bad_request, unauthorized},
    route,
  },
  session,
};

use crate::auth::CREDENTIAL_PROVIDER_ID;
use crate::auth::google::{
  GOOGLE_PROVIDER_ID, GoogleOAuthConfig, safe_callback_path,
};
use crate::auth::routes::dto::{format_primitive, random_token};
use crate::auth::routes::oauth::{
  NewOAuthState, OAUTH_STATE_TTL_SECS, google_authorize_url,
  pkce_s256_challenge, store_oauth_state_full,
};
use crate::auth::routes::sign_in::{
  SignInSocialResponse, SocialNotImplementedBody,
};
use crate::auth::user::current_user;

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct AccountJson {
  pub id: String,
  pub provider_id: String,
  pub account_id: String,
  pub created_at: String,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct LinkSocialBody {
  pub provider: String,
  pub callback_url: Option<String>,
}

#[derive(Debug, Deserialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct UnlinkAccountBody {
  pub provider_id: String,
  pub account_id: String,
}

#[derive(Debug, Serialize, utoipa::ToSchema)]
#[serde(rename_all = "camelCase")]
pub struct OkStatus {
  pub status: bool,
}

#[utoipa::path(
    get,
    path = "/api/auth/list-accounts",
    tag = "auth",
    responses(
        (status = 200, description = "Linked accounts (no passwords)", body = Vec<AccountJson>),
        (status = 401, description = "Missing session")
    )
)]
#[route(GET "/api/auth/list-accounts")]
pub async fn list_accounts(cx: &Cx) -> Result<Json<Vec<AccountJson>>> {
  let session = current_user(cx)
    .await
    .map_err(|e| topcoat::Error::from(std::io::Error::other(e.to_string())))?;
  let Some(su) = session.as_ref() else {
    return Err(unauthorized().into());
  };

  let pool = app_context::<PgPool>(cx);
  let rows = sqlx::query!(
    r#"
        SELECT id, provider_id, account_id, created_at
        FROM accounts
        WHERE user_id = $1
        ORDER BY created_at ASC
        "#,
    su.user.id,
  )
  .fetch_all(pool)
  .await?;

  let list = rows
    .into_iter()
    .map(|r| AccountJson {
      id: r.id.to_string(),
      provider_id: r.provider_id,
      account_id: r.account_id,
      created_at: format_primitive(r.created_at),
    })
    .collect();

  Ok(Json(list))
}

#[utoipa::path(
    post,
    path = "/api/auth/link-social",
    tag = "auth",
    request_body = LinkSocialBody,
    responses(
        (status = 200, description = "OAuth authorize URL for linking", body = SignInSocialResponse),
        (status = 400, description = "Unsupported provider", body = SocialNotImplementedBody),
        (status = 401, description = "Missing session"),
        (status = 501, description = "Social linking not configured", body = SocialNotImplementedBody)
    )
)]
#[route(POST "/api/auth/link-social")]
pub async fn link_social(
  cx: &Cx,
  Json(body): Json<LinkSocialBody>,
) -> Result<(StatusCode, Json<serde_json::Value>)> {
  let session = current_user(cx)
    .await
    .map_err(|e| topcoat::Error::from(std::io::Error::other(e.to_string())))?;
  let Some(su) = session.as_ref() else {
    return Err(unauthorized().into());
  };

  let provider = body.provider.trim().to_ascii_lowercase();
  if provider != GOOGLE_PROVIDER_ID {
    return Ok((
      StatusCode::BAD_REQUEST,
      Json(serde_json::to_value(SocialNotImplementedBody {
        code: "SOCIAL_SIGN_IN_UNAVAILABLE",
        message: format!("social provider '{provider}' is not supported"),
      })?),
    ));
  }

  let Some(config) = GoogleOAuthConfig::from_env() else {
    let has_google_creds = std::env::var("GOOGLE_CLIENT_ID")
      .map(|v| !v.trim().is_empty())
      .unwrap_or(false)
      && std::env::var("GOOGLE_CLIENT_SECRET")
        .map(|v| !v.trim().is_empty())
        .unwrap_or(false);
    let message = if has_google_creds {
      "Google sign-in is not configured (set APP_ORIGIN, WEB_ORIGIN, or BETTER_AUTH_URL)"
                .to_string()
    } else {
      "Google sign-in is not configured (set GOOGLE_CLIENT_ID and GOOGLE_CLIENT_SECRET)"
                .to_string()
    };
    return Ok((
      StatusCode::NOT_IMPLEMENTED,
      Json(serde_json::to_value(SocialNotImplementedBody {
        code: "SOCIAL_SIGN_IN_UNAVAILABLE",
        message,
      })?),
    ));
  };

  let callback_path = safe_callback_path(body.callback_url.as_deref());

  // Bind the OAuth state to the session that starts the link flow.
  // The hex session-token hash is stored server-side only (never embedded
  // in `state`); the callback rejects any session that does not match.
  let session_binding = session::token_hash(cx)
    .await?
    .map(|hash| crate::auth::session_store::token_hash_hex(&hash));
  let Some(session_binding) = session_binding else {
    return Err(unauthorized().into());
  };

  // PKCE (S256 `code_challenge`) + OIDC `nonce`. The verifier stays
  // server-side in the state row; only the challenge travels in the URL.
  let code_verifier = random_token();
  let nonce = random_token();
  let code_challenge = pkce_s256_challenge(&code_verifier);

  let state = random_token();
  let pool = app_context::<PgPool>(cx);
  store_oauth_state_full(
    pool,
    NewOAuthState {
      state: &state,
      callback_path: &callback_path,
      ttl_secs: OAUTH_STATE_TTL_SECS,
      link_user_id: Some(su.user.id),
      session_binding: Some(&session_binding),
      code_verifier: Some(&code_verifier),
      nonce: Some(&nonce),
    },
  )
  .await?;

  let url =
    google_authorize_url(&config, &state, Some(&code_challenge), Some(&nonce));
  Ok((
    StatusCode::OK,
    Json(serde_json::to_value(SignInSocialResponse { url })?),
  ))
}

#[utoipa::path(
    post,
    path = "/api/auth/unlink-account",
    tag = "auth",
    request_body = UnlinkAccountBody,
    responses(
        (status = 200, description = "Account unlinked", body = OkStatus),
        (status = 400, description = "Cannot unlink last login method"),
        (status = 401, description = "Missing session")
    )
)]
#[route(POST "/api/auth/unlink-account")]
pub async fn unlink_account(
  cx: &Cx,
  Json(body): Json<UnlinkAccountBody>,
) -> Result<Json<OkStatus>> {
  let session = current_user(cx)
    .await
    .map_err(|e| topcoat::Error::from(std::io::Error::other(e.to_string())))?;
  let Some(su) = session.as_ref() else {
    return Err(unauthorized().into());
  };

  let provider_id = body.provider_id.trim();
  let account_id = body.account_id.trim();
  if provider_id.is_empty() || account_id.is_empty() {
    return Err(bad_request("providerId and accountId are required").into());
  }

  let pool = app_context::<PgPool>(cx);

  let target = sqlx::query!(
    r#"
        SELECT id, provider_id, password
        FROM accounts
        WHERE user_id = $1 AND provider_id = $2 AND account_id = $3
        "#,
    su.user.id,
    provider_id,
    account_id,
  )
  .fetch_optional(pool)
  .await?;

  let Some(target) = target else {
    return Err(bad_request("account not found").into());
  };

  // Remaining login methods after this unlink must be non-empty.
  let others = sqlx::query!(
    r#"
        SELECT provider_id, password
        FROM accounts
        WHERE user_id = $1 AND id <> $2
        "#,
    su.user.id,
    target.id,
  )
  .fetch_all(pool)
  .await?;

  let has_other_login = others.iter().any(|a| {
    if a.provider_id == CREDENTIAL_PROVIDER_ID {
      a.password.as_deref().is_some_and(|p| !p.is_empty())
    } else {
      true
    }
  });

  if !has_other_login {
    return Err(
      bad_request("cannot unlink the only remaining login method").into(),
    );
  }

  sqlx::query!("DELETE FROM accounts WHERE id = $1", target.id)
    .execute(pool)
    .await?;

  Ok(Json(OkStatus { status: true }))
}
