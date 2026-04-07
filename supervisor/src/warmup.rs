//! Warmup data readiness checker
//!
//! Checks if warmup data files (.jsonl or .gz) are available and fresh enough
//! for a bot to start safely with pre-warmed factor state.

use std::io::Read as _;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use chrono::{Local, NaiveDate};
use glob::glob;
use tracing::info;

/// Result of warmup readiness check
#[derive(Debug, Clone)]
pub struct WarmupStatus {
    /// Whether warmup data is ready
    pub ready: bool,
    /// Reason for not being ready (if not ready)
    pub reason: String,
    /// Path to the warmup file (if ready)
    pub file_path: Option<PathBuf>,
    /// Age of the newest data point
    pub data_age_secs: Option<u64>,
    /// Duration of data available
    pub data_duration_secs: Option<u64>,
}

/// Checks warmup data readiness for symbols
pub struct WarmupChecker {
    /// Base directory containing data files
    data_dir: PathBuf,
    /// Maximum age of data to be considered fresh
    max_age: Duration,
    /// Minimum duration of data required
    min_duration: Duration,
}

impl WarmupChecker {
    pub fn new(data_dir: PathBuf, max_age: Duration, min_duration: Duration) -> Self {
        Self {
            data_dir,
            max_age,
            min_duration,
        }
    }

    /// Check if warmup data is ready for a symbol
    pub async fn check_symbol(&self, symbol: &str) -> WarmupStatus {
        let symbol_lower = symbol.to_lowercase();

        // First try today's .jsonl file (can be read while being written)
        let today = Local::now().format("%Y%m%d").to_string();
        let jsonl_pattern = self
            .data_dir
            .join(format!("{}_{}*.jsonl", symbol_lower, today));

        if let Some(status) = self
            .check_jsonl_file(&jsonl_pattern.to_string_lossy())
            .await
        {
            if status.ready {
                return status;
            }
        }

        // Try yesterday's .gz file
        let yesterday = (Local::now() - chrono::Duration::days(1))
            .format("%Y%m%d")
            .to_string();
        let gz_pattern = self
            .data_dir
            .join(format!("{}_{}.gz", symbol_lower, yesterday));

        if let Some(status) = self.check_gz_file(&gz_pattern).await {
            if status.ready {
                return status;
            }
        }

        // Try any recent .jsonl files
        let any_jsonl_pattern = self.data_dir.join(format!("{}_*.jsonl", symbol_lower));
        if let Some(status) = self
            .check_any_jsonl(&any_jsonl_pattern.to_string_lossy())
            .await
        {
            if status.ready {
                return status;
            }
        }

        // Try any recent .gz files
        let any_gz_pattern = self.data_dir.join(format!("{}_*.gz", symbol_lower));
        if let Some(status) = self.check_any_gz(&any_gz_pattern.to_string_lossy()).await {
            if status.ready {
                return status;
            }
        }

        WarmupStatus {
            ready: false,
            reason: format!(
                "No warmup data found for {} in {}",
                symbol,
                self.data_dir.display()
            ),
            file_path: None,
            data_age_secs: None,
            data_duration_secs: None,
        }
    }

    /// Check a specific .jsonl file
    async fn check_jsonl_file(&self, pattern: &str) -> Option<WarmupStatus> {
        let entries: Vec<_> = glob(pattern).ok()?.filter_map(Result::ok).collect();

        if entries.is_empty() {
            return None;
        }

        // Get the newest file
        let newest = entries.iter().max_by_key(|p| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        })?;

        let metadata = std::fs::metadata(newest).ok()?;
        let modified = metadata.modified().ok()?;
        let age = SystemTime::now()
            .duration_since(modified)
            .unwrap_or_default();

        // Check file size (needs to have meaningful content)
        let file_size = metadata.len();
        if file_size < 1000 {
            return Some(WarmupStatus {
                ready: false,
                reason: format!(
                    "Warmup file too small: {} bytes (need at least 1KB)",
                    file_size
                ),
                file_path: Some(newest.clone()),
                data_age_secs: Some(age.as_secs()),
                data_duration_secs: None,
            });
        }

