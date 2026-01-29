//! Order Book Imbalance (OBI) Factor
//!
//! Computes the imbalance between bid and ask depth within a specified
//! price range, then standardizes it using rolling z-score normalization.

use crate::primitives::WelfordRolling;

use super::Factor;

#[cfg(feature = "serde")]
use serde::{Deserialize, Serialize};

/// Order Book Imbalance Factor.
///
/// This factor measures the imbalance between bid and ask liquidity
/// within a configurable depth range around the mid-price.
///
/// The raw imbalance is standardized using a rolling z-score to produce
/// a normalized signal that can be used for fair price estimation.
///
/// # Formula
///
/// ```text
/// raw_obi = sum(bid_depth) - sum(ask_depth)
/// obi_zscore = (raw_obi - rolling_mean) / rolling_std
/// ```
///
/// # Example
///
/// ```rust,ignore
/// use factor_engine::OBIFactor;
///
/// let mut obi = OBIFactor::new(
///     0.001,  // looking_depth: 0.1% from mid price
///     0.1,    // tick_size
///     0,      // roi_lb_tick
///     3600,   // window for z-score normalization
/// );
///
/// // In your strategy loop:
/// let zscore = obi.update(
///     &bid_depth,
///     &ask_depth,
///     best_bid_tick,
///     best_ask_tick,
///     mid_price,
/// );
/// ```
#[derive(Debug, Clone)]
#[cfg_attr(feature = "serde", derive(Serialize, Deserialize))]
pub struct OBIFactor {
    /// Depth percentage from mid price to look at (e.g., 0.001 = 0.1%)
    looking_depth: f64,
    /// Tick size of the instrument
    tick_size: f64,
    /// Lower bound tick for ROI (Region of Interest)
    roi_lb_tick: i64,
    /// Upper bound tick for ROI
    roi_ub_tick: i64,
    /// Rolling statistics for z-score normalization
    welford: WelfordRolling,
    /// Current raw OBI value
    current_raw: f64,
    /// Current z-score normalized OBI value
    current_zscore: f64,
}

impl OBIFactor {
    /// Creates a new OBI factor.
    ///
    /// # Arguments
    ///
    /// * `looking_depth` - Percentage depth from mid price (e.g., 0.001 for 0.1%)
    /// * `tick_size` - Tick size of the instrument
    /// * `roi_lb_tick` - Lower bound tick for the region of interest
    /// * `roi_ub_tick` - Upper bound tick for the region of interest
    /// * `window` - Window size for z-score normalization
    #[inline]
    pub fn new(
        looking_depth: f64,
        tick_size: f64,
        roi_lb_tick: i64,
        roi_ub_tick: i64,
        window: usize,
    ) -> Self {
        Self {
            looking_depth,
            tick_size,
            roi_lb_tick,
            roi_ub_tick,
            welford: WelfordRolling::new(window),
            current_raw: 0.0,
            current_zscore: 0.0,
        }
    }

    /// Updates the OBI factor with current market depth data.
    ///
    /// # Arguments
    ///
    /// * `bid_depth` - Bid depth array (indexed by tick - roi_lb_tick)
    /// * `ask_depth` - Ask depth array (indexed by tick - roi_lb_tick)
    /// * `best_bid_tick` - Best bid price tick
    /// * `best_ask_tick` - Best ask price tick
    /// * `mid_price` - Current mid price
    ///
    /// # Returns
    ///
    /// The z-score normalized OBI value.
    #[inline]
    pub fn update(
        &mut self,
        bid_depth: &[f64],
        ask_depth: &[f64],
        best_bid_tick: i64,
        best_ask_tick: i64,
        mid_price: f64,
    ) -> f64 {
        // Calculate ask depth sum
        let mut sum_ask_qty = 0.0;
        let from_tick = best_ask_tick.max(self.roi_lb_tick);
        let upto_tick = ((mid_price * (1.0 + self.looking_depth) / self.tick_size).floor() as i64)
            .min(self.roi_ub_tick);

        for price_tick in from_tick..upto_tick {
            let idx = (price_tick - self.roi_lb_tick) as usize;
            if idx < ask_depth.len() {
                sum_ask_qty += ask_depth[idx];
            }
        }

        // Calculate bid depth sum
        let mut sum_bid_qty = 0.0;
        let from_tick = best_bid_tick.min(self.roi_ub_tick);
        let upto_tick = ((mid_price * (1.0 - self.looking_depth) / self.tick_size).ceil() as i64)
            .max(self.roi_lb_tick);

        let mut price_tick = from_tick;
        while price_tick > upto_tick {
            let idx = (price_tick - self.roi_lb_tick) as usize;
            if idx < bid_depth.len() {
                sum_bid_qty += bid_depth[idx];
            }
            price_tick -= 1;
        }

        // Compute raw imbalance
        self.current_raw = sum_bid_qty - sum_ask_qty;

        // Update rolling statistics and compute z-score
        self.welford.update(self.current_raw);
        self.current_zscore = self.welford.population_zscore(self.current_raw);

        self.current_zscore
    }

