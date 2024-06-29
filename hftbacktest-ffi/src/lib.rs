mod backtest;
mod depth;

use std::ffi::c_void;

pub use backtest::*;
pub use depth::*;
use hftbacktest::{
    backtest::{
        assettype::LinearAsset,
        models::{ConstantLatency, RiskAdverseQueueModel},
        order::OrderBus,
        proc::{Local, LocalProcessor, NoPartialFillExchange, Processor},
        reader::{Cache, Reader},
        state::State,
        MultiAssetMultiExchangeBacktest,
    },
    prelude::{BuildError, Event, HashMapMarketDepth},
};
use pyo3::prelude::*;

#[pyclass(eq, eq_int)]
#[derive(Clone, PartialEq)]
pub enum PyAssetType {
    LinearAsset,
    InverseAsset,
}

#[pyclass(eq, eq_int)]
#[derive(Clone, PartialEq)]
pub enum PyLatencyModel {
    ConstantLatency,
    IntpLatencyModel,
}

#[pyclass(eq, eq_int)]
#[derive(Clone, PartialEq)]
pub enum PyQueueModel {
    ConstantLatency,
    IntpLatencyModel,
}

#[pyclass(eq)]
#[derive(Clone, PartialEq)]
pub enum PyDepth {
    HashMapMarketDepth(f64, f64),
}

#[pyclass(eq, eq_int)]
#[derive(Clone, PartialEq)]
pub enum PyExchangeKind {
    NoPartialFillExchange,
    PartialFillExchange,
}

#[pyclass]
#[derive(FromPyObject)]
pub struct PyAssetBuilder {
    data: Vec<String>,
    latency_model: Option<PyLatencyModel>,
    asset_type: Option<PyAssetType>,
    queue_model: Option<PyQueueModel>,
    depth_builder: Option<PyDepth>,
    maker_fee: f64,
    taker_fee: f64,
    exch_kind: PyExchangeKind,
    trade_len: usize,
}

#[pymethods]
impl PyAssetBuilder {
    #[new]
    pub fn new() -> Self {
        Self {
            data: Vec::new(),
            latency_model: None,
            asset_type: None,
            queue_model: None,
            depth_builder: None,
            maker_fee: 0.0,
            taker_fee: 0.0,
            exch_kind: PyExchangeKind::NoPartialFillExchange,
            trade_len: 0,
        }
    }

    pub fn data(&mut self, data: Vec<String>) {
        for item in data {
            self.data.push(item);
        }
    }

    pub fn latency_model(&mut self, latency_model: PyLatencyModel) {
        self.latency_model = Some(latency_model);
    }

    pub fn asset_type(&mut self, asset_type: PyAssetType) {
        self.asset_type = Some(asset_type);
    }

    pub fn maker_fee(&mut self, maker_fee: f64) {
        self.maker_fee = maker_fee;
    }

    pub fn taker_fee(&mut self, taker_fee: f64) {
        self.taker_fee = taker_fee;
    }

    pub fn queue_model(&mut self, queue_model: PyQueueModel) {
        self.queue_model = Some(queue_model);
    }

    pub fn depth(&mut self, depth: PyDepth) {
        self.depth_builder = Some(depth);
    }

    pub fn exchange(&mut self, exch_kind: PyExchangeKind) {
        self.exch_kind = exch_kind;
    }

    pub fn trade_len(&mut self, trade_len: usize) {
        self.trade_len = trade_len;
    }
}

