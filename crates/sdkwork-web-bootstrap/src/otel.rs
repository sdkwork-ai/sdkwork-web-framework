//! Optional OpenTelemetry export (catalog E8) — enable bootstrap `otel` feature.
//!
//! The exporter uses OTLP/gRPC (`opentelemetry-otlp` `grpc-tonic` feature) so
//! the default endpoint `http://localhost:4317` matches the standard OTLP
//! collector gRPC port emitted by the SDKWork Kubernetes ConfigMap
//! (`OTEL_EXPORTER_OTLP_PROTOCOL=grpc`).
//!
//! Trace sampling follows the OpenTelemetry environment variable contract:
//! `OTEL_TRACES_SAMPLER_ARG` (default `0.1`) is parsed as a `parentbased`
//! `trace_id_ratio` ratio in `[0.0, 1.0]`. Values outside that range fall back
//! to the default.

/// Default trace sampling ratio when `OTEL_TRACES_SAMPLER_ARG` is unset or
/// cannot be parsed. Matches the SDKWork Helm chart default
/// (`observability.tracesSamplingRatio: "0.1"`).
pub const DEFAULT_TRACES_SAMPLER_RATIO: f64 = 0.1;

/// Default OTLP/gRPC endpoint. Mirrors the production Helm chart overlay
/// (`http://otel-collector.observability.svc.cluster.local:4317`) and the
/// OpenTelemetry collector default gRPC port.
pub const DEFAULT_OTLP_ENDPOINT: &str = "http://localhost:4317";

/// Parses `OTEL_TRACES_SAMPLER_ARG` as a ratio in `[0.0, 1.0]`.
///
/// Returns [`DEFAULT_TRACES_SAMPLER_RATIO`] when the variable is unset, empty,
/// or outside the valid range. This mirrors the OpenTelemetry
/// `parentbased_traceidratio` sampler contract used by the SDKWork Helm chart.
pub fn resolve_traces_sampler_ratio() -> f64 {
    match std::env::var("OTEL_TRACES_SAMPLER_ARG") {
        Ok(raw) => parse_sampler_ratio(&raw),
        Err(_) => DEFAULT_TRACES_SAMPLER_RATIO,
    }
}

/// Parses a sampler ratio string, returning [`DEFAULT_TRACES_SAMPLER_RATIO`]
/// for empty or out-of-range values.
fn parse_sampler_ratio(raw: &str) -> f64 {
    let trimmed = raw.trim();
    if trimmed.is_empty() {
        return DEFAULT_TRACES_SAMPLER_RATIO;
    }
    match trimmed.parse::<f64>() {
        Ok(value) if (0.0..=1.0).contains(&value) => value,
        _ => DEFAULT_TRACES_SAMPLER_RATIO,
    }
}

#[cfg(feature = "otel")]
pub fn init_otel_tracing(
    service_name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    use opentelemetry::trace::TracerProvider as _;
    use opentelemetry_otlp::WithExportConfig;
    use opentelemetry_sdk::trace::{Sampler, TracerProvider};
    use tracing_subscriber::{layer::SubscriberExt, util::SubscriberInitExt, EnvFilter};

    let endpoint = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
        .unwrap_or_else(|| DEFAULT_OTLP_ENDPOINT.to_owned());

    let sampler_ratio = resolve_traces_sampler_ratio();

    let exporter = opentelemetry_otlp::SpanExporter::builder()
        .with_tonic()
        .with_endpoint(endpoint)
        .build()?;
    let provider = TracerProvider::builder()
        .with_sampler(Sampler::ParentBased(Box::new(Sampler::TraceIdRatioBased(
            sampler_ratio,
        ))))
        .with_batch_exporter(exporter, opentelemetry_sdk::runtime::Tokio)
        .build();
    let tracer = provider.tracer(service_name.to_owned());
    let telemetry = tracing_opentelemetry::layer().with_tracer(tracer);
    tracing_subscriber::registry()
        .with(EnvFilter::from_default_env())
        .with(
            tracing_subscriber::fmt::layer().fmt_fields(crate::tracing_init::RedactingFormatFields),
        )
        .with(telemetry)
        .try_init()?;
    Ok(())
}

