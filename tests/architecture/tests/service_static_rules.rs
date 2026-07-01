//! Ensures framework services follow WEB_BACKEND_SPEC §2 (no Axum request types in services).

use std::fs;
use std::path::PathBuf;

const SERVICE_FILES: &[&str] = &[
    "../../crates/sdkwork-routes-web-framework-backend-api/src/services/admin_service.rs",
    "../../crates/sdkwork-routes-web-framework-backend-api/src/services/validation.rs",
];

const FORBIDDEN_SERVICE_PATTERNS: &[&str] = &[
    "use axum::",
    "axum::extract::",
    "axum::Json",
    "HeaderMap",
    "IntoResponse",
    "WebRequestContext",
];

const FORBIDDEN_SQL_IN_SERVICE: &[&str] = &["sqlx::query", "sqlx::query_as", "sqlx::query_scalar"];

#[test]
fn framework_services_do_not_depend_on_axum_request_types() {
    for relative in SERVICE_FILES {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join(relative);
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let lowered = source.to_ascii_lowercase();
        for pattern in FORBIDDEN_SERVICE_PATTERNS {
            assert!(
                !lowered.contains(&pattern.to_ascii_lowercase()),
                "service file {} must not depend on Axum request types (found {pattern})",
                path.display()
            );
        }
        if relative.contains("admin_service.rs") {
            for pattern in FORBIDDEN_SQL_IN_SERVICE {
                assert!(
                    !lowered.contains(&pattern.to_ascii_lowercase()),
                    "admin service {} must not execute SQL directly (found {pattern})",
                    path.display()
                );
            }
        }
    }
}
