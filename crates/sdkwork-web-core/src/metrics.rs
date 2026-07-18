use crate::api_chain::WebCallState;
use crate::request_context::WebEnvironment;
use crate::surface::api_surface_contract_label;
use std::collections::HashMap;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

const DEFAULT_MAX_LABELED_REQUEST_SERIES: usize = 4_096;
const DEFAULT_MAX_STAGE_SERIES: usize = 128;
const MAX_METRIC_SERIES_KEY_BYTES: usize = 2_048;
const MAX_STAGE_LABEL_BYTES: usize = 128;

/// Process-wide Prometheus dimensions (`OBSERVABILITY_SPEC.md` §3).
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpMetricsDimensions {
    pub service: String,
    pub environment: String,
    pub deployment_profile: String,
    pub runtime_target: String,
    /// Backend runtime profile for the service (e.g. `postgresql`, `sqlite`, `memory`).
    /// Defaults to an empty string for services without a backend store profile.
    pub runtime_profile: String,
}

impl Default for HttpMetricsDimensions {
    fn default() -> Self {
        Self {
            service: std::env::var("OTEL_SERVICE_NAME")
                .ok()
                .filter(|value| !value.trim().is_empty())
                .unwrap_or_else(|| "sdkwork-web-framework".to_owned()),
            environment: environment_metric_label(&WebEnvironment::Dev).to_owned(),
            deployment_profile: "standalone".to_owned(),
            runtime_target: "server".to_owned(),
            runtime_profile: String::new(),
        }
    }
}

impl HttpMetricsDimensions {
    pub fn from_profile_environment(environment: WebEnvironment) -> Self {
        Self {
            environment: environment_metric_label(&environment).to_owned(),
            ..Self::default()
        }
    }

    pub fn with_service(mut self, service: impl Into<String>) -> Self {
        self.service = service.into();
        self
    }

    pub fn with_deployment_profile(mut self, deployment_profile: impl Into<String>) -> Self {
        self.deployment_profile = deployment_profile.into();
        self
    }

    pub fn with_runtime_target(mut self, runtime_target: impl Into<String>) -> Self {
        self.runtime_target = runtime_target.into();
        self
    }

    pub fn with_runtime_profile(mut self, runtime_profile: impl Into<String>) -> Self {
        self.runtime_profile = runtime_profile.into();
        self
    }
}

