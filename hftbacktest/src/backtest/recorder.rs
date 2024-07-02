use std::{
    fs::File,
    io::{Error, Write},
    path::Path,
};

use npyz::{npz::NpzWriter, AutoSerialize, WriterBuilder};
use tokio::io::AsyncWriteExt;
use zip_old::{write::FileOptions, CompressionMethod};

use crate::{
    depth::MarketDepth,
    types::{Bot, BotTypedDepth, Recorder},
};

#[derive(AutoSerialize, npyz::Serialize)]
struct Record {
    timestamp: i64,
    mid_price: f32,
    balance: f64,
    position: f64,
    fee: f64,
    trade_num: i32,
    trade_amount: f64,
    trade_qty: f64,
}

/// Provides recording of the backtesting strategy's state values, which are needed to compute
/// performance metrics.
pub struct BacktestRecorder {
    values: Vec<Vec<Record>>,
}

impl Recorder for BacktestRecorder {
    type Error = Error;

    fn record<MD, I>(&mut self, hbt: &mut I) -> Result<(), Self::Error>
    where
        I: Bot + BotTypedDepth<MD>,
        MD: MarketDepth,
    {
        let timestamp = hbt.current_timestamp();
        for asset_no in 0..hbt.num_assets() {
            let depth = hbt.depth_typed(asset_no);
            let mid_price = (depth.best_bid() + depth.best_ask()) / 2.0;
            let state_values = hbt.state_values(asset_no);
            let values = unsafe { self.values.get_unchecked_mut(asset_no) };
            values.push(
                (Record {
                    timestamp,
                    mid_price,
                    balance: state_values.balance,
                    position: state_values.position,
                    fee: state_values.fee,
                    trade_num: state_values.trade_num,
                    trade_amount: state_values.trade_amount,
                    trade_qty: state_values.trade_qty,
                }),
            );
        }
        Ok(())
    }
}

impl BacktestRecorder {
    /// Constructs an instance of `BacktestRecorder`.
    pub fn new<I>(hbt: &I) -> Self
    where
        I: Bot,
    {
        Self {
            values: {
                let mut vec = Vec::with_capacity(hbt.num_assets());
                for _ in 0..hbt.num_assets() {
                    vec.push(Vec::new());
                }
                vec
            },
        }
    }

    /// Saves record data into a CSV file at the specified path. It creates a separate CSV file for
    /// each asset, with the filename `{prefix}_{asset_no}.csv`.
    /// The columns are `timestamp`, `mid`, `balance`, `position`, `fee`, `trade_num`,
    /// `trade_amount`, `trade_qty`.
    pub fn to_csv<Prefix, P>(&self, prefix: Prefix, path: P) -> Result<(), Error>
    where
        Prefix: AsRef<str>,
        P: AsRef<Path>,
    {
        let prefix = prefix.as_ref();
        for (asset_no, values) in self.values.iter().enumerate() {
            let file_path = path.as_ref().join(format!("{prefix}{asset_no}.csv"));
            let mut file = File::create(file_path)?;
            write!(
                file,
                "timestamp,mid_price,balance,position,fee,trade_num,trade_amount,trade_qty\n",
            )?;
            for Record {
                timestamp,
                mid_price,
                balance,
                position,
                fee,
                trade_num,
                trade_amount,
                trade_qty,
            } in values
            {
                write!(
                    file,
                    "{},{},{},{},{},{},{},{}\n",
                    timestamp,
                    mid_price,
                    balance,
                    position,
                    fee,
                    trade_num,
                    trade_amount,
                    trade_qty
                )?;
            }
        }
        Ok(())
    }

    pub fn to_npz<Prefix, P>(&self, prefix: Prefix, path: P) -> Result<(), Error>
    where
        Prefix: AsRef<str>,
        P: AsRef<Path>,
    {
        let prefix = prefix.as_ref();
        let file_path = path.as_ref().join(format!("{prefix}.npz"));
        let mut npz = NpzWriter::create(file_path)?;
        let options = FileOptions::default()
            .compression_method(CompressionMethod::DEFLATE)
            .compression_level(Some(9));
        for (asset_no, values) in self.values.iter().enumerate() {
            let mut writer = npz
                .array(&format!("{asset_no}"), options)?
                .default_dtype()
                .shape(&[values.len() as u64])
                .begin_nd()?;
            writer.extend(values)?;
            writer.finish()?;
        }
        Ok(())
    }
}
