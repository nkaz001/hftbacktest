#![allow(clippy::not_unsafe_ptr_arg_deref)]

use std::{
    collections::{HashMap, hash_map::Values},
    os::raw::c_void,
    ptr::null,
};

use hftbacktest::prelude::Order;

#[unsafe(no_mangle)]
pub extern "C" fn orders_get(ptr: *const HashMap<u64, Order>, order_id: u64) -> *const Order {
    let orders = unsafe { &*ptr };
    match orders.get(&order_id) {
        None => null(),
        Some(order) => order as *const _,
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn orders_contains(ptr: *const HashMap<u64, Order>, order_id: u64) -> bool {
    let orders = unsafe { &*ptr };
    orders.contains_key(&order_id)
}

#[unsafe(no_mangle)]
pub extern "C" fn orders_len(ptr: *const HashMap<u64, Order>) -> usize {
    let orders = unsafe { &*ptr };
    orders.len()
}

#[unsafe(no_mangle)]
pub extern "C" fn orders_values(ptr: *const HashMap<u64, Order>) -> *mut c_void {
    let orders = unsafe { &*ptr };
    let values = orders.values();
    let boxed = Box::new(values);
    Box::into_raw(boxed) as *mut _
}

#[unsafe(no_mangle)]
pub extern "C" fn orders_values_next(ptr: *mut Values<u64, Order>) -> *const Order {
    let values = unsafe { &mut *ptr };
    match values.next() {
        None => {
            let _ = unsafe { Box::from_raw(ptr) };
            null()
        },
        Some(order) => order as *const _,
    }
}
