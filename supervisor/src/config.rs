//! Configuration parsing for cluster.toml
//!
//! This module handles the unified configuration format for the HFT cluster,
//! including connector, collector, and bot configurations.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

/// Root configuration structure for cluster.toml
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterConfig {
    pub cluster: ClusterSettings,
    /// Single connector (legacy format, deprecated)
    #[serde(default)]
    pub connector: Option<ConnectorConfig>,
    /// Multiple connectors (new format)
    #[serde(default)]
    pub connectors: Vec<ConnectorConfig>,
    pub collector: CollectorConfig,
    pub warmup: WarmupConfig,
    /// Order recorder configuration (optional)
    #[serde(default)]
    pub order_recorder: Option<OrderRecorderConfig>,
    #[serde(default)]
    pub bot_defaults: BotDefaults,
    #[serde(default)]
    pub bots: Vec<BotConfig>,
}

/// General cluster settings
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ClusterSettings {
    /// Cluster name for identification
    pub name: String,
    /// Base directory for data files
    pub data_dir: PathBuf,
    /// How often to check for config changes (seconds)
    #[serde(default = "default_config_reload_interval")]
    pub config_reload_interval_secs: u64,
    /// Directory for log files
    #[serde(default = "default_log_dir")]
    pub log_dir: PathBuf,
    /// Unix socket path for control commands
    #[serde(default = "default_socket_path")]
    pub socket_path: PathBuf,
}

fn default_config_reload_interval() -> u64 {
    30
}

fn default_log_dir() -> PathBuf {
    PathBuf::from("/var/log/hft")
}

fn default_socket_path() -> PathBuf {
    PathBuf::from("/run/hft/supervisor.sock")
}

/// Connector configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ConnectorConfig {
    /// Connector instance name (used for IPC)
    pub name: String,
    /// Connector type: binancefutures, binancespot, bybit
    #[serde(rename = "type")]
    pub connector_type: String,
    /// Path to connector-specific config file (contains API keys)
    pub config_file: PathBuf,
    /// Whether to auto-start connector with supervisor
    #[serde(default)]
    pub auto_start: bool,
}

/// Collector configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CollectorConfig {
    /// Whether collector is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Collection mode: ipc or ws
    #[serde(default = "default_collector_mode")]
    pub mode: String,
    /// Output directory for collected data
    pub output_dir: PathBuf,
    /// Symbols to collect
    pub symbols: Vec<String>,
    /// Days to retain data
    #[serde(default = "default_retention_days")]
    pub retention_days: u32,
    /// Whether to auto-start collector with supervisor
    #[serde(default)]
    pub auto_start: bool,
}

/// Order recorder configuration (writes to ClickHouse)
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OrderRecorderConfig {
    /// Whether order recorder is enabled
    #[serde(default)]
    pub enabled: bool,
    /// Whether to auto-start order recorder with supervisor
    #[serde(default)]
    pub auto_start: bool,
    /// IPC service name for receiving recording events
    #[serde(default = "default_ipc_service")]
    pub ipc_service: String,
    /// ClickHouse HTTP URL
    #[serde(default = "default_clickhouse_url")]
    pub clickhouse_url: String,
    /// ClickHouse database name
    #[serde(default = "default_clickhouse_database")]
    pub database: String,
    /// Batch size before flushing to ClickHouse
    #[serde(default = "default_batch_size")]
    pub batch_size: usize,
    /// Flush interval in seconds
    #[serde(default = "default_flush_interval")]
    pub flush_interval_secs: u64,
}

fn default_ipc_service() -> String {
    "hft_recording".to_string()
}

fn default_clickhouse_url() -> String {
    "http://localhost:8123".to_string()
}

fn default_clickhouse_database() -> String {
    "hft".to_string()
}

fn default_batch_size() -> usize {
    100
}

fn default_flush_interval() -> u64 {
    5
}

fn default_true() -> bool {
    true
}

fn default_collector_mode() -> String {
    "ipc".to_string()
}

