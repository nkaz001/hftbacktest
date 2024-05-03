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

use crate::{backtest::reader::Data, types::BuildError};

/// Latency and queue position models
pub mod models;

/// OrderBus implementation
pub mod order;

/// Local and exchange models
pub mod proc;

/// The data reader
pub mod reader;

pub mod state;

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

pub struct Asset<L: ?Sized, E: ?Sized> {
    local: Box<L>,
    exch: Box<E>,
}

impl<L, E> Asset<L, E> {
    pub fn builder<Q, LM, AT, QM, MD>() -> AssetBuilder<Q, LM, AT, QM, MD> {
        let cache = Cache::new();
        let reader = Reader::new(cache);

        AssetBuilder {
            latency_model: None,
            asset_type: None,
            queue_model: None,
            depth_builder: None,
            reader,
            _q_marker: Default::default(),
        }
    }
}

pub struct AssetBuilder<Q, LM, AT, QM, MD> {
    latency_model: Option<LM>,
    asset_type: Option<AT>,
    queue_model: Option<QM>,
    depth_builder: Option<Box<dyn Fn() -> MD>>,
    reader: Reader<Event>,
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
    pub fn new() -> Self {
        let cache = Cache::new();
        let reader = Reader::new(cache);

        Self {
            latency_model: None,
            asset_type: None,
            queue_model: None,
            depth_builder: None,
            reader,
            _q_marker: Default::default(),
        }
    }

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

    pub fn latency_model(self, latency_model: LM) -> Self {
        Self {
            latency_model: Some(latency_model),
            ..self
        }
    }

    pub fn asset_type(self, asset_type: AT) -> Self {
        Self {
            asset_type: Some(asset_type),
            ..self
        }
    }

    pub fn queue_model(self, queue_model: QM) -> Self {
        Self {
            queue_model: Some(queue_model),
            ..self
        }
    }

    pub fn depth<Builder>(self, builder: Builder) -> Self
    where
        Builder: Fn() -> MD + 'static,
    {
        Self {
            depth_builder: Some(Box::new(builder)),
            ..self
        }
    }

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
            State::new(asset_type),
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
            State::new(asset_type),
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
            State::new(asset_type),
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
            State::new(asset_type),
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
