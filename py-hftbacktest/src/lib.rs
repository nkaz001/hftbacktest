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
            RiskAdverseQueueModel,
        },
        order::OrderBus,
        proc::{Local, LocalProcessor, NoPartialFillExchange, PartialFillExchange, Processor},
        reader::{read_npz, Cache, Reader},
        state::State,
        Asset,
        DataSource,
        MultiAssetMultiExchangeBacktest,
    },
    prelude::{ApplySnapshot, Event, HashMapMarketDepth},
};
pub use order::*;
use procmacro::build_asset;
use pyo3::prelude::*;

mod backtest;
mod depth;
mod order;

#[derive(Clone)]
pub enum AssetType {
    LinearAsset { contract_size: f64 },
    InverseAsset { contract_size: f64 },
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
    RiskAdverseQueueModel {},
    PowerProbQueueModel { n: f32 },
    LogProbQueueModel {},
    LogProbQueueModel2 {},
    PowerProbQueueModel2 { n: f32 },
    PowerProbQueueModel3 { n: f32 },
}

#[derive(Clone)]
pub enum ExchangeKind {
    NoPartialFillExchange {},
    PartialFillExchange {},
}

/// Builds a backtesting asset.
#[pyclass]
pub struct BacktestAsset {
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
    initial_snapshot: Option<String>,
}

unsafe impl Send for BacktestAsset {}

#[pymethods]
impl BacktestAsset {
    /// Constructs an instance of `AssetBuilder`.
    #[new]
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            latency_model: LatencyModel::ConstantLatency {
                entry_latency: 0,
                resp_latency: 0,
            },
            asset_type: AssetType::LinearAsset { contract_size: 1.0 },
            queue_model: QueueModel::LogProbQueueModel2 {},
            tick_size: 0.0,
            lot_size: 0.0,
            maker_fee: 0.0,
            taker_fee: 0.0,
            exch_kind: ExchangeKind::NoPartialFillExchange {},
            trade_len: 0,
            initial_snapshot: None,
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

    /// Sets the asset as a `LinearAsset <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/assettype/struct.LinearAsset.html>`_.
    ///
    /// Args:
    ///     contract_size: contract size of the asset.
    pub fn linear_asset(mut slf: PyRefMut<Self>, contract_size: f64) -> PyRefMut<Self> {
        slf.asset_type = AssetType::LinearAsset { contract_size };
        slf
    }

    /// Sets the asset as a `InverseAsset <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/assettype/struct.InverseAsset.html>`_.
    ///
    /// Args:
    ///     contract_size: contract size of the asset.
    pub fn inverse_asset(mut slf: PyRefMut<Self>, contract_size: f64) -> PyRefMut<Self> {
        slf.asset_type = AssetType::InverseAsset { contract_size };
        slf
    }

    /// Uses `ConstantLatency <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.ConstantLatency.html>`_
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

    /// Uses `IntpOrderLatency <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.IntpOrderLatency.html>`_
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

    /// Uses the `RiskAdverseQueueModel <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.RiskAdverseQueueModel.html>`_
    /// for the queue position model.
    pub fn risk_adverse_queue_model(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
        slf.queue_model = QueueModel::RiskAdverseQueueModel {};
        slf
    }

    /// Uses the `LogProbQueueModel` for the queue position model.
    ///
    /// Please find the details below.
    ///
    /// * `ProbQueueModel <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.ProbQueueModel.html>`_
    /// * `LogProbQueueFunc <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.LogProbQueueFunc.html>`_
    pub fn log_prob_queue_model(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
        slf.queue_model = QueueModel::LogProbQueueModel {};
        slf
    }

    /// Uses the `LogProbQueueModel2` for the queue position model.
    ///
    /// Please find the details below.
    ///
    /// * `ProbQueueModel <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.ProbQueueModel.html>`_
    /// * `LogProbQueueFunc2 <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.LogProbQueueFunc2.html>`_
    pub fn log_prob_queue_model2(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
        slf.queue_model = QueueModel::LogProbQueueModel2 {};
        slf
    }

