//! component.spec.json must stay aligned with extension point registry.

use std::fs;
use std::path::PathBuf;

#[test]
fn component_spec_lists_core_extension_traits() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("specs")
        .join("component.spec.json");
    let raw = fs::read_to_string(&path).expect("read component spec");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse component spec");
    let traits = value["contracts"]["extensionTraits"]
        .as_array()
        .expect("extensionTraits")
        .iter()
        .filter_map(|entry| entry.as_str())
        .collect::<Vec<_>>();

    for required in [
        "WebRequestContextResolver",
        "TenantSigningKeyLookup",
        "JwtSessionRevocationChecker",
        "RateLimitStore",
        "IdempotencyStore",
        "ConcurrentAdmissionStore",
        "ReadinessCheck",
        "WebFrameworkLifecycle",
    ] {
        assert!(
            traits.contains(&required),
            "component.spec.json must list extension trait {required}"
        );
    }
}

#[test]
fn component_spec_declares_m3_capability_maturity() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("specs")
        .join("component.spec.json");
    let raw = fs::read_to_string(&path).expect("read component spec");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse component spec");
    assert_eq!(
        "M3",
        value["metadata"]["capabilityMaturity"]
            .as_str()
            .expect("capabilityMaturity")
    );
}

#[test]
fn component_spec_lists_required_verification_commands() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("specs")
        .join("component.spec.json");
    let raw = fs::read_to_string(&path).expect("read component spec");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse component spec");
    let commands = value["verification"]["commands"]
        .as_array()
        .expect("verification.commands")
        .iter()
        .filter_map(|entry| entry.as_str())
        .collect::<Vec<_>>();

    for required in [
        "cargo test --workspace",
        "cargo clippy --workspace -- -D warnings",
        "cargo test -p sdkwork-web-architecture-tests",
        "cargo test --release -p sdkwork-web-architecture-tests --test pipeline_benchmark",
        "cargo test -p sdkwork-web-bootstrap --test integration",
        "cargo test -p sdkwork-routes-web-framework-backend-api --test openapi_authority",
        "cargo test -p sdkwork-routes-web-framework-backend-api --test routes_contract",
        "cargo test -p sdkwork-web-bootstrap --features admin-api --test admin_api_readiness",
        "node tests/contract/database-framework.contract.test.mjs",
        "node tests/contract/pc-admin-operations.contract.test.mjs",
        "cd apps/sdkwork-web-framework-pc && npm run verify",
    ] {
        assert!(
            commands.contains(&required),
            "component.spec.json verification.commands must include {required}"
        );
    }
}

#[test]
fn component_spec_declares_eighteen_mandatory_pipeline_stages() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("specs")
        .join("component.spec.json");
    let raw = fs::read_to_string(&path).expect("read component spec");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse component spec");
    assert_eq!(
        18,
        value["contracts"]["requestContextFramework"]["mandatoryPipelineStages"]
            .as_u64()
            .expect("mandatoryPipelineStages")
    );
}

#[test]
fn component_spec_lists_required_canonical_specs() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("specs")
        .join("component.spec.json");
    let raw = fs::read_to_string(&path).expect("read component spec");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse component spec");
    let files = value["canonicalSpecs"]
        .as_array()
        .expect("canonicalSpecs")
        .iter()
        .filter_map(|entry| entry["file"].as_str())
        .collect::<Vec<_>>();

    for required in [
        "WEB_FRAMEWORK_STANDARD.md",
        "CODE_STYLE_SPEC.md",
        "NAMING_SPEC.md",
        "RUST_CODE_SPEC.md",
        "TEST_SPEC.md",
        "QUALITY_GATE_SPEC.md",
        "DEPLOYMENT_SPEC.md",
    ] {
        assert!(
            files.contains(&required),
            "component.spec.json canonicalSpecs must include {required}"
        );
    }
}

#[test]
fn component_spec_declares_runtime_entrypoints() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("specs")
        .join("component.spec.json");
    let raw = fs::read_to_string(&path).expect("read component spec");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse component spec");
    let entrypoints = value["contracts"]["runtimeEntrypoints"]
        .as_array()
        .expect("runtimeEntrypoints")
        .iter()
        .filter_map(|entry| entry.as_str())
        .collect::<Vec<_>>();
    assert!(
        entrypoints
            .iter()
            .any(|entry| entry.contains("sdkwork-web-admin-server")),
        "component.spec must declare admin-server runtime entrypoint"
    );
}

#[test]
fn component_spec_capability_maturity_matches_capability_matrix() {
    let spec_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("specs")
        .join("component.spec.json");
    let matrix_path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("specs")
        .join("web-framework-capability.matrix.json");
    let spec: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&spec_path).expect("read component spec"))
            .expect("parse component spec");
    let matrix: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&matrix_path).expect("read capability matrix"))
            .expect("parse capability matrix");
    assert_eq!(
        spec["metadata"]["capabilityMaturity"]
            .as_str()
            .expect("component capabilityMaturity"),
        matrix["framework"]["currentMaturity"]
            .as_str()
            .expect("matrix currentMaturity")
    );
    assert_eq!(
        spec["contracts"]["requestContextFramework"]["mandatoryPipelineStages"]
            .as_u64()
            .expect("component mandatoryPipelineStages"),
        matrix["mandatoryPipelineStages"]
            .as_u64()
            .expect("matrix mandatoryPipelineStages")
    );
}
