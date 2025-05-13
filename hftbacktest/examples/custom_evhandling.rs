use std::collections::HashMap;

use hftbacktest::{
    backtest::{
        Backtest,
        BacktestError,
        DataSource,
        assettype::{AssetType, LinearAsset},
        data::Reader,
        models::{
            CommonFees,
            ConstantLatency,
            FeeModel,
            LatencyModel,
            PowerProbQueueFunc3,
            ProbQueueModel,
            TradingValueFeeModel,
        },
        order::order_bus,
        proc::{Local, LocalProcessor, NoPartialFillExchange, Processor},
        state::State,
    },
    depth::{HashMapMarketDepth, L2MarketDepth, MarketDepth},
    prelude::{Bot, Event, OrdType, Order, OrderId, Side, StateValues, TimeInForce},
};

/// Handling tick events and order response events through the event handler approach requires
/// creating a custom local processor that wraps the Local struct.
pub struct LocalEvHandler<AT, LM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    MD: MarketDepth,
    FM: FeeModel,
{
    local: Local<AT, LM, MD, FM>,
}

/// This implements LocalProcessor trait for the wrapper and delegate calls to the original
/// functions of Local.
impl<AT, LM, MD, FM> LocalProcessor<MD> for LocalEvHandler<AT, LM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    MD: MarketDepth + L2MarketDepth,
    FM: FeeModel,
{
    fn submit_order(
        &mut self,
        order_id: OrderId,
        side: Side,
        price: f64,
        qty: f64,
        order_type: OrdType,
        time_in_force: TimeInForce,
        current_timestamp: i64,
    ) -> Result<(), BacktestError> {
        self.local.submit_order(
            order_id,
            side,
            price,
            qty,
            order_type,
            time_in_force,
            current_timestamp,
        )
    }

    fn modify(
        &mut self,
        order_id: OrderId,
        price: f64,
        qty: f64,
        current_timestamp: i64,
    ) -> Result<(), BacktestError> {
        self.local.modify(order_id, price, qty, current_timestamp)
    }

    fn cancel(&mut self, order_id: OrderId, current_timestamp: i64) -> Result<(), BacktestError> {
        self.local.cancel(order_id, current_timestamp)
    }

    fn clear_inactive_orders(&mut self) {
        self.local.clear_inactive_orders()
    }

    fn position(&self) -> f64 {
        self.local.position()
    }

    fn state_values(&self) -> &StateValues {
        self.local.state_values()
    }

    fn depth(&self) -> &MD {
        self.local.depth()
    }

    fn orders(&self) -> &HashMap<u64, Order> {
        self.local.orders()
    }

    fn last_trades(&self) -> &[Event] {
        self.local.last_trades()
    }

    fn clear_last_trades(&mut self) {
        self.local.clear_last_trades()
    }

    fn feed_latency(&self) -> Option<(i64, i64)> {
        self.local.feed_latency()
    }

    fn order_latency(&self) -> Option<(i64, i64, i64)> {
        self.local.order_latency()
    }
}

/// This implements the Processor trait for the wrapper, delegates calls to the original functions
/// of Local, and adds custom logic when market events occur or order responses are received.
impl<AT, LM, MD, FM> Processor for LocalEvHandler<AT, LM, MD, FM>
where
    AT: AssetType,
    LM: LatencyModel,
    MD: MarketDepth + L2MarketDepth,
    FM: FeeModel,
{
    fn event_seen_timestamp(&self, event: &Event) -> Option<i64> {
        self.local.event_seen_timestamp(event)
    }

    fn process(&mut self, ev: &Event) -> Result<(), BacktestError> {
        self.local.process(ev)?;
        // todo: implement logic for handling market feed events.
        Ok(())
    }

    fn process_recv_order(
        &mut self,
        timestamp: i64,
        wait_resp_order_id: Option<OrderId>,
    ) -> Result<bool, BacktestError> {
        let result = self
            .local
            .process_recv_order2(timestamp, wait_resp_order_id, |order| {
                // todo: Implement logic for handling order response events.
            })?;
        Ok(result)
    }

    fn earliest_recv_order_timestamp(&self) -> i64 {
        self.local.earliest_recv_order_timestamp()
    }

    fn earliest_send_order_timestamp(&self) -> i64 {
        self.local.earliest_send_order_timestamp()
    }
}

fn main() {
    // This shows how to create a Backtest instance from scratch to utilize the custom local
    // processor.
    let data = vec![DataSource::<Event>::File(
        "btcusdt_20250101.npz".to_string(),
    )];

    let reader = Reader::builder()
        .parallel_load(true)
        .data(data)
        .build()
        .unwrap();

    let tick_size = 0.1;
    let lot_size = 0.001;

    let order_latency = ConstantLatency::new(10_000_000, 10_000_000);
    let (order_e2l, order_l2e) = order_bus(order_latency);

    let local = LocalEvHandler {
        local: Local::new(
            HashMapMarketDepth::new(tick_size, lot_size),
            State::new(
                LinearAsset::new(1.0),
                TradingValueFeeModel::new(CommonFees::new(-0.00005, 0.0007)),
            ),
            0,
            order_l2e,
        ),
    };

    let exch = NoPartialFillExchange::new(
        HashMapMarketDepth::new(tick_size, lot_size),
        State::new(
            LinearAsset::new(1.0),
            TradingValueFeeModel::new(CommonFees::new(-0.00005, 0.0007)),
        ),
        ProbQueueModel::new(PowerProbQueueFunc3::new(3.0)),
        order_e2l,
    );

    let mut hbt = Backtest::new(vec![Box::new(local)], vec![Box::new(exch)], vec![reader]);

    // Advances time until the end of the data.
    hbt.goto_end().unwrap();
}
