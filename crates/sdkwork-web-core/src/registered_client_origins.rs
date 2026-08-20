//! Canonical SDKWork desktop and mini program CORS origins.
//!
//! These origins are always merged into environment-derived CORS policies so
//! registered desktop WebView shells and WeChat mini program runtimes keep
//! working even when deployment env files omit them from
//! `SDKWORK_CORS_ALLOWED_ORIGINS`.

/// Registered desktop WebView custom-scheme origins (`WEB_FRAMEWORK_SPEC` §12).
pub const REGISTERED_SDKWORK_DESKTOP_CORS_ORIGINS: &[&str] = &[
    "app://dsh",
    "app://birdcoder",
    "app://sdkwork",
    "app://dtupay",
    "tauri://localhost",
];

/// Registered mini program runtime origins that emit browser-style CORS requests.
pub const REGISTERED_SDKWORK_MINI_PROGRAM_CORS_ORIGINS: &[&str] = &["https://servicewechat.com"];

/// Returns every registered SDKWork client origin in stable order.
pub fn registered_sdkwork_client_cors_origins() -> impl Iterator<Item = &'static str> {
    REGISTERED_SDKWORK_DESKTOP_CORS_ORIGINS
        .iter()
        .copied()
        .chain(REGISTERED_SDKWORK_MINI_PROGRAM_CORS_ORIGINS.iter().copied())
}

/// Appends registered SDKWork client origins that are not already present.
pub fn merge_registered_sdkwork_client_origins(allowed_origins: &mut Vec<String>) {
    for origin in registered_sdkwork_client_cors_origins() {
        if !allowed_origins.iter().any(|allowed| allowed == origin) {
            allowed_origins.push(origin.to_owned());
        }
    }
}

/// Returns whether an Origin value matches a registered SDKWork desktop or mini
/// program client runtime.
pub fn is_registered_sdkwork_client_origin(origin: &str) -> bool {
    registered_sdkwork_client_cors_origins().any(|allowed| allowed == origin)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn merge_is_idempotent_and_preserves_existing_entries() {
        let mut origins = vec!["app://dsh".to_owned(), "https://console.example.com".to_owned()];
        merge_registered_sdkwork_client_origins(&mut origins);
        merge_registered_sdkwork_client_origins(&mut origins);

        assert!(origins.contains(&"app://dsh".to_owned()));
        assert!(origins.contains(&"app://birdcoder".to_owned()));
        assert!(origins.contains(&"https://servicewechat.com".to_owned()));
        assert!(origins.contains(&"https://console.example.com".to_owned()));
        assert_eq!(
            origins
                .iter()
                .filter(|origin| origin.as_str() == "app://dsh")
                .count(),
            1
        );
    }
}
