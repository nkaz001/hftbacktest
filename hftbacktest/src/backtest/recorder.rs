use std::{
    fs::{File, create_dir_all},
    io::{BufWriter, Error, Write},
    path::Path,
    sync::Arc,
};

use arrow::{
    array::{ArrayRef, PrimitiveBuilder, RecordBatch},
    datatypes::{DataType, Float64Type, Int64Type, Schema},
    error::ArrowError,
};
use hftbacktest_derive::NpyDTyped;
use once_cell::sync::Lazy;
use parquet::{arrow::ArrowWriter, basic::Compression, file::properties::WriterProperties};
use zip::{ZipWriter, write::SimpleFileOptions};

use crate::{
    backtest::data::{POD, write_npy},
    depth::MarketDepth,
    types::{Bot, Recorder},
};

#[repr(C)]
#[derive(NpyDTyped)]
struct Record {
    timestamp: i64,
    price: f64,
    position: f64,
    balance: f64,
    fee: f64,
    num_trades: i64,
    num_messages: i64,
    num_cancellations: i64,
    num_creations: i64,
    num_modifications: i64,
    trading_volume: f64,
    trading_value: f64,
}

unsafe impl POD for Record {}

/// Provides recording of the backtesting strategy's state values, which are needed to compute
/// performance metrics.
pub struct BacktestRecorder {
    values: Vec<Vec<Record>>,
}

pub static ACCOUNT_STATE_DATA_POINT_FIELDS: Lazy<Vec<arrow::datatypes::Field>> = Lazy::new(|| {
    vec![
        arrow::datatypes::Field::new("timestamp", DataType::Int64, true),
        arrow::datatypes::Field::new("balance", DataType::Float64, true),
        arrow::datatypes::Field::new("position", DataType::Float64, true),
        arrow::datatypes::Field::new("fee", DataType::Float64, true),
        arrow::datatypes::Field::new("trading_volume", DataType::Float64, true),
        arrow::datatypes::Field::new("trading_value", DataType::Float64, true),
        arrow::datatypes::Field::new("num_trades", DataType::Int64, true),
        arrow::datatypes::Field::new("num_messages", DataType::Int64, true),
        arrow::datatypes::Field::new("num_cancellations", DataType::Int64, true),
        arrow::datatypes::Field::new("num_creations", DataType::Int64, true),
        arrow::datatypes::Field::new("num_modifications", DataType::Int64, true),
        arrow::datatypes::Field::new("price", DataType::Float64, true),
    ]
});

pub trait ColumnsBuilder<'a> {
    type T;

    fn get_batch(&mut self) -> Result<RecordBatch, ArrowError>;
    fn append(&mut self, msg: &'a Self::T) -> Result<(), ArrowError>;

    fn reset(&mut self) -> Result<(), ArrowError>;
}

pub struct AccountStateDataPointColumnsBuilder {
    schema: Schema,
    timestamp_builder: PrimitiveBuilder<Int64Type>,
    balance_builder: PrimitiveBuilder<Float64Type>,
    position_builder: PrimitiveBuilder<Float64Type>,
    fee_builder: PrimitiveBuilder<Float64Type>,
    trading_volume_builder: PrimitiveBuilder<Float64Type>,
    trading_value_builder: PrimitiveBuilder<Float64Type>,
    num_trades_builder: PrimitiveBuilder<Int64Type>,
    num_messages_builder: PrimitiveBuilder<Int64Type>,
    num_cancellations_builder: PrimitiveBuilder<Int64Type>,
    num_creations_builder: PrimitiveBuilder<Int64Type>,
    num_modifications_builder: PrimitiveBuilder<Int64Type>,
    price_builder: PrimitiveBuilder<Float64Type>,
}

pub struct AccountStateDataPoint {
    pub timestamp: i64,
    pub balance: f64,
    pub position: f64,
    pub fee: f64,
    pub trading_volume: f64,
    pub trading_value: f64,
    pub num_trades: i64,
    pub num_messages: i64,
    pub num_cancellations: i64,
    pub num_creations: i64,
    pub num_modifications: i64,
    pub price: f64,
}

impl<'a> ColumnsBuilder<'a> for AccountStateDataPointColumnsBuilder {
    type T = AccountStateDataPoint;

    fn get_batch(&mut self) -> Result<RecordBatch, ArrowError> {
        let arrays: Vec<ArrayRef> = vec![
            Arc::new(self.timestamp_builder.finish()),
            Arc::new(self.balance_builder.finish()),
            Arc::new(self.position_builder.finish()),
            Arc::new(self.fee_builder.finish()),
            Arc::new(self.trading_volume_builder.finish()),
            Arc::new(self.trading_value_builder.finish()),
            Arc::new(self.num_trades_builder.finish()),
            Arc::new(self.num_messages_builder.finish()),
            Arc::new(self.num_cancellations_builder.finish()),
            Arc::new(self.num_creations_builder.finish()),
            Arc::new(self.num_modifications_builder.finish()),
            Arc::new(self.price_builder.finish()),
        ];
        let batch = RecordBatch::try_new(Arc::new(self.schema.clone()), arrays)?;
        Ok(batch)
    }

    fn append(&mut self, msg: &AccountStateDataPoint) -> Result<(), ArrowError> {
        self.timestamp_builder.append_value(msg.timestamp);
        self.balance_builder.append_value(msg.balance);
        self.position_builder.append_value(msg.position);
        self.fee_builder.append_value(msg.fee);
        self.trading_volume_builder.append_value(msg.trading_volume);
        self.trading_value_builder.append_value(msg.trading_value);
        self.num_trades_builder.append_value(msg.num_trades);
        self.num_messages_builder.append_value(msg.num_messages);
        self.num_cancellations_builder
            .append_value(msg.num_cancellations);
        self.num_creations_builder.append_value(msg.num_creations);
        self.num_modifications_builder
            .append_value(msg.num_modifications);
        self.price_builder.append_value(msg.price);
        return Ok(());
    }

