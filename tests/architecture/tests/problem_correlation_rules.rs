//! Problem+json responses must not use uncorrelated defaults in production code.

use std::fs;
use std::path::{Path, PathBuf};

const SCAN_ROOTS: &[&str] = &["../../crates"];

const ALLOWED_DEFAULT_CORRELATION: &[&str] = &["../../crates/sdkwork-web-core/src/problem.rs"];

fn collect_rust_files(dir: &Path, out: &mut Vec<PathBuf>) {
    let entries = fs::read_dir(dir).unwrap_or_else(|error| {
        panic!("failed to read {}: {error}", dir.display());
    });
    for entry in entries {
        let entry = entry.expect("dir entry");
        let path = entry.path();
        if path.is_dir() {
            if path.file_name().is_some_and(|name| name == "target") {
                continue;
            }
            collect_rust_files(&path, out);
            continue;
        }
        if path.extension().is_some_and(|ext| ext == "rs") {
            out.push(path);
        }
    }
}

fn is_allowed(path: &Path) -> bool {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    ALLOWED_DEFAULT_CORRELATION
        .iter()
        .any(|relative| manifest_dir.join(relative) == path)
}

fn strip_test_modules(source: &str) -> String {
    let mut output = String::new();
    let mut depth = 0_i32;
    for line in source.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with("#[cfg(test)]") {
            depth += 1;
            continue;
        }
        if depth > 0 {
            if trimmed == "}" && line.chars().next().is_some_and(|ch| ch == '}') {
                depth -= 1;
            }
            continue;
        }
        output.push_str(line);
        output.push('\n');
    }
    output
}

#[test]
fn production_code_must_not_use_problem_correlation_default() {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    let mut files = Vec::new();
    for relative in SCAN_ROOTS {
        collect_rust_files(&manifest_dir.join(relative), &mut files);
    }

    for path in files {
        if is_allowed(&path) {
            continue;
        }
        let source = fs::read_to_string(&path)
            .unwrap_or_else(|error| panic!("failed to read {}: {error}", path.display()));
        let production = strip_test_modules(&source);
        assert!(
            !production.contains("ProblemCorrelation::default()"),
            "{} must not call ProblemCorrelation::default() outside tests; use request-scoped correlation",
            path.display()
        );
    }
}

#[test]
fn api_problem_must_not_implement_bare_into_response() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/sdkwork-routes-web-framework-backend-api/src/response.rs");
    let source = fs::read_to_string(&path).expect("read response.rs");
    assert!(
        !source.contains("impl IntoResponse for ApiProblem"),
        "ApiProblem must use into_response_for(WebRequestContext) only"
    );
}

#[test]
fn web_framework_error_must_not_implement_bare_into_response() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../crates/sdkwork-web-core/src/error.rs");
    let source = fs::read_to_string(&path).expect("read error.rs");
    assert!(
        !source.contains("impl IntoResponse for WebFrameworkError"),
        "WebFrameworkError must use WebFrameworkRejection for Axum extractors"
    );
    assert!(
        source.contains("pub struct WebFrameworkRejection"),
        "WebFrameworkRejection must provide correlated IntoResponse for extractors"
    );
}
