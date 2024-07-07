use std::ffi::c_void;

pub use backtest::*;
pub use depth::*;
use hftbacktest::{
    backtest::{
        assettype::{InverseAsset, LinearAsset},
        models::{
            ConstantLatency,
            IntpOrderLatency,
            OrderLatencyRow,
            PowerProbQueueFunc3,
            ProbQueueModel,
        },
        order::OrderBus,
        proc::{Local, LocalProcessor, NoPartialFillExchange, PartialFillExchange, Processor},
        reader::{Cache, Reader},
        state::State,
        DataSource,
        MultiAssetMultiExchangeBacktest,
    },
    prelude::{Event, HashMapMarketDepth},
};
pub use order::*;
use pyo3::prelude::*;

mod backtest;
mod depth;
mod order;

#[derive(Clone)]
pub enum AssetType {
    LinearAsset(f64),
    InverseAsset(f64),
}

#[derive(Clone)]
pub enum LatencyModel {
    ConstantLatency {
        entry_latency: i64,
        resp_latency: i64,
    },
    IntpOrderLatency {
        data: Vec<DataSource<OrderLatencyRow>>,
    },
}

#[derive(Clone)]
pub enum QueueModel {
    RiskAdverseQueueModel,
    PowerProbQueueModel(f32),
    LogProbQueueModel,
    LogProbQueueModel2,
    PowerProbQueueModel2(f32),
    PowerProbQueueModel3(f32),
}

#[derive(Clone)]
pub enum ExchangeKind {
    NoPartialFillExchange,
    PartialFillExchange,
}

/// Builds a backtesting asset.
#[pyclass]
pub struct AssetBuilder {
    data: Vec<String>,
    asset_type: AssetType,
    latency_model: LatencyModel,
    queue_model: QueueModel,
    exch_kind: ExchangeKind,
    tick_size: f32,
    lot_size: f32,
    maker_fee: f64,
    taker_fee: f64,
    trade_len: usize,
}

unsafe impl Send for AssetBuilder {}

#[pymethods]
impl AssetBuilder {
    /// Constructs an instance of `AssetBuilder`.
    #[new]
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            latency_model: LatencyModel::ConstantLatency {
                entry_latency: 0,
                resp_latency: 0,
            },
            asset_type: AssetType::LinearAsset(1.0),
            queue_model: QueueModel::LogProbQueueModel2,
            tick_size: 0.0,
            lot_size: 0.0,
            maker_fee: 0.0,
            taker_fee: 0.0,
            exch_kind: ExchangeKind::NoPartialFillExchange,
            trade_len: 0,
        }
    }

    /// Sets the feed data.
    ///
    /// Args:
    ///     data: a list of file paths for the normalized market feed data in `npz`.
    pub fn data(&mut self, data: Vec<String>) {
        for item in data {
            self.data.push(item);
        }
    }

    /// Sets the asset as a `LinearAsset`.
    ///
    /// Args:
    ///     contract_size: contract size of the asset.
    pub fn linear_asset(&mut self, contract_size: f64) {
        self.asset_type = AssetType::LinearAsset(contract_size);
    }

    /// Sets the asset as a `InverseAsset`.
    ///
    /// Args:
    ///     contract_size: contract size of the asset.
    pub fn inverse_asset(&mut self, contract_size: f64) {
        self.asset_type = AssetType::InverseAsset(contract_size);
    }

    /// Uses `ConstantLatency` for the order latency model.
    /// The units of the arguments should match the timestamp units of your data. Nanoseconds are
    /// typically used in HftBacktest.
    ///
    /// Args:
    ///     entry_latency: order entry latency.
    ///     resp_latency: order response latency.
    pub fn constant_latency(&mut self, entry_latency: i64, resp_latency: i64) {
        self.latency_model = LatencyModel::ConstantLatency {
            entry_latency,
            resp_latency,
        };
    }

    /// Uses `IntpOrderLatency` for the order latency model.
    /// Please see the data format.
    /// The units of the historical latencies should match the timestamp units of your data.
    /// Nanoseconds are typically used in HftBacktest.
    ///
    /// Args:
    ///     data: a list of file paths for the historical order latency data in `npz`.
    pub fn intp_order_latency(&mut self, data: Vec<String>) {
        self.latency_model = LatencyModel::IntpOrderLatency {
            data: data
                .iter()
                .map(|file| DataSource::File(file.to_string()))
                .collect(),
        };
    }

    pub fn risk_adverse_queue_model(&mut self) {
        self.queue_model = QueueModel::RiskAdverseQueueModel;
    }

    pub fn log_prob_queue_model(&mut self) {
        self.queue_model = QueueModel::LogProbQueueModel;
    }

    pub fn log_prob_queue_model2(&mut self) {
        self.queue_model = QueueModel::LogProbQueueModel2;
    }

    pub fn power_prob_queue_model(&mut self, n: f32) {
        self.queue_model = QueueModel::PowerProbQueueModel(n);
    }

    pub fn power_prob_queue_model2(&mut self, n: f32) {
        self.queue_model = QueueModel::PowerProbQueueModel2(n);
    }

    pub fn power_prob_queue_model3(&mut self, n: f32) {
        self.queue_model = QueueModel::PowerProbQueueModel3(n);
    }

    pub fn tick_size(&mut self, tick_size: f32) {
        self.tick_size = tick_size;
    }

    pub fn lot_size(&mut self, lot_size: f32) {
        self.lot_size = lot_size;
    }

    pub fn no_partial_fill_exchange(&mut self) {
        self.exch_kind = ExchangeKind::NoPartialFillExchange;
    }

    pub fn partial_fill_exchange(&mut self) {
        self.exch_kind = ExchangeKind::PartialFillExchange;
    }

    pub fn maker_fee(&mut self, maker_fee: f64) {
        self.maker_fee = maker_fee;
    }

    pub fn taker_fee(&mut self, taker_fee: f64) {
        self.taker_fee = taker_fee;
    }

    pub fn trade_len(&mut self, trade_len: usize) {
        self.trade_len = trade_len;
    }
}

