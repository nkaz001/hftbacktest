use algo::gridtrading;
use hftbacktest::{
    connector::binancefutures::{BinanceFutures, Endpoint},
    live::Bot,
    prelude::{Interface, HashMapMarketDepth},
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
        .add("binancefutures", "SOLUSDT", 0.001, 1.0)
        .depth(|asset| HashMapMarketDepth::new(asset.tick_size, asset.lot_size))
        .build()
        .unwrap();

    hbt.run().unwrap();
    hbt
}

fn main() {
    tracing_subscriber::fmt::init();

    let mut hbt = prepare_live();

    let half_spread = 0.05;
    let grid_interval = 0.05;
    let skew = 0.004;
    let order_qty = 1.0;

    gridtrading(&mut hbt, half_spread, grid_interval, skew, order_qty).unwrap();
    hbt.close().unwrap();
}
