#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::mem::forget;

use hftbacktest::{
    depth::HashMapMarketDepth,
    prelude::{ApplySnapshot, Event, MarketDepth, ROIVectorMarketDepth},
};

#[no_mangle]
pub extern "C" fn hashmapdepth_best_bid_tick(ptr: *const HashMapMarketDepth) -> i64 {
    let depth = unsafe { &*ptr };
    depth.best_bid_tick()
}

#[no_mangle]
pub extern "C" fn hashmapdepth_best_ask_tick(ptr: *const HashMapMarketDepth) -> i64 {
    let depth = unsafe { &*ptr };
    depth.best_ask_tick()
}

#[no_mangle]
pub extern "C" fn hashmapdepth_best_bid(ptr: *const HashMapMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.best_bid()
}

#[no_mangle]
pub extern "C" fn hashmapdepth_best_ask(ptr: *const HashMapMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.best_ask()
}

#[no_mangle]
pub extern "C" fn hashmapdepth_tick_size(ptr: *const HashMapMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.tick_size()
}

#[no_mangle]
pub extern "C" fn hashmapdepth_lot_size(ptr: *const HashMapMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.lot_size()
}

#[no_mangle]
pub extern "C" fn hashmapdepth_bid_qty_at_tick(
    ptr: *const HashMapMarketDepth,
    price_tick: i64,
) -> f64 {
    let depth = unsafe { &*ptr };
    depth.bid_qty_at_tick(price_tick)
}

#[no_mangle]
pub extern "C" fn hashmapdepth_ask_qty_at_tick(
    ptr: *const HashMapMarketDepth,
    price_tick: i64,
) -> f64 {
    let depth = unsafe { &*ptr };
    depth.ask_qty_at_tick(price_tick)
}

#[no_mangle]
pub extern "C" fn hashmapdepth_snapshot(
    ptr: *const HashMapMarketDepth,
    len: *mut usize,
) -> *const Event {
    let depth = unsafe { &*ptr };
    let mut snapshot = depth.snapshot();
    snapshot.shrink_to_fit();
    let ptr = snapshot.as_ptr();
    unsafe {
        *len = snapshot.len();
        forget(snapshot);
    }
    ptr
}

#[no_mangle]
pub extern "C" fn hashmapdepth_snapshot_free(event_ptr: *mut Event, len: usize) {
    let _ = unsafe { Vec::from_raw_parts(event_ptr, len, len) };
}

#[no_mangle]
pub extern "C" fn roivecdepth_best_bid_tick(ptr: *const ROIVectorMarketDepth) -> i64 {
    let depth = unsafe { &*ptr };
    depth.best_bid_tick()
}

#[no_mangle]
pub extern "C" fn roivecdepth_best_ask_tick(ptr: *const ROIVectorMarketDepth) -> i64 {
    let depth = unsafe { &*ptr };
    depth.best_ask_tick()
}

#[no_mangle]
pub extern "C" fn roivecdepth_best_bid(ptr: *const ROIVectorMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.best_bid()
}

#[no_mangle]
pub extern "C" fn roivecdepth_best_ask(ptr: *const ROIVectorMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.best_ask()
}

#[no_mangle]
pub extern "C" fn roivecdepth_tick_size(ptr: *const ROIVectorMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.tick_size()
}

#[no_mangle]
pub extern "C" fn roivecdepth_lot_size(ptr: *const ROIVectorMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.lot_size()
}

#[no_mangle]
pub extern "C" fn roivecdepth_bid_qty_at_tick(
    ptr: *const ROIVectorMarketDepth,
    price_tick: i64,
) -> f64 {
    let depth = unsafe { &*ptr };
    depth.bid_qty_at_tick(price_tick)
}

#[no_mangle]
pub extern "C" fn roivecdepth_ask_qty_at_tick(
    ptr: *const ROIVectorMarketDepth,
    price_tick: i64,
) -> f64 {
    let depth = unsafe { &*ptr };
    depth.ask_qty_at_tick(price_tick)
}

#[no_mangle]
pub extern "C" fn roivecdepth_bid_depth(
    ptr: *const ROIVectorMarketDepth,
    len: *mut usize,
) -> *const f64 {
    let depth = unsafe { &*ptr };
    unsafe { *len = depth.bid_depth().len() }
    depth.bid_depth().as_ptr()
}

#[no_mangle]
pub extern "C" fn roivecdepth_ask_depth(
    ptr: *const ROIVectorMarketDepth,
    len: *mut usize,
) -> *const f64 {
    let depth = unsafe { &*ptr };
    unsafe { *len = depth.ask_depth().len() }
    depth.ask_depth().as_ptr()
}
