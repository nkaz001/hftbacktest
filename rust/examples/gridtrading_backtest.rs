use algo::gridtrading;
use hftbacktest::{
    backtest::{
        assettype::LinearAsset,
        backtest::MultiAssetMultiExchangeBacktest,
        models::{IntpOrderLatency, PowerProbQueueFunc3, ProbQueueModel, QueuePos},
        reader::read_npz,
        BacktestAssetBuilder,
        DataSource,
    },
    depth::HashMapMarketDepth,
    Interface,
};

mod algo;

fn prepare_backtest() -> MultiAssetMultiExchangeBacktest<QueuePos, HashMapMarketDepth> {
    let latency_model = IntpOrderLatency::new(read_npz("latency_20240215.npz").unwrap());
    let asset_type = LinearAsset::new(1.0);
    let queue_model = ProbQueueModel::new(PowerProbQueueFunc3::new(3.0));

    let hbt = MultiAssetMultiExchangeBacktest::builder()
        .add(
            BacktestAssetBuilder::new()
                .data(vec![DataSource::File("SOLUSDT_20240215.npz".to_string())])
                .latency_model(latency_model)
                .asset_type(asset_type)
                .queue_model(queue_model)
                .depth(|| HashMapMarketDepth::new(0.001, 1.0))
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();
    hbt
}

fn main() {
    tracing_subscriber::fmt::init();

    let mut hbt = prepare_backtest();

    let half_spread = 0.05;
    let grid_interval = 0.05;
    let skew = 0.004;
    let order_qty = 1.0;

    gridtrading(&mut hbt, half_spread, grid_interval, skew, order_qty).unwrap();
    hbt.close().unwrap();
}
