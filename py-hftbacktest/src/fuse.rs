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
        let output_len = slf.fused.len();
        if ev.is(BUY_EVENT) {
            slf.depth.update_bid_depth_into(ev, &mut slf.fused);
        } else if ev.is(SELL_EVENT) {
            slf.depth.update_ask_depth_into(ev, &mut slf.fused);
        } else {
            return false;
        }
        if !add {
            slf.fused.truncate(output_len);
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

#[cfg(test)]
mod tests {
    use super::*;

    fn bid_event(qty: f64, exch_ts: i64) -> Event {
        Event {
            ev: BUY_EVENT | DEPTH_EVENT,
            exch_ts,
            local_ts: exch_ts + 1,
            px: 100.0,
            qty,
            order_id: 0,
            ival: 0,
            fval: 0.0,
        }
    }

    #[test]
    fn process_event_reuses_output_without_changing_add_semantics() {
        let depth = fusemarketdepth_new(0.1, 0.01);

        let first = bid_event(1.0, 1);
        assert!(fusemarketdepth_process_event(depth, &first, true));
        let mut len = 0;
        fusemarketdepth_fused_events(depth, &mut len);
        assert_eq!(len, 1);

        let discarded = bid_event(2.0, 10);
        assert!(fusemarketdepth_process_event(depth, &discarded, false));
        fusemarketdepth_fused_events(depth, &mut len);
        assert_eq!(len, 1);

        let stale = bid_event(3.0, 5);
        assert!(fusemarketdepth_process_event(depth, &stale, true));
        fusemarketdepth_fused_events(depth, &mut len);
        assert_eq!(len, 1);

        let last = bid_event(3.0, 11);
        assert!(fusemarketdepth_process_event(depth, &last, true));
        fusemarketdepth_fused_events(depth, &mut len);
        assert_eq!(len, 2);

        let invalid = Event {
            ev: DEPTH_EVENT,
            ..bid_event(4.0, 12)
        };
        assert!(!fusemarketdepth_process_event(depth, &invalid, true));
        fusemarketdepth_fused_events(depth, &mut len);
        assert_eq!(len, 2);

        fusemarketdepth_free(depth);
    }
}
