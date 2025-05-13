use algo::gridtrading;
use chrono::Utc;
use hftbacktest::{
    live::{LiveBot, LoggingRecorder, ipc::iceoryx::IceoryxUnifiedChannel},
    prelude::{Bot, HashMapMarketDepth, Status},
};
use tracing::info;

mod algo;

const ORDER_PREFIX: &str = "prefix";

fn prepare_live() -> LiveBot<IceoryxUnifiedChannel, HashMapMarketDepth> {
    let mut hbt = LiveBot::builder()
        .register("binancefutures", "SOLUSDT", 0.001, 1.0)
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

    hbt
}

fn main() {
    tracing_subscriber::fmt::init();

    let mut hbt = prepare_live();

    let relative_half_spread = 0.0005;
    let relative_grid_interval = 0.0005;
    let grid_num = 10;
    let min_grid_step = 0.001; // tick size
    let skew = relative_half_spread / grid_num as f64;
    let order_qty = 1.0;
    let max_position = grid_num as f64 * order_qty;

    let mut recorder = LoggingRecorder::new();
    gridtrading(
        &mut hbt,
        &mut recorder,
        relative_half_spread,
        relative_grid_interval,
        grid_num,
        min_grid_step,
        skew,
        order_qty,
        max_position,
    )
    .unwrap();
    hbt.close().unwrap();
}
