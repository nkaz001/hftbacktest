#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::mem::forget;

use hftbacktest::prelude::{
    ApplySnapshot,
    Event,
    HashMapMarketDepth,
    MarketDepth,
    ROIVectorMarketDepth,
};

#[unsafe(no_mangle)]
pub extern "C" fn hashmapdepth_best_bid_tick(ptr: *const HashMapMarketDepth) -> i64 {
    let depth = unsafe { &*ptr };
    depth.best_bid_tick()
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapdepth_best_ask_tick(ptr: *const HashMapMarketDepth) -> i64 {
    let depth = unsafe { &*ptr };
    depth.best_ask_tick()
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapdepth_best_bid(ptr: *const HashMapMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.best_bid()
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapdepth_best_ask(ptr: *const HashMapMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.best_ask()
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapdepth_best_bid_qty(ptr: *const HashMapMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.best_bid_qty()
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapdepth_best_ask_qty(ptr: *const HashMapMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.best_ask_qty()
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapdepth_tick_size(ptr: *const HashMapMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.tick_size()
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapdepth_lot_size(ptr: *const HashMapMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.lot_size()
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapdepth_bid_qty_at_tick(
    ptr: *const HashMapMarketDepth,
    price_tick: i64,
) -> f64 {
    let depth = unsafe { &*ptr };
    depth.bid_qty_at_tick(price_tick)
}

#[unsafe(no_mangle)]
pub extern "C" fn hashmapdepth_ask_qty_at_tick(
    ptr: *const HashMapMarketDepth,
    price_tick: i64,
) -> f64 {
    let depth = unsafe { &*ptr };
    depth.ask_qty_at_tick(price_tick)
}

#[unsafe(no_mangle)]
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

#[unsafe(no_mangle)]
pub extern "C" fn hashmapdepth_snapshot_free(event_ptr: *mut Event, len: usize) {
    let _ = unsafe { Vec::from_raw_parts(event_ptr, len, len) };
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecdepth_best_bid_tick(ptr: *const ROIVectorMarketDepth) -> i64 {
    let depth = unsafe { &*ptr };
    depth.best_bid_tick()
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecdepth_best_ask_tick(ptr: *const ROIVectorMarketDepth) -> i64 {
    let depth = unsafe { &*ptr };
    depth.best_ask_tick()
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecdepth_best_bid(ptr: *const ROIVectorMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.best_bid()
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecdepth_best_ask(ptr: *const ROIVectorMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.best_ask()
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecdepth_best_bid_qty(ptr: *const ROIVectorMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.best_bid_qty()
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecdepth_best_ask_qty(ptr: *const ROIVectorMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.best_ask_qty()
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecdepth_tick_size(ptr: *const ROIVectorMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.tick_size()
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecdepth_lot_size(ptr: *const ROIVectorMarketDepth) -> f64 {
    let depth = unsafe { &*ptr };
    depth.lot_size()
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecdepth_bid_qty_at_tick(
    ptr: *const ROIVectorMarketDepth,
    price_tick: i64,
) -> f64 {
    let depth = unsafe { &*ptr };
    depth.bid_qty_at_tick(price_tick)
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecdepth_ask_qty_at_tick(
    ptr: *const ROIVectorMarketDepth,
    price_tick: i64,
) -> f64 {
    let depth = unsafe { &*ptr };
    depth.ask_qty_at_tick(price_tick)
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecdepth_bid_depth(
    ptr: *const ROIVectorMarketDepth,
    len: *mut usize,
) -> *const f64 {
    let depth = unsafe { &*ptr };
    unsafe { *len = depth.bid_depth().len() }
    depth.bid_depth().as_ptr()
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecdepth_ask_depth(
    ptr: *const ROIVectorMarketDepth,
    len: *mut usize,
) -> *const f64 {
    let depth = unsafe { &*ptr };
    unsafe { *len = depth.ask_depth().len() }
    depth.ask_depth().as_ptr()
}
#[unsafe(no_mangle)]
pub extern "C" fn roivecdepth_roi_lb_tick(ptr: *const ROIVectorMarketDepth) -> i64 {
    let depth = unsafe { &*ptr };
    depth.roi_tick().0
}

#[unsafe(no_mangle)]
pub extern "C" fn roivecdepth_roi_ub_tick(ptr: *const ROIVectorMarketDepth) -> i64 {
    let depth = unsafe { &*ptr };
    depth.roi_tick().1
}
