use axum::http::HeaderName;
use sdkwork_web_core::CorsPolicy;
use tower_http::cors::AllowOrigin;
pub use tower_http::cors::CorsLayer as CanonicalCorsLayer;

/// Builds the canonical Tower/Axum CORS layer from an SDKWork framework policy.
pub fn cors_layer_from_policy(policy: CorsPolicy) -> CanonicalCorsLayer {
    let allowed_methods = policy.allowed_methods.clone();
    let mut allowed_headers = policy
        .allowed_headers
        .iter()
        // Drop the framework dev wildcard marker here; it is expanded below
        // ("*" is a valid HTTP token, so it would otherwise parse into a
        // literal "*" HeaderName and survive into the response header).
        .filter(|value| value.as_str() != "*")
        .filter_map(|value| value.parse::<HeaderName>().ok())
        .collect::<Vec<_>>();
    if policy.allowed_headers.iter().any(|header| header == "*") {
        // The framework dev wildcard ("*") must not reach tower-http as a
        // wildcard header list: tower-http panics when credentials=true is
        // combined with wildcard ACAH (invalid per the CORS spec, and
        // browsers reject it for credentialed requests anyway). Expand the
        // wildcard to the canonical default header allowlist instead.
        for value in CorsPolicy::default().allowed_headers {
            if let Ok(name) = value.parse::<HeaderName>() {
                if !allowed_headers.contains(&name) {
                    allowed_headers.push(name);
                }
            }
        }
    }
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
    async fn dev_wildcard_headers_expand_instead_of_panic_with_credentials() {
        // tower-http panics when credentials=true is combined with wildcard
        // ACAH; the dev policies ship exactly that combination and the layer
        // must expand it to the canonical default header list instead.
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
                    .header(header::ACCESS_CONTROL_REQUEST_HEADERS, "authorization")
                    .body(Body::empty())
                    .expect("preflight request"),
            )
            .await
            .expect("preflight response");
        assert_eq!(StatusCode::OK, allowed.status());
        let headers = allowed
            .headers()
            .get(header::ACCESS_CONTROL_ALLOW_HEADERS)
            .and_then(|value| value.to_str().ok())
            .unwrap_or_default();
        assert!(
            !headers.contains('*'),
            "wildcard ACAH must be expanded, got: {headers:?}"
        );
        assert!(headers.to_ascii_lowercase().contains("authorization"));
    }

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
