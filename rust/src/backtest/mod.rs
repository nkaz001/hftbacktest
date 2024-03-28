pub mod assettype;
pub mod backtest;
pub mod models;
pub mod order;
pub mod proc;
pub mod reader;
pub mod state;

mod evs;

use std::{io::Error as IoError, marker::PhantomData};

use thiserror::Error;

use crate::{
    backtest::{
        assettype::AssetType,
        backtest::MultiAssetMultiExchangeBacktest,
        models::{LatencyModel, QueueModel},
        order::OrderBus,
        proc::{Local, LocalProcessor, NoPartialFillExchange, Processor},
        reader::{Cache, Reader},
        state::State,
    },
    depth::hashmapmarketdepth::HashMapMarketDepth,
    ty::Event,
    BuildError,
};

#[derive(Error, Debug)]
pub enum Error {
    #[error("Order related to a given order id already exists")]
    OrderAlreadyExist,
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

pub enum DataSource {
    File(String),
    Array,
}

pub struct BtAsset<Q> {
    local: Box<dyn LocalProcessor<Q, HashMapMarketDepth>>,
    exch: Box<dyn Processor>,
}

pub struct BtAssetBuilder<Q, LM, AT, QM, F>
where
    F: Fn() -> HashMapMarketDepth,
{
    latency_model: Option<LM>,
    asset_type: Option<AT>,
    queue_model: Option<QM>,
    depth_func: Option<F>,
    reader: Reader<Event>,
    _q_marker: PhantomData<Q>,
}

impl<Q, LM, AT, QM, F> BtAssetBuilder<Q, LM, AT, QM, F>
where
    F: Fn() -> HashMapMarketDepth,
    AT: AssetType + Clone + 'static,
    Local<AT, Q, LM, HashMapMarketDepth>: LocalProcessor<Q, HashMapMarketDepth>,
    Q: Clone + Default + 'static,
    QM: QueueModel<Q> + 'static,
    LM: LatencyModel + Clone + 'static,
{
    pub fn new() -> Self {
        let cache = Cache::new();
        let reader = Reader::new(cache);

        Self {
            latency_model: None,
            asset_type: None,
            queue_model: None,
            depth_func: None,
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
                DataSource::Array => {
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

    pub fn depth(self, depth_func: F) -> Self {
        Self {
            depth_func: Some(depth_func),
            ..self
        }
    }

    pub fn build(self) -> Result<BtAsset<Q>, BuildError> {
        let ob_local_to_exch = OrderBus::new();
        let ob_exch_to_local = OrderBus::new();

        let create_depth = self
            .depth_func
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

        Ok(BtAsset {
            local: Box::new(local),
            exch: Box::new(exch),
        })
    }
}

pub struct BtBuilder<Q> {
    local: Vec<Box<dyn LocalProcessor<Q, HashMapMarketDepth>>>,
    exch: Vec<Box<dyn Processor>>,
}

impl<Q> BtBuilder<Q>
where
    Q: Clone,
{
    pub fn new() -> Self {
        Self {
            local: vec![],
            exch: vec![],
        }
    }

    pub fn add(self, asset: BtAsset<Q>) -> Self {
        let mut s = Self { ..self };
        s.local.push(asset.local);
        s.exch.push(asset.exch);
        s
    }

    pub fn build(
        self,
    ) -> Result<MultiAssetMultiExchangeBacktest<Q, HashMapMarketDepth>, BuildError> {
        Ok(MultiAssetMultiExchangeBacktest::new(self.local, self.exch))
    }
}
