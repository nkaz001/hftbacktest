//! Data file management API endpoints

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use tokio::fs;

use crate::api::{ApiErrorResponse, ApiResult};
use crate::state::AppState;

/// Data file info
#[derive(Debug, Serialize)]
pub struct DataFileInfo {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified: String,
    pub symbol: Option<String>,
}

/// Query params for listing data files
#[derive(Debug, Deserialize)]
pub struct ListDataFilesQuery {
    pub symbol: Option<String>,
    pub limit: Option<usize>,
}

/// GET /api/data/files - List data files
pub async fn list_data_files(
    State(state): State<AppState>,
    Query(query): Query<ListDataFilesQuery>,
) -> ApiResult<Vec<DataFileInfo>> {
    let data_dir = &state.config.data_dir;

    if !data_dir.exists() {
        return Ok(Json(vec![]));
    }

    let mut files = Vec::new();
    let mut entries = fs::read_dir(data_dir)
        .await
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?
    {
        let path = entry.path();

        // Only include .npz files
        if path.extension().and_then(|e| e.to_str()) != Some("npz") {
            continue;
        }

        let metadata = entry
            .metadata()
            .await
            .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        // Extract symbol from filename (e.g., "BTCUSDT_20240101.npz" -> "BTCUSDT")
        let symbol = name.split('_').next().map(|s| s.to_string());

        // Filter by symbol if specified
        if let Some(ref filter_symbol) = query.symbol {
            if symbol.as_ref() != Some(filter_symbol) {
                continue;
            }
        }

        let modified = metadata
            .modified()
            .ok()
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map(|d| {
                chrono::DateTime::from_timestamp(d.as_secs() as i64, 0)
                    .map(|dt| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_default()
            })
            .unwrap_or_default();

        files.push(DataFileInfo {
            name,
            path: path.to_string_lossy().to_string(),
            size: metadata.len(),
            modified,
            symbol,
        });
    }

    // Sort by modified time descending
    files.sort_by(|a, b| b.modified.cmp(&a.modified));

    // Apply limit
    if let Some(limit) = query.limit {
        files.truncate(limit);
    }

    Ok(Json(files))
}

/// Data directory stats
#[derive(Debug, Serialize)]
pub struct DataStats {
    pub total_files: usize,
    pub total_size: u64,
    pub symbols: Vec<SymbolStats>,
}

#[derive(Debug, Serialize)]
pub struct SymbolStats {
    pub symbol: String,
    pub file_count: usize,
    pub total_size: u64,
    pub latest_file: Option<String>,
}

/// GET /api/data/stats - Get data statistics
pub async fn get_data_stats(State(state): State<AppState>) -> ApiResult<DataStats> {
    let data_dir = &state.config.data_dir;

    if !data_dir.exists() {
        return Ok(Json(DataStats {
            total_files: 0,
            total_size: 0,
            symbols: vec![],
        }));
    }

    let mut total_files = 0;
    let mut total_size = 0u64;
    let mut symbol_map: std::collections::HashMap<String, (usize, u64, String)> =
        std::collections::HashMap::new();

    let mut entries = fs::read_dir(data_dir)
        .await
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?
    {
        let path = entry.path();

        if path.extension().and_then(|e| e.to_str()) != Some("npz") {
            continue;
        }

        let metadata = entry
            .metadata()
            .await
            .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

        let name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("")
            .to_string();

        let symbol = name
            .split('_')
            .next()
            .unwrap_or("unknown")
            .to_string();

        total_files += 1;
        total_size += metadata.len();

        let entry = symbol_map.entry(symbol).or_insert((0, 0, String::new()));
        entry.0 += 1;
        entry.1 += metadata.len();
        if name > entry.2 {
            entry.2 = name;
        }
    }

    let symbols: Vec<SymbolStats> = symbol_map
        .into_iter()
        .map(|(symbol, (count, size, latest))| SymbolStats {
            symbol,
            file_count: count,
            total_size: size,
            latest_file: if latest.is_empty() {
                None
            } else {
                Some(latest)
            },
        })
        .collect();

    Ok(Json(DataStats {
        total_files,
        total_size,
        symbols,
    }))
}

/// Warmup status response
#[derive(Debug, Serialize)]
pub struct WarmupStatus {
    pub symbol: String,
    pub status: String,
    pub progress: Option<f64>,
    pub current_file: Option<String>,
}

/// GET /api/data/warmup/:symbol - Get warmup status for a symbol
pub async fn get_warmup_status(
    State(state): State<AppState>,
    Path(symbol): Path<String>,
) -> ApiResult<WarmupStatus> {
    // Query supervisor for warmup status
    if !state.supervisor.is_available().await {
        return Ok(Json(WarmupStatus {
            symbol,
            status: "unknown".to_string(),
            progress: None,
            current_file: None,
        }));
    }

    // For now, return placeholder - actual implementation would query supervisor
    Ok(Json(WarmupStatus {
        symbol,
        status: "ready".to_string(),
        progress: Some(100.0),
        current_file: None,
    }))
}
