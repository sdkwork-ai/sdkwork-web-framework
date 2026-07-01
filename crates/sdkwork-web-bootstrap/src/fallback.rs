use axum::extract::Request;
use axum::response::Response;
use sdkwork_web_axum::problem_response_for_request;
use sdkwork_web_contract::HttpRoute;
use sdkwork_web_core::WebFrameworkError;
use std::collections::HashSet;

#[derive(Clone, Debug, Default)]
pub struct ContractFallbackConfig {
    pub manifest_paths: HashSet<String>,
}

impl ContractFallbackConfig {
    pub fn from_routes(routes: &[HttpRoute]) -> Self {
        Self {
            manifest_paths: routes
                .iter()
                .map(|route| format!("{} {}", method_label(route.method), route.path))
                .collect(),
        }
    }

    pub fn from_manifest(manifest: &sdkwork_web_core::HttpRouteManifest) -> Self {
        Self::from_routes(manifest.routes())
    }

    pub fn contains(&self, method: &str, path: &str) -> bool {
        self.manifest_paths.contains(&format!("{method} {path}"))
    }
}

fn method_label(method: sdkwork_web_contract::HttpMethod) -> &'static str {
    use sdkwork_web_contract::HttpMethod;
    match method {
        HttpMethod::Get => "GET",
        HttpMethod::Post => "POST",
        HttpMethod::Put => "PUT",
        HttpMethod::Patch => "PATCH",
        HttpMethod::Delete => "DELETE",
    }
}

pub async fn contract_fallback_handler(
    request: Request,
    config: ContractFallbackConfig,
) -> Response {
    let method = request.method().as_str();
    let path = request.uri().path();
    let error = if config.contains(method, path) {
        WebFrameworkError::not_implemented(
            "route is declared in contract manifest but handler is not mounted",
        )
    } else {
        WebFrameworkError::not_found("route is not registered")
    };
    problem_response_for_request(&error, &request)
}
