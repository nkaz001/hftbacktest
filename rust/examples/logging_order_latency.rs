use algo::gridtrading;
use chrono::Utc;
use hftbacktest::{
    connector::binancefutures::{BinanceFutures, Endpoint},
    live::bot::Bot,
    ty::Status,
    Interface,
};
use tracing::info;

mod algo;

const ORDER_PREFIX: &str = "prefix";
const API_KEY: &str = "apikey";
const SECRET: &str = "secret";

fn prepare_live() -> Bot {
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
        .order_recv_hook(|req, resp| {
            if (req.req == Status::New || req.req == Status::Canceled) && (resp.req == Status::None)
            {
                info!(
                    req_timestamp = req.local_timestamp,
                    exch_timestamp = resp.exch_timestamp,
                    resp_timestamp = Utc::now().timestamp_nanos_opt().unwrap(),
                    req = ?req.req,
                    "Order response is received."
                );
            }
            Ok(())
        })
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
