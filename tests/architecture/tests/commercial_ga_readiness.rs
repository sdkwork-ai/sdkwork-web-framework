//! Commercial GA readiness: release artifacts, verify entrypoints, and architecture test registry.

use std::fs;
use std::path::PathBuf;

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

#[test]
fn changelog_exists_for_release_evidence() {
    let path = repo_root().join("CHANGELOG.md");
    let raw = fs::read_to_string(&path).expect("CHANGELOG.md must exist for GA handoff");
    assert!(
        raw.contains("0.1.0"),
        "CHANGELOG must document the current release train"
    );
    assert!(
        raw.contains("Problem correlation") || raw.contains("Problem"),
        "CHANGELOG must summarize M3 alignment changes"
    );
}

#[test]
fn deployments_readme_documents_production_checklist() {
    let path = repo_root().join("deployments").join("README.md");
    let raw = fs::read_to_string(&path).expect("deployments/README.md");
    for required in [
        "Production assembly checklist",
        "production_defaults",
        "mount_service_routes",
        "scripts/verify",
        "Redis",
        "ReadinessCheck",
        "DisabledApiKeyLookupService",
        "assemble_control_plane",
        "21-operations-runbook",
        "24-production-rollout-and-adoption",
    ] {
        assert!(
            raw.contains(required),
            "deployments/README.md must document production handoff item: {required}"
        );
    }
}

#[test]
fn readme_points_to_verify_scripts_and_component_spec() {
    let path = repo_root().join("README.md");
    let raw = fs::read_to_string(&path).expect("README.md");
    assert!(
        raw.contains("scripts/verify.ps1") && raw.contains("scripts/verify.sh"),
        "README must document verify entrypoints"
    );
    assert!(
        raw.contains("component.spec.json"),
        "README must reference component.spec verification commands"
    );
    assert!(
        raw.contains("route_manifest") && raw.contains("mount_service_routes"),
        "README quick start must show recommended integration pattern"
    );
}

#[test]
fn verify_scripts_cover_component_spec_gates() {
    let spec_path = repo_root().join("specs").join("component.spec.json");
    let spec: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&spec_path).expect("read component spec"))
            .expect("parse component spec");
    let commands = spec["verification"]["commands"]
        .as_array()
        .expect("verification.commands")
        .iter()
        .filter_map(|entry| entry.as_str())
        .collect::<Vec<_>>();

    let ps1 = fs::read_to_string(repo_root().join("scripts").join("verify.ps1"))
        .expect("read verify.ps1");
    let sh =
        fs::read_to_string(repo_root().join("scripts").join("verify.sh")).expect("read verify.sh");

    for command in commands {
        let (ps1_needle, sh_needle) = match command {
            "cd apps/sdkwork-web-framework-pc && npm run verify" => {
                ("npm run verify", "npm run verify")
            }
            "cd apps/sdkwork-web-framework-pc && npm run test:e2e" => {
                ("npm run test:e2e", "npm run test:e2e")
            }
            "node scripts/build-pc-admin-e2e.mjs" => (
                "pc-admin-e2e-build.contract.test.mjs",
                "pc-admin-e2e-build.contract.test.mjs",
            ),
            "node tests/contract/pc-admin-e2e-build.contract.test.mjs" => (
                "pc-admin-e2e-build.contract.test.mjs",
                "pc-admin-e2e-build.contract.test.mjs",
            ),
            "node tests/contract/production-rollout.contract.test.mjs" => (
                "production-rollout.contract.test.mjs",
                "production-rollout.contract.test.mjs",
            ),
            "node tests/contract/release-evidence.contract.test.mjs" => (
                "release-evidence.contract.test.mjs",
                "release-evidence.contract.test.mjs",
            ),
            "node tests/contract/adoption-evidence.contract.test.mjs" => (
                "adoption-evidence.contract.test.mjs",
                "adoption-evidence.contract.test.mjs",
            ),
            "node scripts/collect-release-evidence.mjs" => (
                "collect-release-evidence.mjs",
                "collect-release-evidence.mjs",
            ),
            "cd apps/sdkwork-web-framework-pc && npm run test:e2e:integration" => {
                ("test:e2e:integration", "test:e2e:integration")
            }
            "cd apps/sdkwork-web-framework-pc && npm test" => ("npm test", "npm test"),
            "node scripts/generate-pc-admin-operations.mjs --check" => (
                "generate-pc-admin-operations.mjs --check",
                "generate-pc-admin-operations.mjs --check",
            ),
            "node tests/contract/pc-admin-build.smoke.test.mjs" => (
                "pc-admin-build.smoke.test.mjs",
                "pc-admin-build.smoke.test.mjs",
            ),
            "cargo test --release -p sdkwork-web-architecture-tests --test pipeline_benchmark" => {
                ("benchmark-pipeline.ps1", "benchmark-pipeline.sh")
            }
            other => (other, other),
        };
        assert!(
            ps1.contains(ps1_needle),
            "verify.ps1 must cover component.spec command: {command}"
        );
        assert!(
            sh.contains(sh_needle),
            "verify.sh must cover component.spec command: {command}"
        );
    }
}

