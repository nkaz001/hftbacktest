use hftbacktest::{depth::HashMapMarketDepth, prelude::MarketDepth};

#[no_mangle]
pub extern "C" fn depth_best_bid_tick(depth_ptr: *const HashMapMarketDepth) -> i32 {
    let depth = unsafe { &*depth_ptr };
    depth.best_bid_tick()
}

#[no_mangle]
pub extern "C" fn depth_best_ask_tick(depth_ptr: *const HashMapMarketDepth) -> i32 {
    let depth = unsafe { &*depth_ptr };
    depth.best_ask_tick()
}

#[no_mangle]
pub extern "C" fn depth_best_bid(depth_ptr: *const HashMapMarketDepth) -> f32 {
    let depth = unsafe { &*depth_ptr };
    depth.best_bid()
}

#[no_mangle]
pub extern "C" fn depth_best_ask(depth_ptr: *const HashMapMarketDepth) -> f32 {
    let depth = unsafe { &*depth_ptr };
    depth.best_ask()
}

#[no_mangle]
pub extern "C" fn depth_tick_size(depth_ptr: *const HashMapMarketDepth) -> f32 {
    let depth = unsafe { &*depth_ptr };
    depth.tick_size()
}

#[no_mangle]
pub extern "C" fn depth_lot_size(depth_ptr: *const HashMapMarketDepth) -> f32 {
    let depth = unsafe { &*depth_ptr };
    depth.lot_size()
}

#[no_mangle]
pub extern "C" fn depth_bid_qty_at_tick(depth_ptr: *const HashMapMarketDepth, price_tick: i32) -> f32 {
    let depth = unsafe { &*depth_ptr };
    depth.bid_qty_at_tick(price_tick)
}

#[no_mangle]
pub extern "C" fn depth_ask_qty_at_tick(depth_ptr: *const HashMapMarketDepth, price_tick: i32) -> f32 {
    let depth = unsafe { &*depth_ptr };
    depth.ask_qty_at_tick(price_tick)
}
