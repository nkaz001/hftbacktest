//! Status API endpoints

use axum::{extract::State, Json};
use serde::Serialize;

use crate::api::{ApiError, ApiErrorResponse, ApiResult};
use crate::state::AppState;
use crate::supervisor::ClusterStatus;

/// GET /api/status - Get cluster status
pub async fn get_status(State(state): State<AppState>) -> ApiResult<ClusterStatus> {
    if !state.supervisor.is_available().await {
        return Err(ApiErrorResponse::service_unavailable(
            ApiError::supervisor_unavailable(),
        ));
    }

    let status = state
        .supervisor
        .get_status()
        .await
        .map_err(ApiErrorResponse::from)?;

    Ok(Json(status))
}

/// Health check response
#[derive(Debug, Serialize)]
pub struct HealthCheck {
    pub status: String,
    pub supervisor_connected: bool,
    pub version: String,
}

/// GET /api/health - Health check
pub async fn health_check(State(state): State<AppState>) -> Json<HealthCheck> {
    Json(HealthCheck {
        status: "ok".to_string(),
        supervisor_connected: state.supervisor.is_available().await,
        version: env!("CARGO_PKG_VERSION").to_string(),
    })
}
