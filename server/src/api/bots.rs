//! Bot management API endpoints

use axum::{
    extract::{Path, State},
    Json,
};
use serde::Serialize;

use crate::api::{ApiError, ApiErrorResponse, ApiResult, ApiSuccess};
use crate::state::AppState;
use crate::supervisor::BotStatus;

/// GET /api/bots - List all bots
pub async fn list_bots(State(state): State<AppState>) -> ApiResult<Vec<BotStatus>> {
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

    Ok(Json(status.bots))
}

/// GET /api/bots/:id - Get specific bot status
pub async fn get_bot(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> ApiResult<BotStatus> {
    if !state.supervisor.is_available().await {
        return Err(ApiErrorResponse::service_unavailable(
            ApiError::supervisor_unavailable(),
        ));
    }

    let status = state
        .supervisor
        .get_bot_status(&bot_id)
        .await
        .map_err(|e| ApiErrorResponse::not_found(e.to_string()))?;

    Ok(Json(status))
}

/// Bot action response
#[derive(Debug, Serialize)]
pub struct BotActionResponse {
    pub success: bool,
    pub bot_id: String,
    pub action: String,
    pub message: String,
}

/// POST /api/bots/:id/start - Start a bot
pub async fn start_bot(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> ApiResult<BotActionResponse> {
    if !state.supervisor.is_available().await {
        return Err(ApiErrorResponse::service_unavailable(
            ApiError::supervisor_unavailable(),
        ));
    }

    state
        .supervisor
        .start_bot(&bot_id)
        .await
        .map_err(|e| ApiErrorResponse::bad_request(e.to_string()))?;

    Ok(Json(BotActionResponse {
        success: true,
        bot_id,
        action: "start".to_string(),
        message: "Bot start command sent".to_string(),
    }))
}

/// POST /api/bots/:id/stop - Stop a bot
pub async fn stop_bot(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> ApiResult<BotActionResponse> {
    if !state.supervisor.is_available().await {
        return Err(ApiErrorResponse::service_unavailable(
            ApiError::supervisor_unavailable(),
        ));
    }

    state
        .supervisor
        .stop_bot(&bot_id)
        .await
        .map_err(|e| ApiErrorResponse::bad_request(e.to_string()))?;

    Ok(Json(BotActionResponse {
        success: true,
        bot_id,
        action: "stop".to_string(),
        message: "Bot stop command sent".to_string(),
    }))
}

/// POST /api/bots/:id/restart - Restart a bot
pub async fn restart_bot(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> ApiResult<BotActionResponse> {
    if !state.supervisor.is_available().await {
        return Err(ApiErrorResponse::service_unavailable(
            ApiError::supervisor_unavailable(),
        ));
    }

    state
        .supervisor
        .restart_bot(&bot_id)
        .await
        .map_err(|e| ApiErrorResponse::bad_request(e.to_string()))?;

    Ok(Json(BotActionResponse {
        success: true,
        bot_id,
        action: "restart".to_string(),
        message: "Bot restart command sent".to_string(),
    }))
}

/// POST /api/bots/restart-all - Restart all bots
pub async fn restart_all_bots(
    State(state): State<AppState>,
) -> ApiResult<ApiSuccess<()>> {
    if !state.supervisor.is_available().await {
        return Err(ApiErrorResponse::service_unavailable(
            ApiError::supervisor_unavailable(),
        ));
    }

    state
        .supervisor
        .restart_all()
        .await
        .map_err(ApiErrorResponse::from)?;

    Ok(Json(ApiSuccess::<()>::with_message("All bots restart command sent")))
}
