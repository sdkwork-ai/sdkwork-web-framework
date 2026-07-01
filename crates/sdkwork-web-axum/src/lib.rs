pub mod correlation;
pub mod extractors;
pub mod middleware;
pub mod timeout;
pub mod websocket;

pub use correlation::{problem_response_for_request, OwnedProblemCorrelation};
pub use extractors::{RequireOpenApi, RequirePrincipal, WebRequestContextExtractor};
pub use middleware::{
    with_server_request_identity, with_web_request_context, AppRequestContextLayer,
    WebFrameworkLayer,
};
pub use timeout::with_request_timeout;
pub use websocket::{run_websocket_session, WebSocketUpgradeLayer};

// Legacy middleware names
pub use middleware::with_web_request_context as with_app_request_context;
