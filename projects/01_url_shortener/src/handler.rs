use axum::{
    extract::{Path, State},
    http::StatusCode,
    response::{IntoResponse, Redirect},
    Json,
};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use crate::manager::AppManager;

#[derive(Deserialize)]
pub struct ShortenRequest {
    pub url: String,
}

#[derive(Serialize)]
pub struct ShortenResponse {
    pub short_code: String,
}

pub async fn shorten_handler(
    State(manager): State<Arc<AppManager>>,
    Json(payload): Json<ShortenRequest>,
) -> Result<impl IntoResponse, AppError> {
    let short_code = manager.shorten_url(&payload.url).await?;
    Ok((StatusCode::CREATED, Json(ShortenResponse { short_code })))
}

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