        // Estimate data duration based on file size (rough heuristic)
        // Typical event is ~200 bytes, ~5000 events/second = ~1MB/second
        // So 1 hour of data ≈ 3.6GB
        let estimated_duration_secs = (file_size / 1_000_000) as u64; // Very rough estimate

        if age > self.max_age {
            return Some(WarmupStatus {
                ready: false,
                reason: format!(
                    "Warmup data too old: {} seconds (max {})",
                    age.as_secs(),
                    self.max_age.as_secs()
                ),
                file_path: Some(newest.clone()),
                data_age_secs: Some(age.as_secs()),
                data_duration_secs: Some(estimated_duration_secs),
            });
        }

        // For .jsonl files, we can do a quick check of the file content
        // to get more accurate timing
        let (first_ts, last_ts) = self.get_jsonl_timestamps(newest).await;

        if let (Some(first), Some(last)) = (first_ts, last_ts) {
            let duration_secs = ((last - first) / 1000) as u64; // Convert ms to seconds

            if duration_secs < self.min_duration.as_secs() {
                return Some(WarmupStatus {
                    ready: false,
                    reason: format!(
                        "Warmup data duration too short: {} seconds (need {})",
                        duration_secs,
                        self.min_duration.as_secs()
                    ),
                    file_path: Some(newest.clone()),
                    data_age_secs: Some(age.as_secs()),
                    data_duration_secs: Some(duration_secs),
                });
            }

            // Calculate actual data age from last timestamp
            let now_ms = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_millis() as i64;
            let actual_age_secs = ((now_ms - last) / 1000).max(0) as u64;

            if actual_age_secs > self.max_age.as_secs() {
                return Some(WarmupStatus {
                    ready: false,
                    reason: format!(
                        "Warmup data too old: {} seconds (max {})",
                        actual_age_secs,
                        self.max_age.as_secs()
                    ),
                    file_path: Some(newest.clone()),
                    data_age_secs: Some(actual_age_secs),
                    data_duration_secs: Some(duration_secs),
                });
            }

            info!(
                file = %newest.display(),
                duration_secs = duration_secs,
                age_secs = actual_age_secs,
                "Warmup data ready"
            );

            return Some(WarmupStatus {
                ready: true,
                reason: String::new(),
                file_path: Some(newest.clone()),
                data_age_secs: Some(actual_age_secs),
                data_duration_secs: Some(duration_secs),
            });
        }

