use std::mem;

use crate::utils::{AlignedArray, CACHE_LINE_SIZE};

#[derive(Clone, Copy)]
#[repr(C, align(32))]
pub struct EventIntent {
    pub timestamp: i64,
    pub asset_no: usize,
    pub kind: EventIntentKind,
}

/// This is constructed by using transmute in `EventSet::next`.
#[allow(dead_code)]
#[derive(Eq, PartialEq, Clone, Copy)]
#[repr(usize)]
pub enum EventIntentKind {
    LocalData = 0,
    LocalOrder = 1,
    ExchData = 2,
    ExchOrder = 3,
}

/// Manages the event timestamps to determine the next event to be processed.
pub struct EventSet {
    timestamp: AlignedArray<i64, CACHE_LINE_SIZE>,
}

impl EventSet {
    /// Constructs an instance of `EventSet`.
    pub fn new(num_assets: usize) -> Self {
        if num_assets == 0 {
            panic!();
        }
        let mut timestamp = AlignedArray::<i64, CACHE_LINE_SIZE>::new(num_assets * 4);
        for i in 0..(num_assets * 4) {
            timestamp[i] = i64::MAX;
        }
        Self { timestamp }
    }

    /// Returns the next event to be processed, which has the earliest timestamp.
    pub fn next(&self) -> Option<EventIntent> {
        let mut evst_no = 0;
        let mut timestamp = unsafe { *self.timestamp.get_unchecked(0) };
        for (i, &ev_timestamp) in self.timestamp[1..].iter().enumerate() {
            if ev_timestamp < timestamp {
                timestamp = ev_timestamp;
                evst_no = i + 1;
            }
        }
        // Returns None if no valid events are found.
        if timestamp == i64::MAX {
            return None;
        }
        let asset_no = evst_no >> 2;
        let kind = unsafe { mem::transmute::<usize, EventIntentKind>(evst_no & 3) };
        Some(EventIntent {
            timestamp,
            asset_no,
            kind,
        })
    }

    #[inline]
    fn update(&mut self, evst_no: usize, timestamp: i64) {
        let item = unsafe { self.timestamp.get_unchecked_mut(evst_no) };
        *item = timestamp;
    }

    #[inline]
    pub fn update_local_data(&mut self, asset_no: usize, timestamp: i64) {
        self.update(4 * asset_no, timestamp);
    }

    #[inline]
    pub fn update_local_order(&mut self, asset_no: usize, timestamp: i64) {
        self.update(4 * asset_no + 1, timestamp);
    }

    #[inline]
    pub fn update_exch_data(&mut self, asset_no: usize, timestamp: i64) {
        self.update(4 * asset_no + 2, timestamp);
    }

    #[inline]
    pub fn update_exch_order(&mut self, asset_no: usize, timestamp: i64) {
        self.update(4 * asset_no + 3, timestamp);
    }

    #[inline]
    fn invalidate(&mut self, evst_no: usize) {
        let item = unsafe { self.timestamp.get_unchecked_mut(evst_no) };
        *item = i64::MAX;
    }

    #[inline]
    pub fn invalidate_local_data(&mut self, asset_no: usize) {
        self.invalidate(4 * asset_no);
    }

    #[inline]
    pub fn invalidate_exch_data(&mut self, asset_no: usize) {
        self.invalidate(4 * asset_no + 2);
    }
}
