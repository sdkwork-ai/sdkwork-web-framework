//! Sensitive value redaction for structured logs (catalog E5 / C8).

const REDACTED: &str = "[REDACTED]";

fn is_sensitive_header_name(normalized: &str) -> bool {
    matches!(
        normalized,
        "authorization"
            | "x-api-key"
            | "api-key"
            | "access-token"
            | "x-access-token"
            | "cookie"
            | "set-cookie"
            | "x-idempotency-key"
            | "idempotency-key"
    )
}

fn is_sensitive_log_field_name(normalized: &str) -> bool {
    if is_sensitive_header_name(normalized) {
        return true;
    }
    matches!(
        normalized,
        "password"
            | "passwd"
            | "secret"
            | "api_key"
            | "apikey"
            | "token"
            | "auth_token"
            | "access_token"
            | "refresh_token"
            | "bearer"
            | "credential"
            | "credentials"
    ) || normalized.contains("password")
        || normalized.contains("secret")
        || normalized.contains("token")
        || normalized.contains("api_key")
        || normalized.contains("apikey")
}

/// Returns a redacted copy when `name` matches known credential header names.
pub fn redact_sensitive_header(name: &str, value: &str) -> String {
    let normalized = name.trim().to_ascii_lowercase();
    if is_sensitive_header_name(&normalized) {
        REDACTED.to_owned()
    } else {
        value.to_owned()
    }
}

/// Returns a redacted copy when `field_name` matches known credential or secret field names.
pub fn redact_sensitive_log_value(field_name: &str, value: &str) -> String {
    let normalized = field_name.trim().to_ascii_lowercase();
    if is_sensitive_log_field_name(&normalized) {
        REDACTED.to_owned()
    } else {
        value.to_owned()
    }
}

/// Returns true when structured log fields with this name must be redacted.
pub fn is_redacted_log_field(field_name: &str) -> bool {
    is_sensitive_log_field_name(&field_name.trim().to_ascii_lowercase())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacts_authorization_values() {
        assert_eq!(
            REDACTED,
            redact_sensitive_header("Authorization", "Bearer secret")
        );
    }

    #[test]
    fn preserves_safe_headers() {
        assert_eq!(
            "application/json",
            redact_sensitive_header("Content-Type", "application/json")
        );
    }

    #[test]
    fn redacts_password_log_fields() {
        assert_eq!(REDACTED, redact_sensitive_log_value("password", "hunter2"));
        assert_eq!(
            REDACTED,
            redact_sensitive_log_value("user_password", "hunter2")
        );
    }

    #[test]
    fn preserves_safe_log_fields() {
        assert_eq!("100001", redact_sensitive_log_value("tenant_id", "100001"));
    }
}
