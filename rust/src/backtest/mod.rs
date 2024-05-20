use std::{io::Error as IoError, marker::PhantomData};

use thiserror::Error;

use crate::{
    backtest::{
        assettype::AssetType,
        models::{LatencyModel, QueueModel},
        order::OrderBus,
        proc::{Local, LocalProcessor, NoPartialFillExchange, Processor},
        reader::{Cache, Reader},
        state::State,
    },
    depth::MarketDepth,
    types::Event,
};

/// Provides asset types.
pub mod assettype;

mod backtest;
pub use backtest::*;

use crate::{
    backtest::{proc::PartialFillExchange, reader::Data},
    types::BuildError,
};

/// Latency and queue position models
pub mod models;

/// OrderBus implementation
pub mod order;

/// Local and exchange models
pub mod proc;

/// The data reader
pub mod reader;

pub mod state;

pub mod recorder;

mod evs;

#[derive(Error, Debug)]
pub enum BacktestError {
    #[error("Order related to a given order id already exists")]
    OrderIdExist,
    #[error("Order request is in process")]
    OrderRequestInProcess,
    #[error("Order not found")]
    OrderNotFound,
    #[error("order request is invalid")]
    InvalidOrderRequest,
    #[error("order status is invalid to proceed the request")]
    InvalidOrderStatus,
    #[error("end of data")]
    EndOfData,
    #[error("data error: {0:?}")]
    DataError(#[from] IoError),
}

#[derive(Clone, Debug)]
pub enum DataSource {
    File(String),
    Data(Data<Event>),
}

/// Backtesting Asset
pub struct Asset<L: ?Sized, E: ?Sized> {
    local: Box<L>,
    exch: Box<E>,
}

impl<L, E> Asset<L, E> {
    /// Constructs an instance of `Asset`. Use this method if a custom local processor or an
    /// exchange processor is needed.
    pub fn new(local: L, exch: E) -> Self {
        Self {
            local: Box::new(local),
            exch: Box::new(exch),
        }
    }

