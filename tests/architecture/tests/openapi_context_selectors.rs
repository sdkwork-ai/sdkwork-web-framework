//! Committed OpenAPI authority must not expose client context selector inputs (TEST_SPEC / API_SPEC §10).

use sdkwork_web_contract::validate_openapi_document_context_selectors;
use std::fs;
use std::path::PathBuf;

#[test]
fn committed_openapi_has_no_client_context_selectors() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("apis")
        .join("backend-api")
        .join("web-framework")
        .join("openapi.json");
    let raw = fs::read_to_string(&path)
        .unwrap_or_else(|error| panic!("read {}: {error}", path.display()));
    let document: serde_json::Value =
        serde_json::from_str(&raw).unwrap_or_else(|error| panic!("parse openapi.json: {error}"));
    validate_openapi_document_context_selectors(&document).unwrap_or_else(|error| {
        panic!(
            "apis/backend-api/web-framework/openapi.json violates context selector rules: {error}"
        )
    });
}