#[test]
fn architecture_cargo_toml_tests_have_source_files() {
    let manifest_path = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("Cargo.toml");
    let manifest = fs::read_to_string(&manifest_path).expect("read architecture Cargo.toml");
    let tests_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests");

    for line in manifest.lines() {
        let line = line.trim();
        if let Some(path) = line.strip_prefix("path = \"tests/") {
            let relative = path.trim_end_matches('"');
            let full = tests_dir.join(relative);
            assert!(
                full.is_file(),
                "architecture test source missing for Cargo.toml entry: {}",
                full.display()
            );
        }
    }
}

#[test]
fn capability_matrix_lists_correlation_and_fallback_guards() {
    let path = repo_root()
        .join("specs")
        .join("web-framework-capability.matrix.json");
    let raw = fs::read_to_string(&path).expect("read capability matrix");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse matrix");
    let ids = value["domains"]
        .as_array()
        .expect("domains")
        .iter()
        .flat_map(|domain| domain["capabilities"].as_array().expect("capabilities"))
        .filter_map(|entry| entry["id"].as_str())
        .collect::<Vec<_>>();

    for required in [
        "K10", "K11", "K12", "K13", "K14", "K15", "K16", "K17", "K18", "K19",
    ] {
        assert!(
            ids.contains(&required),
            "capability matrix must track guard capability {required}"
        );
    }
}

#[test]
fn operations_runbook_exists_for_production_handoff() {
    let path = repo_root()
        .join("docs")
        .join("architecture")
        .join("tech")
        .join("TECH-21-operations-runbook.md");
    let raw =
        fs::read_to_string(&path).expect("docs/architecture/tech/TECH-21-operations-runbook.md");
    for required in ["/healthz", "/readyz", "/metrics", "graceful", "OTEL"] {
        assert!(
            raw.contains(required),
            "operations runbook must document {required}"
        );
    }
}

#[test]
fn admin_server_env_example_matches_web_framework_env_vocabulary() {
    let env_rs = fs::read_to_string(
        repo_root()
            .join("crates")
            .join("sdkwork-web-bootstrap")
            .join("src")
            .join("env_config.rs"),
    )
    .expect("read env_config.rs");
    let example = fs::read_to_string(repo_root().join("configs").join("admin-server.env.example"))
        .expect("configs/admin-server.env.example");
    for key in [
        "SDKWORK_WEB_FRAMEWORK_ENV",
        "SDKWORK_WEB_FRAMEWORK_ADMIN_BIND",
        "SDKWORK_WEB_FRAMEWORK_STORE_URL",
        "SDKWORK_WEB_FRAMEWORK_STORE_POOL_SIZE",
        "SDKWORK_WEB_FRAMEWORK_JWT_HS256_SECRET",
        "SDKWORK_WEB_FRAMEWORK_REDIS_URL",
        "OTEL_SERVICE_NAME",
        "OTEL_EXPORTER_OTLP_ENDPOINT",
    ] {
        assert!(env_rs.contains(key), "WebFrameworkEnv must parse {key}");
        assert!(
            example.contains(key),
            "admin-server.env.example must document {key}"
        );
    }
}

#[test]
fn specs_readme_exists_per_component_spec() {
    let path = repo_root().join("specs").join("README.md");
    let raw = fs::read_to_string(&path).expect("specs/README.md");
    assert!(raw.contains("component.spec.json"));
    assert!(raw.contains("WEB_FRAMEWORK_STANDARD.md"));
}

#[test]
fn bootstrap_and_routing_doc_exists() {
    let path = repo_root()
        .join("docs")
        .join("architecture")
        .join("tech")
        .join("TECH-22-bootstrap-and-routing.md");
    let raw =
        fs::read_to_string(&path).expect("docs/architecture/tech/TECH-22-bootstrap-and-routing.md");
    for required in [
        "route_manifest",
        "mount_service_routes",
        "contract fallback",
        "/healthz",
    ] {
        assert!(
            raw.contains(required),
            "bootstrap/routing doc must cover {required}"
        );
    }
}

#[test]
fn consumer_integration_template_exists() {
    let path = repo_root()
        .join("docs")
        .join("architecture")
        .join("tech")
        .join("TECH-23-consumer-integration-template.md");
    let raw = fs::read_to_string(&path)
        .expect("docs/architecture/tech/TECH-23-consumer-integration-template.md");
    assert!(raw.contains("production_defaults"));
    assert!(raw.contains("WebRequestContext"));
}

