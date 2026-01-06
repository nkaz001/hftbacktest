//! Health check module
//!
//! Monitors system resources and bot health status.

use std::collections::HashMap;
use std::path::Path;
use std::time::{Duration, Instant};

use serde::{Deserialize, Serialize};

/// Health status for a component
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthStatus {
    pub healthy: bool,
    pub message: String,
    pub last_check: u64, // Unix timestamp
}

/// System resource usage
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemResources {
    pub cpu_usage_percent: f64,
    pub memory_used_mb: u64,
    pub memory_total_mb: u64,
    pub disk_used_gb: f64,
    pub disk_total_gb: f64,
}

/// Bot health metrics
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotHealth {
    pub bot_id: String,
    pub healthy: bool,
    pub pid: Option<u32>,
    pub uptime_secs: Option<u64>,
    pub cpu_percent: Option<f64>,
    pub memory_mb: Option<u64>,
    pub last_activity: Option<u64>, // Unix timestamp
}

/// Overall cluster health
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterHealth {
    pub healthy: bool,
    pub connector: HealthStatus,
    pub collector: HealthStatus,
    pub bots: HashMap<String, BotHealth>,
    pub system: SystemResources,
    pub timestamp: u64,
}

/// Health checker for the cluster
pub struct HealthChecker {
    data_dir: std::path::PathBuf,
    last_check: Instant,
}

impl HealthChecker {
    pub fn new(data_dir: std::path::PathBuf) -> Self {
        Self {
            data_dir,
            last_check: Instant::now(),
        }
    }

    /// Check system resources
    pub fn check_system_resources(&self) -> SystemResources {
        // Read /proc/meminfo for memory info (Linux-specific)
        let (memory_used_mb, memory_total_mb) = self.read_memory_info().unwrap_or((0, 0));

        // Read /proc/stat for CPU usage (simplified)
        let cpu_usage_percent = self.read_cpu_usage().unwrap_or(0.0);

        // Check disk usage for data directory
        let (disk_used_gb, disk_total_gb) = self.read_disk_usage().unwrap_or((0.0, 0.0));

        SystemResources {
            cpu_usage_percent,
            memory_used_mb,
            memory_total_mb,
            disk_used_gb,
            disk_total_gb,
        }
    }

    /// Check if a process is running
    pub fn is_process_running(&self, pid: u32) -> bool {
        Path::new(&format!("/proc/{}", pid)).exists()
    }

    /// Get process CPU and memory usage
    pub fn get_process_stats(&self, pid: u32) -> Option<(f64, u64)> {
        // Read /proc/{pid}/stat for CPU
        // Read /proc/{pid}/statm for memory
        let statm_path = format!("/proc/{}/statm", pid);
        let statm = std::fs::read_to_string(&statm_path).ok()?;
        let parts: Vec<&str> = statm.split_whitespace().collect();

        // Second field is resident set size in pages (typically 4KB)
        let rss_pages: u64 = parts.get(1)?.parse().ok()?;
        let memory_mb = (rss_pages * 4) / 1024; // Convert to MB

        // CPU percentage would require tracking over time, simplified here
        let cpu_percent = 0.0; // TODO: implement proper CPU tracking

        Some((cpu_percent, memory_mb))
    }

    /// Check connector health by looking for its process
    pub fn check_connector(&self, name: &str) -> HealthStatus {
        // Look for connector process
        let connector_running = self.find_process_by_name("connector").is_some();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if connector_running {
            HealthStatus {
                healthy: true,
                message: "Connector is running".to_string(),
                last_check: now,
            }
        } else {
            HealthStatus {
                healthy: false,
                message: "Connector process not found".to_string(),
                last_check: now,
            }
        }
    }

    /// Check collector health
    pub fn check_collector(&self) -> HealthStatus {
        let collector_running = self.find_process_by_name("collector").is_some();

        let now = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs();

        if collector_running {
            // Also check if files are being written
            let recent_files = self.check_recent_data_files();
            if recent_files {
                HealthStatus {
                    healthy: true,
                    message: "Collector is running and writing data".to_string(),
                    last_check: now,
                }
            } else {
                HealthStatus {
                    healthy: false,
                    message: "Collector is running but no recent data files".to_string(),
                    last_check: now,
                }
            }
        } else {
            HealthStatus {
                healthy: false,
                message: "Collector process not found".to_string(),
                last_check: now,
            }
        }
    }

