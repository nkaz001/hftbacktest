//! Strategy configuration structures.

use serde::{Deserialize, Serialize};

/// Global configuration for the OBI MM strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GlobalConfig {
    /// Order quantity per grid level
    pub order_qty: f64,
    /// Maximum position allowed
    pub max_position: f64,
    /// Number of grid levels
    pub grid_num: usize,
    /// Interval between grid levels
    pub grid_interval: f64,
    /// Lower bound for ROI (Region of Interest)
    pub roi_lb: f64,
    /// Upper bound for ROI
    pub roi_ub: f64,
    /// Tick size (price precision). Each symbol has its own tick_size from the exchange.
    /// If 0.0 or absent, falls back to CLI arg or connector auto-resolve.
    #[serde(default)]
    pub tick_size: f64,
    /// Lot size (quantity precision). Each symbol has its own lot_size from the exchange.
    /// If 0.0 or absent, falls back to CLI arg or connector auto-resolve.
    #[serde(default)]
    pub lot_size: f64,
}

impl Default for GlobalConfig {
    fn default() -> Self {
        Self {
            order_qty: 0.01,
            max_position: 0.1,
            grid_num: 3,
            grid_interval: 0.1,
            roi_lb: 0.0,
            roi_ub: 200000.0,
            tick_size: 0.0,
            lot_size: 0.0,
        }
    }
}

/// Strategy parameters for the OBI MM strategy.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StrategyParams {
    /// Depth percentage to look at for OBI calculation (e.g., 0.001 = 0.1%)
    pub looking_depth: f64,
    /// Window size for OBI z-score normalization
    pub window: usize,
    /// Window size for volatility calculation (number of samples)
    #[serde(default = "default_volatility_window")]
    pub volatility_window: usize,
    /// Half spread as a fraction of mid price
    pub half_spread: f64,
    /// Skew factor for position adjustment
    pub skew: f64,
    /// Alpha coefficient for fair price adjustment
    pub c1: f64,
    /// Power for volatility adjustment
    pub power: f64,
    /// Minimum order live time in seconds before cancellation
    pub live_seconds: f64,
    /// Interval for strategy updates in nanoseconds
    pub update_interval_ns: i64,
    /// Interval for volatility calculation in nanoseconds
    pub volatility_interval_ns: i64,
}

fn default_volatility_window() -> usize {
    60
}

impl Default for StrategyParams {
    fn default() -> Self {
        Self {
            looking_depth: 0.001,
            window: 3600,
            volatility_window: 60,
            half_spread: 0.0002,
            skew: 0.01,
            c1: 0.0001,
            power: 1.0,
            live_seconds: 30.0,
            update_interval_ns: 10_000_000_000, // 10 seconds
            volatility_interval_ns: 60_000_000_000, // 60 seconds
        }
    }
}

/// Stop loss configuration.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StopLossConfig {
    /// Historical mean volatility
    pub volatility_mean: f64,
    /// Historical std of volatility
    pub volatility_std: f64,
    /// Historical mean price change
    pub change_mean: f64,
    /// Historical std of price change
    pub change_std: f64,
    /// Volatility z-score threshold
    pub volatility_threshold: f64,
    /// Change z-score threshold
    pub change_threshold: f64,
}

impl Default for StopLossConfig {
    fn default() -> Self {
        Self {
            volatility_mean: 0.005,
            volatility_std: 0.002,
            change_mean: 0.001,
            change_std: 0.003,
            volatility_threshold: 3.0,
            change_threshold: 3.0,
        }
    }
}

/// Complete strategy configuration.
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct StrategyConfig {
    pub global: GlobalConfig,
    pub params: StrategyParams,
    pub stop_loss: StopLossConfig,
}

impl StrategyConfig {
    /// Loads configuration from a TOML file.
    pub fn from_file(path: &str) -> anyhow::Result<Self> {
        let content = std::fs::read_to_string(path)?;
        let config: Self = toml::from_str(&content)?;
        Ok(config)
    }

    /// Saves configuration to a TOML file.
    pub fn to_file(&self, path: &str) -> anyhow::Result<()> {
        let content = toml::to_string_pretty(self)?;
        std::fs::write(path, content)?;
        Ok(())
    }
}
