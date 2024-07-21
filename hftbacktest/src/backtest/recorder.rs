use std::{
    fs::File,
    io::{Error, Write},
    path::Path,
};

use npyz::{npz::NpzWriter, AutoSerialize, WriterBuilder};
use zip_old::{write::FileOptions, CompressionMethod};

use crate::{
    depth::MarketDepth,
    types::{Bot, Recorder},
};

#[derive(AutoSerialize, npyz::Serialize)]
struct Record {
    timestamp: i64,
    price: f64,
    balance: f64,
    position: f64,
    fee: f64,
    trading_volume: f64,
    trading_value: f64,
    num_trades: i64,
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
        MD: MarketDepth,
        I: Bot<MD>,
    {
        let timestamp = hbt.current_timestamp();
        for asset_no in 0..hbt.num_assets() {
            let depth = hbt.depth(asset_no);
            let mid_price = (depth.best_bid() + depth.best_ask()) / 2.0;
            let state_values = hbt.state_values(asset_no);
            let values = unsafe { self.values.get_unchecked_mut(asset_no) };
            values.push(Record {
                timestamp,
                price: mid_price,
                balance: state_values.balance,
                position: state_values.position,
                fee: state_values.fee,
                trading_volume: state_values.trading_volume,
                trading_value: state_values.trading_value,
                num_trades: state_values.num_trades,
            });
        }
        Ok(())
    }
}

impl BacktestRecorder {
    /// Constructs an instance of `BacktestRecorder`.
    pub fn new<I, MD>(hbt: &I) -> Self
    where
        MD: MarketDepth,
        I: Bot<MD>,
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
                "timestamp,balance,position,fee,trading_volume,trading_value,num_trades,price\n",
            )?;
            for Record {
                timestamp,
                balance,
                position,
                fee,
                trading_volume,
                trading_value,
                num_trades,
                price: mid_price,
            } in values
            {
                write!(
                    file,
                    "{},{},{},{},{},{},{},{}\n",
                    timestamp,
                    balance,
                    position,
                    fee,
                    trading_volume,
                    trading_value,
                    num_trades,
                    mid_price,
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
