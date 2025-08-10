#![allow(clippy::not_unsafe_ptr_arg_deref)]

use hftbacktest::{
    depth::FusedHashMapMarketDepth,
    prelude::Event,
    types::{
        BUY_EVENT,
        DEPTH_BBO_EVENT,
        DEPTH_CLEAR_EVENT,
        DEPTH_EVENT,
        DEPTH_SNAPSHOT_EVENT,
        SELL_EVENT,
        Side,
    },
};

pub struct FuseMarketDepth {
    fused: Vec<Event>,
    depth: FusedHashMapMarketDepth,
}

#[unsafe(no_mangle)]
pub extern "C" fn fusemarketdepth_new(tick_size: f64, lot_size: f64) -> *mut FuseMarketDepth {
    let boxed = Box::new(FuseMarketDepth {
        fused: Default::default(),
        depth: FusedHashMapMarketDepth::new(tick_size, lot_size),
    });
    Box::into_raw(boxed)
}

#[unsafe(no_mangle)]
pub extern "C" fn fusemarketdepth_free(slf: *mut FuseMarketDepth) {
    if !slf.is_null() {
        unsafe {
            drop(Box::from_raw(slf));
        }
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn fusemarketdepth_process_event(
    slf: *mut FuseMarketDepth,
    ev: *const Event,
    add: bool,
) -> bool {
    let slf = unsafe { &mut *slf };
    let mut ev = unsafe { &*ev }.clone();
    if ev.is(DEPTH_EVENT) | ev.is(DEPTH_SNAPSHOT_EVENT) {
        let mut evs = if ev.is(BUY_EVENT) {
            slf.depth.update_bid_depth(ev)
        } else if ev.is(SELL_EVENT) {
            slf.depth.update_ask_depth(ev)
        } else {
            return false;
        };
        if add {
            slf.fused.append(&mut evs);
        }
    } else if ev.is(DEPTH_CLEAR_EVENT) {
        if ev.is(BUY_EVENT) {
            slf.depth.clear_depth(Side::Buy, ev.px, ev.exch_ts);
        } else if ev.is(SELL_EVENT) {
            slf.depth.clear_depth(Side::Sell, ev.px, ev.exch_ts);
        } else {
            slf.depth.clear_depth(Side::None, 0.0, ev.exch_ts);
        }
        if add {
            slf.fused.push(ev);
        }
    } else if ev.is(DEPTH_BBO_EVENT) {
        ev.ev = (ev.ev & !DEPTH_BBO_EVENT) | DEPTH_EVENT;
        let mut evs = if ev.is(BUY_EVENT) {
            slf.depth.update_best_bid(ev)
        } else if ev.is(SELL_EVENT) {
            slf.depth.update_best_ask(ev)
        } else {
            return false;
        };
        if add {
            slf.fused.append(&mut evs);
        }
    } else {
        return false;
    }
    true
}

#[unsafe(no_mangle)]
pub extern "C" fn fusemarketdepth_fused_events(
    slf: *mut FuseMarketDepth,
    len: *mut usize,
) -> *const Event {
    let slf = unsafe { &mut *slf };
    unsafe { *len = slf.fused.len() }
    slf.fused.as_ptr()
}
