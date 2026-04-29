use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use axum::Json;

#[derive(Debug, thiserror::Error)]
pub enum ApiError {
    #[error("{0}")]
    BadRequest(String),
    #[error("{0}")]
    BadGateway(String),
    #[error("{0}")]
    GatewayTimeout(String),
    #[error("{0}")]
    Internal(String),
    #[error(transparent)]
    Sql(#[from] sqlx::Error),
    #[error(transparent)]
    Any(#[from] anyhow::Error),
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let log = self.to_string();
        let (status, msg) = match self {
            ApiError::BadRequest(m) => (StatusCode::BAD_REQUEST, m),
            ApiError::BadGateway(m) => (StatusCode::BAD_GATEWAY, m),
            ApiError::GatewayTimeout(m) => (StatusCode::GATEWAY_TIMEOUT, m),
            ApiError::Internal(m) => (StatusCode::INTERNAL_SERVER_ERROR, m),
            ApiError::Sql(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
            ApiError::Any(e) => (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()),
        };
        tracing::error!(%log, "request error");
        let body = Json(serde_json::json!({ "error": msg }));
        (status, body).into_response()
    }
}