#[pymodule]
#[pyo3(name = "_hftbacktest")]
fn pyhftbacktest(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(build_backtester, m)?)?;
    m.add_class::<AssetBuilder>()?;
    m.add_class::<PyMultiAssetMultiExchangeBacktest>()?;
    Ok(())
}

#[pyclass]
pub struct PyMultiAssetMultiExchangeBacktest {
    ptr: Box<MultiAssetMultiExchangeBacktest<HashMapMarketDepth>>,
}

unsafe impl Send for PyMultiAssetMultiExchangeBacktest {}

#[pymethods]
impl PyMultiAssetMultiExchangeBacktest {
    pub fn as_ptr(&mut self) -> PyResult<usize> {
        Ok(
            &mut *self.ptr as *mut MultiAssetMultiExchangeBacktest<HashMapMarketDepth>
                as *mut c_void as usize,
        )
    }
}

macro_rules! build_asset {
    (
        $data:expr ;
        $maker_fee: expr ;
        $taker_fee: expr ;
        $trade_len: expr ;
        $AT:ident: { $($AT_args:expr),* } ;
        $LM:ident: { $($LM_args:expr),* } ;
        $QM:ident: { $($QM_args:expr),* } ;
        $MD:ident: { $($MD_args:expr),* } ;
        $EM:ident
    ) => {
        {
            let cache = Cache::new();
            let mut reader = Reader::new(cache);

            for file in $data.iter() {
                reader.add_file(file.to_string());
            }

            let ob_local_to_exch = OrderBus::new();
            let ob_exch_to_local = OrderBus::new();

            let asset_type = $AT::new($($AT_args),*);
            let latency_model = $LM::new($($LM_args),*);
            let market_depth = $MD::new($($MD_args),*);

            let local: Box<dyn LocalProcessor<$MD, Event>> = Box::new(Local::new(
                reader.clone(),
                market_depth,
                State::new(asset_type.clone(), $maker_fee, $taker_fee),
                latency_model.clone(),
                $trade_len,
                ob_local_to_exch.clone(),
                ob_exch_to_local.clone(),
            ));

            let market_depth = $MD::new($($MD_args),*);
            let queue_model = $QM::new($($QM_args),*);
            let exch: Box<dyn Processor> = Box::new($EM::new(
                reader,
                market_depth,
                State::new(asset_type, $maker_fee, $taker_fee),
                latency_model,
                queue_model,
                ob_exch_to_local,
                ob_local_to_exch,
            ));

            PyAsset {
                local,
                exch
            }
        }
    }
}

pub struct PyAsset {
    local: Box<dyn LocalProcessor<HashMapMarketDepth, Event>>,
    exch: Box<dyn Processor>,
}

type PowerProbQueueModel3 = ProbQueueModel<PowerProbQueueFunc3, HashMapMarketDepth>;

