use std::{ffi::c_void, mem::size_of, ptr::slice_from_raw_parts_mut};

pub use backtest::*;
pub use depth::*;
use hftbacktest::{
    backtest::{
        assettype::{InverseAsset, LinearAsset},
        data::{read_npz_file, Cache, Data, DataPtr, Reader},
        models::{
            CommonFees,
            ConstantLatency,
            FlatPerTradeFeeModel,
            IntpOrderLatency,
            LogProbQueueFunc,
            LogProbQueueFunc2,
            OrderLatencyRow,
            PowerProbQueueFunc,
            PowerProbQueueFunc2,
            PowerProbQueueFunc3,
            ProbQueueModel,
            RiskAdverseQueueModel,
            TradingQtyFeeModel,
            TradingValueFeeModel,
        },
        order::OrderBus,
        proc::{Local, LocalProcessor, NoPartialFillExchange, PartialFillExchange, Processor},
        state::State,
        Asset,
        Backtest,
        DataSource,
    },
    prelude::{ApplySnapshot, Event, HashMapMarketDepth, ROIVectorMarketDepth},
};
use hftbacktest_derive::build_asset;
pub use order::*;
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
    PowerProbQueueModel { n: f64 },
    LogProbQueueModel {},
    LogProbQueueModel2 {},
    PowerProbQueueModel2 { n: f64 },
    PowerProbQueueModel3 { n: f64 },
}

#[derive(Clone)]
pub enum ExchangeKind {
    NoPartialFillExchange {},
    PartialFillExchange {},
}

#[derive(Clone)]
pub enum FeeModel {
    TradingValueFeeModel { fees: CommonFees },
    TradingQtyFeeModel { fees: CommonFees },
    FlatPerTradeFeeModel { fees: CommonFees },
}

/// Builds a backtesting asset.
#[pyclass(subclass)]
pub struct BacktestAsset {
    data: Vec<DataSource<Event>>,
    asset_type: AssetType,
    latency_model: LatencyModel,
    queue_model: QueueModel,
    exch_kind: ExchangeKind,
    tick_size: f64,
    lot_size: f64,
    last_trades_cap: usize,
    roi_lb: f64,
    roi_ub: f64,
    initial_snapshot: Option<DataSource<Event>>,
    fee_model: FeeModel,
}

unsafe impl Send for BacktestAsset {}

