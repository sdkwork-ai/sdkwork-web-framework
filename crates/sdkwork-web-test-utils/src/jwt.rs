//! Re-exports unsigned JWT helpers from [`sdkwork_web_core::jwt_fixtures`].

pub use sdkwork_web_core::jwt_fixtures::{
    access_token_jwt, auth_token_jwt, auth_token_jwt_with_permissions, bootstrap_access_token_jwt,
    encode_unsigned_test_jwt,
};
