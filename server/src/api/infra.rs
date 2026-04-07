//! Infrastructure management API endpoints

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;

use crate::api::{ApiError, ApiErrorResponse, ApiResult, ApiSuccess};
use crate::state::AppState;
use crate::supervisor::{ClusterHealthStatus, InfraStatus};

/// GET /api/infra - List all infrastructure components
pub async fn list_infra(State(state): State<AppState>) -> ApiResult<Vec<InfraStatus>> {
    if !state.supervisor.is_available().await {
        return Err(ApiErrorResponse::service_unavailable(
            ApiError::supervisor_unavailable(),
        ));
    }

    let status = state
        .supervisor
        .list_infra()
        .await
        .map_err(ApiErrorResponse::from)?;

    Ok(Json(status))
}

/// GET /api/infra/:component - Get specific component status
pub async fn get_infra_status(
    State(state): State<AppState>,
    Path(component): Path<String>,
) -> ApiResult<InfraStatus> {
    if !state.supervisor.is_available().await {
        return Err(ApiErrorResponse::service_unavailable(
            ApiError::supervisor_unavailable(),
        ));
    }

    let status = state
        .supervisor
        .get_infra_status(&component)
        .await
        .map_err(|e| ApiErrorResponse::not_found(e.to_string()))?;

    Ok(Json(status))
}

/// Infrastructure action response
#[derive(Debug, Serialize)]
pub struct InfraActionResponse {
    pub success: bool,
    pub component: String,
    pub action: String,
    pub message: String,
}

/// POST /api/infra/:component/start - Start an infrastructure component
pub async fn start_infra(
    State(state): State<AppState>,
    Path(component): Path<String>,
) -> ApiResult<InfraActionResponse> {
    if !state.supervisor.is_available().await {
        return Err(ApiErrorResponse::service_unavailable(
            ApiError::supervisor_unavailable(),
        ));
    }

    state
        .supervisor
        .start_infra(&component)
        .await
        .map_err(|e| ApiErrorResponse::bad_request(e.to_string()))?;

    Ok(Json(InfraActionResponse {
        success: true,
        component,
        action: "start".to_string(),
        message: "Component start command sent".to_string(),
    }))
}

/// POST /api/infra/:component/stop - Stop an infrastructure component
pub async fn stop_infra(
    State(state): State<AppState>,
    Path(component): Path<String>,
) -> ApiResult<InfraActionResponse> {
    if !state.supervisor.is_available().await {
        return Err(ApiErrorResponse::service_unavailable(
            ApiError::supervisor_unavailable(),
        ));
    }

    state
        .supervisor
        .stop_infra(&component)
        .await
        .map_err(|e| ApiErrorResponse::bad_request(e.to_string()))?;

    Ok(Json(InfraActionResponse {
        success: true,
        component,
        action: "stop".to_string(),
        message: "Component stop command sent".to_string(),
    }))
}

/// POST /api/infra/:component/restart - Restart an infrastructure component
pub async fn restart_infra(
    State(state): State<AppState>,
    Path(component): Path<String>,
) -> ApiResult<InfraActionResponse> {
    if !state.supervisor.is_available().await {
        return Err(ApiErrorResponse::service_unavailable(
            ApiError::supervisor_unavailable(),
        ));
    }

    state
        .supervisor
        .restart_infra(&component)
        .await
        .map_err(|e| ApiErrorResponse::bad_request(e.to_string()))?;

    Ok(Json(InfraActionResponse {
        success: true,
        component,
        action: "restart".to_string(),
        message: "Component restart command sent".to_string(),
    }))
}

/// POST /api/cluster/start-all - Start all components
pub async fn start_all(State(state): State<AppState>) -> ApiResult<ApiSuccess<()>> {
    if !state.supervisor.is_available().await {
        return Err(ApiErrorResponse::service_unavailable(
            ApiError::supervisor_unavailable(),
        ));
    }

    state
        .supervisor
        .start_all()
        .await
        .map_err(ApiErrorResponse::from)?;

    Ok(Json(ApiSuccess::<()>::with_message(
        "Start all command sent",
    )))
}

/// POST /api/cluster/stop-all - Stop all components
pub async fn stop_all(State(state): State<AppState>) -> ApiResult<ApiSuccess<()>> {
    if !state.supervisor.is_available().await {
        return Err(ApiErrorResponse::service_unavailable(
            ApiError::supervisor_unavailable(),
        ));
    }

    state
        .supervisor
        .stop_all()
        .await
        .map_err(ApiErrorResponse::from)?;

    Ok(Json(ApiSuccess::<()>::with_message(
        "Stop all command sent",
    )))
}

/// GET /api/cluster/health - Get cluster health status
pub async fn get_cluster_health(State(state): State<AppState>) -> ApiResult<ClusterHealthStatus> {
    if !state.supervisor.is_available().await {
        return Err(ApiErrorResponse::service_unavailable(
            ApiError::supervisor_unavailable(),
        ));
    }

    let health = state
        .supervisor
        .get_cluster_health()
        .await
        .map_err(ApiErrorResponse::from)?;

    Ok(Json(health))
}
