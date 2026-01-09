//! Core OBI Market Making Strategy Implementation.
//!
//! This module contains the stateless strategy logic that can be used
//! with both backtesting and live trading.
//!
//! This strategy uses the `factor-engine` crate for all factor computations,
//! providing O(1) streaming calculations for:
//! - Order Book Imbalance (OBI) with z-score normalization
//! - Rolling volatility from price returns
//! - Stop loss detection based on volatility/change thresholds

use std::collections::HashMap;

use factor_engine::factors::stop_loss::StopLossStatus;
use factor_engine::factors::Factor;
use factor_engine::{OBIFactor, StopLossChecker, VolatilityFactor};
use hftbacktest::prelude::*;
use tracing::{info, warn};

use crate::config::{GlobalConfig, StopLossConfig, StrategyParams};

/// OBI Market Making Strategy.
///
/// This strategy uses Order Book Imbalance as the alpha signal to adjust
/// the fair price, with volatility-based spread adjustment and stop loss protection.
///
/// All factor computations are delegated to the `factor-engine` crate.
pub struct ObiMmStrategy {
    /// Global configuration
    pub global: GlobalConfig,
    /// Strategy parameters
    pub params: StrategyParams,
    /// OBI factor from factor-engine (O(1) z-score computation)
    pub obi_factor: OBIFactor,
    /// Volatility factor from factor-engine (O(1) rolling std)
    pub volatility_factor: VolatilityFactor,
    /// Stop loss checker from factor-engine
    pub stop_loss: StopLossChecker,
    /// Last check period
    pub(crate) last_check_period: i64,
    /// Last change period (for volatility calculation)
    pub(crate) last_change_period: i64,
    /// Last price for return calculation
    pub(crate) last_price: f64,
    /// Current minute change
    pub(crate) minute_change: f64,
    /// Tick size
    pub(crate) tick_size: f64,
}

impl ObiMmStrategy {
    /// Returns true if all factors have enough data to produce meaningful values.
    #[inline]
    pub fn is_ready(&self) -> bool {
        self.obi_factor.is_ready() && self.volatility_factor.is_ready()
    }

    /// Returns the warmup progress as (obi_samples, obi_required, vol_samples, vol_required).
    #[inline]
    pub fn warmup_progress(&self) -> (usize, usize, usize, usize) {
        (
            self.obi_factor.sample_count(),
            self.params.window,
            self.volatility_factor.sample_count(),
            self.params.volatility_window,
        )
    }

    /// Creates a new OBI MM strategy.
    ///
    /// All factor engines are initialized with parameters from the config.
    pub fn new(
        global: GlobalConfig,
        params: StrategyParams,
        stop_loss_config: StopLossConfig,
        tick_size: f64,
    ) -> Self {
        let roi_lb_tick = (global.roi_lb / tick_size).round() as i64;
        let roi_ub_tick = (global.roi_ub / tick_size).round() as i64;

        Self {
            // Initialize OBI factor from factor-engine
            obi_factor: OBIFactor::new(
                params.looking_depth,
                tick_size,
                roi_lb_tick,
                roi_ub_tick,
                params.window,
            ),
            // Initialize volatility factor from factor-engine
            volatility_factor: VolatilityFactor::new(
                params.volatility_interval_ns,
                params.volatility_window,
            ),
            // Initialize stop loss checker from factor-engine
            stop_loss: StopLossChecker::new(
                stop_loss_config.volatility_mean,
                stop_loss_config.volatility_std,
                stop_loss_config.change_mean,
                stop_loss_config.change_std,
                stop_loss_config.volatility_threshold,
                stop_loss_config.change_threshold,
            ),
            global,
            params,
            last_check_period: -1,
            last_change_period: -1,
            last_price: 0.0,
            minute_change: 0.0,
            tick_size,
        }
    }

