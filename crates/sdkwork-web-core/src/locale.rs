//! Request locale resolution and framework message catalog (`I18N_SPEC.md` / `API_SPEC.md` §15.3).

use sdkwork_utils_rust::resolve_locale_preference;
use serde::{Deserialize, Serialize};
use std::sync::Arc;

/// BCP 47 locale resolved once per HTTP request.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebLocale {
    tag: String,
    raw_accept_language: Option<String>,
}

impl WebLocale {
    pub fn new(tag: impl Into<String>) -> Self {
        Self {
            tag: tag.into(),
            raw_accept_language: None,
        }
    }

    pub fn resolved(tag: impl Into<String>, raw_accept_language: Option<String>) -> Self {
        Self {
            tag: tag.into(),
            raw_accept_language,
        }
    }

    pub fn tag(&self) -> &str {
        &self.tag
    }

    pub fn raw_accept_language(&self) -> Option<&str> {
        self.raw_accept_language.as_deref()
    }
}

impl Serialize for WebLocale {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        self.tag.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for WebLocale {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: serde::Deserializer<'de>,
    {
        let tag = String::deserialize(deserializer)?;
        Ok(Self {
            tag,
            raw_accept_language: None,
        })
    }
}

/// Application locale policy wired through [`crate::request_context::WebRequestContextProfile`].
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct WebLocalePolicy {
    pub default_locale: WebLocale,
    pub fallback_locale: WebLocale,
    pub supported_locales: Vec<WebLocale>,
}

impl Default for WebLocalePolicy {
    fn default() -> Self {
        Self {
            default_locale: WebLocale::new("zh-CN"),
            fallback_locale: WebLocale::new("en-US"),
            supported_locales: vec![
                WebLocale::new("zh-CN"),
                WebLocale::new("en-US"),
                WebLocale::new("ja-JP"),
                WebLocale::new("de-DE"),
                WebLocale::new("fr-FR"),
                WebLocale::new("ru-RU"),
                WebLocale::new("ko-KR"),
            ],
        }
    }
}

impl WebLocalePolicy {
    pub fn supported_tags(&self) -> Vec<String> {
        self.supported_locales
            .iter()
            .map(|locale| locale.tag().to_owned())
            .collect()
    }

    pub fn from_env(
        default: Option<&str>,
        supported: Option<&str>,
        fallback: Option<&str>,
    ) -> Self {
        let mut policy = Self::default();
        if let Some(value) = default.and_then(|tag| sdkwork_utils_rust::normalize_locale_tag(tag)) {
            policy.default_locale = WebLocale::new(value);
        }
        if let Some(value) = fallback.and_then(|tag| sdkwork_utils_rust::normalize_locale_tag(tag))
        {
            policy.fallback_locale = WebLocale::new(value);
        }
        if let Some(value) = supported {
            let parsed = value
                .split(',')
                .filter_map(sdkwork_utils_rust::normalize_locale_tag)
                .map(WebLocale::new)
                .collect::<Vec<_>>();
            if !parsed.is_empty() {
                policy.supported_locales = parsed;
            }
        }
        policy
    }

