//! Ensures framework route handlers follow WEB_FRAMEWORK_STANDARD §7 (no raw credential header parsing).

use std::fs;
use std::path::PathBuf;

const ROUTE_HANDLER_FILES: &[&str] =
    &["../../crates/sdkwork-routes-web-framework-backend-api/src/handlers.rs"];

const FORBIDDEN_HANDLER_PATTERNS: &[&str] = &[
    "headers().get(\"authorization\")",
    "headers().get(\"access-token\")",
    "headers().get(\"x-api-key\")",
    "HeaderMap",
    "Authorization:",
    "Access-Token:",
    "X-Api-Key",
    "sqlx::",
    "sqlx::query",
    "SELECT ",
    "INSERT ",
    "UPDATE ",
    "DELETE FROM",
];

#[test]
fn framework_route_handlers_do_not_parse_credential_headers() {
    for relative in ROUTE_HANDLER_FILES {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let lowered = source.to_ascii_lowercase();
        for pattern in FORBIDDEN_HANDLER_PATTERNS {
            assert!(
                !lowered.contains(&pattern.to_ascii_lowercase()),
                "handler file {} must not parse credential headers directly (found {pattern})",
                path.display()
            );
        }
        assert!(
            source.contains("WebRequestContext"),
            "handler file {} must declare WebRequestContext parameters",
            path.display()
        );
        assert!(
            source.contains("finish_api_json") && source.contains("finish_api_response"),
            "handler file {} must finish errors via finish_api_json/finish_api_response for Problem correlation",
            path.display()
        );
    }
}
