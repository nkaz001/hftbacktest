use std::collections::HashMap;
use std::mem;
use std::os::raw::c_void;

use hftbacktest::{
    backtest::MultiAssetMultiExchangeBacktest,
    depth::HashMapMarketDepth,
    prelude::{Bot, BotTypedDepth, BotTypedTrade},
    types::{OrdType, TimeInForce},
};
use hftbacktest::prelude::Order;

type Backtest = MultiAssetMultiExchangeBacktest<HashMapMarketDepth>;

#[no_mangle]
pub extern "C" fn hbt_current_timestamp(hbt_ptr: *const Backtest) -> i64 {
    let hbt = unsafe { &*hbt_ptr };
    hbt.current_timestamp()
}

#[no_mangle]
pub extern "C" fn hbt_depth_typed(hbt_ptr: *const Backtest, asset_no: usize) -> usize {
    let hbt = unsafe { &*hbt_ptr };
    let depth = hbt.depth_typed(asset_no);
    depth as *const _ as usize
}

#[no_mangle]
pub extern "C" fn hbt_trade_typed(hbt_ptr: *const Backtest, asset_no: usize, len_ptr: *mut usize) -> *mut c_void {
    let hbt = unsafe { &*hbt_ptr };
    let trade = hbt.trade_typed(asset_no);
    unsafe {
        *len_ptr = trade.len();
    }
    trade.as_ptr() as *mut _
}

#[no_mangle]
pub extern "C" fn hbt_position(hbt_ptr: *const Backtest, asset_no: usize) -> f64 {
    let hbt = unsafe { &*hbt_ptr };
    hbt.position(asset_no)
}

#[no_mangle]
pub extern "C" fn hbt_close(hbt_ptr: *mut Backtest) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    match hbt.close() {
        Ok(()) => 0,
        Err(_) => 1,
    }
}

#[no_mangle]
pub extern "C" fn hbt_elapse(hbt_ptr: *mut Backtest, duration: i64) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    match hbt.elapse(duration) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn hbt_elapse_bt(hbt_ptr: *mut Backtest, duration: i64) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    match hbt.elapse_bt(duration) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn hbt_num_assets(hbt_ptr: *const Backtest) -> usize {
    let hbt = unsafe { &*hbt_ptr };
    hbt.num_assets()
}

#[no_mangle]
pub extern "C" fn hbt_wait_order_response(
    hbt_ptr: *mut Backtest,
    asset_no: usize,
    order_id: i64,
    timeout: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    match hbt.wait_order_response(asset_no, order_id, timeout) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn hbt_wait_next_feed(hbt_ptr: *mut Backtest, include_resp: bool, timeout: i64) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    match hbt.wait_next_feed(include_resp, timeout) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn hbt_submit_buy_order(
    hbt_ptr: *mut Backtest,
    asset_no: usize,
    order_id: i64,
    price: f32,
    qty: f32,
    time_in_force: u8,
    order_type: u8,
    wait: bool,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    let tif = unsafe { mem::transmute::<u8, TimeInForce>(time_in_force) };
    match hbt.submit_buy_order(
        asset_no,
        order_id,
        price,
        qty,
        tif,
        unsafe { mem::transmute::<u8, OrdType>(order_type) },
        wait,
    ) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn hbt_submit_sell_order(
    hbt_ptr: *mut Backtest,
    asset_no: usize,
    order_id: i64,
    price: f32,
    qty: f32,
    time_in_force: u8,
    order_type: u8,
    wait: bool,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    match hbt.submit_sell_order(
        asset_no,
        order_id,
        price,
        qty,
        unsafe { mem::transmute::<u8, TimeInForce>(time_in_force) },
        unsafe { mem::transmute::<u8, OrdType>(order_type) },
        wait,
    ) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn hbt_cancel(hbt_ptr: *mut Backtest, asset_no: usize, order_id: i64, wait: bool) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    match hbt.cancel(asset_no, order_id, wait) {
        Ok(true) => 0,
        Ok(false) => 1,
        Err(_) => -1,
    }
}

#[no_mangle]
pub extern "C" fn hbt_clear_last_trades(hbt_ptr: *mut Backtest, asset_no: usize) {
    let hbt = unsafe { &mut *hbt_ptr };
    if asset_no == usize::MAX {
        hbt.clear_last_trades(None);
    } else {
        hbt.clear_last_trades(Some(asset_no));
    }
}

#[no_mangle]
pub extern "C" fn hbt_clear_inactive_orders(hbt_ptr: *mut Backtest, asset_no: usize) {
    let hbt = unsafe { &mut *hbt_ptr };
    if asset_no == usize::MAX {
        hbt.clear_inactive_orders(None);
    } else {
        hbt.clear_inactive_orders(Some(asset_no));
    }
}

#[no_mangle]
pub extern "C" fn hbt_orders(hbt_ptr: *const Backtest, asset_no: usize) -> *const HashMap<i64, Order> {
    let hbt = unsafe { &*hbt_ptr };
    hbt.orders(asset_no) as *const _
}
