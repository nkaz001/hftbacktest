use std::{
    fs::File,
    io::{Error, Write},
    path::Path,
};

use crate::{
    depth::MarketDepth,
    types::{Interface, Recorder},
};

/// Provides recording of the backtesting strategy's state values, which are needed to compute
/// performance metrics.
pub struct BacktestRecorder {
    values: Vec<Vec<(i64, f32, f64, f64, f64, i32, f64, f64)>>,
}

impl Recorder for BacktestRecorder {
    type Error = Error;

    fn record<MD, I>(&mut self, hbt: &mut I) -> Result<(), Self::Error>
    where
        I: Interface<MD>,
        MD: MarketDepth,
    {
        let timestamp = hbt.current_timestamp();
        for asset_no in 0..hbt.num_assets() {
            let depth = hbt.depth(asset_no);
            let mid_price = (depth.best_bid() + depth.best_ask()) / 2.0;
            let state_values = hbt.state_values(asset_no);
            let values = unsafe { self.values.get_unchecked_mut(asset_no) };
            values.push((
                timestamp,
                mid_price,
                state_values.balance,
                state_values.position,
                state_values.fee,
                state_values.trade_num,
                state_values.trade_amount,
                state_values.trade_qty,
            ));
        }
        Ok(())
    }
}

impl BacktestRecorder {
    /// Constructs an instance of `BacktestRecorder`.
    pub fn new<MD, I>(hbt: &I) -> Self
    where
        I: Interface<MD>,
        MD: MarketDepth,
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
            for (
                timestamp,
                mid_price,
                balance,
                position,
                fee,
                trade_num,
                trade_amount,
                trade_qty,
            ) in values
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
}