fn default_retention_days() -> u32 {
    30
}

/// Warmup configuration for bot startup
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WarmupConfig {
    /// Directory containing warmup data files
    pub data_dir: PathBuf,
    /// Maximum age of warmup data in seconds
    #[serde(default = "default_max_age_seconds")]
    pub max_age_seconds: u64,
    /// Minimum duration of warmup data in seconds
    #[serde(default = "default_min_duration_seconds")]
    pub min_duration_seconds: u64,
}

fn default_max_age_seconds() -> u64 {
    7200 // 2 hours
}

fn default_min_duration_seconds() -> u64 {
    3600 // 1 hour
}

/// Default settings for bots
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct BotDefaults {
    /// Default tick size
    #[serde(default)]
    pub tick_size: Option<f64>,
    /// Default lot size
    #[serde(default)]
    pub lot_size: Option<f64>,
}

/// Individual bot configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BotConfig {
    /// Unique bot identifier
    pub id: String,
    /// Whether this bot is enabled
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// Strategy name
    pub strategy: String,
    /// Trading symbol
    pub symbol: String,
    /// Connector name to use (must match a connector's name field)
    /// If not specified, uses the first/only connector
    #[serde(default)]
    pub connector: Option<String>,
    /// Path to strategy-specific config file
    pub config_file: PathBuf,
    /// Override tick size
    #[serde(default)]
    pub tick_size: Option<f64>,
    /// Override lot size
    #[serde(default)]
    pub lot_size: Option<f64>,
    /// Auto-restart on crash
    #[serde(default = "default_true")]
    pub auto_restart: bool,
    /// Maximum restart attempts before giving up
    #[serde(default = "default_max_restarts")]
    pub max_restarts: u32,
    /// Restart backoff in seconds
    #[serde(default = "default_restart_backoff")]
    pub restart_backoff_secs: u64,
}

fn default_max_restarts() -> u32 {
    5
}

fn default_restart_backoff() -> u64 {
    10
}

impl BotConfig {
    /// Get effective tick size (bot override or cluster default)
    pub fn effective_tick_size(&self, defaults: &BotDefaults) -> Option<f64> {
        self.tick_size.or(defaults.tick_size)
    }

    /// Get effective lot size (bot override or cluster default)
    pub fn effective_lot_size(&self, defaults: &BotDefaults) -> Option<f64> {
        self.lot_size.or(defaults.lot_size)
    }
}

impl ClusterConfig {
    /// Load configuration from a TOML file
    pub fn load<P: AsRef<Path>>(path: P) -> Result<Self> {
        let path = path.as_ref();
        let content = std::fs::read_to_string(path)
            .with_context(|| format!("Failed to read config file: {}", path.display()))?;
        let config: ClusterConfig = toml::from_str(&content)
            .with_context(|| format!("Failed to parse config file: {}", path.display()))?;
        config.validate()?;
        Ok(config)
    }

    /// Get all connectors (combines legacy single connector with new multi-connector format)
    pub fn all_connectors(&self) -> Vec<&ConnectorConfig> {
        let mut result: Vec<&ConnectorConfig> = self.connectors.iter().collect();
        if let Some(ref connector) = self.connector {
            // Add legacy connector if not already present by name
            if !result.iter().any(|c| c.name == connector.name) {
                result.push(connector);
            }
        }
        result
    }

    /// Get connector by name
    pub fn get_connector(&self, name: &str) -> Option<&ConnectorConfig> {
        self.all_connectors().into_iter().find(|c| c.name == name)
    }

    /// Get the default connector name (first connector)
    pub fn default_connector_name(&self) -> Option<&str> {
        self.all_connectors().first().map(|c| c.name.as_str())
    }