    /// Uses the `PowerProbQueueModel` for the queue position model.
    ///
    /// Please find the details below.
    ///
    /// * `ProbQueueModel <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.ProbQueueModel.html>`_
    /// * `PowerProbQueueFunc <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.PowerProbQueueFunc.html>`_
    pub fn power_prob_queue_model(mut slf: PyRefMut<Self>, n: f32) -> PyRefMut<Self> {
        slf.queue_model = QueueModel::PowerProbQueueModel { n };
        slf
    }

    /// Uses the `PowerProbQueueModel2` for the queue position model.
    ///
    /// Please find the details below.
    ///
    /// * `ProbQueueModel <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.ProbQueueModel.html>`_
    /// * `PowerProbQueueFunc2 <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.PowerProbQueueFunc2.html>`_
    pub fn power_prob_queue_model2(mut slf: PyRefMut<Self>, n: f32) -> PyRefMut<Self> {
        slf.queue_model = QueueModel::PowerProbQueueModel2 { n };
        slf
    }

    /// Uses the `PowerProbQueueModel3` for the queue position model.
    ///
    /// Please find the details below.
    ///
    /// * `ProbQueueModel <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.ProbQueueModel.html>`_
    /// * `PowerProbQueueFunc3 <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.PowerProbQueueFunc3.html>`_
    pub fn power_prob_queue_model3(mut slf: PyRefMut<Self>, n: f32) -> PyRefMut<Self> {
        slf.queue_model = QueueModel::PowerProbQueueModel3 { n };
        slf
    }

    /// Sets the initial snapshot.
    pub fn initial_snapshot(mut slf: PyRefMut<Self>, snapshot_file: String) -> PyRefMut<Self> {
        slf.initial_snapshot = Some(snapshot_file);
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

    /// Uses the `NoPartiallFillExchange <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/proc/struct.NoPartialFillExchange.html>`_
    /// for the exchange model.
    pub fn no_partial_fill_exchange(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
        slf.exch_kind = ExchangeKind::NoPartialFillExchange {};
        slf
    }

    /// Uses the `PartiallFillExchange <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/proc/struct.PartialFillExchange.html>`_
    /// for the exchange model.
    pub fn partial_fill_exchange(mut slf: PyRefMut<Self>) -> PyRefMut<Self> {
        slf.exch_kind = ExchangeKind::PartialFillExchange {};
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
    m.add_class::<BacktestAsset>()?;
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

type LogProbQueueModel = ProbQueueModel<LogProbQueueFunc, HashMapMarketDepth>;
type LogProbQueueModel2 = ProbQueueModel<LogProbQueueFunc2, HashMapMarketDepth>;
type PowerProbQueueModel = ProbQueueModel<PowerProbQueueFunc, HashMapMarketDepth>;
type PowerProbQueueModel2 = ProbQueueModel<PowerProbQueueFunc2, HashMapMarketDepth>;

type PowerProbQueueModel3 = ProbQueueModel<PowerProbQueueFunc3, HashMapMarketDepth>;

type LogProbQueueModelFunc = LogProbQueueFunc;
type LogProbQueueModel2Func = LogProbQueueFunc2;
type PowerProbQueueModelFunc = PowerProbQueueFunc;
type PowerProbQueueModel2Func = PowerProbQueueFunc2;
type PowerProbQueueModel3Func = PowerProbQueueFunc3;

#[pyfunction]
pub fn build_backtester(
    assets: Vec<PyRefMut<BacktestAsset>>,
) -> PyResult<PyMultiAssetMultiExchangeBacktest> {
    let mut local = Vec::new();
    let mut exch = Vec::new();
    for asset in assets {
        let asst = build_asset!(
            asset,
            [
                LinearAsset { contract_size },
                InverseAsset { contract_size }
            ],
            [
                ConstantLatency {
                    entry_latency,
                    resp_latency
                },
                IntpOrderLatency { data }
            ],
            [
                RiskAdverseQueueModel {},
                LogProbQueueModel {},
                LogProbQueueModel2 {},
                PowerProbQueueModel { n },
                PowerProbQueueModel2 { n },
                PowerProbQueueModel3 { n }
            ],
            [NoPartialFillExchange {}, PartialFillExchange {}]
        );
        local.push(asst.local);
        exch.push(asst.exch);
    }

    let hbt = MultiAssetMultiExchangeBacktest::new(local, exch);
    Ok(PyMultiAssetMultiExchangeBacktest { ptr: Box::new(hbt) })
}
