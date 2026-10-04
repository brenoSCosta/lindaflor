pub mod avatar;
pub mod google;
pub mod password;
pub mod routes;
pub mod service;
pub mod session_config;
pub mod session_store;
pub mod totp;
pub mod user;

pub use password::{hash_password, verify_password};
pub use session_config::session_config;
pub use user::{SessionUser, User, current_user, current_user_owned};

/// Credential provider id (email/password accounts).
pub const CREDENTIAL_PROVIDER_ID: &str = "credential";