    fn reset(&mut self) -> Result<(), ArrowError> {
        self.timestamp_builder = Default::default();
        self.balance_builder = Default::default();
        self.position_builder = Default::default();
        self.fee_builder = Default::default();
        self.trading_volume_builder = Default::default();
        self.trading_value_builder = Default::default();
        self.num_trades_builder = Default::default();
        self.num_messages_builder = Default::default();
        self.num_cancellations_builder = Default::default();
        self.num_creations_builder = Default::default();
        self.num_modifications_builder = Default::default();
        self.price_builder = Default::default();
        return Ok(());
    }
}

impl AccountStateDataPointColumnsBuilder {
    pub fn new(schema: Schema) -> AccountStateDataPointColumnsBuilder {
        AccountStateDataPointColumnsBuilder {
            schema,
            timestamp_builder: Default::default(),
            balance_builder: Default::default(),
            position_builder: Default::default(),
            fee_builder: Default::default(),
            trading_volume_builder: Default::default(),
            trading_value_builder: Default::default(),
            num_trades_builder: Default::default(),
            num_messages_builder: Default::default(),
            num_cancellations_builder: Default::default(),
            num_creations_builder: Default::default(),
            num_modifications_builder: Default::default(),
            price_builder: Default::default(),
        }
    }
}

impl Recorder for BacktestRecorder {
    type Error = Error;

    fn record<MD, I>(&mut self, hbt: &I) -> Result<(), Self::Error>
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
                num_messages: state_values.num_messages,
                num_cancellations: state_values.num_cancellations,
                num_creations: state_values.num_creations,
                num_modifications: state_values.num_modifications,
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
        let base_path = path.as_ref();
        create_dir_all(base_path)?;

        // Buffer output to reduce frequent file I/O
        for (asset_no, values) in self.values.iter().enumerate() {
            let file_path = base_path.join(format!("{prefix}{asset_no}.csv"));
            let mut file = BufWriter::new(File::create(file_path)?); // Use BufWriter for buffered writing

            // Write header
            file.write_all(
                b"timestamp,balance,position,fee,trading_volume,trading_value,num_trades,num_messages,num_cancellations,num_creations,num_modifications,price\n",
            )?;

            // Write records
            for record in values {
                let line = format!(
                    "{},{},{},{},{},{},{},{},{},{},{},{}\n",
                    record.timestamp,
                    record.balance,
                    record.position,
                    record.fee,
                    record.trading_volume,
                    record.trading_value,
                    record.num_trades,
                    record.num_messages,
                    record.num_cancellations,
                    record.num_creations,
                    record.num_modifications,
                    record.price,
                );
                file.write_all(line.as_bytes())?;
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
        let base_path = path.as_ref();
        create_dir_all(base_path)?;

        let file_path = base_path.join(format!("{prefix}.npz"));
        let file = File::create(file_path)?;

        let mut zip = ZipWriter::new(file);

        let options = SimpleFileOptions::default()
            .compression_method(zip::CompressionMethod::DEFLATE)
            .compression_level(Some(9));

        for (asset_no, values) in self.values.iter().enumerate() {
            zip.start_file(format!("{asset_no}.npy"), options)?;
            write_npy(&mut zip, values)?;
        }

        zip.finish()?;
        Ok(())
    }

    pub fn to_parquet<Prefix, P>(&self, prefix: Prefix, path: P) -> Result<(), Error>
    where
        Prefix: AsRef<str>,
        P: AsRef<Path>,
    {
        let prefix = prefix.as_ref();
        let base_path = path.as_ref();
        create_dir_all(base_path)?;

        // Buffer output to reduce frequent file I/O
        for (asset_no, values) in self.values.iter().enumerate() {
            let parquet_schema = Schema::new(ACCOUNT_STATE_DATA_POINT_FIELDS.clone());
            let arrow_schema = Arc::new(parquet_schema.clone());
            let parquet_props = WriterProperties::builder()
                .set_compression(Compression::SNAPPY)
                .build();

            let file_path = base_path.join(format!("{prefix}{asset_no}.snappy.parquet"));
            let file = File::create(file_path).unwrap();

            let mut wrt =
                ArrowWriter::try_new(file, arrow_schema.clone(), Some(parquet_props)).unwrap();

            let mut builder = AccountStateDataPointColumnsBuilder::new(parquet_schema.clone());

            let max_rows_per_batch: usize = 10;
            let mut row: usize = 0;

            // Write records
            for record in values {
                row += 1;
                let single_row = AccountStateDataPoint {
                    timestamp: record.timestamp,
                    balance: record.balance,
                    position: record.position,
                    fee: record.fee,
                    trading_volume: record.trading_volume,
                    trading_value: record.trading_value,
                    num_trades: record.num_trades,
                    num_messages: record.num_messages,
                    num_cancellations: record.num_cancellations,
                    num_creations: record.num_creations,
                    num_modifications: record.num_modifications,
                    price: record.price,
                };
                builder.append(&single_row).unwrap();
                row += 1;

                if row > 0 && row % max_rows_per_batch == 0 {
                    let batch = builder.get_batch().unwrap();
                    wrt.write(&batch).unwrap();
                    builder.reset().unwrap();
                }
            }

            // Write remaining data
            {
                let batch = builder.get_batch().unwrap();
                wrt.write(&batch).unwrap();
                builder.reset().unwrap();
            }

            wrt.close().unwrap();
        }
        Ok(())
    }
}