    /// Runs the strategy for one iteration.
    ///
    /// Returns true if orders were updated, false otherwise.
    pub fn update<MD>(&mut self, bot: &mut impl Bot<MD>, asset_no: usize) -> bool
    where
        MD: MarketDepth + L2MarketDepth,
    {
        let current_timestamp = bot.current_timestamp();

        // Clear filled orders
        bot.clear_inactive_orders(Some(asset_no));

        // Calculate current period (every 10 seconds)
        let current_period = current_timestamp / self.params.update_interval_ns;

        // Check if we should update
        if current_period <= self.last_check_period {
            return false;
        }

        // Get market data
        let depth = bot.depth(asset_no);
        let position = bot.position(asset_no);

        let best_bid = depth.best_bid();
        let best_ask = depth.best_ask();

        if best_bid <= 0.0 || best_ask <= 0.0 {
            return false;
        }

        let mid_price = (best_bid + best_ask) / 2.0;
        let best_bid_tick = depth.best_bid_tick();
        let best_ask_tick = depth.best_ask_tick();

        // Update volatility (every 60 seconds)
        let change_period = current_period / 6;
        let mut volatility = self.stop_loss.volatility_mean();

        if change_period > self.last_change_period {
            if self.last_price > 0.0 {
                self.minute_change = mid_price / self.last_price - 1.0;
            } else {
                self.minute_change = 0.0;
            }

            // Update volatility factor
            volatility = self.volatility_factor.update(current_timestamp, mid_price);
            if volatility == 0.0 {
                volatility = self.stop_loss.volatility_mean();
            }

            // Check stop loss
            let prev_status = self.stop_loss.status();
            self.stop_loss.check(volatility, self.minute_change);
            let new_status = self.stop_loss.status();

            if new_status != prev_status {
                warn!(
                    ?prev_status, ?new_status, %volatility, minute_change = %self.minute_change,
                    "Stop loss status changed"
                );
            }

            self.last_price = mid_price;
            self.last_change_period = change_period;
        }

        // Calculate OBI using factor-engine
        // The OBIFactor handles depth scanning and z-score normalization internally
        let depth = bot.depth(asset_no);
        let alpha = self.obi_factor.update_with_accessor(
            best_bid_tick,
            best_ask_tick,
            mid_price,
            |tick| depth.bid_qty_at_tick(tick),
            |tick| depth.ask_qty_at_tick(tick),
        );

        self.last_check_period = current_period;

        // Skip trading if factors are not ready (still warming up)
        if !self.is_ready() {
            return false;
        }

        // Compute prices
        let volatility_ratio =
            (volatility / self.stop_loss.volatility_mean()).powf(self.params.power);
        let half_spread = self.params.half_spread * volatility_ratio;

        let fair_price = mid_price + self.params.c1 * alpha * mid_price;
        let normalized_position = position / self.global.order_qty;
        let reservation_price = fair_price - self.params.skew * normalized_position * mid_price;

        let mut bid_price = (reservation_price - half_spread * mid_price).min(best_bid);
        let mut ask_price = (reservation_price + half_spread * mid_price).max(best_ask);

        bid_price = (bid_price / self.tick_size).floor() * self.tick_size;
        ask_price = (ask_price / self.tick_size).ceil() * self.tick_size;

        // Update orders based on stop loss status
        if self.stop_loss.status() == StopLossStatus::Normal {
            self.update_grid_orders(
                bot,
                asset_no,
                bid_price,
                ask_price,
                position,
                current_timestamp,
            );
        } else {
            warn!(%mid_price, %position, "STOP LOSS TRIGGERED - executing");
            self.execute_stop_loss(bot, asset_no, mid_price, position);
        }

        true
    }