    /// Returns the current raw (non-normalized) OBI value.
    #[inline]
    pub fn raw(&self) -> f64 {
        self.current_raw
    }

    /// Returns the current z-score normalized OBI value.
    #[inline]
    pub fn zscore(&self) -> f64 {
        self.current_zscore
    }

    /// Returns the rolling mean of raw OBI values.
    #[inline]
    pub fn mean(&self) -> f64 {
        self.welford.mean()
    }

    /// Returns the rolling standard deviation of raw OBI values.
    #[inline]
    pub fn std(&self) -> f64 {
        self.welford.std()
    }

    /// Returns the number of samples processed.
    #[inline]
    pub fn sample_count(&self) -> usize {
        self.welford.count()
    }

    /// Updates the OBI factor using a depth accessor function.
    ///
    /// This method is useful when working with market depth implementations
    /// that don't provide direct array access (e.g., HashMapMarketDepth).
    ///
    /// # Arguments
    ///
    /// * `best_bid_tick` - Best bid price tick
    /// * `best_ask_tick` - Best ask price tick
    /// * `mid_price` - Current mid price
    /// * `bid_qty_at_tick` - Function to get bid quantity at a given tick
    /// * `ask_qty_at_tick` - Function to get ask quantity at a given tick
    ///
    /// # Returns
    ///
    /// The z-score normalized OBI value.
    #[inline]
    pub fn update_with_accessor<F, G>(
        &mut self,
        best_bid_tick: i64,
        best_ask_tick: i64,
        mid_price: f64,
        bid_qty_at_tick: F,
        ask_qty_at_tick: G,
    ) -> f64
    where
        F: Fn(i64) -> f64,
        G: Fn(i64) -> f64,
    {
        // Calculate ask depth sum
        let mut sum_ask_qty = 0.0;
        let from_tick = best_ask_tick.max(self.roi_lb_tick);
        let upto_tick = ((mid_price * (1.0 + self.looking_depth) / self.tick_size).floor() as i64)
            .min(self.roi_ub_tick);

        for price_tick in from_tick..upto_tick {
            sum_ask_qty += ask_qty_at_tick(price_tick);
        }

        // Calculate bid depth sum
        let mut sum_bid_qty = 0.0;
        let from_tick = best_bid_tick.min(self.roi_ub_tick);
        let upto_tick = ((mid_price * (1.0 - self.looking_depth) / self.tick_size).ceil() as i64)
            .max(self.roi_lb_tick);

        let mut price_tick = from_tick;
        while price_tick > upto_tick {
            sum_bid_qty += bid_qty_at_tick(price_tick);
            price_tick -= 1;
        }

        // Compute raw imbalance
        self.current_raw = sum_bid_qty - sum_ask_qty;

        // Update rolling statistics and compute z-score
        self.welford.update(self.current_raw);
        self.current_zscore = self.welford.population_zscore(self.current_raw);

        self.current_zscore
    }

    /// Updates the OBI factor with a raw imbalance value directly.
    ///
    /// Use this when you've already computed the raw OBI externally.
    #[inline]
    pub fn update_raw(&mut self, raw_obi: f64) -> f64 {
        self.current_raw = raw_obi;
        self.welford.update(raw_obi);
        self.current_zscore = self.welford.zscore(raw_obi);
        self.current_zscore
    }
}

impl Factor for OBIFactor {
    #[inline]
    fn value(&self) -> f64 {
        self.current_zscore
    }

    #[inline]
    fn reset(&mut self) {
        self.welford.reset();
        self.current_raw = 0.0;
        self.current_zscore = 0.0;
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
    fn test_obi_factor_basic() {
        // Use larger looking_depth (1% = 0.01) to capture meaningful depth
        let mut obi = OBIFactor::new(0.01, 0.1, 0, 1000, 10);

        // Create simple depth arrays
        let bid_depth = vec![100.0; 1000];
        let ask_depth = vec![50.0; 1000];

        // Best bid at tick 500, best ask at tick 501, mid_price = 50.05
        // With 1% depth:
        //   Ask range: 501 to floor(50.05 * 1.01 / 0.1) = 505
        //   Bid range: 500 down to ceil(50.05 * 0.99 / 0.1) = 496
        let zscore = obi.update(&bid_depth, &ask_depth, 500, 501, 50.05);

        // With asymmetric depth (bid=100, ask=50), raw OBI should be positive
        assert!(obi.raw() > 0.0, "raw OBI should be positive, got {}", obi.raw());
        assert!(zscore.is_finite());
    }
}