#[test]
fn pc_admin_sdk_generator_exists() {
    let path = repo_root()
        .join("scripts")
        .join("generate-pc-admin-operations.mjs");
    assert!(
        path.is_file(),
        "manifest-driven PC admin SDK generator must exist"
    );
    let raw = fs::read_to_string(&path).expect("read generator script");
    assert!(
        raw.contains("--check"),
        "generator must support --check drift mode"
    );
}

#[test]
fn pc_admin_playwright_e2e_exists() {
    let pc_root = repo_root().join("apps").join("sdkwork-web-framework-pc");
    for relative in [
        "playwright.config.ts",
        "e2e/console.smoke.spec.ts",
        "e2e/console.error-paths.spec.ts",
        "package.json",
    ] {
        assert!(
            pc_root.join(relative).is_file(),
            "PC admin Playwright E2E must include {relative}"
        );
    }
    let package = fs::read_to_string(pc_root.join("package.json")).expect("read package.json");
    assert!(
        package.contains("test:e2e"),
        "PC admin package.json must expose test:e2e script"
    );
    assert!(
        package.contains("@playwright/test"),
        "PC admin package.json must depend on @playwright/test"
    );
}

#[test]
fn pc_admin_playwright_integration_e2e_exists() {
    let repo = repo_root();
    for relative in [
        "scripts/e2e-constants.mjs",
        "scripts/e2e-web-stack.mjs",
        "scripts/build-pc-admin-e2e.mjs",
        "apps/sdkwork-web-framework-pc/e2e/console.integration.spec.ts",
        "apps/sdkwork-web-framework-pc/playwright.integration.config.ts",
    ] {
        assert!(
            repo.join(relative).is_file(),
            "PC admin integration E2E must include {relative}"
        );
    }
}

#[test]
fn production_rollout_and_adoption_evidence_exists() {
    let repo = repo_root();
    let rollout = fs::read_to_string(
        repo.join("docs")
            .join("architecture")
            .join("tech")
            .join("TECH-24-production-rollout-and-adoption.md"),
    )
    .expect("docs/architecture/tech/TECH-24-production-rollout-and-adoption.md");
    for required in [
        "Pre-flight",
        "Canary",
        "Rollback",
        "production-adoption.evidence.template.json",
        "18-owasp-api-top10-mapping",
    ] {
        assert!(
            rollout.contains(required),
            "production rollout doc must cover {required}"
        );
    }
    assert!(
        repo.join("specs")
            .join("production-adoption.evidence.template.json")
            .is_file(),
        "adoption evidence template must exist"
    );
    assert!(
        repo.join("tests")
            .join("contract")
            .join("production-rollout.contract.test.mjs")
            .is_file(),
        "production rollout contract test must exist"
    );
}

#[test]
fn owasp_api_mapping_doc_exists() {
    let path = repo_root()
        .join("docs")
        .join("architecture")
        .join("tech")
        .join("TECH-18-owasp-api-top10-mapping.md");
    let raw = fs::read_to_string(&path)
        .expect("docs/architecture/tech/TECH-18-owasp-api-top10-mapping.md");
    assert!(raw.contains("API1:2023"));
    assert!(raw.contains("validate_production_assembly"));
}

#[test]
fn release_evidence_collector_exists() {
    let repo = repo_root();
    for relative in [
        "scripts/collect-release-evidence.mjs",
        "scripts/validate-adoption-evidence.mjs",
        "tests/contract/release-evidence.contract.test.mjs",
        "tests/contract/adoption-evidence.contract.test.mjs",
    ] {
        assert!(
            repo.join(relative).is_file(),
            "release evidence bundle must include {relative}"
        );
    }
}

#[test]
fn framework_pathfinder_adoption_evidence_exists() {
    let path = repo_root()
        .join("specs")
        .join("framework-adoption.evidence.json");
    let raw = fs::read_to_string(&path).expect("specs/framework-adoption.evidence.json");
    let value: serde_json::Value = serde_json::from_str(&raw).expect("parse adoption evidence");
    assert_eq!(
        "sdkwork.web-framework.adoption-evidence",
        value["kind"].as_str().unwrap()
    );
    let adoptions = value["adoptions"].as_array().expect("adoptions");
    assert!(
        adoptions.len() >= 2,
        "framework pathfinder must document admin-server and PC console adoptions"
    );
}

#[test]
fn docs_catalog_lists_new_guard_capabilities() {
    let path = repo_root()
        .join("docs")
        .join("architecture")
        .join("tech")
        .join("TECH-13-capability-catalog.md");
    let raw = fs::read_to_string(&path).expect("read capability catalog");
    for required in [
        "K10", "K11", "K12", "K13", "K14", "K15", "K16", "K17", "K18", "K19", "K8",
    ] {
        assert!(
            raw.contains(required),
            "docs/13-capability-catalog.md must document capability {required}"
        );
    }
}
