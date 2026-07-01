//! Capability matrix must not leave implemented items as `pending`.

use std::fs;
use std::path::PathBuf;

#[test]
fn capability_matrix_has_no_stale_pending_for_implemented_modules() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("specs")
        .join("web-framework-capability.matrix.json");
    let raw = fs::read_to_string(&path).expect("read capability matrix");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse matrix");
    let capabilities = value["domains"]
        .as_array()
        .expect("domains")
        .iter()
        .flat_map(|domain| domain["capabilities"].as_array().expect("capabilities"))
        .collect::<Vec<_>>();

    for entry in &capabilities {
        let id = entry["id"].as_str().expect("capability id");
        let impl_status = entry["implStatus"].as_str().expect("implStatus");
        assert!(
            impl_status != "pending",
            "capability {id} must not remain pending"
        );
    }

    assert_eq!(
        "M3",
        value["framework"]["currentMaturity"]
            .as_str()
            .expect("currentMaturity")
    );
    assert_eq!(
        18,
        value["mandatoryPipelineStages"]
            .as_u64()
            .expect("mandatoryPipelineStages")
    );
}

#[test]
fn tracked_capabilities_are_implemented_or_documented() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("specs")
        .join("web-framework-capability.matrix.json");
    let raw = fs::read_to_string(&path).expect("read capability matrix");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse matrix");
    let capabilities = value["domains"]
        .as_array()
        .expect("domains")
        .iter()
        .flat_map(|domain| domain["capabilities"].as_array().expect("capabilities"))
        .collect::<Vec<_>>();

    for entry in &capabilities {
        let id = entry["id"].as_str().expect("capability id");
        let impl_status = entry["implStatus"].as_str().expect("implStatus");
        assert!(
            matches!(impl_status, "implemented" | "documented" | "partial"),
            "capability {id} has unexpected implStatus: {impl_status}"
        );
    }
}