#[pymethods]
impl BacktestAsset {
    /// Constructs an instance of `AssetBuilder`.
    #[allow(clippy::new_without_default)]
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
            exch_kind: ExchangeKind::NoPartialFillExchange {},
            last_trades_cap: 0,
            roi_lb: 0.0,
            roi_ub: 0.0,
            initial_snapshot: None,
            fee_model: FeeModel::TradingValueFeeModel {
                fees: CommonFees::new(0.0, 0.0),
            },
        }
    }

    /// Sets the lower bound price of the `ROIVectorMarketDepth <https://docs.rs/hftbacktest/latest/hftbacktest/depth/struct.ROIVectorMarketDepth.html>`_.
    /// Only valid if `ROIVectorMarketDepthBacktest` is built.
    ///
    /// Args:
    ///     roi_lb: the lower bound price of the range of interest.
    pub fn roi_lb(mut slf: PyRefMut<Self>, roi_lb: f64) -> PyRefMut<Self> {
        slf.roi_lb = roi_lb;
        slf
    }

    /// Sets the lower bound price of the `ROIVectorMarketDepth <https://docs.rs/hftbacktest/latest/hftbacktest/depth/struct.ROIVectorMarketDepth.html>`_.
    /// Only valid if `ROIVectorMarketDepthBacktest` is built.
    ///
    /// Args:
    ///     roi_lb: the lower bound price of the range of interest.
    pub fn roi_ub(mut slf: PyRefMut<Self>, roi_ub: f64) -> PyRefMut<Self> {
        slf.roi_ub = roi_ub;
        slf
    }

    pub fn add_file(mut slf: PyRefMut<Self>, data: String) -> PyRefMut<Self> {
        slf.data.push(DataSource::File(data));
        slf
    }

    pub fn _add_data_ndarray(mut slf: PyRefMut<Self>, data: usize, len: usize) -> PyRefMut<Self> {
        let arr = slice_from_raw_parts_mut(data as *mut u8, len * size_of::<Event>());
        let data = unsafe { Data::<Event>::from_data_ptr(DataPtr::from_ptr(arr), 0) };
        slf.data.push(DataSource::Data(data));
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

    pub fn _intp_order_latency_ndarray(
        mut slf: PyRefMut<Self>,
        data: usize,
        len: usize,
    ) -> PyRefMut<Self> {
        let arr = slice_from_raw_parts_mut(data as *mut u8, len * size_of::<OrderLatencyRow>());
        let data = unsafe { Data::<OrderLatencyRow>::from_data_ptr(DataPtr::from_ptr(arr), 0) };
        slf.latency_model = LatencyModel::IntpOrderLatency {
            data: vec![DataSource::Data(data)],
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
    pub fn power_prob_queue_model(mut slf: PyRefMut<Self>, n: f64) -> PyRefMut<Self> {
        slf.queue_model = QueueModel::PowerProbQueueModel { n };
        slf
    }

    /// Uses the `PowerProbQueueModel2` for the queue position model.
    ///
    /// Please find the details below.
    ///
    /// * `ProbQueueModel <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.ProbQueueModel.html>`_
    /// * `PowerProbQueueFunc2 <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.PowerProbQueueFunc2.html>`_
    pub fn power_prob_queue_model2(mut slf: PyRefMut<Self>, n: f64) -> PyRefMut<Self> {
        slf.queue_model = QueueModel::PowerProbQueueModel2 { n };
        slf
    }

    /// Uses the `PowerProbQueueModel3` for the queue position model.
    ///
    /// Please find the details below.
    ///
    /// * `ProbQueueModel <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.ProbQueueModel.html>`_
    /// * `PowerProbQueueFunc3 <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.PowerProbQueueFunc3.html>`_
    pub fn power_prob_queue_model3(mut slf: PyRefMut<Self>, n: f64) -> PyRefMut<Self> {
        slf.queue_model = QueueModel::PowerProbQueueModel3 { n };
        slf
    }

    /// Sets the initial snapshot.
    pub fn initial_snapshot(mut slf: PyRefMut<Self>, file: String) -> PyRefMut<Self> {
        slf.initial_snapshot = Some(DataSource::File(file));
        slf
    }

    pub fn _initial_snapshot_ndarray(
        mut slf: PyRefMut<Self>,
        data: usize,
        len: usize,
    ) -> PyRefMut<Self> {
        let arr = slice_from_raw_parts_mut(data as *mut u8, len * size_of::<Event>());
        let data = unsafe { Data::<Event>::from_data_ptr(DataPtr::from_ptr(arr), 0) };
        slf.initial_snapshot = Some(DataSource::Data(data));
        slf
    }

    /// Sets the tick size of the asset.
    pub fn tick_size(mut slf: PyRefMut<Self>, tick_size: f64) -> PyRefMut<Self> {
        slf.tick_size = tick_size;
        slf
    }

    /// Sets the lot size of the asset.
    pub fn lot_size(mut slf: PyRefMut<Self>, lot_size: f64) -> PyRefMut<Self> {
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

    /// Sets the initial capacity of the vector storing the last market trades.
    /// The default value is `0`, indicating that no last trades are stored.
    pub fn last_trades_capacity(mut slf: PyRefMut<Self>, capacity: usize) -> PyRefMut<Self> {
        slf.last_trades_cap = capacity;
        slf
    }

    /// Uses `TradingValueFeeModel <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.TradingValueFeeModel.html>`_.
    /// A negative fee represents rebates.
    pub fn trading_value_fee_model(
        mut slf: PyRefMut<Self>,
        maker_fee: f64,
        taker_fee: f64,
    ) -> PyRefMut<Self> {
        slf.fee_model = FeeModel::TradingValueFeeModel {
            fees: CommonFees::new(maker_fee, taker_fee),
        };
        slf
    }

    /// Uses `TradingQtyFeeModel <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.TradingQtyFeeModel.html>`_.
    /// A negative fee represents rebates.
    pub fn trading_qty_fee_model(
        mut slf: PyRefMut<Self>,
        maker_fee: f64,
        taker_fee: f64,
    ) -> PyRefMut<Self> {
        slf.fee_model = FeeModel::TradingQtyFeeModel {
            fees: CommonFees::new(maker_fee, taker_fee),
        };
        slf
    }

    /// Uses `FlatPerTradeFeeModel <https://docs.rs/hftbacktest/latest/hftbacktest/backtest/models/struct.FlatPerTradeFeeModel.html>`_.
    /// A negative fee represents rebates.
    pub fn flat_per_trade_fee_model(
        mut slf: PyRefMut<Self>,
        maker_fee: f64,
        taker_fee: f64,
    ) -> PyRefMut<Self> {
        slf.fee_model = FeeModel::FlatPerTradeFeeModel {
            fees: CommonFees::new(maker_fee, taker_fee),
        };
        slf
    }
}

#[pymodule]
fn _hftbacktest(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(build_hashmap_backtest, m)?)?;
    m.add_function(wrap_pyfunction!(build_roivec_backtest, m)?)?;
    m.add_class::<BacktestAsset>()?;
    Ok(())
}

type LogProbQueueModelFunc = LogProbQueueFunc;
type LogProbQueueModel2Func = LogProbQueueFunc2;
type PowerProbQueueModelFunc = PowerProbQueueFunc;
type PowerProbQueueModel2Func = PowerProbQueueFunc2;
type PowerProbQueueModel3Func = PowerProbQueueFunc3;

#[pyfunction]
pub fn build_hashmap_backtest(assets: Vec<PyRefMut<BacktestAsset>>) -> PyResult<usize> {
    let mut local = Vec::new();
    let mut exch = Vec::new();
    for asset in assets {
        let asst = build_asset!(
            asset,
            HashMapMarketDepth,
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
            [NoPartialFillExchange {}, PartialFillExchange {}],
            [
                TradingValueFeeModel { fees },
                TradingQtyFeeModel { fees },
                FlatPerTradeFeeModel { fees },
            ]
        );
        local.push(asst.local);
        exch.push(asst.exch);
    }

    let hbt = Backtest::new(local, exch);
    Ok(Box::into_raw(Box::new(hbt)) as *mut c_void as usize)
}

#[pyfunction]
pub fn build_roivec_backtest(assets: Vec<PyRefMut<BacktestAsset>>) -> PyResult<usize> {
    let mut local = Vec::new();
    let mut exch = Vec::new();
    for asset in assets {
        let asst = build_asset!(
            asset,
            ROIVectorMarketDepth,
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
            [NoPartialFillExchange {}, PartialFillExchange {}],
            [
                TradingValueFeeModel { fees },
                TradingQtyFeeModel { fees },
                FlatPerTradeFeeModel { fees },
            ]
        );
        local.push(asst.local);
        exch.push(asst.exch);
    }

    let hbt = Backtest::new(local, exch);
    Ok(Box::into_raw(Box::new(hbt)) as *mut c_void as usize)
}
