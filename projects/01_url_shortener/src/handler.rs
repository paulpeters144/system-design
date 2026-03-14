use crate::manager::AppManager;
use axum::{
    Json,
    extract::{ConnectInfo, Path, State},
    http::{HeaderMap, StatusCode, header},
    response::{IntoResponse, Redirect},
};
use serde::{Deserialize, Serialize};
use std::net::SocketAddr;
use std::sync::Arc;
use utoipa::ToSchema;

#[derive(Deserialize, ToSchema)]
pub struct ShortenRequest {
    pub url: String,
}

#[derive(Serialize, ToSchema)]
pub struct ShortenResponse {
    pub short_code: String,
}

#[utoipa::path(
    post,
    path = "/shorten",
    request_body = ShortenRequest,
    responses(
        (status = 201, description = "URL shortened successfully", body = ShortenResponse),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn shorten_handler(
    State(manager): State<Arc<AppManager>>,
    Json(payload): Json<ShortenRequest>,
) -> Result<impl IntoResponse, AppError> {
    let short_code = manager.shorten_url(&payload.url).await?;
    Ok((StatusCode::CREATED, Json(ShortenResponse { short_code })))
}

#[utoipa::path(
    get,
    path = "/{code}",
    params(
        ("code" = String, Path, description = "The short code to redirect from")
    ),
    responses(
        (status = 307, description = "Redirect to the long URL"),
        (status = 404, description = "Short code not found"),
        (status = 500, description = "Internal server error")
    )
)]
pub async fn redirect_handler(
    Path(code): Path<String>,
    headers: HeaderMap,
    ConnectInfo(addr): ConnectInfo<SocketAddr>,
    State(manager): State<Arc<AppManager>>,
) -> Result<impl IntoResponse, AppError> {
    // We need the record (not just the long URL) to get the url_id for analytics
    let record = manager.get_record_by_code(&code).await?;

    match record {
        Some(r) => {
            let url = r.long_url.clone();
            let url_id = r.id;
            let ip = addr.ip().to_string();
            let ua = headers
                .get(header::USER_AGENT)
                .and_then(|v| v.to_str().ok())
                .map(|s| s.to_string());

            // Record analytics asynchronously
            let manager_clone = manager.clone();
            tokio::spawn(async move {
                manager_clone.record_analytics(url_id, Some(ip), ua).await;
            });

            // Note: manager.get_long_url handles caching, but since we needed the ID,
            // we called get_record_by_code. To benefit from caching, we could
            // call get_long_url if we don't care about ID, but the plan asks for analytics.
            // If we want both, we might need to cache the whole record.

            Ok(Redirect::temporary(&url))
        }
        None => Err(AppError::NotFound),
    }
}

pub enum AppError {
    NotFound,
    Other(anyhow::Error),
}

impl IntoResponse for AppError {
    fn into_response(self) -> axum::response::Response {
        match self {
            AppError::NotFound => (StatusCode::NOT_FOUND, "Not Found").into_response(),
            AppError::Other(err) => (
                StatusCode::INTERNAL_SERVER_ERROR,
                format!("Internal server error: {}", err),
            )
                .into_response(),
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Other(err)
    }
}
