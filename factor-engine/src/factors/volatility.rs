//! Volatility Factor
//!
//! Computes rolling volatility based on price returns over a specified interval.

use crate::primitives::WelfordRolling;

use super::Factor;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Rolling volatility factor based on price returns.
///
/// This factor computes the standard deviation of returns over a rolling window.
/// Returns are calculated at fixed time intervals.
///
/// # Example
///
/// ```rust,ignore
/// use factor_engine::VolatilityFactor;
///
/// let mut vol = VolatilityFactor::new(
///     60_000_000_000,  // 60 second interval for returns
///     60,              // 60-period rolling window
/// );
///
/// // In your strategy loop:
/// let volatility = vol.update(current_timestamp, mid_price);
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct VolatilityFactor {
    /// Interval in nanoseconds for computing returns
    interval_ns: i64,
    /// Rolling statistics for volatility calculation
    welford: WelfordRolling,
    /// Last recorded price
    last_price: f64,
    /// Last timestamp when a return was computed
    last_timestamp: i64,
    /// Current volatility estimate
    current_value: f64,
    /// Number of return samples collected
    sample_count: usize,
}

impl VolatilityFactor {
    /// Creates a new volatility factor.
    ///
    /// # Arguments
    ///
    /// * `interval_ns` - Time interval in nanoseconds for computing returns
    /// * `window` - Number of return samples in the rolling window
    #[inline]
    pub fn new(interval_ns: i64, window: usize) -> Self {
        Self {
            interval_ns,
            welford: WelfordRolling::new(window),
            last_price: 0.0,
            last_timestamp: 0,
            current_value: 0.0,
            sample_count: 0,
        }
    }

    /// Updates the volatility factor with current price and timestamp.
    ///
    /// Returns are only computed when the time interval has elapsed.
    ///
    /// # Arguments
    ///
    /// * `timestamp` - Current timestamp in nanoseconds
    /// * `price` - Current mid price
    ///
    /// # Returns
    ///
    /// The current rolling volatility estimate.
    #[inline]
    pub fn update(&mut self, timestamp: i64, price: f64) -> f64 {
        if self.last_timestamp == 0 {
            self.last_price = price;
            self.last_timestamp = timestamp;
            return 0.0;
        }

        // Check if we've passed the interval
        if timestamp - self.last_timestamp >= self.interval_ns {
            if self.last_price > 0.0 {
                let ret = price / self.last_price - 1.0;
                self.welford.update(ret);
                // Use population std (ddof=0) to match numpy default
                self.current_value = self.welford.population_std();
                self.sample_count += 1;
            }

            self.last_price = price;
            self.last_timestamp = timestamp;
        }

        self.current_value
    }

    /// Returns the current volatility estimate.
    #[inline]
    pub fn volatility(&self) -> f64 {
        self.current_value
    }

    /// Returns the latest computed return.
    #[inline]
    pub fn mean_return(&self) -> f64 {
        self.welford.mean()
    }

    /// Returns the number of return samples collected.
    #[inline]
    pub fn sample_count(&self) -> usize {
        self.sample_count
    }
}

impl Factor for VolatilityFactor {
    #[inline]
    fn value(&self) -> f64 {
        self.current_value
    }

    #[inline]
    fn reset(&mut self) {
        self.welford.reset();
        self.last_price = 0.0;
        self.last_timestamp = 0;
        self.current_value = 0.0;
        self.sample_count = 0;
    }

    #[inline]
    fn is_ready(&self) -> bool {
        self.welford.is_ready()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_volatility_factor() {
        let mut vol = VolatilityFactor::new(1_000_000_000, 5); // 1 second interval

        // Simulate prices over time
        let prices = [100.0, 101.0, 99.0, 102.0, 98.0, 103.0];

        for (i, &price) in prices.iter().enumerate() {
            let ts = (i as i64) * 1_000_000_000; // 1 second intervals
            vol.update(ts, price);
        }

        // Should have computed volatility after enough samples
        assert!(vol.sample_count() >= 4);
        assert!(vol.volatility() > 0.0);
    }
}
