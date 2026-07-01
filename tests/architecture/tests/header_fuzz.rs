//! Forged client identity header fuzz vectors (catalog K4 / maturity §3.2).

use axum::body::Body;
use axum::extract::Request;
use sdkwork_web_core::{
    DefaultWebRequestContextResolver, WebCallInterceptorChain, WebCallRuntime,
    WebFrameworkErrorKind,
};
use sdkwork_web_test_utils::fixtures;
use std::fs;
use std::path::PathBuf;

fn forged_header_vectors() -> Vec<(String, String)> {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("vectors")
        .join("forged-identity-headers.json");
    let raw = fs::read_to_string(path).expect("read forged header vector");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse vector");
    value["vectors"]
        .as_array()
        .expect("vectors")
        .iter()
        .map(|entry| {
            (
                entry["header"].as_str().expect("header").to_owned(),
                entry["value"].as_str().expect("value").to_owned(),
            )
        })
        .collect()
}

fn protected_dual_token_request(header: &str, value: &str) -> Request<Body> {
    Request::builder()
        .method("GET")
        .uri(fixtures::app_api_path())
        .header(
            "Authorization",
            format!("Bearer {}", fixtures::auth_token()),
        )
        .header("Access-Token", fixtures::access_token())
        .header(header, value)
        .body(Body::empty())
        .expect("request")
}

#[tokio::test]
async fn forged_identity_headers_are_rejected_by_pipeline() {
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default());
    let chain = WebCallInterceptorChain::standard();
    for (header, value) in forged_header_vectors() {
        let mut request = protected_dual_token_request(&header, &value);
        let mut state = sdkwork_web_core::WebCallState::from_request(&request);
        let error = chain
            .before(&mut state, &mut request, &runtime)
            .await
            .expect_err(&format!("forged header {header} must be rejected"));
        assert_eq!(
            WebFrameworkErrorKind::BadRequest,
            error.kind,
            "header {header}"
        );
    }
}

#[tokio::test]
async fn production_cors_policy_rejects_allow_all_origins() {
    use sdkwork_web_core::CorsPolicy;
    let policy = CorsPolicy {
        allow_all_origins: true,
        allowed_origins: vec![],
        allowed_methods: vec![],
        allowed_headers: vec![],
        allow_credentials: false,
    };
    let error = policy
        .validate_for_production()
        .expect_err("unsafe cors must fail production validation");
    assert!(error.contains("allow_all_origins"));
}