    /// Check if there are recently modified data files
    fn check_recent_data_files(&self) -> bool {
        let max_age = Duration::from_secs(60); // Files should be updated within 1 minute

        if let Ok(entries) = std::fs::read_dir(&self.data_dir) {
            for entry in entries.filter_map(Result::ok) {
                let path = entry.path();
                if path.extension().map(|e| e == "jsonl").unwrap_or(false) {
                    if let Ok(metadata) = path.metadata() {
                        if let Ok(modified) = metadata.modified() {
                            if let Ok(age) = std::time::SystemTime::now().duration_since(modified) {
                                if age < max_age {
                                    return true;
                                }
                            }
                        }
                    }
                }
            }
        }
        false
    }

    /// Find a process by command name
    fn find_process_by_name(&self, name: &str) -> Option<u32> {
        let proc_dir = Path::new("/proc");
        if !proc_dir.exists() {
            return None;
        }

        for entry in std::fs::read_dir(proc_dir).ok()?.filter_map(Result::ok) {
            let pid_str = entry.file_name();
            if let Some(pid) = pid_str.to_str().and_then(|s| s.parse::<u32>().ok()) {
                let cmdline_path = entry.path().join("cmdline");
                if let Ok(cmdline) = std::fs::read_to_string(&cmdline_path) {
                    if cmdline.contains(name) {
                        return Some(pid);
                    }
                }
            }
        }
        None
    }

    /// Read memory info from /proc/meminfo
    fn read_memory_info(&self) -> Option<(u64, u64)> {
        let content = std::fs::read_to_string("/proc/meminfo").ok()?;

        let mut total = 0u64;
        let mut available = 0u64;

        for line in content.lines() {
            if line.starts_with("MemTotal:") {
                total = line
                    .split_whitespace()
                    .nth(1)?
                    .parse::<u64>()
                    .ok()?
                    / 1024; // Convert KB to MB
            } else if line.starts_with("MemAvailable:") {
                available = line
                    .split_whitespace()
                    .nth(1)?
                    .parse::<u64>()
                    .ok()?
                    / 1024;
            }
        }

        let used = total.saturating_sub(available);
        Some((used, total))
    }

    /// Read CPU usage (simplified)
    fn read_cpu_usage(&self) -> Option<f64> {
        // This is a simplified implementation
        // For accurate CPU usage, we'd need to track changes over time
        let content = std::fs::read_to_string("/proc/stat").ok()?;
        let first_line = content.lines().next()?;

        if !first_line.starts_with("cpu ") {
            return None;
        }

        let values: Vec<u64> = first_line
            .split_whitespace()
            .skip(1)
            .filter_map(|s| s.parse().ok())
            .collect();

        if values.len() < 4 {
            return None;
        }

        let user = values[0];
        let nice = values[1];
        let system = values[2];
        let idle = values[3];

        let total = user + nice + system + idle;
        let active = user + nice + system;

        if total == 0 {
            return Some(0.0);
        }

        Some((active as f64 / total as f64) * 100.0)
    }

    /// Read disk usage for data directory
    fn read_disk_usage(&self) -> Option<(f64, f64)> {
        // Use nix crate for statvfs
        use nix::sys::statvfs::statvfs;

        let stat = statvfs(&self.data_dir).ok()?;
        let block_size = stat.block_size() as u64;
        let total_blocks = stat.blocks();
        let free_blocks = stat.blocks_available();

        let total_gb = (total_blocks * block_size) as f64 / (1024.0 * 1024.0 * 1024.0);
        let free_gb = (free_blocks * block_size) as f64 / (1024.0 * 1024.0 * 1024.0);
        let used_gb = total_gb - free_gb;

        Some((used_gb, total_gb))
    }
}
