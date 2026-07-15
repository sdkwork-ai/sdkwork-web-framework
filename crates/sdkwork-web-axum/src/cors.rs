use axum::http::HeaderName;
use sdkwork_web_core::CorsPolicy;
use tower_http::cors::AllowOrigin;
pub use tower_http::cors::CorsLayer as CanonicalCorsLayer;

/// Builds the canonical Tower/Axum CORS layer from an SDKWork framework policy.
pub fn cors_layer_from_policy(policy: CorsPolicy) -> CanonicalCorsLayer {
    let allowed_methods = policy.allowed_methods.clone();
    let allowed_headers = policy
        .allowed_headers
        .iter()
        .filter_map(|value| value.parse::<HeaderName>().ok())
        .collect::<Vec<_>>();
    let allow_credentials = policy.allow_credentials;
    let origin_policy = policy;

    CanonicalCorsLayer::new()
        .allow_origin(AllowOrigin::predicate(move |origin, _| {
            origin
                .to_str()
                .ok()
                .is_some_and(|origin| origin_policy.allows_origin_value(origin))
        }))
        .allow_methods(allowed_methods)
        .allow_headers(allowed_headers)
        .allow_credentials(allow_credentials)
}

#[cfg(test)]
mod tests {
    use super::cors_layer_from_policy;
    use axum::body::Body;
    use axum::http::{header, Method, Request, StatusCode};
    use axum::routing::get;
    use axum::Router;
    use sdkwork_web_core::CorsPolicy;
    use tower::ServiceExt;

    #[tokio::test]
    async fn tower_layer_uses_framework_private_network_policy() {
        let router = Router::new()
            .route("/healthz", get(|| async { StatusCode::OK }))
            .layer(cors_layer_from_policy(
                CorsPolicy::development_private_network(),
            ));

        let allowed = router
            .clone()
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/healthz")
                    .header(header::ORIGIN, "http://192.168.50.12:5173")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                    .body(Body::empty())
                    .expect("preflight request"),
            )
            .await
            .expect("preflight response");
        assert_eq!(StatusCode::OK, allowed.status());
        assert_eq!(
            Some("http://192.168.50.12:5173"),
            allowed
                .headers()
                .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
                .and_then(|value| value.to_str().ok()),
        );

        let rejected = router
            .oneshot(
                Request::builder()
                    .method(Method::OPTIONS)
                    .uri("/healthz")
                    .header(header::ORIGIN, "https://evil.example.com")
                    .header(header::ACCESS_CONTROL_REQUEST_METHOD, "GET")
                    .body(Body::empty())
                    .expect("preflight request"),
            )
            .await
            .expect("preflight response");
        assert!(rejected
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_ORIGIN)
            .is_none());
    }
}
