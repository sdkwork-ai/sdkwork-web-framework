//! Architecture guard: workspace crates must not depend on business repositories.

use std::process::Command;

const WORKSPACE_CRATES: &[&str] = &[
    "sdkwork-web-contract",
    "sdkwork-web-core",
    "sdkwork-web-axum",
    "sdkwork-web-bootstrap",
    "sdkwork-web-store-redis",
    "sdkwork-web-store-sqlx",
    "sdkwork-web-test-utils",
    "sdkwork-web-framework-admin-repository-sqlx",
    "sdkwork-routes-web-framework-backend-api",
    "sdkwork-web-admin-server",
    "sdkwork-webstore-database-host",
    "sdkwork-web-schema-registry",
];

const FORBIDDEN_DEPENDENCY_FRAGMENTS: &[&str] = &[
    "sdkwork-appbase",
    "sdkwork-clawrouter",
    "openchat",
    "sdkwork-iam",
];

#[test]
fn workspace_crates_have_no_business_dependencies() {
    for crate_name in WORKSPACE_CRATES {
        let output = Command::new("cargo")
            .args(["tree", "-p", crate_name, "--prefix", "none"])
            .output()
            .unwrap_or_else(|error| panic!("failed to run cargo tree for {crate_name}: {error}"));
        assert!(
            output.status.success(),
            "cargo tree failed for {crate_name}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        let tree = String::from_utf8_lossy(&output.stdout);
        for forbidden in FORBIDDEN_DEPENDENCY_FRAGMENTS {
            assert!(
                !tree.contains(forbidden),
                "crate {crate_name} must not depend on {forbidden}\n{tree}"
            );
        }
    }
}