#[cfg(not(feature = "otel"))]
pub fn init_otel_tracing(
    _service_name: &str,
) -> Result<(), Box<dyn std::error::Error + Send + Sync>> {
    Err("bootstrap compiled without `otel` feature".into())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Restores an environment variable to its prior value when dropped. Test
    /// isolation is important because the sampler resolver reads process env.
    struct EnvGuard {
        key: &'static str,
        previous: Option<String>,
    }

    impl EnvGuard {
        fn lock(key: &'static str) -> Self {
            let previous = std::env::var(key).ok();
            Self { key, previous }
        }
    }

    impl Drop for EnvGuard {
        fn drop(&mut self) {
            match &self.previous {
                Some(value) => std::env::set_var(self.key, value),
                None => std::env::remove_var(self.key),
            }
        }
    }

    /// Serializes tests that mutate `OTEL_TRACES_SAMPLER_ARG`. Without this,
    /// parallel test execution would race on the shared process environment.
    static SAMPLER_ENV_MUTEX: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn default_ratio_is_used_when_env_missing() {
        // Acquire mutex BEFORE EnvGuard so drop order restores env var while
        // still holding the mutex (env restores first, mutex releases second).
        let _lock = SAMPLER_ENV_MUTEX.lock().unwrap();
        let _env = EnvGuard::lock("OTEL_TRACES_SAMPLER_ARG");
        std::env::remove_var("OTEL_TRACES_SAMPLER_ARG");
        assert_eq!(resolve_traces_sampler_ratio(), DEFAULT_TRACES_SAMPLER_RATIO);
        assert_eq!(DEFAULT_TRACES_SAMPLER_RATIO, 0.1);
    }

    #[test]
    fn explicit_ratio_is_parsed() {
        let _lock = SAMPLER_ENV_MUTEX.lock().unwrap();
        let _env = EnvGuard::lock("OTEL_TRACES_SAMPLER_ARG");

        std::env::set_var("OTEL_TRACES_SAMPLER_ARG", "0.25");
        assert_eq!(resolve_traces_sampler_ratio(), 0.25);

        std::env::set_var("OTEL_TRACES_SAMPLER_ARG", "1.0");
        assert_eq!(resolve_traces_sampler_ratio(), 1.0);

        std::env::set_var("OTEL_TRACES_SAMPLER_ARG", "0");
        assert_eq!(resolve_traces_sampler_ratio(), 0.0);
    }

    #[test]
    fn out_of_range_or_invalid_falls_back_to_default() {
        let _lock = SAMPLER_ENV_MUTEX.lock().unwrap();
        let _env = EnvGuard::lock("OTEL_TRACES_SAMPLER_ARG");

        std::env::set_var("OTEL_TRACES_SAMPLER_ARG", "1.5");
        assert_eq!(resolve_traces_sampler_ratio(), DEFAULT_TRACES_SAMPLER_RATIO);

        std::env::set_var("OTEL_TRACES_SAMPLER_ARG", "-0.1");
        assert_eq!(resolve_traces_sampler_ratio(), DEFAULT_TRACES_SAMPLER_RATIO);

        std::env::set_var("OTEL_TRACES_SAMPLER_ARG", "garbage");
        assert_eq!(resolve_traces_sampler_ratio(), DEFAULT_TRACES_SAMPLER_RATIO);

        std::env::set_var("OTEL_TRACES_SAMPLER_ARG", "  ");
        assert_eq!(resolve_traces_sampler_ratio(), DEFAULT_TRACES_SAMPLER_RATIO);
    }

    #[test]
    fn default_endpoint_targets_grpc_collector_port() {
        assert_eq!(DEFAULT_OTLP_ENDPOINT, "http://localhost:4317");
    }
}
