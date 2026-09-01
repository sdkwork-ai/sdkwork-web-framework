//! Process-wide startup default region (REGION_SPEC §8).
//!
//! Every bootstrapped process registers its resolved default region once at
//! startup; any module can then read the deployment's region from anywhere in
//! the process through [`runtime_region_code`]. This keeps pricing, routing,
//! locale, and compliance consumers on the same identifier without threading
//! configuration through every call site.

use std::sync::OnceLock;

/// REGION_SPEC §4.1: default value when no region is specified.
pub const DEFAULT_REGION_CODE: &str = "global";
/// REGION_SPEC §4.1: maximum region code length.
pub const MAX_REGION_CODE_LEN: usize = 64;

/// Immutable process-wide snapshot of the startup default region.
#[derive(Debug, Clone, Copy, Eq, PartialEq)]
pub struct RuntimeRegion {
    region_code: &'static str,
}

impl RuntimeRegion {
    fn new(region_code: &'static str) -> Self {
        Self { region_code }
    }

    /// The canonical `regionCode` this process was started with. Falls back to
    /// `global` when no region was registered.
    pub fn region_code(self) -> &'static str {
        self.region_code
    }
}

static RUNTIME_REGION: OnceLock<RuntimeRegion> = OnceLock::new();

/// Registers the process-wide default region. The first successful
/// registration wins; re-registering the identical code is a no-op and a
/// conflicting second registration is rejected so a late bootstrap cannot
/// silently rebind a region the process already resolved.
///
/// A blank value registers the REGION_SPEC default `global`. Values that fail
/// the REGION_SPEC §4.1 format (`^[a-z][a-z0-9_]*$`, at most 64 characters)
/// are rejected with a diagnosable message.
pub fn register_runtime_region(region_code: &str) -> Result<RuntimeRegion, String> {
    let normalized = normalized_region_code(region_code)?;
    let stored = match RUNTIME_REGION.get() {
        Some(existing) => {
            if existing.region_code() != normalized.as_str() {
                return Err(format!(
                    "runtime region is already registered as `{}`; refusing to rebind to `{normalized}`",
                    existing.region_code()
                ));
            }
            return Ok(*existing);
        }
        None => {
            let region = RuntimeRegion::new(leak_region_code(normalized));
            RUNTIME_REGION
                .set(region)
                .map_err(|_| "runtime region was registered concurrently".to_owned())?;
            region
        }
    };
    Ok(stored)
}

/// Returns the process-wide default region snapshot, defaulting to `global`
/// when no startup registration happened.
pub fn runtime_region() -> RuntimeRegion {
    RUNTIME_REGION
        .get()
        .copied()
        .unwrap_or(RuntimeRegion::new(DEFAULT_REGION_CODE))
}

/// Returns the canonical `regionCode` of the current process's default region.
/// Safe to call from anywhere after startup.
pub fn runtime_region_code() -> &'static str {
    runtime_region().region_code()
}

/// Whether a startup region was explicitly registered.
pub fn is_runtime_region_registered() -> bool {
    RUNTIME_REGION.get().is_some()
}

/// Normalizes a region code per REGION_SPEC §4.1: trims, lowercases, defaults
/// blank values to `global`, and validates format and length.
fn normalized_region_code(value: &str) -> Result<String, String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Ok(DEFAULT_REGION_CODE.to_owned());
    }
    let code = trimmed.to_ascii_lowercase();
    if !is_valid_region_code(&code) {
        return Err(format!(
            "region code `{trimmed}` must match ^[a-z][a-z0-9_]*$ and be at most {MAX_REGION_CODE_LEN} characters"
        ));
    }
    Ok(code)
}

fn is_valid_region_code(code: &str) -> bool {
    if code.is_empty() || code.len() > MAX_REGION_CODE_LEN {
        return false;
    }
    let mut chars = code.chars();
    match chars.next() {
        Some(first) if first.is_ascii_alphabetic() && first.is_ascii_lowercase() => {}
        _ => return false,
    }
    chars.all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
}

fn leak_region_code(code: String) -> &'static str {
    Box::leak(code.into_boxed_str())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn process_wide_registration_lifecycle() {
        // The registry is process-global and tests in one binary run in
        // parallel, so every stateful assertion lives in this single test.
        assert_eq!(DEFAULT_REGION_CODE, runtime_region_code());
        assert!(!is_runtime_region_registered());

        let region = register_runtime_region("cn").expect("valid region");
        assert_eq!("cn", region.region_code());
        assert_eq!("cn", runtime_region_code());
        assert!(is_runtime_region_registered());

        // Identical re-registration (including case/whitespace variants) is a no-op.
        assert_eq!(region, register_runtime_region(" CN ").expect("same region"));
        assert_eq!(region, register_runtime_region("cn").expect("same region"));

        // A conflicting re-registration is rejected without rebinding.
        assert!(register_runtime_region("eu").is_err());
        assert_eq!(region, runtime_region());

        // Blank values normalize to the REGION_SPEC default `global`, which
        // conflicts with the already-registered `cn` here.
        assert!(register_runtime_region("  ").is_err());
    }

    #[test]
    fn normalizes_case_and_whitespace_without_touching_the_registry() {
        assert_eq!(
            "cn",
            normalized_region_code(" CN ").expect("normalized")
        );
        assert_eq!(
            DEFAULT_REGION_CODE,
            normalized_region_code("  ").expect("blank defaults to global")
        );
    }

    #[test]
    fn rejects_invalid_region_codes_without_touching_the_registry() {
        for invalid in ["Us-East-1", "with space", "a_b-c", &"a".repeat(65)] {
            let result = normalized_region_code(invalid);
            assert!(result.is_err(), "{invalid:?} must be rejected");
        }
        // Uppercase input is normalized to lowercase before validation, so a
        // valid code written in mixed case resolves to its canonical form.
        assert_eq!("upper", normalized_region_code("UPPER").expect("normalized"));
    }
}
