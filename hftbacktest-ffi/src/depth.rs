use hftbacktest::{depth::HashMapMarketDepth, prelude::MarketDepth};

#[no_mangle]
pub extern "C" fn depth_best_bid_tick(depth_ptr: usize) -> i32 {
    let depth = unsafe { &*(depth_ptr as *const HashMapMarketDepth) };
    depth.best_bid_tick()
}

#[no_mangle]
pub extern "C" fn depth_best_ask_tick(depth_ptr: usize) -> i32 {
    let depth = unsafe { &*(depth_ptr as *const HashMapMarketDepth) };
    depth.best_ask_tick()
}

#[no_mangle]
pub extern "C" fn depth_best_bid(depth_ptr: usize) -> f32 {
    let depth = unsafe { &*(depth_ptr as *const HashMapMarketDepth) };
    depth.best_bid()
}

#[no_mangle]
pub extern "C" fn depth_best_ask(depth_ptr: usize) -> f32 {
    let depth = unsafe { &*(depth_ptr as *const HashMapMarketDepth) };
    depth.best_ask()
}

#[no_mangle]
pub extern "C" fn depth_tick_size(depth_ptr: usize) -> f32 {
    let depth = unsafe { &*(depth_ptr as *const HashMapMarketDepth) };
    depth.tick_size()
}

#[no_mangle]
pub extern "C" fn depth_lot_size(depth_ptr: usize) -> f32 {
    let depth = unsafe { &*(depth_ptr as *const HashMapMarketDepth) };
    depth.lot_size()
}

#[no_mangle]
pub extern "C" fn depth_bid_qty_at_tick(depth_ptr: usize, price_tick: i32) -> f32 {
    let depth = unsafe { &*(depth_ptr as *const HashMapMarketDepth) };
    depth.bid_qty_at_tick(price_tick)
}

#[no_mangle]
pub extern "C" fn depth_ask_qty_at_tick(depth_ptr: usize, price_tick: i32) -> f32 {
    let depth = unsafe { &*(depth_ptr as *const HashMapMarketDepth) };
    depth.ask_qty_at_tick(price_tick)
}
