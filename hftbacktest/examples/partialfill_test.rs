/// Test binary for comparing PartialFillExchange vs NoPartialFillExchange.
/// Runs gridtrading strategy with configurable exchange model and order qty multiplier.
use std::{collections::HashMap, fmt::Debug, path::PathBuf};

use clap::Parser;
use hftbacktest::{
    backtest::{
        Backtest, ExchangeKind, L2AssetBuilder,
        assettype::LinearAsset,
        data::{DataSource, read_npz_file},
        models::{
            CommonFees, ConstantLatency, PowerProbQueueFunc3, ProbQueueModel, TradingValueFeeModel,
        },
        recorder::BacktestRecorder,
    },
    prelude::*,
    types::Event,
};

#[derive(Parser, Debug)]
#[command(about = "PartialFill vs NoPartialFill comparison test")]
struct Args {
    /// Data directory containing NPZ files
    #[arg(long)]
    data_dir: String,

    /// Symbol name (e.g. BTCUSDT)
    #[arg(long)]
    symbol: String,

    /// Exchange kind: "partial" or "nopartial"
    #[arg(long, default_value = "nopartial")]
    exchange_kind: String,

    /// Order quantity multiplier (1, 4, 16)
    #[arg(long, default_value_t = 1.0)]
    qty_multiplier: f64,

    /// Tick size
    #[arg(long, default_value_t = 0.01)]
    tick_size: f64,

    /// Lot size
    #[arg(long, default_value_t = 0.001)]
    lot_size: f64,

    /// Base order quantity
    #[arg(long, default_value_t = 0.01)]
    base_qty: f64,

    /// Output directory
    #[arg(long, default_value = "./results")]
    output: String,

    /// Max days to process (0 = all)
    #[arg(long, default_value_t = 0)]
    max_days: usize,

    /// Maker fee
    #[arg(long, default_value_t = 0.0001)]
    maker_fee: f64,

    /// Taker fee
    #[arg(long, default_value_t = 0.0005)]
    taker_fee: f64,
}

fn gridtrading<MD, I, R>(
    hbt: &mut I,
    recorder: &mut R,
    order_qty: f64,
    max_position: f64,
) -> Result<(), Box<dyn std::error::Error>>
where
    MD: MarketDepth,
    I: Bot<MD>,
    <I as Bot<MD>>::Error: Debug,
    R: Recorder,
    <R as Recorder>::Error: Debug,
{
    let tick_size = hbt.depth(0).tick_size() as f64;
    let relative_half_spread = 0.0005;
    let relative_grid_interval = 0.0005;
    let grid_num = 10;
    let skew = relative_half_spread / grid_num as f64;
    let min_grid_step = tick_size;

    let min_grid_step = (min_grid_step / tick_size).round() * tick_size;
    let mut int = 0;

    while ElapseResult::Ok == hbt.elapse(100_000_000).unwrap() {
        int += 1;
        if int % 10 == 0 {
            recorder.record(hbt).unwrap();
        }

        let depth = hbt.depth(0);
        let position = hbt.position(0);

        if depth.best_bid_tick() == INVALID_MIN || depth.best_ask_tick() == INVALID_MAX {
            continue;
        }

        let mid_price = (depth.best_bid() + depth.best_ask()) as f64 / 2.0;
        let normalized_position = position / order_qty;

        let relative_bid_depth = relative_half_spread + skew * normalized_position;
        let relative_ask_depth = relative_half_spread - skew * normalized_position;
        let forecast_mid_price = mid_price;

        let bid_price =
            (forecast_mid_price * (1.0 - relative_bid_depth)).min(depth.best_bid() as f64);
        let ask_price =
            (forecast_mid_price * (1.0 + relative_ask_depth)).max(depth.best_ask() as f64);

        let grid_interval = ((forecast_mid_price * relative_grid_interval / min_grid_step).round()
            * min_grid_step)
            .max(min_grid_step);

        let mut bid_price = (bid_price / grid_interval).floor() * grid_interval;
        let mut ask_price = (ask_price / grid_interval).ceil() * grid_interval;

        hbt.clear_inactive_orders(Some(0));

        {
            let orders = hbt.orders(0);
            let mut new_bid_orders = HashMap::new();
            if position < max_position && bid_price.is_finite() {
                for _ in 0..grid_num {
                    let bid_price_tick = (bid_price / tick_size).round() as u64;
                    new_bid_orders.insert(bid_price_tick, bid_price);
                    bid_price -= grid_interval;
                }
            }
            let cancel_order_ids: Vec<u64> = orders
                .values()
                .filter(|order| {
                    order.side == Side::Buy
                        && order.cancellable()
                        && !new_bid_orders.contains_key(&order.order_id)
                })
                .map(|order| order.order_id)
                .collect();
            let new_orders: Vec<(u64, f64)> = new_bid_orders
                .into_iter()
                .filter(|(order_id, _)| !orders.contains_key(order_id))
                .collect();
            for order_id in cancel_order_ids {
                hbt.cancel(0, order_id, false).unwrap();
            }
            for (order_id, order_price) in new_orders {
                hbt.submit_buy_order(
                    0,
                    order_id,
                    order_price,
                    order_qty,
                    TimeInForce::GTX,
                    OrdType::Limit,
                    false,
                    false,
                )
                .unwrap();
            }
        }

        {
            let orders = hbt.orders(0);
            let mut new_ask_orders = HashMap::new();
            if position > -max_position && ask_price.is_finite() {
                for _ in 0..grid_num {
                    let ask_price_tick = (ask_price / tick_size).round() as u64;
                    new_ask_orders.insert(ask_price_tick, ask_price);
                    ask_price += grid_interval;
                }
            }
            let cancel_order_ids: Vec<u64> = orders
                .values()
                .filter(|order| {
                    order.side == Side::Sell
                        && order.cancellable()
                        && !new_ask_orders.contains_key(&order.order_id)
                })
                .map(|order| order.order_id)
                .collect();
            let new_orders: Vec<(u64, f64)> = new_ask_orders
                .into_iter()
                .filter(|(order_id, _)| !orders.contains_key(order_id))
                .collect();
            for order_id in cancel_order_ids {
                hbt.cancel(0, order_id, false).unwrap();
            }
            for (order_id, order_price) in new_orders {
                hbt.submit_sell_order(
                    0,
                    order_id,
                    order_price,
                    order_qty,
                    TimeInForce::GTX,
                    OrdType::Limit,
                    false,
                    false,
                )
                .unwrap();
            }
        }
    }
    Ok(())
}

