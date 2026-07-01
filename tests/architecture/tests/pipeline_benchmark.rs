//! Release-profile pipeline overhead benchmark (catalog K6 / maturity §3.1).
//!
//! Debug CI budget: p99 < 25ms. Release M4 target: p99 < 0.5ms @ empty handler path.

use axum::body::Body;
use axum::extract::Request;
use sdkwork_web_core::{
    memory_rate_limit_store, DefaultWebRequestContextResolver, WebCallInterceptorChain,
    WebCallRuntime,
};
use std::time::{Duration, Instant};

const DEBUG_P99_BUDGET: Duration = Duration::from_millis(25);
const RELEASE_P99_BUDGET: Duration = Duration::from_micros(500);

fn profile_p99_budget() -> Duration {
    if cfg!(debug_assertions) {
        DEBUG_P99_BUDGET
    } else {
        RELEASE_P99_BUDGET
    }
}

async fn run_pipeline_before_once(runtime: &WebCallRuntime<DefaultWebRequestContextResolver>) {
    let chain = WebCallInterceptorChain::standard();
    let mut request = Request::builder()
        .method("GET")
        .uri("/app/v3/api/users")
        .header(
            "Authorization",
            "Bearer api_key_id=key;tenant_id=100001;organization_id=0;user_id=30;app_id=app;environment=prod;deployment_mode=saas;data_scope=tenant;permission_scope=read",
        )
        .header(
            "Access-Token",
            "tenant_id=100001;organization_id=0;user_id=30;app_id=app;environment=prod;deployment_mode=saas",
        )
        .body(Body::empty())
        .expect("request");
    let mut state = sdkwork_web_core::WebCallState::from_request(&request);
    let _ = chain.before(&mut state, &mut request, runtime).await;
}

#[tokio::test]
async fn pipeline_before_p99_within_maturity_budget() {
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_rate_limit_store(memory_rate_limit_store());
    let samples = if cfg!(debug_assertions) { 256 } else { 1024 };
    let mut durations = Vec::with_capacity(samples);

    for _ in 0..samples {
        let start = Instant::now();
        run_pipeline_before_once(&runtime).await;
        durations.push(start.elapsed());
    }

    durations.sort();
    let p99 = durations[durations.len() * 99 / 100];
    let budget = profile_p99_budget();
    assert!(
        p99 <= budget,
        "pipeline before p99 {:?} exceeded budget {:?} (profile={})",
        p99,
        budget,
        if cfg!(debug_assertions) {
            "debug"
        } else {
            "release"
        }
    );
}

#[tokio::test]
async fn pipeline_before_median_within_half_of_p99_budget() {
    let runtime = WebCallRuntime::new(DefaultWebRequestContextResolver::default())
        .with_rate_limit_store(memory_rate_limit_store());
    let samples = 128;
    let mut durations = Vec::with_capacity(samples);

    for _ in 0..samples {
        let start = Instant::now();
        run_pipeline_before_once(&runtime).await;
        durations.push(start.elapsed());
    }

    durations.sort();
    let median = durations[durations.len() / 2];
    let budget = profile_p99_budget() / 2;
    assert!(
        median <= budget,
        "pipeline before median {:?} exceeded half of p99 budget {:?}",
        median,
        budget
    );
}