    /// Returns a builder for `Asset`.
    pub fn builder<Q, LM, AT, QM, MD>() -> AssetBuilder<Q, LM, AT, QM, MD>
    where
        AT: AssetType + Clone + 'static,
        MD: MarketDepth + 'static,
        Q: Clone + Default + 'static,
        QM: QueueModel<Q, MD> + 'static,
        LM: LatencyModel + Clone + 'static,
    {
        AssetBuilder::new()
    }
}

/// Exchange model kind.
pub enum ExchangeKind {
    NoPartialFillExchange,
    PartialFillExchange,
}

/// A builder for `Asset`.
pub struct AssetBuilder<Q, LM, AT, QM, MD> {
    latency_model: Option<LM>,
    asset_type: Option<AT>,
    queue_model: Option<QM>,
    depth_builder: Option<Box<dyn Fn() -> MD>>,
    reader: Reader<Event>,
    maker_fee: f64,
    taker_fee: f64,
    exch_kind: ExchangeKind,
    trade_len: usize,
    _q_marker: PhantomData<Q>,
}

impl<Q, LM, AT, QM, MD> AssetBuilder<Q, LM, AT, QM, MD>
where
    AT: AssetType + Clone + 'static,
    MD: MarketDepth + 'static,
    Q: Clone + Default + 'static,
    QM: QueueModel<Q, MD> + 'static,
    LM: LatencyModel + Clone + 'static,
{
    /// Constructs an instance of `AssetBuilder`.
    pub fn new() -> Self {
        let cache = Cache::new();
        let reader = Reader::new(cache);

        Self {
            latency_model: None,
            asset_type: None,
            queue_model: None,
            depth_builder: None,
            reader,
            maker_fee: 0.0,
            taker_fee: 0.0,
            exch_kind: ExchangeKind::NoPartialFillExchange,
            trade_len: 1000,
            _q_marker: Default::default(),
        }
    }

    /// Sets the feed data. Currently, only `DataSource::File` is supported.
    pub fn data(mut self, data: Vec<DataSource>) -> Self {
        for item in data {
            match item {
                DataSource::File(filename) => {
                    self.reader.add_file(filename);
                }
                DataSource::Data(data) => {
                    todo!();
                }
            }
        }
        self
    }

    /// Sets a latency model.
    pub fn latency_model(self, latency_model: LM) -> Self {
        Self {
            latency_model: Some(latency_model),
            ..self
        }
    }

    /// Sets an asset type.
    pub fn asset_type(self, asset_type: AT) -> Self {
        Self {
            asset_type: Some(asset_type),
            ..self
        }
    }

    /// Sets the maker fee.
    pub fn maker_fee(self, maker_fee: f64) -> Self {
        Self { maker_fee, ..self }
    }

    /// Sets the taker fee.
    pub fn taker_fee(self, taker_fee: f64) -> Self {
        Self { taker_fee, ..self }
    }

    /// Sets a queue model.
    pub fn queue_model(self, queue_model: QM) -> Self {
        Self {
            queue_model: Some(queue_model),
            ..self
        }
    }

    /// Sets a market depth builder.
    pub fn depth<Builder>(self, builder: Builder) -> Self
    where
        Builder: Fn() -> MD + 'static,
    {
        Self {
            depth_builder: Some(Box::new(builder)),
            ..self
        }
    }

    /// Sets an exchange model. The default value is [`NoPartialFillExchange`].
    pub fn exchange(self, exch_kind: ExchangeKind) -> Self {
        Self { exch_kind, ..self }
    }

    /// Sets the length of market trades to be stored in the local processor. The default value is
    /// `1000`.
    pub fn trade_len(self, trade_len: usize) -> Self {
        Self { trade_len, ..self }
    }

    /// Builds an `Asset`.
    pub fn build(self) -> Result<Asset<dyn LocalProcessor<Q, MD>, dyn Processor>, BuildError> {
        let ob_local_to_exch = OrderBus::new();
        let ob_exch_to_local = OrderBus::new();

        let create_depth = self
            .depth_builder
            .as_ref()
            .ok_or(BuildError::BuilderIncomplete("depth"))?;
        let order_latency = self
            .latency_model
            .clone()
            .ok_or(BuildError::BuilderIncomplete("order_latency"))?;
        let asset_type = self
            .asset_type
            .clone()
            .ok_or(BuildError::BuilderIncomplete("asset_type"))?;

        let local = Local::new(
            self.reader.clone(),
            create_depth(),
            State::new(asset_type, self.maker_fee, self.taker_fee),
            order_latency,
            self.trade_len,
            ob_local_to_exch.clone(),
            ob_exch_to_local.clone(),
        );

        let order_latency = self
            .latency_model
            .clone()
            .ok_or(BuildError::BuilderIncomplete("order_latency"))?;
        let queue_model = self
            .queue_model
            .ok_or(BuildError::BuilderIncomplete("queue_model"))?;
        let asset_type = self
            .asset_type
            .clone()
            .ok_or(BuildError::BuilderIncomplete("asset_type"))?;

        match self.exch_kind {
            ExchangeKind::NoPartialFillExchange => {
                let exch = NoPartialFillExchange::new(
                    self.reader.clone(),
                    create_depth(),
                    State::new(asset_type, self.maker_fee, self.taker_fee),
                    order_latency,
                    queue_model,
                    ob_exch_to_local,
                    ob_local_to_exch,
                );

                Ok(Asset {
                    local: Box::new(local),
                    exch: Box::new(exch),
                })
            }
            ExchangeKind::PartialFillExchange => {
                let exch = PartialFillExchange::new(
                    self.reader.clone(),
                    create_depth(),
                    State::new(asset_type, self.maker_fee, self.taker_fee),
                    order_latency,
                    queue_model,
                    ob_exch_to_local,
                    ob_local_to_exch,
                );

                Ok(Asset {
                    local: Box::new(local),
                    exch: Box::new(exch),
                })
            }
        }
    }

    pub fn build_wip(
        self,
    ) -> Result<Asset<Local<AT, Q, LM, MD>, NoPartialFillExchange<AT, Q, LM, QM, MD>>, BuildError>
    {
        let ob_local_to_exch = OrderBus::new();
        let ob_exch_to_local = OrderBus::new();

        let create_depth = self
            .depth_builder
            .as_ref()
            .ok_or(BuildError::BuilderIncomplete("depth"))?;
        let order_latency = self
            .latency_model
            .clone()
            .ok_or(BuildError::BuilderIncomplete("order_latency"))?;
        let asset_type = self
            .asset_type
            .clone()
            .ok_or(BuildError::BuilderIncomplete("asset_type"))?;

        let local = Local::new(
            self.reader.clone(),
            create_depth(),
            State::new(asset_type, self.maker_fee, self.taker_fee),
            order_latency,
            1000,
            ob_local_to_exch.clone(),
            ob_exch_to_local.clone(),
        );

        let order_latency = self
            .latency_model
            .clone()
            .ok_or(BuildError::BuilderIncomplete("order_latency"))?;
        let queue_model = self
            .queue_model
            .ok_or(BuildError::BuilderIncomplete("queue_model"))?;
        let asset_type = self
            .asset_type
            .clone()
            .ok_or(BuildError::BuilderIncomplete("asset_type"))?;
        let exch = NoPartialFillExchange::new(
            self.reader.clone(),
            create_depth(),
            State::new(asset_type, self.maker_fee, self.taker_fee),
            order_latency,
            queue_model,
            ob_exch_to_local,
            ob_local_to_exch,
        );

        Ok(Asset {
            local: Box::new(local),
            exch: Box::new(exch),
        })
    }
}
