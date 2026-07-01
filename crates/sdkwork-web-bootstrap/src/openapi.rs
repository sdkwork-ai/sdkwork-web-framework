use axum::response::Json;
use axum::routing::get;
use axum::Router;
use serde_json::Value;
use std::sync::Arc;

#[derive(Clone)]
pub struct OpenApiMount {
    pub path: &'static str,
    pub document: Arc<Value>,
}

async fn serve_openapi_document(document: Arc<Value>) -> Json<Value> {
    Json((*document).clone())
}

pub fn mount_openapi_json(router: Router, mounts: &[OpenApiMount]) -> Router {
    let mut router = router;
    for mount in mounts {
        let document = mount.document.clone();
        let path = mount.path;
        router = router.route(path, get(move || serve_openapi_document(document.clone())));
    }
    router
}