    /// Validate the configuration
    pub fn validate(&self) -> Result<()> {
        // Check for duplicate bot IDs
        let mut seen_ids = std::collections::HashSet::new();
        for bot in &self.bots {
            if !seen_ids.insert(&bot.id) {
                anyhow::bail!("Duplicate bot ID: {}", bot.id);
            }
        }

        // Validate paths exist or are creatable
        if !self.cluster.data_dir.exists() {
            tracing::warn!(
                "Data directory does not exist: {}",
                self.cluster.data_dir.display()
            );
        }

        // Get all connectors
        let connectors = self.all_connectors();

        // Must have at least one connector
        if connectors.is_empty() {
            anyhow::bail!("At least one connector must be configured");
        }

        // Check for duplicate connector names
        let mut seen_names = std::collections::HashSet::new();
        for connector in &connectors {
            if !seen_names.insert(&connector.name) {
                anyhow::bail!("Duplicate connector name: {}", connector.name);
            }
        }

        // Validate connector config files exist
        for connector in &connectors {
            if !connector.config_file.exists() {
                anyhow::bail!(
                    "Connector '{}' config file not found: {}",
                    connector.name,
                    connector.config_file.display()
                );
            }
        }

        // Validate bot config files exist and connector references are valid
        let connector_names: std::collections::HashSet<_> =
            connectors.iter().map(|c| c.name.as_str()).collect();
        for bot in &self.bots {
            if bot.enabled {
                if !bot.config_file.exists() {
                    anyhow::bail!(
                        "Bot {} config file not found: {}",
                        bot.id,
                        bot.config_file.display()
                    );
                }
                // Validate connector reference if specified
                if let Some(ref connector_name) = bot.connector {
                    if !connector_names.contains(connector_name.as_str()) {
                        anyhow::bail!(
                            "Bot {} references unknown connector: {}",
                            bot.id,
                            connector_name
                        );
                    }
                }
            }
        }

        Ok(())
    }

    /// Get enabled bots
    pub fn enabled_bots(&self) -> impl Iterator<Item = &BotConfig> {
        self.bots.iter().filter(|b| b.enabled)
    }

    /// Compute config hash for change detection
    pub fn config_hash(&self) -> u64 {
        use std::collections::hash_map::DefaultHasher;
        use std::hash::{Hash, Hasher};

        let mut hasher = DefaultHasher::new();
        // Hash the serialized config
        if let Ok(toml_str) = toml::to_string(self) {
            toml_str.hash(&mut hasher);
        }
        hasher.finish()
    }

    /// Compare with another config and return changed bot IDs
    pub fn changed_bots(&self, other: &ClusterConfig) -> Vec<String> {
        let mut changed = Vec::new();

        let self_bots: HashMap<_, _> = self.bots.iter().map(|b| (&b.id, b)).collect();
        let other_bots: HashMap<_, _> = other.bots.iter().map(|b| (&b.id, b)).collect();

        // Check for added, removed, or modified bots
        for (id, bot) in &self_bots {
            match other_bots.get(id) {
                None => changed.push((*id).clone()), // Removed
                Some(other_bot) => {
                    // Compare relevant fields
                    if bot.enabled != other_bot.enabled
                        || bot.strategy != other_bot.strategy
                        || bot.symbol != other_bot.symbol
                        || bot.config_file != other_bot.config_file
                    {
                        changed.push((*id).clone());
                    }
                }
            }
        }

        // Check for newly added bots
        for id in other_bots.keys() {
            if !self_bots.contains_key(id) {
                changed.push((*id).clone());
            }
        }

        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_minimal_config_legacy() {
        // Test legacy [connector] format
        let toml = r#"
[cluster]
name = "test"
data_dir = "/tmp/data"

[connector]
name = "bf"
type = "binancefutures"
config_file = "/etc/hft/connector.toml"

[collector]
output_dir = "/tmp/data/raw"
symbols = ["btcusdt"]

[warmup]
data_dir = "/tmp/data/raw"
"#;
        let config: ClusterConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.cluster.name, "test");
        assert!(config.connector.is_some());
        assert_eq!(config.connector.as_ref().unwrap().name, "bf");
        assert_eq!(config.all_connectors().len(), 1);
        assert!(config.bots.is_empty());
    }

