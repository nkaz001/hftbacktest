#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::{collections::HashMap, mem};

use hftbacktest::{
    backtest::{Backtest, BacktestError},
    depth::{HashMapMarketDepth, ROIVectorMarketDepth},
    prelude::{Bot, ElapseResult, Event, Order, StateValues},
    types::{OrdType, TimeInForce},
};

type HashMapMarketDepthBacktest = Backtest<HashMapMarketDepth>;
type ROIVectorMarketDepthBacktest = Backtest<ROIVectorMarketDepth>;

fn handle_result(result: Result<ElapseResult, BacktestError>) -> i64 {
    match result {
        Ok(ElapseResult::Ok) => 0,
        Ok(ElapseResult::EndOfData) => 1,
        Ok(ElapseResult::MarketFeed) => 2,
        Ok(ElapseResult::OrderResponse) => 3,
        Err(BacktestError::OrderIdExist) => 10,
        Err(BacktestError::OrderRequestInProcess) => 11,
        Err(BacktestError::OrderNotFound) => 12,
        Err(BacktestError::InvalidOrderRequest) => 13,
        Err(BacktestError::InvalidOrderStatus) => 14,
        Err(BacktestError::EndOfData) => 15,
        Err(BacktestError::DataError(error)) => {
            println!("BacktestError::DataError: {error:?}");
            100
        },
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_current_timestamp(hbt_ptr: *const HashMapMarketDepthBacktest) -> i64 {
    let hbt = unsafe { &*hbt_ptr };
    hbt.current_timestamp()
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_depth(
    hbt_ptr: *const HashMapMarketDepthBacktest,
    asset_no: usize,
) -> *const HashMapMarketDepth {
    let hbt = unsafe { &*hbt_ptr };
    let depth = hbt.depth(asset_no);
    depth as *const _
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_last_trades(
    hbt_ptr: *const HashMapMarketDepthBacktest,
    asset_no: usize,
    len_ptr: *mut usize,
) -> *const Event {
    let hbt = unsafe { &*hbt_ptr };
    let trade = hbt.last_trades(asset_no);
    unsafe {
        *len_ptr = trade.len();
    }
    trade.as_ptr() as *mut _
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_position(
    hbt_ptr: *const HashMapMarketDepthBacktest,
    asset_no: usize,
) -> f64 {
    let hbt = unsafe { &*hbt_ptr };
    hbt.position(asset_no)
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_close(hbt_ptr: *mut HashMapMarketDepthBacktest) -> i64 {
    let mut hbt = unsafe { Box::from_raw(hbt_ptr) };
    match hbt.close() {
        Ok(()) => 0,
        Err(BacktestError::OrderIdExist) => 10,
        Err(BacktestError::OrderRequestInProcess) => 11,
        Err(BacktestError::OrderNotFound) => 12,
        Err(BacktestError::InvalidOrderRequest) => 13,
        Err(BacktestError::InvalidOrderStatus) => 14,
        Err(BacktestError::EndOfData) => 15,
        Err(BacktestError::DataError(_)) => 100,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_elapse(hbt_ptr: *mut HashMapMarketDepthBacktest, duration: i64) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.elapse(duration))
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_elapse_bt(
    hbt_ptr: *mut HashMapMarketDepthBacktest,
    duration: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.elapse_bt(duration))
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_num_assets(hbt_ptr: *const HashMapMarketDepthBacktest) -> usize {
    let hbt = unsafe { &*hbt_ptr };
    hbt.num_assets()
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_wait_order_response(
    hbt_ptr: *mut HashMapMarketDepthBacktest,
    asset_no: usize,
    order_id: u64,
    timeout: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.wait_order_response(asset_no, order_id, timeout))
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_wait_next_feed(
    hbt_ptr: *mut HashMapMarketDepthBacktest,
    include_resp: bool,
    timeout: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.wait_next_feed(include_resp, timeout))
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_submit_buy_order(
    hbt_ptr: *mut HashMapMarketDepthBacktest,
    asset_no: usize,
    order_id: u64,
    price: f64,
    qty: f64,
    time_in_force: u8,
    order_type: u8,
    wait: bool,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    let tif = unsafe { mem::transmute::<u8, TimeInForce>(time_in_force) };
    handle_result(hbt.submit_buy_order(
        asset_no,
        order_id,
        price,
        qty,
        tif,
        unsafe { mem::transmute::<u8, OrdType>(order_type) },
        wait,
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_submit_sell_order(
    hbt_ptr: *mut HashMapMarketDepthBacktest,
    asset_no: usize,
    order_id: u64,
    price: f64,
    qty: f64,
    time_in_force: u8,
    order_type: u8,
    wait: bool,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.submit_sell_order(
        asset_no,
        order_id,
        price,
        qty,
        unsafe { mem::transmute::<u8, TimeInForce>(time_in_force) },
        unsafe { mem::transmute::<u8, OrdType>(order_type) },
        wait,
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_modify(
    hbt_ptr: *mut HashMapMarketDepthBacktest,
    asset_no: usize,
    order_id: u64,
    price: f64,
    qty: f64,
    wait: bool,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.modify(asset_no, order_id, price, qty, wait))
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_cancel(
    hbt_ptr: *mut HashMapMarketDepthBacktest,
    asset_no: usize,
    order_id: u64,
    wait: bool,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.cancel(asset_no, order_id, wait))
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_clear_last_trades(
    hbt_ptr: *mut HashMapMarketDepthBacktest,
    asset_no: usize,
) {
    let hbt = unsafe { &mut *hbt_ptr };
    if asset_no == usize::MAX {
        hbt.clear_last_trades(None);
    } else {
        hbt.clear_last_trades(Some(asset_no));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_clear_inactive_orders(
    hbt_ptr: *mut HashMapMarketDepthBacktest,
    asset_no: usize,
) {
    let hbt = unsafe { &mut *hbt_ptr };
    if asset_no == usize::MAX {
        hbt.clear_inactive_orders(None);
    } else {
        hbt.clear_inactive_orders(Some(asset_no));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_orders(
    hbt_ptr: *const HashMapMarketDepthBacktest,
    asset_no: usize,
) -> *const HashMap<u64, Order> {
    let hbt = unsafe { &*hbt_ptr };
    hbt.orders(asset_no) as *const _
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_state_values(
    hbt_ptr: *const HashMapMarketDepthBacktest,
    asset_no: usize,
) -> *const StateValues {
    let hbt = unsafe { &*hbt_ptr };
    hbt.state_values(asset_no) as *const _
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_feed_latency(
    hbt_ptr: *const HashMapMarketDepthBacktest,
    asset_no: usize,
    exch_ts: *mut i64,
    local_ts: *mut i64,
) -> bool {
    let hbt = unsafe { &*hbt_ptr };
    match hbt.feed_latency(asset_no) {
        None => false,
        Some((exch_ts_, local_ts_)) => {
            unsafe {
                *exch_ts = exch_ts_;
                *local_ts = local_ts_;
            }
            true
        },
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_order_latency(
    hbt_ptr: *const HashMapMarketDepthBacktest,
    asset_no: usize,
    req_ts: *mut i64,
    exch_ts: *mut i64,
    resp_ts: *mut i64,
) -> bool {
    let hbt = unsafe { &*hbt_ptr };
    match hbt.order_latency(asset_no) {
        None => false,
        Some((req_ts_, exch_ts_, resp_ts_)) => {
            unsafe {
                *req_ts = req_ts_;
                *exch_ts = exch_ts_;
                *resp_ts = resp_ts_;
            }
            true
        },
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapbt_goto_end(hbt_ptr: *mut HashMapMarketDepthBacktest) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.goto_end())
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_current_timestamp(hbt_ptr: *const ROIVectorMarketDepthBacktest) -> i64 {
    let hbt = unsafe { &*hbt_ptr };
    hbt.current_timestamp()
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_depth(
    hbt_ptr: *const ROIVectorMarketDepthBacktest,
    asset_no: usize,
) -> *const ROIVectorMarketDepth {
    let hbt = unsafe { &*hbt_ptr };
    let depth = hbt.depth(asset_no);
    depth as *const _
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_last_trades(
    hbt_ptr: *const ROIVectorMarketDepthBacktest,
    asset_no: usize,
    len_ptr: *mut usize,
) -> *const Event {
    let hbt = unsafe { &*hbt_ptr };
    let trade = hbt.last_trades(asset_no);
    unsafe {
        *len_ptr = trade.len();
    }
    trade.as_ptr() as *mut _
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_position(
    hbt_ptr: *const ROIVectorMarketDepthBacktest,
    asset_no: usize,
) -> f64 {
    let hbt = unsafe { &*hbt_ptr };
    hbt.position(asset_no)
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_close(hbt_ptr: *mut ROIVectorMarketDepthBacktest) -> i64 {
    let mut hbt = unsafe { Box::from_raw(hbt_ptr) };
    match hbt.close() {
        Ok(()) => 0,
        Err(BacktestError::OrderIdExist) => 10,
        Err(BacktestError::OrderRequestInProcess) => 11,
        Err(BacktestError::OrderNotFound) => 12,
        Err(BacktestError::InvalidOrderRequest) => 13,
        Err(BacktestError::InvalidOrderStatus) => 14,
        Err(BacktestError::EndOfData) => 15,
        Err(BacktestError::DataError(_)) => 100,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_elapse(
    hbt_ptr: *mut ROIVectorMarketDepthBacktest,
    duration: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.elapse(duration))
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_elapse_bt(
    hbt_ptr: *mut ROIVectorMarketDepthBacktest,
    duration: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.elapse_bt(duration))
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_num_assets(hbt_ptr: *const ROIVectorMarketDepthBacktest) -> usize {
    let hbt = unsafe { &*hbt_ptr };
    hbt.num_assets()
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_wait_order_response(
    hbt_ptr: *mut ROIVectorMarketDepthBacktest,
    asset_no: usize,
    order_id: u64,
    timeout: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.wait_order_response(asset_no, order_id, timeout))
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_wait_next_feed(
    hbt_ptr: *mut ROIVectorMarketDepthBacktest,
    include_resp: bool,
    timeout: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.wait_next_feed(include_resp, timeout))
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_submit_buy_order(
    hbt_ptr: *mut ROIVectorMarketDepthBacktest,
    asset_no: usize,
    order_id: u64,
    price: f64,
    qty: f64,
    time_in_force: u8,
    order_type: u8,
    wait: bool,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    let tif = unsafe { mem::transmute::<u8, TimeInForce>(time_in_force) };
    handle_result(hbt.submit_buy_order(
        asset_no,
        order_id,
        price,
        qty,
        tif,
        unsafe { mem::transmute::<u8, OrdType>(order_type) },
        wait,
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_submit_sell_order(
    hbt_ptr: *mut ROIVectorMarketDepthBacktest,
    asset_no: usize,
    order_id: u64,
    price: f64,
    qty: f64,
    time_in_force: u8,
    order_type: u8,
    wait: bool,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.submit_sell_order(
        asset_no,
        order_id,
        price,
        qty,
        unsafe { mem::transmute::<u8, TimeInForce>(time_in_force) },
        unsafe { mem::transmute::<u8, OrdType>(order_type) },
        wait,
    ))
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_modify(
    hbt_ptr: *mut ROIVectorMarketDepthBacktest,
    asset_no: usize,
    order_id: u64,
    price: f64,
    qty: f64,
    wait: bool,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.modify(asset_no, order_id, price, qty, wait))
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_cancel(
    hbt_ptr: *mut ROIVectorMarketDepthBacktest,
    asset_no: usize,
    order_id: u64,
    wait: bool,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.cancel(asset_no, order_id, wait))
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_clear_last_trades(
    hbt_ptr: *mut ROIVectorMarketDepthBacktest,
    asset_no: usize,
) {
    let hbt = unsafe { &mut *hbt_ptr };
    if asset_no == usize::MAX {
        hbt.clear_last_trades(None);
    } else {
        hbt.clear_last_trades(Some(asset_no));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_clear_inactive_orders(
    hbt_ptr: *mut ROIVectorMarketDepthBacktest,
    asset_no: usize,
) {
    let hbt = unsafe { &mut *hbt_ptr };
    if asset_no == usize::MAX {
        hbt.clear_inactive_orders(None);
    } else {
        hbt.clear_inactive_orders(Some(asset_no));
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_orders(
    hbt_ptr: *const ROIVectorMarketDepthBacktest,
    asset_no: usize,
) -> *const HashMap<u64, Order> {
    let hbt = unsafe { &*hbt_ptr };
    hbt.orders(asset_no) as *const _
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_state_values(
    hbt_ptr: *const ROIVectorMarketDepthBacktest,
    asset_no: usize,
) -> *const StateValues {
    let hbt = unsafe { &*hbt_ptr };
    hbt.state_values(asset_no) as *const _
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_feed_latency(
    hbt_ptr: *const ROIVectorMarketDepthBacktest,
    asset_no: usize,
    exch_ts: *mut i64,
    local_ts: *mut i64,
) -> bool {
    let hbt = unsafe { &*hbt_ptr };
    match hbt.feed_latency(asset_no) {
        None => false,
        Some((exch_ts_, local_ts_)) => {
            unsafe {
                *exch_ts = exch_ts_;
                *local_ts = local_ts_;
            }
            true
        },
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecbt_order_latency(
    hbt_ptr: *const ROIVectorMarketDepthBacktest,
    asset_no: usize,
    req_ts: *mut i64,
    exch_ts: *mut i64,
    resp_ts: *mut i64,
) -> bool {
    let hbt = unsafe { &*hbt_ptr };
    match hbt.order_latency(asset_no) {
        None => false,
        Some((req_ts_, exch_ts_, resp_ts_)) => {
            unsafe {
                *req_ts = req_ts_;
                *exch_ts = exch_ts_;
                *resp_ts = resp_ts_;
            }
            true
        },
    }
}
