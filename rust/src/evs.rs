#[derive(Clone, Copy)]
pub struct Event {
    pub timestamp: i64,
    pub asset_no: usize,
    pub ty: EventType
}

#[derive(Eq, PartialEq, Clone, Copy)]
pub enum EventType {
    LocalData,
    LocalOrder,
    ExchData,
    ExchOrder
}

pub struct EventSet {
    ev: Vec<Event>,
    invalid: usize,
    num_assets: usize,
}

impl EventSet {
    pub fn new(num_assets: usize) -> Self {
        if num_assets == 0 {
            panic!();
        }
        let mut ev = Vec::new();
        for asset_no in 0..num_assets {
            ev.push(Event {
                timestamp: i64::MAX,
                asset_no,
                ty: EventType::LocalData,
            });
            ev.push(Event {
                timestamp: i64::MAX,
                asset_no,
                ty: EventType::LocalOrder,
            });
            ev.push(Event {
                timestamp: i64::MAX,
                asset_no,
                ty: EventType::ExchData,
            });
            ev.push(Event {
                timestamp: i64::MAX,
                asset_no,
                ty: EventType::ExchOrder,
            });
        }
        Self {
            ev,
            invalid: 0,
            num_assets,
        }
    }

    pub fn next(&self) -> Option<Event> {
        if self.invalid == self.num_assets {
            return None;
        }
        let mut r = unsafe { *self.ev.get_unchecked(0) };
        for ev in self.ev[1..].iter() {
            if ev.timestamp < r.timestamp {
                r = *ev;
            }
        }
        Some(r)
    }

    fn update(&mut self, evst_no: usize, timestamp: i64) {
        let item = unsafe {
            self.ev.get_unchecked_mut(evst_no)
        };
        item.timestamp = timestamp;
    }

    pub fn update_local_data(&mut self, asset_no: usize, timestamp: i64) {
        self.update(4 * asset_no, timestamp);
    }

    pub fn update_local_order(&mut self, asset_no: usize, timestamp: i64) {
        self.update(4 * asset_no + 1, timestamp);
    }

    pub fn update_exch_data(&mut self, asset_no: usize, timestamp: i64) {
        self.update(4 * asset_no + 2, timestamp);
    }

    pub fn update_exch_order(&mut self, asset_no: usize, timestamp: i64) {
        self.update(4 * asset_no + 3, timestamp);
    }

    fn invalidate(&mut self, evst_no: usize) {
        let item = unsafe {
            self.ev.get_unchecked_mut(evst_no)
        };
        item.timestamp = i64::MAX;
        self.invalid += 1;
    }

    pub fn invalidate_local_data(&mut self, asset_no: usize) {
        self.invalidate(4 * asset_no);
    }

    pub fn invalidate_exch_data(&mut self, asset_no: usize) {
        self.invalidate(4 * asset_no + 2);
    }
}