use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::manager::AppManager;
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
    State(manager): State<Arc<AppManager>>,
) -> Result<impl IntoResponse, AppError> {
    match manager.get_long_url(&code).await? {
        Some(url) => Ok(Redirect::temporary(&url)),
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
            ).into_response(),
        }
    }
}

impl From<anyhow::Error> for AppError {
    fn from(err: anyhow::Error) -> Self {
        AppError::Other(err)
    }
}
