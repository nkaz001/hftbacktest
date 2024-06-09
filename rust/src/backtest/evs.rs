use std::{
    mem,
    mem::{forget, size_of},
};

#[derive(Clone, Copy)]
#[repr(C, align(32))]
pub(crate) struct Event {
    pub timestamp: i64,
    pub asset_no: usize,
    pub ty: EventType,
}

// This is constructed by using transmute in `EventSet::next`.
#[allow(dead_code)]
#[derive(Eq, PartialEq, Clone, Copy)]
#[repr(usize)]
pub enum EventType {
    LocalData = 0,
    LocalOrder = 1,
    ExchData = 2,
    ExchOrder = 3,
}

#[repr(C, align(64))]
struct Align64([u8; 64]);

fn aligned_vec_i64(count: usize) -> Box<[i64]> {
    let capacity = (count * 8 / size_of::<Align64>()) + 1;
    let mut aligned: Vec<Align64> = Vec::with_capacity(capacity);

    let ptr = aligned.as_mut_ptr();
    let cap = aligned.capacity();

    forget(aligned);

    unsafe {
        Vec::from_raw_parts(ptr as *mut i64, count, cap * size_of::<Align64>() / 8)
            .into_boxed_slice()
    }
}

/// Manages the event timestamps to determine the next event to be processed.
pub struct EventSet {
    timestamp: Box<[i64]>,
    invalid: usize,
    num_assets: usize,
}

impl EventSet {
    /// Constructs an instance of `EventSet`.
    pub fn new(num_assets: usize) -> Self {
        if num_assets == 0 {
            panic!();
        }
        let mut timestamp = aligned_vec_i64(num_assets * 4);
        for i in 0..(num_assets * 4) {
            timestamp[i] = i64::MAX;
        }
        Self {
            timestamp,
            invalid: 0,
            num_assets,
        }
    }

    /// Returns the next event to be processed, which has the earliest timestamp.
    pub fn next(&self) -> Option<Event> {
        if self.invalid == self.num_assets {
            return None;
        }
        let mut evst_no = 0;
        let mut timestamp = unsafe { *self.timestamp.get_unchecked(0) };
        for (i, &ev_timestamp) in self.timestamp[1..].iter().enumerate() {
            if ev_timestamp < timestamp {
                timestamp = ev_timestamp;
                evst_no = i + 1;
            }
        }
        let asset_no = evst_no >> 2;
        let ty = unsafe { mem::transmute(evst_no & 3) };
        Some(Event {
            timestamp,
            asset_no,
            ty,
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
        self.invalid += 1;
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
