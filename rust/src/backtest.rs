use std::collections::HashMap;
use std::marker::PhantomData;
use crate::assettype::AssetType;
use crate::depth::MarketDepth;
use crate::Error;
use crate::evs::{EventSet, EventType};
use crate::order::{Order, OrdType, Side, TimeInForce};
use crate::proc::{LocalProcessor, Processor};
use crate::reader::{UNTIL_END_OF_DATA, WAIT_ORDER_RESPONSE_NONE};

pub struct HftBacktest<AT: AssetType, Q: Clone, L: Processor + LocalProcessor<AT, Q>> {
    cur_ts: i64,
    evs: EventSet,
    local: Vec<L>,
    exch: Vec<Box<dyn Processor>>,
    _at: PhantomData<AT>,
    _q: PhantomData<Q>
}

impl<AT: AssetType, Q: Clone, L: Processor + LocalProcessor<AT, Q>> HftBacktest<AT, Q, L> {
    pub fn new(
        local: Vec<L>,
        exch: Vec<Box<dyn Processor>>
    ) -> Self
    {
        let num_assets = local.len();
        if local.len() != num_assets || exch.len() != num_assets {
            panic!();
        }
        Self {
            cur_ts: i64::MAX,
            evs: EventSet::new(num_assets),
            local,
            exch,
            _at: Default::default(),
            _q: Default::default(),
        }
    }

    pub fn current_timestamp(&self) -> i64 {
        self.cur_ts
    }

    pub fn position(&self, asset_no: usize) -> f64 {
        self.local.get(asset_no).unwrap().state().position
    }

    pub fn depth(&self, asset_no: usize) -> &MarketDepth {
        &self.local.get(asset_no).unwrap().depth()
    }

    pub fn orders(&self, asset_no: usize) -> &HashMap<i64, Order<Q>> {
        &self.local.get(asset_no).unwrap().orders()
    }

    pub fn submit_buy_order(
        &mut self,
        asset_no: usize,
        order_id: i64,
        price: f32,
        qty: f32,
        time_in_force: TimeInForce,
        order_type: OrdType,
        wait: bool
    ) -> Result<bool, Error>
    {
        let local = self.local.get_mut(asset_no).unwrap();
        local.submit_order(
            order_id,
            Side::Buy,
            price,
            qty,
            order_type,
            time_in_force,
            self.cur_ts
        )?;
        self.evs.update_exch_order(asset_no, local.frontmost_send_order_timestamp());

        if wait {
            return self.goto(UNTIL_END_OF_DATA, order_id)
        }
        Ok(true)
    }

    pub fn submit_sell_order(
        &mut self,
        asset_no: usize,
        order_id: i64,
        price: f32,
        qty: f32,
        time_in_force: TimeInForce,
        order_type: OrdType,
        wait: bool
    ) -> Result<bool, Error>
    {
        let local = self.local.get_mut(asset_no).unwrap();
        local.submit_order(
            order_id,
            Side::Sell,
            price,
            qty,
            order_type,
            time_in_force,
            self.cur_ts
        )?;
        self.evs.update_exch_order(asset_no, local.frontmost_send_order_timestamp());

        if wait {
            return self.goto(UNTIL_END_OF_DATA, order_id)
        }
        Ok(true)
    }

    pub fn cancel(&mut self, asset_no: usize, order_id: i64, wait: bool) -> Result<bool, Error> {
        let local = self.local.get_mut(asset_no).unwrap();
        local.cancel(order_id, self.cur_ts)?;
        self.evs.update_exch_order(asset_no, local.frontmost_send_order_timestamp());

        if wait {
            return self.goto(UNTIL_END_OF_DATA, order_id)
        }
        Ok(true)
    }

    pub fn clear_inactive_orders(&mut self, asset_no: Option<usize>) {
        match asset_no {
            Some(asset_no) => {
                self.local.get_mut(asset_no).unwrap().clear_inactive_orders();
            }
            None => {
                for local in self.local.iter_mut() {
                    local.clear_inactive_orders();
                }
            }
        }
    }

    fn initialize_evs(&mut self) -> Result<(), Error> {
        for (asset_no, local) in self.local.iter_mut().enumerate() {
            match local.initialize_data() {
                Ok(ts) => {
                    self.evs.update_local_data(asset_no, ts)
                },
                Err(Error::EndOfData) => {
                    self.evs.invalidate_local_data(asset_no);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        for (asset_no, exch) in self.exch.iter_mut().enumerate() {
            match exch.initialize_data() {
                Ok(ts) => {
                    self.evs.update_exch_data(asset_no, ts)
                },
                Err(Error::EndOfData) => {
                    self.evs.invalidate_exch_data(asset_no);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    pub fn elapse(&mut self, duration: i64) -> Result<bool, Error> {
        if self.cur_ts == i64::MAX {
            self.initialize_evs()?;
            match self.evs.next() {
                Some(ev) => {
                    self.cur_ts = ev.timestamp;
                }
                None => {
                    return Ok(false);
                }
            }
        }
        self.goto(self.cur_ts + duration, WAIT_ORDER_RESPONSE_NONE)
    }

    pub fn goto(&mut self, timestamp: i64, wait_order_response: i64) -> Result<bool, Error> {
        loop {
            match self.evs.next() {
                Some(ev) => {
                    if ev.timestamp > timestamp {
                        self.cur_ts = timestamp;
                        return Ok(true);
                    }
                    match ev.ty {
                        EventType::LocalData => {
                            let local = unsafe {
                                self.local.get_unchecked_mut(ev.asset_no)
                            };
                            match local.process_data() {
                                Ok((next_ts, _)) => {
                                    self.evs.update_local_data(ev.asset_no, next_ts);
                                }
                                Err(Error::EndOfData) => {
                                    self.evs.invalidate_local_data(ev.asset_no);
                                }
                                Err(e) => {
                                    return Err(e);
                                }
                            }
                        }
                        EventType::LocalOrder => {
                            let local = unsafe {
                                self.local.get_unchecked_mut(ev.asset_no)
                            };
                            let t = local.process_recv_order(
                                ev.timestamp,
                                wait_order_response
                            )?;
                            self.evs.update_local_order(ev.asset_no, local.frontmost_recv_order_timestamp());
                        }
                        EventType::ExchData => {
                            let exch = unsafe {
                                self.exch.get_unchecked_mut(ev.asset_no)
                            };
                            match exch.process_data() {
                                Ok((next_ts, _)) => {
                                    self.evs.update_exch_data(ev.asset_no, next_ts);
                                }
                                Err(Error::EndOfData) => {
                                    self.evs.invalidate_exch_data(ev.asset_no);
                                }
                                Err(e) => {
                                    return Err(e);
                                }
                            }
                            self.evs.update_local_order(ev.asset_no, exch.frontmost_send_order_timestamp());
                        }
                        EventType::ExchOrder => {
                            let exch = unsafe {
                                self.exch.get_unchecked_mut(ev.asset_no)
                            };
                            let t = exch.process_recv_order(ev.timestamp, wait_order_response)?;
                            self.evs.update_exch_order(ev.asset_no, exch.frontmost_recv_order_timestamp());
                        }
                    }
                }
                None => {
                    return Ok(false);
                }
            }
        }
    }
}