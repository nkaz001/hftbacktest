use algo::gridtrading;
use hftbacktest::{
    connector::binancefutures::{BinanceFutures, Endpoint},
    live::{Bot, LoggingRecorder},
    prelude::{HashMapMarketDepth, Interface},
};

mod algo;

const ORDER_PREFIX: &str = "prefix";
const API_KEY: &str = "apikey";
const SECRET: &str = "secret";

fn prepare_live() -> Bot<HashMapMarketDepth> {
    let binance_futures = BinanceFutures::builder()
        .endpoint(Endpoint::Testnet)
        .api_key(API_KEY)
        .secret(SECRET)
        .order_prefix(ORDER_PREFIX)
        .build()
        .unwrap();

    let mut hbt = Bot::builder()
        .register("binancefutures", binance_futures)
        .add("binancefutures", "1000SHIBUSDT", 0.000001, 1.0)
        .depth(|asset| HashMapMarketDepth::new(asset.tick_size, asset.lot_size))
        .build()
        .unwrap();

    hbt.run().unwrap();
    hbt
}

fn main() {
    tracing_subscriber::fmt::init();

    let mut hbt = prepare_live();

    let half_spread_bp = 0.0005;
    let grid_interval_bp = 0.0005;
    let grid_num = 20;
    let skew = half_spread_bp / grid_num as f64;
    let order_qty = 1.0;

    let mut recorder = LoggingRecorder::new();
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
}
