use std::{
    collections::{hash_map::Values, HashMap},
    os::raw::c_void,
    ptr::null,
};

use hftbacktest::prelude::Order;

#[no_mangle]
pub extern "C" fn orders_get(orders: *const HashMap<i64, Order>, order_id: i64) -> *const Order {
    let orders = unsafe { &*orders };
    match orders.get(&order_id) {
        None => null(),
        Some(order) => order as *const _,
    }
}

#[no_mangle]
pub extern "C" fn orders_contains(orders: *const HashMap<i64, Order>, order_id: i64) -> bool {
    let orders = unsafe { &*orders };
    orders.contains_key(&order_id)
}

#[no_mangle]
pub extern "C" fn orders_len(orders: *const HashMap<i64, Order>) -> usize {
    let orders = unsafe { &*orders };
    orders.len()
}

#[no_mangle]
pub extern "C" fn orders_values(orders: *const HashMap<i64, Order>) -> *mut c_void {
    let orders = unsafe { &*orders };
    let it = orders.values();
    let vit = Box::new(it);
    Box::into_raw(vit) as *mut _
}

#[no_mangle]
pub extern "C" fn orders_values_next(it: *mut Values<i64, Order>) -> *const Order {
    let vit = unsafe { &mut *it };
    match vit.next() {
        None => {
            let _ = unsafe { Box::from_raw(it) };
            null()
        }
        Some(order) => order as *const _,
    }
}