        // Fallback to file metadata based check
        Some(WarmupStatus {
            ready: true,
            reason: String::new(),
            file_path: Some(newest.clone()),
            data_age_secs: Some(age.as_secs()),
            data_duration_secs: Some(estimated_duration_secs),
        })
    }

    /// Get first and last timestamps from a .jsonl file
    async fn get_jsonl_timestamps(&self, path: &Path) -> (Option<i64>, Option<i64>) {
        use std::io::{BufRead, BufReader, Seek, SeekFrom};

        let file = match std::fs::File::open(path) {
            Ok(f) => f,
            Err(_) => return (None, None),
        };

        let mut reader = BufReader::new(file);

        // Read first line
        let mut first_line = String::new();
        if reader.read_line(&mut first_line).is_err() {
            return (None, None);
        }

        let first_ts = self.extract_timestamp(&first_line);

        // Seek to end and read last complete line
        let file_len = match reader.seek(SeekFrom::End(0)) {
            Ok(len) => len,
            Err(_) => return (first_ts, None),
        };

        // Read last 4KB to find the last complete line
        let seek_pos = file_len.saturating_sub(4096);
        if reader.seek(SeekFrom::Start(seek_pos)).is_err() {
            return (first_ts, None);
        }

        let mut buffer = String::new();
        if reader.read_to_string(&mut buffer).is_err() {
            return (first_ts, None);
        }

        // Find the last complete line
        let last_line = buffer
            .lines()
            .rev()
            .find(|line| !line.is_empty() && line.contains("\"E\":"));

        let last_ts = last_line.and_then(|line| self.extract_timestamp(line));

        (first_ts, last_ts)
    }

    /// Extract timestamp from a JSON line
    fn extract_timestamp(&self, line: &str) -> Option<i64> {
        // Look for "E": followed by timestamp (event time in Binance format)
        if let Some(pos) = line.find("\"E\":") {
            let start = pos + 4;
            let remaining = &line[start..];

            // Find the end of the number
            let end = remaining
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(remaining.len());

            remaining[..end].parse().ok()
        } else {
            None
        }
    }

    /// Check a .gz file
    async fn check_gz_file(&self, path: &Path) -> Option<WarmupStatus> {
        if !path.exists() {
            return None;
        }

        let metadata = std::fs::metadata(path).ok()?;
        let modified = metadata.modified().ok()?;
        let age = SystemTime::now()
            .duration_since(modified)
            .unwrap_or_default();

        // For .gz files, we assume they contain at least 1 hour of data
        // since they are created by rotating complete days
        let file_size = metadata.len();

        if file_size < 10_000 {
            return Some(WarmupStatus {
                ready: false,
                reason: format!("Warmup file too small: {} bytes", file_size),
                file_path: Some(path.to_path_buf()),
                data_age_secs: Some(age.as_secs()),
                data_duration_secs: None,
            });
        }

        // .gz files from yesterday should be fine
        Some(WarmupStatus {
            ready: true,
            reason: String::new(),
            file_path: Some(path.to_path_buf()),
            data_age_secs: Some(age.as_secs()),
            data_duration_secs: Some(86400), // Assume full day
        })
    }

    /// Check any .jsonl files matching pattern
    async fn check_any_jsonl(&self, pattern: &str) -> Option<WarmupStatus> {
        let entries: Vec<_> = glob(pattern).ok()?.filter_map(Result::ok).collect();

        if entries.is_empty() {
            return None;
        }

        // Get the newest file
        let newest = entries.iter().max_by_key(|p| {
            std::fs::metadata(p)
                .and_then(|m| m.modified())
                .unwrap_or(SystemTime::UNIX_EPOCH)
        })?;

        self.check_jsonl_file(&newest.to_string_lossy()).await
    }

    /// Check any .gz files matching pattern
    async fn check_any_gz(&self, pattern: &str) -> Option<WarmupStatus> {
        let entries: Vec<_> = glob(pattern).ok()?.filter_map(Result::ok).collect();

        if entries.is_empty() {
            return None;
        }

        // Get the newest file by date in filename
        let newest = entries.iter().max_by_key(|p| {
            // Extract date from filename like btcusdt_20260106.gz
            p.file_stem()
                .and_then(|s| s.to_str())
                .and_then(|s| s.split('_').last())
                .and_then(|d| NaiveDate::parse_from_str(d, "%Y%m%d").ok())
        })?;

        self.check_gz_file(newest).await
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::tempdir;

    #[tokio::test]
    async fn test_warmup_checker_no_files() {
        let dir = tempdir().unwrap();
        let checker = WarmupChecker::new(
            dir.path().to_path_buf(),
            Duration::from_secs(7200),
            Duration::from_secs(3600),
        );

        let status = checker.check_symbol("btcusdt").await;
        assert!(!status.ready);
        assert!(status.reason.contains("No warmup data found"));
    }

    #[tokio::test]
    async fn test_warmup_checker_with_jsonl() {
        let dir = tempdir().unwrap();
        let today = Local::now().format("%Y%m%d").to_string();
        let file_path = dir.path().join(format!("btcusdt_{}.jsonl", today));

        // Create a file with some JSON content
        let mut file = std::fs::File::create(&file_path).unwrap();
        let now_ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as i64;

        // Write enough data to pass size check
        for i in 0..1000 {
            let ts = now_ms - (3600_000 - i * 1000); // 1 hour of data
            writeln!(file, r#"{{"e":"depthUpdate","E":{},"s":"BTCUSDT"}}"#, ts).unwrap();
        }
        file.flush().unwrap();

        let checker = WarmupChecker::new(
            dir.path().to_path_buf(),
            Duration::from_secs(7200),
            Duration::from_secs(300), // 5 minutes minimum
        );

        let status = checker.check_symbol("btcusdt").await;
        assert!(status.ready, "Expected ready, got: {:?}", status);
    }
}