    /// Updates grid orders for normal operation.
    fn update_grid_orders<MD>(
        &self,
        bot: &mut impl Bot<MD>,
        asset_no: usize,
        mut bid_price: f64,
        mut ask_price: f64,
        position: f64,
        current_timestamp: i64,
    ) where
        MD: MarketDepth,
    {
        // Build new order grids
        let mut new_bid_orders: HashMap<u64, f64> = HashMap::new();
        let mut new_ask_orders: HashMap<u64, f64> = HashMap::new();

        if position < self.global.max_position && bid_price.is_finite() {
            for _ in 0..self.global.grid_num {
                let bid_price_tick = (bid_price / self.tick_size).round() as u64;
                new_bid_orders.insert(bid_price_tick, bid_price);
                bid_price -= self.global.grid_interval;
            }
        }

        if position > -self.global.max_position && ask_price.is_finite() {
            for _ in 0..self.global.grid_num {
                let ask_price_tick = (ask_price / self.tick_size).round() as u64;
                new_ask_orders.insert(ask_price_tick, ask_price);
                ask_price += self.global.grid_interval;
            }
        }

        // First pass: collect order info and orders to cancel
        let mut holding_buy_order_num = 0i32;
        let mut holding_sell_order_num = 0i32;
        let mut orders_to_cancel: Vec<u64> = Vec::new();

        {
            let orders = bot.orders(asset_no);
            for order in orders.values() {
                if order.cancellable() {
                    let dominated = match order.side {
                        Side::Buy => !new_bid_orders.contains_key(&order.order_id),
                        Side::Sell => !new_ask_orders.contains_key(&order.order_id),
                        _ => false,
                    };

                    if dominated {
                        let order_live_seconds =
                            (current_timestamp - order.exch_timestamp) as f64 / 1_000_000_000.0;

                        if order_live_seconds >= self.params.live_seconds {
                            orders_to_cancel.push(order.order_id);
                            match order.side {
                                Side::Buy => holding_buy_order_num -= 1,
                                Side::Sell => holding_sell_order_num -= 1,
                                _ => {}
                            }
                        } else {
                            match order.side {
                                Side::Buy => holding_buy_order_num += 1,
                                Side::Sell => holding_sell_order_num += 1,
                                _ => {}
                            }
                        }
                    } else {
                        match order.side {
                            Side::Buy => holding_buy_order_num += 1,
                            Side::Sell => holding_sell_order_num += 1,
                            _ => {}
                        }
                    }
                }
            }
        }

        // Cancel orders
        for order_id in orders_to_cancel {
            let _ = bot.cancel(asset_no, order_id, false);
        }

        holding_buy_order_num = holding_buy_order_num.max(0);
        holding_sell_order_num = holding_sell_order_num.max(0);

        // Collect existing order IDs
        let existing_order_ids: Vec<u64> = {
            let orders = bot.orders(asset_no);
            orders.keys().copied().collect()
        };

        // Submit new bid orders
        for (&order_id, &order_price) in &new_bid_orders {
            if !existing_order_ids.contains(&order_id) {
                let effective_position =
                    position + 0.3 * holding_buy_order_num as f64 * self.global.order_qty;
                if effective_position < self.global.max_position {
                    let _ = bot.submit_buy_order(
                        asset_no,
                        order_id,
                        order_price,
                        self.global.order_qty,
                        TimeInForce::GTX,
                        OrdType::Limit,
                        false,  // wait
                        false,  // reduce_only
                    );
                }
            }
        }

        // Submit new ask orders
        for (&order_id, &order_price) in &new_ask_orders {
            if !existing_order_ids.contains(&order_id) {
                let effective_position =
                    position - 0.3 * holding_sell_order_num as f64 * self.global.order_qty;
                if effective_position > -self.global.max_position {
                    let _ = bot.submit_sell_order(
                        asset_no,
                        order_id,
                        order_price,
                        self.global.order_qty,
                        TimeInForce::GTX,
                        OrdType::Limit,
                        false,  // wait
                        false,  // reduce_only
                    );
                }
            }
        }
    }

    /// Executes stop loss: cancel all orders and close position.
    fn execute_stop_loss<MD>(
        &self,
        bot: &mut impl Bot<MD>,
        asset_no: usize,
        mid_price: f64,
        position: f64,
    ) where
        MD: MarketDepth,
    {
        // Collect order IDs to cancel
        let order_ids: Vec<u64> = {
            let orders = bot.orders(asset_no);
            orders
                .values()
                .filter(|o| o.cancellable())
                .map(|o| o.order_id)
                .collect()
        };

        // Cancel all orders
        info!(count = order_ids.len(), "Cancelling all orders");
        for order_id in order_ids {
            let _ = bot.cancel(asset_no, order_id, false);
        }

        // Close position with market order (reduce_only=true to bypass min notional)
        let order_qty = position.abs();
        if order_qty > 1e-9 {
            if position > 0.0 {
                let order_price = ((mid_price / self.tick_size).round() - 200.0) * self.tick_size;
                info!(%order_qty, %order_price, "Closing LONG position with SELL market order (reduce_only)");
                let _ = bot.submit_sell_order(
                    asset_no,
                    519,
                    order_price,
                    order_qty,
                    TimeInForce::IOC,
                    OrdType::Market,
                    true,  // wait
                    true,  // reduce_only - bypasses min notional requirement
                );
            } else {
                let order_price = ((mid_price / self.tick_size).round() + 200.0) * self.tick_size;
                info!(%order_qty, %order_price, "Closing SHORT position with BUY market order (reduce_only)");
                let _ = bot.submit_buy_order(
                    asset_no,
                    519,
                    order_price,
                    order_qty,
                    TimeInForce::IOC,
                    OrdType::Market,
                    true,  // wait
                    true,  // reduce_only - bypasses min notional requirement
                );
            }
        } else {
            info!("No position to close");
        }
    }

    /// Resets the strategy state.
    pub fn reset(&mut self) {
        self.last_check_period = -1;
        self.last_change_period = -1;
        self.last_price = 0.0;
        self.minute_change = 0.0;
        self.obi_factor.reset();
        self.volatility_factor.reset();
        self.stop_loss.reset();
    }
}