#[pymodule]
fn hftbacktest_ffi(m: &Bound<'_, PyModule>) -> PyResult<()> {
    m.add_function(wrap_pyfunction!(build_backtester, m)?)?;
    m.add_class::<PyAssetBuilder>()?;
    m.add_class::<PyAssetType>()?;
    m.add_class::<PyLatencyModel>()?;
    m.add_class::<PyQueueModel>()?;
    m.add_class::<PyExchangeKind>()?;
    m.add_class::<PyDepth>()?;
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

#[pyfunction]
pub fn build_backtester(
    assets: Vec<PyRefMut<PyAssetBuilder>>,
) -> PyResult<PyMultiAssetMultiExchangeBacktest> {
    let mut locals = Vec::new();
    let mut exchs = Vec::new();

    for asset in assets {
        let cache = Cache::new();
        let mut reader = Reader::new(cache);
        let ob_local_to_exch = OrderBus::new();
        let ob_exch_to_local = OrderBus::new();

        for file in asset.data.iter() {
            reader.add_file(file.to_string());
        }

        let order_latency = asset
            .latency_model
            .clone()
            .ok_or(BuildError::BuilderIncomplete("order_latency"))
            .unwrap();
        let asset_type = asset
            .asset_type
            .clone()
            .ok_or(BuildError::BuilderIncomplete("asset_type"))
            .unwrap();

        let local: Box<dyn LocalProcessor<HashMapMarketDepth, Event>> =
            match (&asset_type, &order_latency) {
                (PyAssetType::LinearAsset, PyLatencyModel::ConstantLatency) => {
                    let asset_type = LinearAsset::new(1.0);
                    let order_latency = ConstantLatency::new(10_000_000, 10_000_000);
                    Box::new(Local::new(
                        reader.clone(),
                        { HashMapMarketDepth::new(0.000001, 1.0) },
                        State::new(asset_type, asset.maker_fee, asset.taker_fee),
                        order_latency,
                        asset.trade_len,
                        ob_local_to_exch.clone(),
                        ob_exch_to_local.clone(),
                    ))
                }
                (PyAssetType::LinearAsset, PyLatencyModel::IntpLatencyModel) => {
                    let asset_type = LinearAsset::new(1.0);
                    let order_latency = ConstantLatency::new(10_000_000, 10_000_000);
                    Box::new(Local::new(
                        reader.clone(),
                        { HashMapMarketDepth::new(0.000001, 1.0) },
                        State::new(asset_type, asset.maker_fee, asset.taker_fee),
                        order_latency,
                        asset.trade_len,
                        ob_local_to_exch.clone(),
                        ob_exch_to_local.clone(),
                    ))
                }
                (PyAssetType::InverseAsset, PyLatencyModel::ConstantLatency) => {
                    let asset_type = LinearAsset::new(1.0);
                    let order_latency = ConstantLatency::new(10_000_000, 10_000_000);
                    Box::new(Local::new(
                        reader.clone(),
                        { HashMapMarketDepth::new(0.000001, 1.0) },
                        State::new(asset_type, asset.maker_fee, asset.taker_fee),
                        order_latency,
                        asset.trade_len,
                        ob_local_to_exch.clone(),
                        ob_exch_to_local.clone(),
                    ))
                }
                (PyAssetType::InverseAsset, PyLatencyModel::IntpLatencyModel) => {
                    let asset_type = LinearAsset::new(1.0);
                    let order_latency = ConstantLatency::new(10_000_000, 10_000_000);
                    Box::new(Local::new(
                        reader.clone(),
                        { HashMapMarketDepth::new(0.000001, 1.0) },
                        State::new(asset_type, asset.maker_fee, asset.taker_fee),
                        order_latency,
                        asset.trade_len,
                        ob_local_to_exch.clone(),
                        ob_exch_to_local.clone(),
                    ))
                }
            };
        locals.push(local);

        let exch_kind = asset.exch_kind.clone();
        let exch: Box<dyn Processor> = match (asset_type, order_latency, exch_kind) {
            (
                PyAssetType::LinearAsset,
                PyLatencyModel::ConstantLatency,
                PyExchangeKind::NoPartialFillExchange,
            ) => {
                let asset_type = LinearAsset::new(1.0);
                let order_latency = ConstantLatency::new(10_000_000, 10_000_000);
                Box::new(NoPartialFillExchange::new(
                    reader.clone(),
                    { HashMapMarketDepth::new(0.000001, 1.0) },
                    State::new(asset_type, asset.maker_fee, asset.taker_fee),
                    order_latency,
                    RiskAdverseQueueModel::new(),
                    ob_exch_to_local,
                    ob_local_to_exch,
                ))
            }
            _ => {
                panic!()
            }
        };
        exchs.push(exch);
    }

    let hbt = MultiAssetMultiExchangeBacktest::new(locals, exchs);
    Ok(PyMultiAssetMultiExchangeBacktest { ptr: Box::new(hbt) })
}
