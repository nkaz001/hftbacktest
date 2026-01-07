//! Configuration API endpoints

use std::path::PathBuf;

use axum::{
    extract::{Path, State},
    Json,
};
use serde::{Deserialize, Serialize};

use crate::api::{ApiErrorResponse, ApiResult, ApiSuccess};
use crate::state::AppState;

/// Config file info
#[derive(Debug, Serialize)]
pub struct ConfigFileInfo {
    pub path: String,
    pub exists: bool,
    pub size: Option<u64>,
    pub modified: Option<String>,
}

/// GET /api/config/cluster - Get cluster config content
pub async fn get_cluster_config(
    State(state): State<AppState>,
) -> ApiResult<serde_json::Value> {
    let config_path = &state.config.cluster_config_path;

    if !config_path.exists() {
        return Err(ApiErrorResponse::not_found(format!(
            "Config file not found: {}",
            config_path.display()
        )));
    }

    let content = tokio::fs::read_to_string(config_path)
        .await
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

    // Parse TOML and convert to JSON for easier frontend handling
    let value: toml::Value = toml::from_str(&content)
        .map_err(|e| ApiErrorResponse::internal(format!("Failed to parse config: {}", e)))?;

    let json_value = serde_json::to_value(value)
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

    Ok(Json(json_value))
}

/// Update config request
#[derive(Debug, Deserialize)]
pub struct UpdateConfigRequest {
    pub content: String,
}

/// PUT /api/config/cluster - Update cluster config
pub async fn update_cluster_config(
    State(state): State<AppState>,
    Json(req): Json<UpdateConfigRequest>,
) -> ApiResult<ApiSuccess<()>> {
    let config_path = &state.config.cluster_config_path;

    // Validate TOML syntax
    let _: toml::Value = toml::from_str(&req.content)
        .map_err(|e| ApiErrorResponse::bad_request(format!("Invalid TOML: {}", e)))?;

    // Backup existing config
    if config_path.exists() {
        let backup_path = config_path.with_extension("toml.bak");
        tokio::fs::copy(config_path, &backup_path)
            .await
            .map_err(|e| ApiErrorResponse::internal(format!("Failed to backup config: {}", e)))?;
    }

    // Write new config
    tokio::fs::write(config_path, &req.content)
        .await
        .map_err(|e| ApiErrorResponse::internal(format!("Failed to write config: {}", e)))?;

    // Trigger reload
    if state.supervisor.is_available().await {
        if let Err(e) = state.supervisor.reload().await {
            tracing::warn!(error = %e, "Failed to reload supervisor config");
        }
    }

    Ok(Json(ApiSuccess::<()>::with_message("Configuration updated")))
}

/// Strategy config info
#[derive(Debug, Serialize)]
pub struct StrategyConfigInfo {
    pub bot_id: String,
    pub config_path: String,
    pub exists: bool,
}

/// GET /api/config/strategies - List strategy configs
pub async fn list_strategy_configs(
    State(state): State<AppState>,
) -> ApiResult<Vec<StrategyConfigInfo>> {
    // Read cluster config to get strategy config paths
    let cluster_config_path = &state.config.cluster_config_path;

    if !cluster_config_path.exists() {
        return Err(ApiErrorResponse::not_found("Cluster config not found"));
    }

    let content = tokio::fs::read_to_string(cluster_config_path)
        .await
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

    let config: toml::Value = toml::from_str(&content)
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

    let mut strategies = Vec::new();

    if let Some(bots) = config.get("bots").and_then(|b| b.as_array()) {
        for bot in bots {
            if let (Some(id), Some(config_file)) = (
                bot.get("id").and_then(|v| v.as_str()),
                bot.get("config_file").and_then(|v| v.as_str()),
            ) {
                let path = PathBuf::from(config_file);
                strategies.push(StrategyConfigInfo {
                    bot_id: id.to_string(),
                    config_path: config_file.to_string(),
                    exists: path.exists(),
                });
            }
        }
    }

    Ok(Json(strategies))
}

/// GET /api/config/strategies/:bot_id - Get strategy config content
pub async fn get_strategy_config(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
) -> ApiResult<serde_json::Value> {
    // Find the config path for this bot
    let cluster_config_path = &state.config.cluster_config_path;
    let content = tokio::fs::read_to_string(cluster_config_path)
        .await
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

    let config: toml::Value = toml::from_str(&content)
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

    let config_path = config
        .get("bots")
        .and_then(|b| b.as_array())
        .and_then(|bots| {
            bots.iter().find(|b| {
                b.get("id")
                    .and_then(|v| v.as_str())
                    .map(|id| id == bot_id)
                    .unwrap_or(false)
            })
        })
        .and_then(|b| b.get("config_file"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiErrorResponse::not_found(format!("Bot not found: {}", bot_id)))?;

    let strategy_content = tokio::fs::read_to_string(config_path)
        .await
        .map_err(|e| {
            ApiErrorResponse::not_found(format!("Strategy config not found: {}", e))
        })?;

    let value: toml::Value = toml::from_str(&strategy_content)
        .map_err(|e| ApiErrorResponse::internal(format!("Failed to parse config: {}", e)))?;

    let json_value = serde_json::to_value(value)
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

    Ok(Json(json_value))
}

/// PUT /api/config/strategies/:bot_id - Update strategy config
pub async fn update_strategy_config(
    State(state): State<AppState>,
    Path(bot_id): Path<String>,
    Json(req): Json<UpdateConfigRequest>,
) -> ApiResult<ApiSuccess<()>> {
    // Find the config path for this bot
    let cluster_config_path = &state.config.cluster_config_path;
    let content = tokio::fs::read_to_string(cluster_config_path)
        .await
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

    let config: toml::Value = toml::from_str(&content)
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

    let config_path = config
        .get("bots")
        .and_then(|b| b.as_array())
        .and_then(|bots| {
            bots.iter().find(|b| {
                b.get("id")
                    .and_then(|v| v.as_str())
                    .map(|id| id == bot_id)
                    .unwrap_or(false)
            })
        })
        .and_then(|b| b.get("config_file"))
        .and_then(|v| v.as_str())
        .ok_or_else(|| ApiErrorResponse::not_found(format!("Bot not found: {}", bot_id)))?;

    // Validate TOML syntax
    let _: toml::Value = toml::from_str(&req.content)
        .map_err(|e| ApiErrorResponse::bad_request(format!("Invalid TOML: {}", e)))?;

    // Backup existing config
    let path = PathBuf::from(config_path);
    if path.exists() {
        let backup_path = path.with_extension("toml.bak");
        tokio::fs::copy(&path, &backup_path)
            .await
            .map_err(|e| ApiErrorResponse::internal(format!("Failed to backup config: {}", e)))?;
    }

    // Write new config
    tokio::fs::write(&path, &req.content)
        .await
        .map_err(|e| ApiErrorResponse::internal(format!("Failed to write config: {}", e)))?;

    Ok(Json(ApiSuccess::<()>::with_message(format!(
        "Strategy config for {} updated",
        bot_id
    ))))
}
