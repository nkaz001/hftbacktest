use algo::gridtrading;
use clap::Parser;
use hftbacktest::{
    backtest::{
        assettype::LinearAsset,
        models::{IntpOrderLatency, PowerProbQueueFunc3, ProbQueueModel},
        reader::read_npz,
        recorder::BacktestRecorder,
        AssetBuilder,
        DataSource,
        ExchangeKind,
        MultiAssetMultiExchangeBacktest,
    },
    prelude::{ApplySnapshot, Bot, HashMapMarketDepth},
};

mod algo;

#[derive(Parser, Debug)]
#[command(about = None, long_about = None)]
struct Args {
    #[arg(long)]
    name: String,
    #[arg(long)]
    output_path: String,
    #[arg(long, num_args = 1..)]
    data_files: Vec<String>,
    #[arg(long)]
    initial_snapshot: Option<String>,
    #[arg(long, num_args = 1..)]
    latency_files: Vec<String>,
    #[arg(long)]
    tick_size: f32,
    #[arg(long)]
    lot_size: f32,
    #[arg(long)]
    relative_half_spread: f64,
    #[arg(long)]
    relative_grid_interval: f64,
    #[arg(long)]
    skew: f64,
    #[arg(long)]
    grid_num: usize,
    #[arg(long)]
    min_grid_step: Option<f64>,
    #[arg(long)]
    order_qty: f64,
    #[arg(long)]
    max_position: f64,
    #[arg(long, default_value_t = -0.00005)]
    maker_fee: f64,
    #[arg(long, default_value_t = 0.0007)]
    taker_fee: f64,
}

fn prepare_backtest(
    latency_files: Vec<String>,
    data_files: Vec<String>,
    initial_snapshot: Option<String>,
    tick_size: f32,
    lot_size: f32,
    maker_fee: f64,
    taker_fee: f64,
) -> MultiAssetMultiExchangeBacktest<HashMapMarketDepth> {
    let latency_model = IntpOrderLatency::new(
        latency_files
            .iter()
            .map(|file| DataSource::File(file.clone()))
            .collect(),
    )
    .unwrap();
    let asset_type = LinearAsset::new(1.0);
    let queue_model = ProbQueueModel::new(PowerProbQueueFunc3::new(3.0));

    let hbt = MultiAssetMultiExchangeBacktest::builder()
        .add(
            AssetBuilder::new()
                .data(
                    data_files
                        .iter()
                        .map(|file| DataSource::File(file.clone()))
                        .collect(),
                )
                .latency_model(latency_model)
                .asset_type(asset_type)
                .maker_fee(maker_fee)
                .taker_fee(taker_fee)
                .queue_model(queue_model)
                .depth(move || {
                    let mut depth = HashMapMarketDepth::new(tick_size, lot_size);
                    if let Some(file) = initial_snapshot.as_ref() {
                        depth.apply_snapshot(&read_npz(file).unwrap());
                    }
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

    let args = Args::parse();

    let mut hbt = prepare_backtest(
        args.latency_files,
        args.data_files,
        args.initial_snapshot,
        args.tick_size,
        args.lot_size,
        args.maker_fee,
        args.taker_fee,
    );
    let mut recorder = BacktestRecorder::new(&hbt);
    gridtrading(
        &mut hbt,
        &mut recorder,
        args.relative_half_spread,
        args.relative_grid_interval,
        args.grid_num,
        args.min_grid_step.unwrap_or(args.tick_size as f64),
        args.skew,
        args.order_qty,
        args.max_position,
    )
    .unwrap();
    hbt.close().unwrap();
    recorder.to_csv(args.name, args.output_path).unwrap();
}