    /// Load locale policy from `ENVIRONMENT_SPEC.md` application env keys.
    pub fn from_application_env(application_code: &str) -> Self {
        let prefix = format!("SDKWORK_{}_", application_code.to_ascii_uppercase());
        Self::from_env(
            std::env::var(format!("{prefix}DEFAULT_LOCALE"))
                .ok()
                .as_deref(),
            std::env::var(format!("{prefix}SUPPORTED_LOCALES"))
                .ok()
                .as_deref(),
            std::env::var(format!("{prefix}FALLBACK_LOCALE"))
                .ok()
                .as_deref(),
        )
    }
}

/// Resolve the effective request locale using SDKWork precedence rules.
pub fn resolve_request_locale(
    policy: &WebLocalePolicy,
    accept_language: Option<&str>,
    jwt_locale: Option<&str>,
    tenant_default_locale: Option<&str>,
) -> WebLocale {
    let supported = policy.supported_tags();
    let explicit = [jwt_locale, tenant_default_locale]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>();
    let tag = resolve_locale_preference(
        accept_language,
        &explicit,
        &supported,
        policy.default_locale.tag(),
        policy.fallback_locale.tag(),
    );
    WebLocale::resolved(tag, accept_language.map(str::to_owned))
}

pub fn extract_locale_claim_from_jwt(raw_token: Option<&str>) -> Option<String> {
    let raw = raw_token?;
    let claims = crate::parsers::parse_claims(raw).ok()?;
    claims
        .get("locale")
        .and_then(|value| sdkwork_utils_rust::normalize_locale_tag(value))
}

/// Framework-owned platform error catalog (`errors.result.<code>` keys).
pub trait FrameworkMessageCatalog: Send + Sync {
    fn resolve(&self, locale: &WebLocale, result_code: i32) -> Option<String>;
}

#[derive(Clone, Default)]
pub struct EmbeddedFrameworkMessageCatalog;

impl FrameworkMessageCatalog for EmbeddedFrameworkMessageCatalog {
    fn resolve(&self, locale: &WebLocale, result_code: i32) -> Option<String> {
        framework_message_for(locale.tag(), result_code).map(str::to_owned)
    }
}

pub fn framework_message_catalog() -> Arc<dyn FrameworkMessageCatalog> {
    Arc::new(EmbeddedFrameworkMessageCatalog)
}

fn framework_message_for(locale: &str, result_code: i32) -> Option<&'static str> {
    let zh = locale.starts_with("zh");
    match result_code {
        40001 => Some(if zh {
            "请求参数无效"
        } else {
            "One or more request fields are invalid"
        }),
        40101 => Some(if zh {
            "需要身份认证"
        } else {
            "Authentication is required"
        }),
        40103 => Some(if zh {
            "凭证无效"
        } else {
            "The provided credential is invalid"
        }),
        40301 => Some(if zh {
            "权限不足"
        } else {
            "Permission is required for this operation"
        }),
        40401 => Some(if zh {
            "资源不存在"
        } else {
            "The requested resource was not found"
        }),
        40501 => Some(if zh {
            "HTTP 方法不被允许"
        } else {
            "The HTTP method is not allowed for this route"
        }),
        40801 => Some(if zh {
            "请求超时"
        } else {
            "The request timed out"
        }),
        40901 => Some(if zh {
            "资源状态冲突"
        } else {
            "The request conflicts with the current resource state"
        }),
        41301 => Some(if zh {
            "请求体过大"
        } else {
            "The request payload is too large"
        }),
        42901 => Some(if zh {
            "请求过于频繁"
        } else {
            "Rate limit exceeded"
        }),
        50101 => Some(if zh {
            "接口尚未实现"
        } else {
            "The requested operation is not implemented"
        }),
        50301 => Some(if zh {
            "依赖服务暂时不可用"
        } else {
            "A required dependency is temporarily unavailable"
        }),
        50001 => Some(if zh {
            "服务器内部错误"
        } else {
            "An internal error occurred"
        }),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_jwt_locale_before_accept_language() {
        let policy = WebLocalePolicy::default();
        let locale = resolve_request_locale(&policy, Some("en-US,en;q=0.9"), Some("zh-CN"), None);
        assert_eq!("zh-CN", locale.tag());
    }

    #[test]
    fn serializes_locale_as_tag_string() {
        let locale = WebLocale::new("en-US");
        assert_eq!(
            serde_json::to_value(locale).unwrap(),
            serde_json::Value::String("en-US".to_owned())
        );
    }

    #[test]
    fn localized_platform_message_exists_for_zh_cn() {
        let locale = WebLocale::new("zh-CN");
        let catalog = EmbeddedFrameworkMessageCatalog;
        assert_eq!(
            Some("需要身份认证".to_owned()),
            catalog.resolve(&locale, 40101)
        );
    }
}
