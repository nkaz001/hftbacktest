//! Log viewing API endpoints

use axum::{
    extract::{Path, Query, State},
    Json,
};
use serde::{Deserialize, Serialize};
use tokio::fs;
use tokio::io::{AsyncBufReadExt, BufReader};

use crate::api::{ApiErrorResponse, ApiResult};
use crate::state::AppState;

/// Log file info
#[derive(Debug, Serialize)]
pub struct LogFileInfo {
    pub name: String,
    pub path: String,
    pub size: u64,
    pub modified: String,
    pub component: String,
}

/// GET /api/logs/files - List available log files
pub async fn list_log_files(State(state): State<AppState>) -> ApiResult<Vec<LogFileInfo>> {
    let log_dir = &state.config.log_dir;

    if !log_dir.exists() {
        return Ok(Json(vec![]));
    }

    let mut files = Vec::new();
    let mut entries = fs::read_dir(log_dir)
        .await
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

    while let Some(entry) = entries
        .next_entry()
        .await
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?
    {
        let path = entry.path();

        // Only include .log files
        let ext = path.extension().and_then(|e| e.to_str());
        if ext != Some("log") && ext != Some("jsonl") {
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

        // Extract component from filename
        let component = name
            .split('.')
            .next()
            .unwrap_or("unknown")
            .to_string();

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

        files.push(LogFileInfo {
            name,
            path: path.to_string_lossy().to_string(),
            size: metadata.len(),
            modified,
            component,
        });
    }

    // Sort by modified time descending
    files.sort_by(|a, b| b.modified.cmp(&a.modified));

    Ok(Json(files))
}

/// Query params for reading logs
#[derive(Debug, Deserialize)]
pub struct ReadLogsQuery {
    /// Number of lines to read (default 100)
    pub lines: Option<usize>,
    /// Start from line offset
    pub offset: Option<usize>,
    /// Filter by log level
    pub level: Option<String>,
    /// Search pattern
    pub search: Option<String>,
}

/// Log entry
#[derive(Debug, Serialize)]
pub struct LogEntry {
    pub line_number: usize,
    pub timestamp: Option<String>,
    pub level: Option<String>,
    pub message: String,
}

/// Log response
#[derive(Debug, Serialize)]
pub struct LogResponse {
    pub file: String,
    pub total_lines: usize,
    pub entries: Vec<LogEntry>,
}

/// GET /api/logs/:component - Read logs for a component
pub async fn read_logs(
    State(state): State<AppState>,
    Path(component): Path<String>,
    Query(query): Query<ReadLogsQuery>,
) -> ApiResult<LogResponse> {
    let log_dir = &state.config.log_dir;

    // Find log file for component
    let log_path = log_dir.join(format!("{}.log", component));

    if !log_path.exists() {
        return Err(ApiErrorResponse::not_found(format!(
            "Log file not found: {}",
            component
        )));
    }

    let file = fs::File::open(&log_path)
        .await
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

    let reader = BufReader::new(file);
    let mut lines_stream = reader.lines();

    let mut all_lines: Vec<String> = Vec::new();
    while let Some(line) = lines_stream
        .next_line()
        .await
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?
    {
        all_lines.push(line);
    }

    let total_lines = all_lines.len();
    let lines_to_read = query.lines.unwrap_or(100);
    let offset = query.offset.unwrap_or(0);

    let mut entries: Vec<LogEntry> = Vec::new();

    for (idx, line) in all_lines.iter().enumerate().skip(offset).take(lines_to_read) {
        // Parse log line - assumes format: "2024-01-01T12:00:00 [INFO] message"
        let (timestamp, level, message) = parse_log_line(line);

        // Filter by level if specified
        if let Some(ref filter_level) = query.level {
            if let Some(ref log_level) = level {
                if log_level.to_lowercase() != filter_level.to_lowercase() {
                    continue;
                }
            }
        }

        // Filter by search pattern if specified
        if let Some(ref search) = query.search {
            if !line.to_lowercase().contains(&search.to_lowercase()) {
                continue;
            }
        }

        entries.push(LogEntry {
            line_number: idx + 1,
            timestamp,
            level,
            message,
        });
    }

    Ok(Json(LogResponse {
        file: component,
        total_lines,
        entries,
    }))
}

/// Parse a log line into timestamp, level, and message
fn parse_log_line(line: &str) -> (Option<String>, Option<String>, String) {
    // Try to parse common log formats

    // Format: "2024-01-01T12:00:00.000Z  INFO component: message"
    if let Some(space_idx) = line.find("  ") {
        let timestamp = &line[..space_idx];
        let rest = &line[space_idx + 2..];

        // Try to extract level
        let parts: Vec<&str> = rest.splitn(2, ' ').collect();
        if parts.len() == 2 {
            let level = parts[0].to_string();
            let message = parts[1].to_string();

            if matches!(level.as_str(), "TRACE" | "DEBUG" | "INFO" | "WARN" | "ERROR") {
                return (
                    Some(timestamp.to_string()),
                    Some(level),
                    message,
                );
            }
        }

        return (Some(timestamp.to_string()), None, rest.to_string());
    }

    // Format: "[2024-01-01 12:00:00] [INFO] message"
    if line.starts_with('[') {
        if let Some(end_bracket) = line.find(']') {
            let timestamp = line[1..end_bracket].to_string();
            let rest = line[end_bracket + 1..].trim_start();

            if rest.starts_with('[') {
                if let Some(level_end) = rest.find(']') {
                    let level = rest[1..level_end].to_string();
                    let message = rest[level_end + 1..].trim_start().to_string();
                    return (Some(timestamp), Some(level), message);
                }
            }

            return (Some(timestamp), None, rest.to_string());
        }
    }

    // No recognized format
    (None, None, line.to_string())
}

/// GET /api/logs/:component/tail - Get last N lines (for real-time viewing)
pub async fn tail_logs(
    State(state): State<AppState>,
    Path(component): Path<String>,
    Query(query): Query<ReadLogsQuery>,
) -> ApiResult<LogResponse> {
    let log_dir = &state.config.log_dir;
    let log_path = log_dir.join(format!("{}.log", component));

    if !log_path.exists() {
        return Err(ApiErrorResponse::not_found(format!(
            "Log file not found: {}",
            component
        )));
    }

    let content = fs::read_to_string(&log_path)
        .await
        .map_err(|e| ApiErrorResponse::internal(e.to_string()))?;

    let all_lines: Vec<&str> = content.lines().collect();
    let total_lines = all_lines.len();
    let lines_to_read = query.lines.unwrap_or(50);

    let start_idx = total_lines.saturating_sub(lines_to_read);

    let entries: Vec<LogEntry> = all_lines
        .iter()
        .enumerate()
        .skip(start_idx)
        .map(|(idx, line)| {
            let (timestamp, level, message) = parse_log_line(line);
            LogEntry {
                line_number: idx + 1,
                timestamp,
                level,
                message,
            }
        })
        .collect();

    Ok(Json(LogResponse {
        file: component,
        total_lines,
        entries,
    }))
}
