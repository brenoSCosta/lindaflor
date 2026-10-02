//! Better Auth-compatible JSON routes under `/api/auth/`.

pub mod accounts;
pub mod admin;
pub mod change_email;
pub mod change_password;
pub mod delete_user;
pub mod dto;
pub mod email_verification;
pub mod logout;
pub mod oauth;
pub mod password_reset;
pub mod session;
pub mod sessions;
pub mod sign_in;
pub mod sign_up;
pub mod two_factor;
pub mod update_user;

pub use accounts::{
  AccountJson, LinkSocialBody, OkStatus as AccountsOkStatus, UnlinkAccountBody,
  link_social, list_accounts, unlink_account,
};
pub use admin::{
  AdminOkStatus, AdminSessionJson, AdminUpdateUserBody, AdminUserResponse,
  BanUserBody, ImpersonateUserBody, ListUserSessionsResponse,
  ListUsersResponse, RevokeUserSessionBody, SetRoleBody, UserIdBody,
  impersonate_user, stop_impersonating,
};
pub use change_email::{
  ChangeEmailBody, ConfirmChangeEmailBody, OkStatus as ChangeEmailOkStatus,
};
pub use change_password::{
  ChangePasswordBody, OkStatus as ChangePasswordOkStatus,
};
pub use delete_user::{DeleteUserBody, OkStatus as DeleteUserOkStatus};
pub use dto::{AuthSessionJson, AuthUserJson, SessionPayload};
pub use email_verification::{
  OkStatus as VerifyOkStatus, SendVerificationEmailBody, VerifyEmailBody,
  send_verification_email, verify_email, verify_email_post,
};
pub use logout::{SignOutResponse, sign_out};
pub use oauth::google_callback;
pub use password_reset::{
  OkStatus as ResetOkStatus, RequestPasswordResetBody, ResetPasswordBody,
  request_password_reset, reset_password,
};
pub use session::get_session;
pub use sessions::{
  ListedSessionJson, OkStatus as SessionsOkStatus, RevokeSessionBody,
  list_sessions, revoke_other_sessions, revoke_session,
};
pub use sign_in::{
  SignInEmailBody, SignInSocialBody, SignInSocialResponse,
  SocialNotImplementedBody, TwoFactorRequiredBody, sign_in_email,
  sign_in_social,
};
pub use sign_up::{SignUpEmailBody, sign_up_email};
pub use two_factor::{
  BackupCodesResponse, DisableTwoFactorBody, EnableTwoFactorResponse,
  GenerateBackupCodesBody, OkStatus as TwoFactorOkStatus, VerifyBackupCodeBody,
  VerifyTotpBody, disable as two_factor_disable, enable as two_factor_enable,
  generate_backup_codes_route as two_factor_generate_backup_codes,
  verify_backup_code as two_factor_verify_backup_code,
  verify_totp as two_factor_verify_totp,
};
pub use update_user::{UpdateUserBody, UpdateUserResponse};

/// Reference route symbols so inventory discover includes them when linked from tests.
#[doc(hidden)]
pub fn link_for_discover() {
  let _ = (
    sign_up_email,
    sign_in_email,
    sign_in_social,
    get_session,
    sign_out,
    request_password_reset,
    reset_password,
    send_verification_email,
    verify_email,
    verify_email_post,
    change_email::change_email,
    change_email::confirm_change_email,
    change_password::change_password,
    delete_user::delete_user,
    update_user::update_user,
    list_sessions,
    revoke_session,
    revoke_other_sessions,
    list_accounts,
    link_social,
    unlink_account,
    impersonate_user,
    stop_impersonating,
    admin::list_users,
    admin::set_role,
    admin::update_user,
    admin::ban_user,
    admin::unban_user,
    admin::remove_user,
    admin::list_user_sessions,
    admin::revoke_user_session,
    admin::revoke_user_sessions,
    two_factor_enable,
    two_factor_verify_totp,
    two_factor_disable,
    two_factor_verify_backup_code,
    two_factor_generate_backup_codes,
    google_callback,
  );
}