#[pyfunction]
pub fn build_backtester(
    assets: Vec<PyRefMut<AssetBuilder>>,
) -> PyResult<PyMultiAssetMultiExchangeBacktest> {
    let mut local = Vec::new();
    let mut exch = Vec::new();
    for asset in assets {
        let asst = match (
            &asset.asset_type,
            &asset.latency_model,
            &asset.queue_model,
            &asset.exch_kind,
        ) {
            (
                AssetType::LinearAsset(contract_size),
                LatencyModel::ConstantLatency {
                    entry_latency,
                    resp_latency,
                },
                QueueModel::PowerProbQueueModel3(n),
                ExchangeKind::NoPartialFillExchange,
            ) => {
                build_asset! {
                    asset.data;
                    asset.maker_fee;
                    asset.taker_fee;
                    asset.trade_len;
                    LinearAsset: {*contract_size};
                    ConstantLatency: {*entry_latency, *resp_latency};
                    PowerProbQueueModel3: {PowerProbQueueFunc3::new(*n)};
                    HashMapMarketDepth: {asset.tick_size, asset.lot_size};
                    NoPartialFillExchange
                }
            }
            (
                AssetType::LinearAsset(contract_size),
                LatencyModel::IntpOrderLatency { data: latency_data },
                QueueModel::PowerProbQueueModel3(n),
                ExchangeKind::NoPartialFillExchange,
            ) => {
                build_asset! {
                    asset.data;
                    asset.maker_fee;
                    asset.taker_fee;
                    asset.trade_len;
                    LinearAsset: {*contract_size};
                    IntpOrderLatency: {latency_data.clone()};
                    PowerProbQueueModel3: {PowerProbQueueFunc3::new(*n)};
                    HashMapMarketDepth: {asset.tick_size, asset.lot_size};
                    NoPartialFillExchange
                }
            }
            (
                AssetType::InverseAsset(contract_size),
                LatencyModel::ConstantLatency {
                    entry_latency,
                    resp_latency,
                },
                QueueModel::PowerProbQueueModel3(n),
                ExchangeKind::NoPartialFillExchange,
            ) => {
                build_asset! {
                    asset.data;
                    asset.maker_fee;
                    asset.taker_fee;
                    asset.trade_len;
                    InverseAsset: {*contract_size};
                    ConstantLatency: {*entry_latency, *resp_latency};
                    PowerProbQueueModel3: {PowerProbQueueFunc3::new(*n)};
                    HashMapMarketDepth: {asset.tick_size, asset.lot_size};
                    NoPartialFillExchange
                }
            }
            (
                AssetType::InverseAsset(contract_size),
                LatencyModel::IntpOrderLatency { data: latency_data },
                QueueModel::PowerProbQueueModel3(n),
                ExchangeKind::NoPartialFillExchange,
            ) => {
                build_asset! {
                    asset.data;
                    asset.maker_fee;
                    asset.taker_fee;
                    asset.trade_len;
                    InverseAsset: {*contract_size};
                    IntpOrderLatency: {latency_data.clone()};
                    PowerProbQueueModel3: {PowerProbQueueFunc3::new(*n)};
                    HashMapMarketDepth: {asset.tick_size, asset.lot_size};
                    NoPartialFillExchange
                }
            }
            (
                AssetType::LinearAsset(contract_size),
                LatencyModel::ConstantLatency {
                    entry_latency,
                    resp_latency,
                },
                QueueModel::PowerProbQueueModel3(n),
                ExchangeKind::PartialFillExchange,
            ) => {
                build_asset! {
                    asset.data;
                    asset.maker_fee;
                    asset.taker_fee;
                    asset.trade_len;
                    LinearAsset: {*contract_size};
                    ConstantLatency: {*entry_latency, *resp_latency};
                    PowerProbQueueModel3: {PowerProbQueueFunc3::new(*n)};
                    HashMapMarketDepth: {asset.tick_size, asset.lot_size};
                    PartialFillExchange
                }
            }
            (
                AssetType::LinearAsset(contract_size),
                LatencyModel::IntpOrderLatency { data: latency_data },
                QueueModel::PowerProbQueueModel3(n),
                ExchangeKind::PartialFillExchange,
            ) => {
                build_asset! {
                    asset.data;
                    asset.maker_fee;
                    asset.taker_fee;
                    asset.trade_len;
                    LinearAsset: {*contract_size};
                    IntpOrderLatency: {latency_data.clone()};
                    PowerProbQueueModel3: {PowerProbQueueFunc3::new(*n)};
                    HashMapMarketDepth: {asset.tick_size, asset.lot_size};
                    PartialFillExchange
                }
            }
            (
                AssetType::InverseAsset(contract_size),
                LatencyModel::ConstantLatency {
                    entry_latency,
                    resp_latency,
                },
                QueueModel::PowerProbQueueModel3(n),
                ExchangeKind::PartialFillExchange,
            ) => {
                build_asset! {
                    asset.data;
                    asset.maker_fee;
                    asset.taker_fee;
                    asset.trade_len;
                    InverseAsset: {*contract_size};
                    ConstantLatency: {*entry_latency, *resp_latency};
                    PowerProbQueueModel3: {PowerProbQueueFunc3::new(*n)};
                    HashMapMarketDepth: {asset.tick_size, asset.lot_size};
                    PartialFillExchange
                }
            }
            (
                AssetType::InverseAsset(contract_size),
                LatencyModel::IntpOrderLatency { data: latency_data },
                QueueModel::PowerProbQueueModel3(n),
                ExchangeKind::PartialFillExchange,
            ) => {
                build_asset! {
                    asset.data;
                    asset.maker_fee;
                    asset.taker_fee;
                    asset.trade_len;
                    InverseAsset: {*contract_size};
                    IntpOrderLatency: {latency_data.clone()};
                    PowerProbQueueModel3: {PowerProbQueueFunc3::new(*n)};
                    HashMapMarketDepth: {asset.tick_size, asset.lot_size};
                    PartialFillExchange
                }
            }
            _ => todo!(),
        };
        local.push(asst.local);
        exch.push(asst.exch);
    }

    let hbt = MultiAssetMultiExchangeBacktest::new(local, exch);
    Ok(PyMultiAssetMultiExchangeBacktest { ptr: Box::new(hbt) })
}
