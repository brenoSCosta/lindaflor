use topcoat::{
  Result,
  router::{
    content::{Html, Json},
    error::not_found,
    route,
  },
};
use utoipa::OpenApi;

use crate::api::{self, HealthResponse};
use crate::auth::routes::{
  AccountJson, AccountsOkStatus, AdminOkStatus, AdminSessionJson,
  AdminUpdateUserBody, AdminUserResponse, AuthSessionJson, AuthUserJson,
  BackupCodesResponse, BanUserBody, ChangeEmailBody, ChangeEmailOkStatus,
  ChangePasswordBody, ChangePasswordOkStatus, ConfirmChangeEmailBody,
  DeleteUserBody, DeleteUserOkStatus, DisableTwoFactorBody,
  EnableTwoFactorResponse, GenerateBackupCodesBody, ImpersonateUserBody,
  LinkSocialBody, ListUserSessionsResponse, ListUsersResponse,
  ListedSessionJson, RequestPasswordResetBody, ResetOkStatus,
  ResetPasswordBody, RevokeSessionBody, RevokeUserSessionBody,
  SendVerificationEmailBody, SessionPayload, SessionsOkStatus, SetRoleBody,
  SignInEmailBody, SignInSocialBody, SignInSocialResponse, SignOutResponse,
  SignUpEmailBody, SocialNotImplementedBody, TwoFactorOkStatus,
  TwoFactorRequiredBody, UnlinkAccountBody, UpdateUserBody, UpdateUserResponse,
  UserIdBody, VerifyBackupCodeBody, VerifyEmailBody, VerifyOkStatus,
  VerifyTotpBody, accounts, admin, change_email, change_password, delete_user,
  email_verification, logout, oauth, password_reset, session, sessions,
  sign_in, sign_up, two_factor, update_user,
};
use crate::config::openapi_docs_enabled;

fn require_openapi_docs() -> Result<()> {
  if openapi_docs_enabled() {
    Ok(())
  } else {
    Err(not_found().into())
  }
}

#[derive(OpenApi)]
#[openapi(
    info(
        title = "Linda Flor API",
        version = "0.1.0",
        description = "HTTP API for Linda Flor"
    ),
    paths(
        api::health,
        sign_up::sign_up_email,
        sign_in::sign_in_email,
        sign_in::sign_in_social,
        oauth::google_callback,
        session::get_session,
        logout::sign_out,
        password_reset::request_password_reset,
        password_reset::reset_password,
        email_verification::send_verification_email,
        email_verification::verify_email,
        email_verification::verify_email_post,
        change_email::change_email,
        change_email::confirm_change_email,
        change_password::change_password,
        delete_user::delete_user,
        update_user::update_user,
        sessions::list_sessions,
        sessions::revoke_session,
        sessions::revoke_other_sessions,
        accounts::list_accounts,
        accounts::link_social,
        accounts::unlink_account,
        admin::list_users,
        admin::set_role,
        admin::update_user,
        admin::ban_user,
        admin::unban_user,
        admin::remove_user,
        admin::list_user_sessions,
        admin::revoke_user_session,
        admin::revoke_user_sessions,
        admin::impersonate_user,
        admin::stop_impersonating,
        two_factor::enable,
        two_factor::verify_totp,
        two_factor::disable,
        two_factor::verify_backup_code,
        two_factor::generate_backup_codes_route
    ),
    components(schemas(
        HealthResponse,
        SessionPayload,
        AuthUserJson,
        AuthSessionJson,
        SignUpEmailBody,
        SignInEmailBody,
        SignInSocialBody,
        SignInSocialResponse,
        SocialNotImplementedBody,
        TwoFactorRequiredBody,
        SignOutResponse,
        RequestPasswordResetBody,
        ResetPasswordBody,
        ResetOkStatus,
        SendVerificationEmailBody,
        VerifyEmailBody,
        VerifyOkStatus,
        ChangeEmailBody,
        ConfirmChangeEmailBody,
        ChangeEmailOkStatus,
        ChangePasswordBody,
        ChangePasswordOkStatus,
        DeleteUserBody,
        DeleteUserOkStatus,
        UpdateUserBody,
        UpdateUserResponse,
        ListedSessionJson,
        RevokeSessionBody,
        SessionsOkStatus,
        AccountJson,
        LinkSocialBody,
        UnlinkAccountBody,
        AccountsOkStatus,
        ListUsersResponse,
        ListUserSessionsResponse,
        AdminUserResponse,
        AdminSessionJson,
        AdminOkStatus,
        SetRoleBody,
        AdminUpdateUserBody,
        BanUserBody,
        UserIdBody,
        RevokeUserSessionBody,
        ImpersonateUserBody,
        EnableTwoFactorResponse,
        VerifyTotpBody,
        DisableTwoFactorBody,
        VerifyBackupCodeBody,
        GenerateBackupCodesBody,
        BackupCodesResponse,
        TwoFactorOkStatus
    )),
    tags(
        (name = "system", description = "System health and metadata"),
        (name = "auth", description = "Authentication; see /api/auth/* routes")
    )
)]
struct ApiDoc;

/// OpenAPI 3 document for the HTTP API.
///
/// Served only when `APP_ENV` is development.
#[route(GET "/api/openapi.json")]
pub async fn openapi_json() -> Result<Json<utoipa::openapi::OpenApi>> {
  require_openapi_docs()?;
  Ok(Json(ApiDoc::openapi()))
}

/// Scalar API reference UI.
///
/// Served only when `APP_ENV` is development.
#[route(GET "/api/docs")]
pub async fn scalar_docs() -> Result<Html<&'static str>> {
  require_openapi_docs()?;
  Ok(Html(SCALAR_HTML))
}

/// Swagger UI API reference.
///
/// Served only when `APP_ENV` is development.
#[route(GET "/api/swagger")]
pub async fn swagger_docs() -> Result<Html<&'static str>> {
  require_openapi_docs()?;
  Ok(Html(SWAGGER_HTML))
}

const SCALAR_HTML: &str = r##"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Linda Flor API — Scalar</title>
  </head>
  <body>
    <div id="app"></div>
    <script src="https://cdn.jsdelivr.net/npm/@scalar/api-reference"></script>
    <script>
      Scalar.createApiReference("#app", {
        url: "/api/openapi.json",
      });
    </script>
  </body>
</html>
"##;

const SWAGGER_HTML: &str = r##"<!doctype html>
<html lang="en">
  <head>
    <meta charset="utf-8" />
    <meta name="viewport" content="width=device-width, initial-scale=1.0" />
    <title>Linda Flor API — Swagger</title>
    <link
      rel="stylesheet"
      href="https://unpkg.com/swagger-ui-dist@5.17.14/swagger-ui.css"
    />
  </head>
  <body>
    <div id="swagger-ui"></div>
    <script src="https://unpkg.com/swagger-ui-dist@5.17.14/swagger-ui-bundle.js"></script>
    <script>
      window.onload = () => {
        window.ui = SwaggerUIBundle({
          url: "/api/openapi.json",
          dom_id: "#swagger-ui",
          deepLinking: true,
          presets: [SwaggerUIBundle.presets.apis, SwaggerUIBundle.presets.standalone],
          plugins: [SwaggerUIBundle.plugins.DownloadUrl],
        });
      };
    </script>
  </body>
</html>
"##;