fn main() {
    tracing_subscriber::fmt::init();

    let args = Args::parse();

    let exch_kind = match args.exchange_kind.as_str() {
        "partial" => ExchangeKind::PartialFillExchange,
        "nopartial" => ExchangeKind::NoPartialFillExchange,
        other => {
            eprintln!("Unknown exchange kind: {other}. Use 'partial' or 'nopartial'.");
            std::process::exit(1);
        }
    };

    let order_qty = args.base_qty * args.qty_multiplier;
    let max_position = order_qty * 10.0;

    // Collect data files
    let data_dir = PathBuf::from(&args.data_dir);
    let mut data_files: Vec<String> = std::fs::read_dir(&data_dir)
        .expect("Cannot read data directory")
        .filter_map(|e| e.ok())
        .map(|e| e.file_name().to_string_lossy().to_string())
        .filter(|name| {
            name.starts_with(&args.symbol) && name.ends_with(".npz") && !name.contains("_last")
        })
        .collect();
    data_files.sort();

    if args.max_days > 0 && data_files.len() > args.max_days {
        data_files.truncate(args.max_days);
    }

    if data_files.is_empty() {
        eprintln!(
            "No data files found for {} in {}",
            args.symbol, args.data_dir
        );
        std::process::exit(1);
    }

    // Find initial snapshot (first file's _last counterpart or SOD)
    let first_date = &data_files[0];
    let snapshot_name = first_date.replace(".npz", "_last.npz");
    let snapshot_path = data_dir.join(&snapshot_name);
    let has_snapshot = snapshot_path.exists();

    eprintln!(
        "Config: symbol={}, exchange={}, qty_mult={}, order_qty={}, days={}, snapshot={}",
        args.symbol,
        args.exchange_kind,
        args.qty_multiplier,
        order_qty,
        data_files.len(),
        has_snapshot,
    );

    let data: Vec<DataSource<Event>> = data_files
        .iter()
        .map(|f| DataSource::File(data_dir.join(f).to_string_lossy().to_string()))
        .collect();

    let latency_model = ConstantLatency::new(1_000_000, 1_000_000); // 1ms each way
    let asset_type = LinearAsset::new(1.0);
    let queue_model = ProbQueueModel::new(PowerProbQueueFunc3::new(2.0));
    let tick_size = args.tick_size;
    let lot_size = args.lot_size;

    let snapshot_file = if has_snapshot {
        Some(snapshot_path.to_string_lossy().to_string())
    } else {
        None
    };

    let hbt = Backtest::builder()
        .add_asset(
            L2AssetBuilder::new()
                .data(data)
                .latency_model(latency_model)
                .asset_type(asset_type)
                .fee_model(TradingValueFeeModel::new(CommonFees::new(
                    args.maker_fee,
                    args.taker_fee,
                )))
                .exchange(exch_kind)
                .queue_model(queue_model)
                .depth(move || {
                    let mut depth = HashMapMarketDepth::new(tick_size, lot_size);
                    if let Some(ref file) = snapshot_file {
                        depth.apply_snapshot(&read_npz_file(file, "data").unwrap());
                    }
                    depth
                })
                .build()
                .unwrap(),
        )
        .build()
        .unwrap();

    let mut hbt = hbt;
    let mut recorder = BacktestRecorder::new(&hbt);

    let result =
        gridtrading::<HashMapMarketDepth, _, _>(&mut hbt, &mut recorder, order_qty, max_position);

    match result {
        Ok(()) => eprintln!("Backtest completed successfully."),
        Err(e) => eprintln!("Backtest error: {e:?}"),
    }

    hbt.close().unwrap();

    // Create output directory
    std::fs::create_dir_all(&args.output).unwrap();

    let prefix = format!(
        "{}_{}_{:.0}x_",
        args.symbol, args.exchange_kind, args.qty_multiplier,
    );
    recorder.to_csv(&prefix, &args.output).unwrap();
    eprintln!("Results written to {}/{prefix}0.csv", args.output);
}
