//! Production tracing bootstrap with credential redaction (catalog E5).

use sdkwork_web_core::redact_sensitive_log_value;
use std::fmt;
use tracing::field::{Field, Visit};
use tracing_subscriber::field::RecordFields;
use tracing_subscriber::fmt::format::{FormatFields, Writer};
use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

pub(crate) struct RedactingFormatFields;

impl<'writer> FormatFields<'writer> for RedactingFormatFields {
    fn format_fields<R>(&self, writer: Writer<'writer>, fields: R) -> fmt::Result
    where
        R: RecordFields,
    {
        let mut visitor = RedactingFieldVisitor {
            writer,
            wrote_field: false,
        };
        fields.record(&mut visitor);
        Ok(())
    }
}

struct RedactingFieldVisitor<'writer> {
    writer: Writer<'writer>,
    wrote_field: bool,
}

impl RedactingFieldVisitor<'_> {
    fn write_field(&mut self, field: &Field, value: &str) -> fmt::Result {
        if self.wrote_field {
            self.writer.write_str(" ")?;
        }
        self.writer.write_str(field.name())?;
        self.writer.write_str("=")?;
        self.writer
            .write_str(&redact_sensitive_log_value(field.name(), value))?;
        self.wrote_field = true;
        Ok(())
    }
}

impl Visit for RedactingFieldVisitor<'_> {
    fn record_str(&mut self, field: &Field, value: &str) {
        let _ = self.write_field(field, value);
    }

    fn record_bool(&mut self, field: &Field, value: bool) {
        let _ = self.write_field(field, &value.to_string());
    }

    fn record_i64(&mut self, field: &Field, value: i64) {
        let _ = self.write_field(field, &value.to_string());
    }

    fn record_u64(&mut self, field: &Field, value: u64) {
        let _ = self.write_field(field, &value.to_string());
    }

    fn record_i128(&mut self, field: &Field, value: i128) {
        let _ = self.write_field(field, &value.to_string());
    }

    fn record_u128(&mut self, field: &Field, value: u128) {
        let _ = self.write_field(field, &value.to_string());
    }

    fn record_f64(&mut self, field: &Field, value: f64) {
        let _ = self.write_field(field, &value.to_string());
    }

    fn record_debug(&mut self, field: &Field, value: &dyn fmt::Debug) {
        let _ = self.write_field(field, &format!("{value:?}"));
    }
}

/// Initialize the global tracing subscriber with env-filter and log redaction.
pub fn init_tracing() {
    let _ = tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(tracing_subscriber::fmt::layer().fmt_fields(RedactingFormatFields))
        .try_init();
}

/// Select OTel export when configured, otherwise structured tracing with redaction.
pub fn init_tracing_from_env() {
    let otel_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .filter(|value| !value.trim().is_empty());

    #[cfg(feature = "otel")]
    if otel_endpoint.is_some() {
        let service_name = std::env::var("OTEL_SERVICE_NAME")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| "sdkwork-web-framework".to_owned());
        if crate::otel::init_otel_tracing(&service_name).is_ok() {
            return;
        }
        eprintln!(
            "sdkwork-web-bootstrap: OTEL_EXPORTER_OTLP_ENDPOINT is set but OpenTelemetry init failed; falling back to init_tracing()"
        );
    }

    #[cfg(not(feature = "otel"))]
    if otel_endpoint.is_some() {
        eprintln!(
            "sdkwork-web-bootstrap: OTEL_EXPORTER_OTLP_ENDPOINT is set but bootstrap was built without the `otel` feature; using init_tracing()"
        );
    }

    init_tracing();
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn init_tracing_from_env_falls_back_without_otel_endpoint() {
        let previous_endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT").ok();
        std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT");
        init_tracing_from_env();
        match previous_endpoint {
            Some(value) => std::env::set_var("OTEL_EXPORTER_OTLP_ENDPOINT", value),
            None => std::env::remove_var("OTEL_EXPORTER_OTLP_ENDPOINT"),
        }
    }
}
