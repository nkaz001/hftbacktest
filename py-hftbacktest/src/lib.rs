use std::ffi::c_void;

pub use backtest::*;
pub use depth::*;
use hftbacktest::{
    backtest::{
        assettype::{InverseAsset, LinearAsset},
        models::{
            ConstantLatency,
            IntpOrderLatency,
            LogProbQueueFunc,
            LogProbQueueFunc2,
            OrderLatencyRow,
            PowerProbQueueFunc,
            PowerProbQueueFunc2,
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
    pub fn data(mut slf: PyRefMut<Self>, data: Vec<String>) -> PyRefMut<Self> {
        for item in data {
            slf.data.push(item);
        }
        slf
    }

    /// Sets the asset as a [`LinearAsset`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/assettype/struct.LinearAsset.html).
    ///
    /// Args:
    ///     contract_size: contract size of the asset.
    pub fn linear_asset(mut slf: PyRefMut<Self>, contract_size: f64) -> PyRefMut<Self> {
        slf.asset_type = AssetType::LinearAsset(contract_size);
        slf
    }

    /// Sets the asset as a [`InverseAsset`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/assettype/struct.InverseAsset.html).
    ///
    /// Args:
    ///     contract_size: contract size of the asset.
    pub fn inverse_asset(mut slf: PyRefMut<Self>, contract_size: f64) -> PyRefMut<Self> {
        slf.asset_type = AssetType::InverseAsset(contract_size);
        slf
    }

    /// Uses [`ConstantLatency`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.ConstantLatency.html)
    /// for the order latency model.
    /// The units of the arguments should match the timestamp units of your data. Nanoseconds are
    /// typically used in HftBacktest.
    ///
    /// Args:
    ///     entry_latency: order entry latency.
    ///     resp_latency: order response latency.
    pub fn constant_latency(
        mut slf: PyRefMut<Self>,
        entry_latency: i64,
        resp_latency: i64,
    ) -> PyRefMut<Self> {
        slf.latency_model = LatencyModel::ConstantLatency {
            entry_latency,
            resp_latency,
        };
        slf
    }

    /// Uses [`IntpOrderLatency`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.IntpOrderLatency.html)
    /// for the order latency model.
    /// Please see the data format.
    /// The units of the historical latencies should match the timestamp units of your data.
    /// Nanoseconds are typically used in HftBacktest.
    ///
    /// Args:
    ///     data: a list of file paths for the historical order latency data in `npz`.
    pub fn intp_order_latency(mut slf: PyRefMut<Self>, data: Vec<String>) -> PyRefMut<Self> {
        slf.latency_model = LatencyModel::IntpOrderLatency {
            data: data
                .iter()
                .map(|file| DataSource::File(file.to_string()))
                .collect(),
        };
        slf
    }

    /// Uses the [`RiskAdverseQueueModel`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.RiskAdverseQueueModel.html)
    /// for the queue position model.
    pub fn risk_adverse_queue_model(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
        slf.queue_model = QueueModel::RiskAdverseQueueModel;
        slf
    }

    /// Uses the `LogProbQueueModel` for the queue position model.
    ///
    /// Please find the details below.
    /// * [`ProbQueueModel`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.ProbQueueModel.html)
    /// * [`LogProbQueueFunc`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.LogProbQueueFunc.html)
    pub fn log_prob_queue_model(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
        slf.queue_model = QueueModel::LogProbQueueModel;
        slf
    }

    /// Uses the `LogProbQueueModel2` for the queue position model.
    ///
    /// Please find the details below.
    /// * [`ProbQueueModel`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.ProbQueueModel.html)
    /// * [`LogProbQueueFunc2`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.LogProbQueueFunc2.html)
    pub fn log_prob_queue_model2(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
        slf.queue_model = QueueModel::LogProbQueueModel2;
        slf
    }

    /// Uses the `PowerProbQueueModel` for the queue position model.
    ///
    /// Please find the details below.
    /// * [`ProbQueueModel`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.ProbQueueModel.html)
    /// * [`PowerProbQueueFunc`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.PowerProbQueueFunc.html)
    pub fn power_prob_queue_model(mut slf: PyRefMut<Self>, n: f32) -> PyRefMut<Self> {
        slf.queue_model = QueueModel::PowerProbQueueModel(n);
        slf
    }

    /// Uses the `PowerProbQueueModel2` for the queue position model.
    ///
    /// Please find the details below.
    /// * [`ProbQueueModel`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.ProbQueueModel.html)
    /// * [`PowerProbQueueFunc2`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.PowerProbQueueFunc2.html)
    pub fn power_prob_queue_model2(mut slf: PyRefMut<Self>, n: f32) -> PyRefMut<Self> {
        slf.queue_model = QueueModel::PowerProbQueueModel2(n);
        slf
    }

    /// Uses the `PowerProbQueueModel3` for the queue position model.
    ///
    /// Please find the details below.
    /// * [`ProbQueueModel`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.ProbQueueModel.html)
    /// * [`PowerProbQueueFunc3`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.PowerProbQueueFunc3.html)
    pub fn power_prob_queue_model3(mut slf: PyRefMut<Self>, n: f32) -> PyRefMut<Self> {
        slf.queue_model = QueueModel::PowerProbQueueModel3(n);
        slf
    }

    /// Sets the tick size of the asset.
    pub fn tick_size(mut slf: PyRefMut<Self>, tick_size: f32) -> PyRefMut<Self> {
        slf.tick_size = tick_size;
        slf
    }

    /// Sets the lot size of the asset.
    pub fn lot_size(mut slf: PyRefMut<Self>, lot_size: f32) -> PyRefMut<Self> {
        slf.lot_size = lot_size;
        slf
    }

    /// Uses the [`NoPartiallFillExchange`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/proc/struct.NoPartialFillExchange.html)
    /// for the exchange model.
    pub fn no_partial_fill_exchange(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
        slf.exch_kind = ExchangeKind::NoPartialFillExchange;
        slf
    }

    /// Uses the [`PartiallFillExchange`](https://docs.rs/hftbacktest/latest/hftbacktest/backtest/proc/struct.PartialFillExchange.html)
    /// for the exchange model.
    pub fn partial_fill_exchange(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
        slf.exch_kind = ExchangeKind::PartialFillExchange;
        slf
    }

    /// Sets the maker fee. A negative fee represents rebates.
    pub fn maker_fee(mut slf: PyRefMut<Self>, maker_fee: f64) -> PyRefMut<Self> {
        slf.maker_fee = maker_fee;
        slf
    }

    /// Sets the taker fee. A negative fee represents rebates.
    pub fn taker_fee(mut slf: PyRefMut<Self>, taker_fee: f64) -> PyRefMut<Self> {
        slf.taker_fee = taker_fee;
        slf
    }

    /// Sets the initial capacity of the vector storing the trades occurring in the market.
    pub fn trade_len(mut slf: PyRefMut<Self>, trade_len: usize) -> PyRefMut<Self> {
        slf.trade_len = trade_len;
        slf
    }
}

#[pymodule]
fn _hftbacktest(m: &Bound<'_, PyModule>) -> PyResult<()> {
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

type LogProbQueueModel = ProbQueueModel<LogProbQueueFunc, HashMapMarketDepth>;
type LogProbQueueModel2 = ProbQueueModel<LogProbQueueFunc2, HashMapMarketDepth>;
type PowerProbQueueModel = ProbQueueModel<PowerProbQueueFunc, HashMapMarketDepth>;
type PowerProbQueueModel2 = ProbQueueModel<PowerProbQueueFunc2, HashMapMarketDepth>;
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
