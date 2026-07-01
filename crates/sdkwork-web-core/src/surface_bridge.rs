use crate::request_context::WebApiSurface;
use sdkwork_web_contract::ApiSurface;

impl From<ApiSurface> for WebApiSurface {
    fn from(value: ApiSurface) -> Self {
        match value {
            ApiSurface::OpenApi => Self::OpenApi,
            ApiSurface::AppApi => Self::AppApi,
            ApiSurface::BackendApi => Self::BackendApi,
            ApiSurface::GatewayApi => Self::GatewayApi,
            ApiSurface::Unknown => Self::Unknown,
        }
    }
}

impl From<WebApiSurface> for ApiSurface {
    fn from(value: WebApiSurface) -> Self {
        match value {
            WebApiSurface::OpenApi => Self::OpenApi,
            WebApiSurface::AppApi => Self::AppApi,
            WebApiSurface::BackendApi => Self::BackendApi,
            WebApiSurface::GatewayApi => Self::GatewayApi,
            WebApiSurface::Unknown => Self::Unknown,
        }
    }
}