pub fn environment_metric_label(environment: &WebEnvironment) -> &'static str {
    match environment {
        WebEnvironment::Dev => "development",
        WebEnvironment::Test => "test",
        WebEnvironment::Prod => "production",
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HttpRequestLabels {
    pub dimensions: HttpMetricsDimensions,
    pub api_surface: String,
    pub route: String,
    pub method: String,
    pub status: u16,
    pub operation_id: Option<String>,
    pub backend_layer: String,
}

impl HttpRequestLabels {
    pub fn prometheus_key(&self) -> String {
        let operation_id = self
            .operation_id
            .as_deref()
            .filter(|value| !value.is_empty())
            .unwrap_or("-");
        let runtime_profile = if self.dimensions.runtime_profile.is_empty() {
            "-"
        } else {
            &self.dimensions.runtime_profile
        };
        format!(
            "service=\"{}\",environment=\"{}\",deployment_profile=\"{}\",runtime_target=\"{}\",runtime_profile=\"{}\",api_surface=\"{}\",route=\"{}\",method=\"{}\",status=\"{}\",operationId=\"{operation_id}\",backend_layer=\"{}\"",
            escape_prometheus_label(&self.dimensions.service),
            escape_prometheus_label(&self.dimensions.environment),
            escape_prometheus_label(&self.dimensions.deployment_profile),
            escape_prometheus_label(&self.dimensions.runtime_target),
            escape_prometheus_label(runtime_profile),
            escape_prometheus_label(&self.api_surface),
            escape_prometheus_label(&self.route),
            escape_prometheus_label(&self.method),
            self.status,
            escape_prometheus_label(&self.backend_layer),
        )
    }
}

pub fn http_request_labels_from_state(
    state: &WebCallState,
    dimensions: &HttpMetricsDimensions,
    status: u16,
) -> HttpRequestLabels {
    let route = state
        .route_template
        .as_deref()
        .filter(|value| !value.is_empty())
        .map(str::to_owned)
        .unwrap_or_else(|| "unmatched".to_owned());
    HttpRequestLabels {
        dimensions: dimensions.clone(),
        api_surface: api_surface_contract_label(&state.api_surface).to_owned(),
        route,
        method: state.method.clone(),
        status,
        operation_id: state.operation_id.clone(),
        backend_layer: "handler".to_owned(),
    }
}

fn escape_prometheus_label(value: &str) -> String {
    value.replace('\\', "\\\\").replace('"', "\\\"")
}

pub struct HttpMetricsRegistry {
    dimensions: Mutex<HttpMetricsDimensions>,
    requests_total: AtomicU64,
    labeled_requests: Mutex<HashMap<String, u64>>,
    stage_durations: Mutex<HashMap<String, StageDurationStats>>,
    max_labeled_request_series: usize,
    max_stage_series: usize,
    dropped_labeled_request_series_total: AtomicU64,
    dropped_stage_series_total: AtomicU64,
}

impl Default for HttpMetricsRegistry {
    fn default() -> Self {
        Self {
            dimensions: Mutex::new(HttpMetricsDimensions::default()),
            requests_total: AtomicU64::new(0),
            labeled_requests: Mutex::new(HashMap::new()),
            stage_durations: Mutex::new(HashMap::new()),
            max_labeled_request_series: DEFAULT_MAX_LABELED_REQUEST_SERIES,
            max_stage_series: DEFAULT_MAX_STAGE_SERIES,
            dropped_labeled_request_series_total: AtomicU64::new(0),
            dropped_stage_series_total: AtomicU64::new(0),
        }
    }
}

#[derive(Clone, Debug, Default)]
struct StageDurationStats {
    count: u64,
    sum_micros: u64,
}

impl HttpMetricsRegistry {
    pub fn new() -> Arc<Self> {
        Self::with_dimensions(HttpMetricsDimensions::default())
    }

    pub fn with_dimensions(dimensions: HttpMetricsDimensions) -> Arc<Self> {
        Self::with_dimensions_and_series_limits(
            dimensions,
            DEFAULT_MAX_LABELED_REQUEST_SERIES,
            DEFAULT_MAX_STAGE_SERIES,
        )
    }

    pub fn with_dimensions_and_series_limits(
        dimensions: HttpMetricsDimensions,
        max_labeled_request_series: usize,
        max_stage_series: usize,
    ) -> Arc<Self> {
        Arc::new(Self {
            dimensions: Mutex::new(dimensions),
            requests_total: AtomicU64::new(0),
            labeled_requests: Mutex::new(HashMap::new()),
            stage_durations: Mutex::new(HashMap::new()),
            max_labeled_request_series: max_labeled_request_series
                .min(DEFAULT_MAX_LABELED_REQUEST_SERIES),
            max_stage_series: max_stage_series.min(DEFAULT_MAX_STAGE_SERIES),
            dropped_labeled_request_series_total: AtomicU64::new(0),
            dropped_stage_series_total: AtomicU64::new(0),
        })
    }

    pub fn set_dimensions(&self, dimensions: HttpMetricsDimensions) {
        *self.dimensions.lock().expect("metrics dimensions mutex") = dimensions;
    }

    pub fn dimensions(&self) -> HttpMetricsDimensions {
        self.dimensions
            .lock()
            .expect("metrics dimensions mutex")
            .clone()
    }

    /// Infra scrape paths should not inflate application request counters.
    pub fn should_record_path(path: &str) -> bool {
        let normalized = path.trim();
        let normalized = if normalized.is_empty() {
            "/"
        } else {
            normalized.trim_end_matches('/')
        };
        !matches!(
            normalized,
            "/health" | "/healthz" | "/livez" | "/readyz" | "/metrics"
        )
    }

    pub fn inc_requests(&self) {
        saturating_increment(&self.requests_total);
    }

    pub fn record_request(&self, labels: &HttpRequestLabels) {
        self.inc_requests();
        let key = labels.prometheus_key();
        if key.len() > MAX_METRIC_SERIES_KEY_BYTES {
            saturating_increment(&self.dropped_labeled_request_series_total);
            return;
        }
        let mut labeled = self
            .labeled_requests
            .lock()
            .expect("metrics labeled map mutex");
        if let Some(count) = labeled.get_mut(&key) {
            *count = count.saturating_add(1);
        } else if labeled.len() < self.max_labeled_request_series {
            labeled.insert(key, 1);
        } else {
            saturating_increment(&self.dropped_labeled_request_series_total);
        }
    }

    /// Records interceptor `before` duration for catalog E2 stage timing.
    pub fn record_pipeline_stage_duration(&self, stage: &str, elapsed: std::time::Duration) {
        if stage.len() > MAX_STAGE_LABEL_BYTES {
            saturating_increment(&self.dropped_stage_series_total);
            return;
        }
        let mut stages = self
            .stage_durations
            .lock()
            .expect("metrics stage duration mutex");
        if !stages.contains_key(stage) && stages.len() >= self.max_stage_series {
            saturating_increment(&self.dropped_stage_series_total);
            return;
        }
        let entry = stages.entry(stage.to_owned()).or_default();
        entry.count = entry.count.saturating_add(1);
        entry.sum_micros = entry
            .sum_micros
            .saturating_add(elapsed.as_micros().min(u64::MAX as u128) as u64);
    }

    pub fn render_prometheus(&self) -> String {
        let dimensions = self.dimensions();
        let mut output = format!(
            "# HELP sdkwork_http_requests_total Total HTTP requests observed by the web framework.\n\
             # TYPE sdkwork_http_requests_total counter\n\
             sdkwork_http_requests_total {}\n",
            self.requests_total.load(Ordering::Relaxed)
        );
        output.push_str(
            "# HELP sdkwork_health_status Service health status (1 = serving).\n\
             # TYPE sdkwork_health_status gauge\n",
        );
        output.push_str(&format!(
            "sdkwork_health_status{{service=\"{}\",environment=\"{}\",deployment_profile=\"{}\",runtime_target=\"{}\",runtime_profile=\"{}\"}} 1\n",
            escape_prometheus_label(&dimensions.service),
            escape_prometheus_label(&dimensions.environment),
            escape_prometheus_label(&dimensions.deployment_profile),
            escape_prometheus_label(&dimensions.runtime_target),
            escape_prometheus_label(if dimensions.runtime_profile.is_empty() {
                "-"
            } else {
                &dimensions.runtime_profile
            }),
        ));
        output.push_str(
            "# HELP sdkwork_http_metric_series_dropped_total Metric observations dropped because the bounded series registry was full or a label key exceeded its byte limit.\n\
             # TYPE sdkwork_http_metric_series_dropped_total counter\n",
        );
        output.push_str(&format!(
            "sdkwork_http_metric_series_dropped_total{{kind=\"request\"}} {}\n",
            self.dropped_labeled_request_series_total
                .load(Ordering::Relaxed)
        ));
        output.push_str(&format!(
            "sdkwork_http_metric_series_dropped_total{{kind=\"pipeline_stage\"}} {}\n",
            self.dropped_stage_series_total.load(Ordering::Relaxed)
        ));
        let labeled = self
            .labeled_requests
            .lock()
            .expect("metrics labeled map mutex");
        if !labeled.is_empty() {
            output.push_str(
                "# HELP sdkwork_http_requests_labeled_total HTTP requests by route/surface/status.\n\
                 # TYPE sdkwork_http_requests_labeled_total counter\n",
            );
            for (labels, count) in labeled.iter() {
                output.push_str(&format!(
                    "sdkwork_http_requests_labeled_total{{{labels}}} {count}\n"
                ));
            }
        }
        let stages = self
            .stage_durations
            .lock()
            .expect("metrics stage duration mutex");
        if !stages.is_empty() {
            output.push_str(
                "# HELP sdkwork_pipeline_stage_duration_microseconds_sum Accumulated interceptor before-stage time.\n\
                 # TYPE sdkwork_pipeline_stage_duration_microseconds_sum counter\n",
            );
            for (stage, stats) in stages.iter() {
                let stage = escape_prometheus_label(stage);
                output.push_str(&format!(
                    "sdkwork_pipeline_stage_duration_microseconds_sum{{stage=\"{stage}\"}} {}\n",
                    stats.sum_micros
                ));
                output.push_str(&format!(
                    "sdkwork_pipeline_stage_duration_microseconds_count{{stage=\"{stage}\"}} {}\n",
                    stats.count
                ));
            }
        }
        output
    }
}

fn saturating_increment(counter: &AtomicU64) {
    let _ = counter.fetch_update(Ordering::Relaxed, Ordering::Relaxed, |current| {
        Some(current.saturating_add(1))
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::WebApiSurface;

    #[test]
    fn skips_infra_paths() {
        assert!(!HttpMetricsRegistry::should_record_path("/healthz"));
        assert!(!HttpMetricsRegistry::should_record_path("/livez"));
        assert!(!HttpMetricsRegistry::should_record_path("/metrics"));
        assert!(HttpMetricsRegistry::should_record_path("/app/v3/api/users"));
    }

    #[test]
    fn increments_request_counter() {
        let registry = HttpMetricsRegistry::new();
        registry.inc_requests();
        registry.inc_requests();
        assert!(registry
            .render_prometheus()
            .contains("sdkwork_http_requests_total 2"));
    }

    #[test]
    fn records_labeled_counters_with_observability_labels() {
        let registry = HttpMetricsRegistry::with_dimensions(HttpMetricsDimensions {
            service: "orders-api".to_owned(),
            environment: "production".to_owned(),
            deployment_profile: "cloud".to_owned(),
            runtime_target: "server".to_owned(),
            runtime_profile: "postgresql".to_owned(),
        });
        registry.record_request(&HttpRequestLabels {
            dimensions: registry.dimensions(),
            api_surface: "app-api".to_owned(),
            route: "/app/v3/api/users/{userId}".to_owned(),
            method: "GET".to_owned(),
            status: 200,
            operation_id: Some("users.list".to_owned()),
            backend_layer: "handler".to_owned(),
        });
        let rendered = registry.render_prometheus();
        assert!(rendered.contains("sdkwork_http_requests_labeled_total"));
        assert!(rendered.contains("service=\"orders-api\""));
        assert!(rendered.contains("api_surface=\"app-api\""));
        assert!(rendered.contains("route=\"/app/v3/api/users/{userId}\""));
        assert!(rendered.contains("operationId=\"users.list\""));
        assert!(rendered.contains("backend_layer=\"handler\""));
        assert!(rendered.contains("runtime_profile=\"postgresql\""));
        assert!(rendered.contains("sdkwork_health_status"));
    }

    #[test]
    fn records_pipeline_stage_durations() {
        let registry = HttpMetricsRegistry::new();
        registry.record_pipeline_stage_duration("cors", std::time::Duration::from_micros(25));
        let rendered = registry.render_prometheus();
        assert!(rendered
            .contains("sdkwork_pipeline_stage_duration_microseconds_sum{stage=\"cors\"} 25"));
        assert!(rendered
            .contains("sdkwork_pipeline_stage_duration_microseconds_count{stage=\"cors\"} 1"));
    }

    #[test]
    fn builds_labels_from_call_state() {
        let request = axum::http::Request::builder()
            .method("GET")
            .uri("/app/v3/api/users/42")
            .body(axum::body::Body::empty())
            .expect("request");
        let mut state = WebCallState::from_request(&request);
        state.api_surface = WebApiSurface::AppApi;
        state.route_template = Some("/app/v3/api/users/{userId}".to_owned());
        state.operation_id = Some("users.get".to_owned());
        let labels = http_request_labels_from_state(&state, &HttpMetricsDimensions::default(), 200);
        assert_eq!("app-api", labels.api_surface);
        assert_eq!("/app/v3/api/users/{userId}", labels.route);
    }

    #[test]
    fn unmatched_paths_share_one_bounded_series() {
        let registry = HttpMetricsRegistry::with_dimensions_and_series_limits(
            HttpMetricsDimensions::default(),
            4,
            2,
        );
        for path in ["/arbitrary-alpha", "/arbitrary-beta", "/users/alice"] {
            let request = axum::http::Request::builder()
                .method("GET")
                .uri(path)
                .body(axum::body::Body::empty())
                .expect("request");
            let state = WebCallState::from_request(&request);
            registry.record_request(&http_request_labels_from_state(
                &state,
                &registry.dimensions(),
                404,
            ));
        }
        let rendered = registry.render_prometheus();
        assert!(rendered.contains("route=\"unmatched\""));
        assert!(!rendered.contains("arbitrary-alpha"));
        assert!(!rendered.contains("arbitrary-beta"));
    }

    #[test]
    fn series_limits_drop_new_cardinality_without_losing_existing_counters() {
        let registry = HttpMetricsRegistry::with_dimensions_and_series_limits(
            HttpMetricsDimensions::default(),
            1,
            1,
        );
        for route in ["/known-a", "/known-a", "/known-b"] {
            registry.record_request(&HttpRequestLabels {
                dimensions: registry.dimensions(),
                api_surface: "app-api".to_owned(),
                route: route.to_owned(),
                method: "GET".to_owned(),
                status: 200,
                operation_id: None,
                backend_layer: "handler".to_owned(),
            });
        }
        registry.record_pipeline_stage_duration("cors", std::time::Duration::from_micros(1));
        registry.record_pipeline_stage_duration("auth", std::time::Duration::from_micros(1));
        let rendered = registry.render_prometheus();
        assert!(rendered.contains("route=\"/known-a\""));
        assert!(!rendered.contains("route=\"/known-b\""));
        assert!(rendered.contains("sdkwork_http_metric_series_dropped_total{kind=\"request\"} 1"));
        assert!(rendered
            .contains("sdkwork_http_metric_series_dropped_total{kind=\"pipeline_stage\"} 1"));
    }
}
