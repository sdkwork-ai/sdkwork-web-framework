//! Java/Rust pipeline stage parity via shared vectors (catalog K7).

use sdkwork_web_core::{WebCallStage, STANDARD_STAGE_ORDER};
use std::fs;
use std::path::PathBuf;

fn vector_path(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("vectors")
        .join(name)
}

#[test]
fn shared_vector_matches_rust_standard_stage_order() {
    let raw = fs::read_to_string(vector_path("pipeline-stage-order.json")).expect("read vector");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse vector");
    let stages = value["stages"].as_array().expect("stages array");
    assert_eq!(18, value["mandatoryStageCount"].as_u64().expect("count"));
    assert_eq!(18, stages.len());
    assert_eq!(18, STANDARD_STAGE_ORDER.len());

    for (index, entry) in stages.iter().enumerate() {
        let rust_name = entry["rust"].as_str().expect("rust stage name");
        let java_name = entry["java"].as_str().expect("java stage name");
        let interceptor = entry["interceptor"].as_str().expect("interceptor name");
        assert_eq!(
            rust_name,
            java_name,
            "stage {} must use identical Rust/Java names",
            index + 1
        );
        assert_eq!(
            rust_name,
            interceptor,
            "stage {} interceptor alias must match",
            index + 1
        );
        let expected = format!("{:?}", STANDARD_STAGE_ORDER[index]);
        assert_eq!(
            rust_name,
            expected,
            "vector stage {} does not match STANDARD_STAGE_ORDER",
            index + 1
        );
        assert_eq!((index + 1) as u64, entry["order"].as_u64().expect("order"));
    }
}

#[test]
fn web_call_stage_enum_covers_all_vector_stages() {
    let raw = fs::read_to_string(vector_path("pipeline-stage-order.json")).expect("read vector");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse vector");
    for entry in value["stages"].as_array().expect("stages") {
        let rust_name = entry["rust"].as_str().expect("rust");
        let parsed: WebCallStage = match rust_name {
            "RequestIdentity" => WebCallStage::RequestIdentity,
            "SurfaceClassification" => WebCallStage::SurfaceClassification,
            "Cors" => WebCallStage::Cors,
            "MethodGuard" => WebCallStage::MethodGuard,
            "CrossSiteRequest" => WebCallStage::CrossSiteRequest,
            "SqlInjectionGuard" => WebCallStage::SqlInjectionGuard,
            "RequestSizeLimit" => WebCallStage::RequestSizeLimit,
            "RateLimit" => WebCallStage::RateLimit,
            "Idempotency" => WebCallStage::Idempotency,
            "RequestContextResolution" => WebCallStage::RequestContextResolution,
            "Authentication" => WebCallStage::Authentication,
            "Authorization" => WebCallStage::Authorization,
            "TenantIsolation" => WebCallStage::TenantIsolation,
            "ContextInjection" => WebCallStage::ContextInjection,
            "Logging" => WebCallStage::Logging,
            "Audit" => WebCallStage::Audit,
            "HeaderSecurity" => WebCallStage::HeaderSecurity,
            "ResponseIdentity" => WebCallStage::ResponseIdentity,
            other => panic!("unknown stage in vector: {other}"),
        };
        assert!(STANDARD_STAGE_ORDER.contains(&parsed));
    }
}
