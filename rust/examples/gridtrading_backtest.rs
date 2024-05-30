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
        MultiAssetMultiExchangeBacktest,
    },
    prelude::{HashMapMarketDepth, Interface},
};
use hftbacktest::backtest::ExchangeKind;

mod algo;

fn prepare_backtest() -> MultiAssetMultiExchangeBacktest<QueuePos, HashMapMarketDepth> {
    let latency_model = IntpOrderLatency::new(read_npz("latency_20240215.npz").unwrap());
    let asset_type = LinearAsset::new(1.0);
    let queue_model = ProbQueueModel::new(PowerProbQueueFunc3::new(3.0));

    let hbt = MultiAssetMultiExchangeBacktest::builder()
        .add(
            AssetBuilder::new()
                .data(vec![DataSource::File("SOLUSDT_20240215.npz".to_string())])
                .latency_model(latency_model)
                .asset_type(asset_type)
                .maker_fee(-0.00005)
                .taker_fee(0.0007)
                .queue_model(queue_model)
                .depth(|| HashMapMarketDepth::new(0.001, 1.0))
                .exchange(ExchangeKind::NoPartialFillExchange)
                .trade_len(1000)
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();
    hbt
}

fn main() {
    tracing_subscriber::fmt::init();

    let half_spread_bp = 0.0005;
    let grid_interval_bp = 0.0005;
    let grid_num = 20;
    let skew = half_spread_bp / grid_num as f64;
    let order_qty = 1.0;

    let mut start = Instant::now();
    let mut hbt = prepare_backtest();
    let mut recorder = BacktestRecorder::new(&hbt);
    gridtrading(
        &mut hbt,
        &mut recorder,
        half_spread_bp,
        grid_interval_bp,
        grid_num,
        skew,
        order_qty
    )
        .unwrap();
    hbt.close().unwrap();
    print!("{} seconds", start.elapsed().as_secs());
    recorder.to_csv(".").unwrap();
}