    #[test]
    fn test_parse_multi_connector() {
        // Test new [[connectors]] array format
        let toml = r#"
[cluster]
name = "multi"
data_dir = "/tmp/data"

[[connectors]]
name = "bf"
type = "binancefutures"
config_file = "/etc/hft/bf.toml"

[[connectors]]
name = "bybit"
type = "bybit_rest"
config_file = "/etc/hft/bybit.toml"

[collector]
output_dir = "/tmp/data/raw"
symbols = ["btcusdt"]

[warmup]
data_dir = "/tmp/data/raw"
"#;
        let config: ClusterConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.cluster.name, "multi");
        assert!(config.connector.is_none());
        assert_eq!(config.connectors.len(), 2);
        assert_eq!(config.all_connectors().len(), 2);
        assert_eq!(config.get_connector("bf").unwrap().connector_type, "binancefutures");
        assert_eq!(config.get_connector("bybit").unwrap().connector_type, "bybit_rest");
        assert_eq!(config.default_connector_name(), Some("bf"));
    }

    #[test]
    fn test_parse_with_bots() {
        let toml = r#"
[cluster]
name = "prod"
data_dir = "/var/lib/hft/data"

[[connectors]]
name = "bf"
type = "binancefutures"
config_file = "/etc/hft/connector.toml"

[collector]
output_dir = "/var/lib/hft/data/raw"
symbols = ["btcusdt", "ethusdt"]

[warmup]
data_dir = "/var/lib/hft/data/raw"

[bot_defaults]
tick_size = 0.1
lot_size = 0.001

[[bots]]
id = "obi_mm_btcusdt"
strategy = "obi_mm"
symbol = "btcusdt"
connector = "bf"
config_file = "/etc/hft/strategies/btcusdt.toml"

[[bots]]
id = "obi_mm_ethusdt"
enabled = false
strategy = "obi_mm"
symbol = "ethusdt"
config_file = "/etc/hft/strategies/ethusdt.toml"
"#;
        let config: ClusterConfig = toml::from_str(toml).unwrap();
        assert_eq!(config.bots.len(), 2);
        assert!(config.bots[0].enabled);
        assert!(!config.bots[1].enabled);
        assert_eq!(config.bots[0].connector, Some("bf".to_string()));
        assert_eq!(config.bots[1].connector, None);

        let enabled: Vec<_> = config.enabled_bots().collect();
        assert_eq!(enabled.len(), 1);
        assert_eq!(enabled[0].id, "obi_mm_btcusdt");
    }

    #[test]
    fn test_parse_with_order_recorder() {
        let toml = r#"
[cluster]
name = "test"
data_dir = "/tmp/data"

[[connectors]]
name = "bf"
type = "binancefutures"
config_file = "/etc/hft/connector.toml"
auto_start = true

[collector]
output_dir = "/tmp/data/raw"
symbols = ["btcusdt"]
auto_start = true

[warmup]
data_dir = "/tmp/data/raw"

[order_recorder]
enabled = true
auto_start = true
clickhouse_url = "http://clickhouse:8123"
database = "hft_prod"
batch_size = 200
flush_interval_secs = 10
"#;
        let config: ClusterConfig = toml::from_str(toml).unwrap();
        let connectors = config.all_connectors();
        assert_eq!(connectors.len(), 1);
        assert!(connectors[0].auto_start);
        assert!(config.collector.auto_start);

        let order_recorder = config.order_recorder.unwrap();
        assert!(order_recorder.enabled);
        assert!(order_recorder.auto_start);
        assert_eq!(order_recorder.clickhouse_url, "http://clickhouse:8123");
        assert_eq!(order_recorder.database, "hft_prod");
        assert_eq!(order_recorder.batch_size, 200);
        assert_eq!(order_recorder.flush_interval_secs, 10);
        assert_eq!(order_recorder.ipc_service, "hft_recording"); // default
    }
}
