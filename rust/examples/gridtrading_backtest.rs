use std::time::Instant;

use algo::gridtrading;
use hftbacktest::{
    backtest::{
        assettype::LinearAsset,
        models::{IntpOrderLatency, PowerProbQueueFunc3, ProbQueueModel, QueuePos},
        reader::read_npz,
        recorder::BacktestRecorder,
        AssetBuilder,
        DataSource,
        ExchangeKind,
        MultiAssetMultiExchangeBacktest,
    },
    prelude::{ApplySnapshot, HashMapMarketDepth, Interface},
};

mod algo;

fn prepare_backtest() -> MultiAssetMultiExchangeBacktest<QueuePos, HashMapMarketDepth> {
    let latency_data = (20240501..20240532)
        .map(|date| DataSource::File(format!("latency_{date}.npz")))
        .collect();

    let latency_model = IntpOrderLatency::new(latency_data).unwrap();
    let asset_type = LinearAsset::new(1.0);
    let queue_model = ProbQueueModel::new(PowerProbQueueFunc3::new(3.0));

    let data = (20240501..20240532)
        .map(|date| DataSource::File(format!("1000SHIBUSDT_{date}.npz")))
        .collect();

    let hbt = MultiAssetMultiExchangeBacktest::builder()
        .add(
            AssetBuilder::new()
                .data(data)
                .latency_model(latency_model)
                .asset_type(asset_type)
                .maker_fee(-0.00005)
                .taker_fee(0.0007)
                .queue_model(queue_model)
                .depth(|| {
                    let mut depth = HashMapMarketDepth::new(0.000001, 1.0);
                    depth.apply_snapshot(&read_npz("1000SHIBUSDT_20240501_SOD.npz").unwrap());
                    depth
                })
                .exchange(ExchangeKind::NoPartialFillExchange)
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();
    hbt
}

fn main() {
    tracing_subscriber::fmt::init();

    let relative_half_spread = 0.0005;
    let relative_grid_interval = 0.0005;
    let grid_num = 10;
    let skew = relative_half_spread / grid_num as f64;
    let order_qty = 1.0;
    let max_position = grid_num as f64 * order_qty;

    let mut hbt = prepare_backtest();
    let mut recorder = BacktestRecorder::new(&hbt);
    gridtrading(
        &mut hbt,
        &mut recorder,
        relative_half_spread,
        relative_grid_interval,
        grid_num,
        skew,
        order_qty,
        max_position,
    )
    .unwrap();
    hbt.close().unwrap();
    recorder.to_csv("gridtrading", ".").unwrap();
}
