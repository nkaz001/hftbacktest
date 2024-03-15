use hftbacktest::{
    connector::binancefutures::BinanceFutures,
    live::{bot::Bot, LiveBuilder},
    Interface,
};

mod algo;

use algo::gridtrading;

const STREAM_URL: &str = "wss://fstream.binancefuture.com/stream?streams=";
const API_URL: &str = "https://testnet.binancefuture.com";
const ORDER_PREFIX: &str = "prefix";
const API_KEY: &str = "apikey";
const SECRET: &str = "secret";

fn prepare_live() -> Bot {
    let binance_futures = BinanceFutures::new(
        STREAM_URL,
        API_URL,
        ORDER_PREFIX,
        API_KEY,
        SECRET
    );

    let mut hbt = LiveBuilder::new()
        .register("binancefutures", binance_futures)
        .add("binancefutures", "SOLUSDT", 0.001, 1.0)
        .build()
        .unwrap();

    hbt.run();
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