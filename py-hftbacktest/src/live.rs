#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::{collections::HashMap, mem};

use hftbacktest::{
    depth::{HashMapMarketDepth, ROIVectorMarketDepth},
    live::{BotError, LiveBot, ipc::iceoryx::IceoryxUnifiedChannel},
    prelude::{Bot, ElapseResult, Event, Order, StateValues},
    types::{OrdType, TimeInForce},
};

pub type HashMapMarketDepthLiveBot = LiveBot<IceoryxUnifiedChannel, HashMapMarketDepth>;
pub type ROIVectorMarketDepthLiveBot = LiveBot<IceoryxUnifiedChannel, ROIVectorMarketDepth>;

fn handle_result(result: Result<ElapseResult, BotError>) -> i64 {
    match result {
        Ok(ElapseResult::Ok) => 0,
        Ok(ElapseResult::EndOfData) => 1,
        Ok(ElapseResult::MarketFeed) => 2,
        Ok(ElapseResult::OrderResponse) => 3,
        Err(BotError::OrderIdExist) => 10,
        Err(BotError::OrderNotFound) => 12,
        Err(BotError::InvalidOrderStatus) => 14,
        Err(BotError::InstrumentNotFound) => 16,
        Err(BotError::Timeout) => 17,
        Err(BotError::Interrupted) => 18,
        Err(BotError::Custom(error)) => {
            println!("BotError::Custom: {error:?}");
            19
        },
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmaplive_current_timestamp(hbt_ptr: *const HashMapMarketDepthLiveBot) -> i64 {
    let hbt = unsafe { &*hbt_ptr };
    hbt.current_timestamp()
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmaplive_depth(
    hbt_ptr: *const HashMapMarketDepthLiveBot,
    asset_no: usize,
) -> *const HashMapMarketDepth {
    let hbt = unsafe { &*hbt_ptr };
    let depth = hbt.depth(asset_no);
    depth as *const _
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmaplive_last_trades(
    hbt_ptr: *const HashMapMarketDepthLiveBot,
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
pub extern "C" fn hashmaplive_position(
    hbt_ptr: *const HashMapMarketDepthLiveBot,
    asset_no: usize,
) -> f64 {
    let hbt = unsafe { &*hbt_ptr };
    hbt.position(asset_no)
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmaplive_close(hbt_ptr: *mut HashMapMarketDepthLiveBot) -> i64 {
    let mut hbt = unsafe { Box::from_raw(hbt_ptr) };
    match hbt.close() {
        Ok(()) => 0,
        Err(BotError::OrderIdExist) => 10,
        Err(BotError::OrderNotFound) => 12,
        Err(BotError::InvalidOrderStatus) => 14,
        Err(BotError::InstrumentNotFound) => 16,
        Err(BotError::Timeout) => 17,
        Err(BotError::Interrupted) => 18,
        Err(BotError::Custom(_)) => 19,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmaplive_elapse(
    hbt_ptr: *mut HashMapMarketDepthLiveBot,
    duration: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.elapse(duration))
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmaplive_elapse_bt(
    hbt_ptr: *mut HashMapMarketDepthLiveBot,
    duration: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.elapse_bt(duration))
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmaplive_num_assets(hbt_ptr: *const HashMapMarketDepthLiveBot) -> usize {
    let hbt = unsafe { &*hbt_ptr };
    hbt.num_assets()
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmaplive_wait_order_response(
    hbt_ptr: *mut HashMapMarketDepthLiveBot,
    asset_no: usize,
    order_id: u64,
    timeout: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.wait_order_response(asset_no, order_id, timeout))
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmaplive_wait_next_feed(
    hbt_ptr: *mut HashMapMarketDepthLiveBot,
    include_resp: bool,
    timeout: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.wait_next_feed(include_resp, timeout))
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmaplive_submit_buy_order(
    hbt_ptr: *mut HashMapMarketDepthLiveBot,
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
pub extern "C" fn hashmaplive_submit_sell_order(
    hbt_ptr: *mut HashMapMarketDepthLiveBot,
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
pub extern "C" fn hashmaplive_cancel(
    hbt_ptr: *mut HashMapMarketDepthLiveBot,
    asset_no: usize,
    order_id: u64,
    wait: bool,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.cancel(asset_no, order_id, wait))
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmaplive_clear_last_trades(
    hbt_ptr: *mut HashMapMarketDepthLiveBot,
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
pub extern "C" fn hashmaplive_clear_inactive_orders(
    hbt_ptr: *mut HashMapMarketDepthLiveBot,
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
pub extern "C" fn hashmaplive_orders(
    hbt_ptr: *const HashMapMarketDepthLiveBot,
    asset_no: usize,
) -> *const HashMap<u64, Order> {
    let hbt = unsafe { &*hbt_ptr };
    hbt.orders(asset_no) as *const _
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmaplive_state_values(
    hbt_ptr: *const HashMapMarketDepthLiveBot,
    asset_no: usize,
) -> *const StateValues {
    let hbt = unsafe { &*hbt_ptr };
    hbt.state_values(asset_no) as *const _
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmaplive_feed_latency(
    hbt_ptr: *const HashMapMarketDepthLiveBot,
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
pub extern "C" fn hashmaplive_order_latency(
    hbt_ptr: *const HashMapMarketDepthLiveBot,
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
pub extern "C" fn roiveclive_current_timestamp(hbt_ptr: *const ROIVectorMarketDepthLiveBot) -> i64 {
    let hbt = unsafe { &*hbt_ptr };
    hbt.current_timestamp()
}

#[unsafe(no_mangle)]
pub extern "C" fn roiveclive_depth(
    hbt_ptr: *const ROIVectorMarketDepthLiveBot,
    asset_no: usize,
) -> *const ROIVectorMarketDepth {
    let hbt = unsafe { &*hbt_ptr };
    let depth = hbt.depth(asset_no);
    depth as *const _
}

#[unsafe(no_mangle)]
pub extern "C" fn roiveclive_last_trades(
    hbt_ptr: *const ROIVectorMarketDepthLiveBot,
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
pub extern "C" fn roiveclive_position(
    hbt_ptr: *const ROIVectorMarketDepthLiveBot,
    asset_no: usize,
) -> f64 {
    let hbt = unsafe { &*hbt_ptr };
    hbt.position(asset_no)
}

#[unsafe(no_mangle)]
pub extern "C" fn roiveclive_close(hbt_ptr: *mut ROIVectorMarketDepthLiveBot) -> i64 {
    let mut hbt = unsafe { Box::from_raw(hbt_ptr) };
    match hbt.close() {
        Ok(()) => 0,
        Err(BotError::OrderIdExist) => 10,
        Err(BotError::OrderNotFound) => 12,
        Err(BotError::InvalidOrderStatus) => 14,
        Err(BotError::InstrumentNotFound) => 16,
        Err(BotError::Timeout) => 17,
        Err(BotError::Interrupted) => 18,
        Err(BotError::Custom(_)) => 19,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn roiveclive_elapse(
    hbt_ptr: *mut ROIVectorMarketDepthLiveBot,
    duration: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.elapse(duration))
}

#[unsafe(no_mangle)]
pub extern "C" fn roiveclive_elapse_bt(
    hbt_ptr: *mut ROIVectorMarketDepthLiveBot,
    duration: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.elapse_bt(duration))
}

#[unsafe(no_mangle)]
pub extern "C" fn roiveclive_num_assets(hbt_ptr: *const ROIVectorMarketDepthLiveBot) -> usize {
    let hbt = unsafe { &*hbt_ptr };
    hbt.num_assets()
}

#[unsafe(no_mangle)]
pub extern "C" fn roiveclive_wait_order_response(
    hbt_ptr: *mut ROIVectorMarketDepthLiveBot,
    asset_no: usize,
    order_id: u64,
    timeout: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.wait_order_response(asset_no, order_id, timeout))
}

#[unsafe(no_mangle)]
pub extern "C" fn roiveclive_wait_next_feed(
    hbt_ptr: *mut ROIVectorMarketDepthLiveBot,
    include_resp: bool,
    timeout: i64,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.wait_next_feed(include_resp, timeout))
}

#[unsafe(no_mangle)]
pub extern "C" fn roiveclive_submit_buy_order(
    hbt_ptr: *mut ROIVectorMarketDepthLiveBot,
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
pub extern "C" fn roiveclive_submit_sell_order(
    hbt_ptr: *mut ROIVectorMarketDepthLiveBot,
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
pub extern "C" fn roiveclive_cancel(
    hbt_ptr: *mut ROIVectorMarketDepthLiveBot,
    asset_no: usize,
    order_id: u64,
    wait: bool,
) -> i64 {
    let hbt = unsafe { &mut *hbt_ptr };
    handle_result(hbt.cancel(asset_no, order_id, wait))
}

#[unsafe(no_mangle)]
pub extern "C" fn roiveclive_clear_last_trades(
    hbt_ptr: *mut ROIVectorMarketDepthLiveBot,
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
pub extern "C" fn roiveclive_clear_inactive_orders(
    hbt_ptr: *mut ROIVectorMarketDepthLiveBot,
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
pub extern "C" fn roiveclive_orders(
    hbt_ptr: *const ROIVectorMarketDepthLiveBot,
    asset_no: usize,
) -> *const HashMap<u64, Order> {
    let hbt = unsafe { &*hbt_ptr };
    hbt.orders(asset_no) as *const _
}

#[unsafe(no_mangle)]
pub extern "C" fn roiveclive_state_values(
    hbt_ptr: *const ROIVectorMarketDepthLiveBot,
    asset_no: usize,
) -> *const StateValues {
    let hbt = unsafe { &*hbt_ptr };
    hbt.state_values(asset_no) as *const _
}

#[unsafe(no_mangle)]
pub extern "C" fn roiveclive_feed_latency(
    hbt_ptr: *const ROIVectorMarketDepthLiveBot,
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
pub extern "C" fn roiveclive_order_latency(
    hbt_ptr: *const ROIVectorMarketDepthLiveBot,
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
