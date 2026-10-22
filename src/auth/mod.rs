pub mod avatar;
pub mod google;
pub mod password;
pub mod routes;
pub mod service;
pub mod session_config;
pub mod session_store;
pub mod step_up;
pub mod totp;
pub mod totp_crypto;
pub mod user;
pub mod validation;

pub use password::{hash_password, verify_password};
pub use session_config::session_config;
pub use user::{
  SessionUser, User, current_user, current_user_owned, is_currently_banned,
};

/// Credential provider id (email/password accounts).
pub const CREDENTIAL_PROVIDER_ID: &str = "credential";
