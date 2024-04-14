use std::{collections::HashMap, marker::PhantomData};

use crate::{
    backtest::{
        evs::{EventSet, EventType},
        proc::{LocalProcessor, Processor},
        reader::{UNTIL_END_OF_DATA, WAIT_ORDER_RESPONSE_NONE},
        state::StateValues,
        BacktestAsset,
        Error,
    },
    depth::{hashmapmarketdepth::HashMapMarketDepth, MarketDepth},
    ty::{Event, OrdType, Order, Side, TimeInForce},
    BuildError,
    Interface,
};

pub struct MultiAssetMultiExchangeBacktestBuilder<Q, MD> {
    local: Vec<Box<dyn LocalProcessor<Q, MD>>>,
    exch: Vec<Box<dyn Processor>>,
}

impl<Q, MD> MultiAssetMultiExchangeBacktestBuilder<Q, MD>
where
    Q: Clone,
{
    pub fn add(self, asset: BacktestAsset<dyn LocalProcessor<Q, MD>, dyn Processor>) -> Self {
        let mut self_ = Self { ..self };
        self_.local.push(asset.local);
        self_.exch.push(asset.exch);
        self_
    }

    pub fn build(self) -> Result<MultiAssetMultiExchangeBacktest<Q, MD>, BuildError> {
        let num_assets = self.local.len();
        if self.local.len() != num_assets || self.exch.len() != num_assets {
            panic!();
        }
        Ok(MultiAssetMultiExchangeBacktest {
            cur_ts: i64::MAX,
            evs: EventSet::new(num_assets),
            local: self.local,
            exch: self.exch,
            _q_marker: Default::default(),
        })
    }
}

pub struct MultiAssetMultiExchangeBacktest<Q, MD> {
    cur_ts: i64,
    evs: EventSet,
    local: Vec<Box<dyn LocalProcessor<Q, MD>>>,
    exch: Vec<Box<dyn Processor>>,
    _q_marker: PhantomData<Q>,
}

impl<Q, MD> MultiAssetMultiExchangeBacktest<Q, MD>
where
    Q: Clone,
    MD: MarketDepth,
{
    pub fn builder() -> MultiAssetMultiExchangeBacktestBuilder<Q, MD> {
        MultiAssetMultiExchangeBacktestBuilder {
            local: vec![],
            exch: vec![],
        }
    }

    pub fn new(local: Vec<Box<dyn LocalProcessor<Q, MD>>>, exch: Vec<Box<dyn Processor>>) -> Self {
        let num_assets = local.len();
        if local.len() != num_assets || exch.len() != num_assets {
            panic!();
        }
        Self {
            cur_ts: i64::MAX,
            evs: EventSet::new(num_assets),
            local,
            exch,
            _q_marker: Default::default(),
        }
    }

    fn initialize_evs(&mut self) -> Result<(), Error> {
        for (asset_no, local) in self.local.iter_mut().enumerate() {
            match local.initialize_data() {
                Ok(ts) => self.evs.update_local_data(asset_no, ts),
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
                Ok(ts) => self.evs.update_exch_data(asset_no, ts),
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
                            let local = unsafe { self.local.get_unchecked_mut(ev.asset_no) };
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
                            let local = unsafe { self.local.get_unchecked_mut(ev.asset_no) };
                            let t = local.process_recv_order(ev.timestamp, wait_order_response)?;
                            self.evs.update_local_order(
                                ev.asset_no,
                                local.frontmost_recv_order_timestamp(),
                            );
                        }
                        EventType::ExchData => {
                            let exch = unsafe { self.exch.get_unchecked_mut(ev.asset_no) };
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
                            self.evs.update_local_order(
                                ev.asset_no,
                                exch.frontmost_send_order_timestamp(),
                            );
                        }
                        EventType::ExchOrder => {
                            let exch = unsafe { self.exch.get_unchecked_mut(ev.asset_no) };
                            let t = exch.process_recv_order(ev.timestamp, wait_order_response)?;
                            self.evs.update_exch_order(
                                ev.asset_no,
                                exch.frontmost_recv_order_timestamp(),
                            );
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

impl<Q, MD> Interface<Q, MD> for MultiAssetMultiExchangeBacktest<Q, MD>
where
    Q: Clone,
    MD: MarketDepth,
{
    type Error = Error;

    fn current_timestamp(&self) -> i64 {
        self.cur_ts
    }

    fn position(&self, asset_no: usize) -> f64 {
        self.local.get(asset_no).unwrap().position()
    }

    fn state_values(&self, asset_no: usize) -> StateValues {
        self.local.get(asset_no).unwrap().state_values()
    }

    fn depth(&self, asset_no: usize) -> &MD {
        &self.local.get(asset_no).unwrap().depth()
    }

    fn trade(&self, asset_no: usize) -> &Vec<Event> {
        let local = self.local.get(asset_no).unwrap();
        local.trade()
    }

    fn clear_last_trades(&mut self, asset_no: Option<usize>) {
        match asset_no {
            Some(an) => {
                let local = self.local.get_mut(an).unwrap();
                local.clear_last_trades();
            }
            None => {
                for local in self.local.iter_mut() {
                    local.clear_last_trades();
                }
            }
        }
    }

    fn orders(&self, asset_no: usize) -> &HashMap<i64, Order<Q>> {
        &self.local.get(asset_no).unwrap().orders()
    }

    fn submit_buy_order(
        &mut self,
        asset_no: usize,
        order_id: i64,
        price: f32,
        qty: f32,
        time_in_force: TimeInForce,
        order_type: OrdType,
        wait: bool,
    ) -> Result<bool, Self::Error> {
        let local = self.local.get_mut(asset_no).unwrap();
        local.submit_order(
            order_id,
            Side::Buy,
            price,
            qty,
            order_type,
            time_in_force,
            self.cur_ts,
        )?;
        self.evs
            .update_exch_order(asset_no, local.frontmost_send_order_timestamp());

        if wait {
            return self.goto(UNTIL_END_OF_DATA, order_id);
        }
        Ok(true)
    }

    fn submit_sell_order(
        &mut self,
        asset_no: usize,
        order_id: i64,
        price: f32,
        qty: f32,
        time_in_force: TimeInForce,
        order_type: OrdType,
        wait: bool,
    ) -> Result<bool, Self::Error> {
        let local = self.local.get_mut(asset_no).unwrap();
        local.submit_order(
            order_id,
            Side::Sell,
            price,
            qty,
            order_type,
            time_in_force,
            self.cur_ts,
        )?;
        self.evs
            .update_exch_order(asset_no, local.frontmost_send_order_timestamp());

        if wait {
            return self.goto(UNTIL_END_OF_DATA, order_id);
        }
        Ok(true)
    }

    fn cancel(&mut self, asset_no: usize, order_id: i64, wait: bool) -> Result<bool, Self::Error> {
        let local = self.local.get_mut(asset_no).unwrap();
        local.cancel(order_id, self.cur_ts)?;
        self.evs
            .update_exch_order(asset_no, local.frontmost_send_order_timestamp());

        if wait {
            return self.goto(UNTIL_END_OF_DATA, order_id);
        }
        Ok(true)
    }

    fn clear_inactive_orders(&mut self, asset_no: Option<usize>) {
        match asset_no {
            Some(asset_no) => {
                self.local
                    .get_mut(asset_no)
                    .unwrap()
                    .clear_inactive_orders();
            }
            None => {
                for local in self.local.iter_mut() {
                    local.clear_inactive_orders();
                }
            }
        }
    }

    fn elapse(&mut self, duration: i64) -> Result<bool, Self::Error> {
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

    fn elapse_bt(&mut self, duration: i64) -> Result<bool, Self::Error> {
        self.elapse(duration)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}

pub struct MultiAssetSingleExchangeBacktestBuilder<Q, Local, Exchange> {
    local: Vec<Local>,
    exch: Vec<Exchange>,
    _q_marker: PhantomData<Q>,
}

impl<Q, Local, Exchange> MultiAssetSingleExchangeBacktestBuilder<Q, Local, Exchange>
where
    Q: Clone,
    Local: LocalProcessor<Q, HashMapMarketDepth> + 'static,
    Exchange: Processor + 'static,
{
    pub fn add(self, asset: BacktestAsset<Local, Exchange>) -> Self {
        let mut self_ = Self { ..self };
        self_.local.push(*asset.local);
        self_.exch.push(*asset.exch);
        self_
    }

    pub fn build(
        self,
    ) -> Result<MultiAssetSingleExchangeBacktest<Q, HashMapMarketDepth, Local, Exchange>, BuildError>
    {
        let num_assets = self.local.len();
        if self.local.len() != num_assets || self.exch.len() != num_assets {
            panic!();
        }
        Ok(MultiAssetSingleExchangeBacktest {
            cur_ts: i64::MAX,
            evs: EventSet::new(num_assets),
            local: self.local,
            exch: self.exch,
            _q_marker: Default::default(),
            _md_marker: Default::default(),
        })
    }
}

pub struct MultiAssetSingleExchangeBacktest<Q, MD, Local, Exchange> {
    cur_ts: i64,
    evs: EventSet,
    local: Vec<Local>,
    exch: Vec<Exchange>,
    _q_marker: PhantomData<Q>,
    _md_marker: PhantomData<MD>,
}

impl<Q, MD, Local, Exchange> MultiAssetSingleExchangeBacktest<Q, MD, Local, Exchange>
where
    Q: Clone,
    MD: MarketDepth,
    Local: LocalProcessor<Q, MD>,
    Exchange: Processor,
{
    pub fn builder() -> MultiAssetSingleExchangeBacktestBuilder<Q, Local, Exchange> {
        MultiAssetSingleExchangeBacktestBuilder {
            local: vec![],
            exch: vec![],
            _q_marker: Default::default(),
        }
    }

    pub fn new(local: Vec<Local>, exch: Vec<Exchange>) -> Self {
        let num_assets = local.len();
        if local.len() != num_assets || exch.len() != num_assets {
            panic!();
        }
        Self {
            cur_ts: i64::MAX,
            evs: EventSet::new(num_assets),
            local,
            exch,
            _q_marker: Default::default(),
            _md_marker: Default::default(),
        }
    }

    fn initialize_evs(&mut self) -> Result<(), Error> {
        for (asset_no, local) in self.local.iter_mut().enumerate() {
            match local.initialize_data() {
                Ok(ts) => self.evs.update_local_data(asset_no, ts),
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
                Ok(ts) => self.evs.update_exch_data(asset_no, ts),
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
                            let local = unsafe { self.local.get_unchecked_mut(ev.asset_no) };
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
                            let local = unsafe { self.local.get_unchecked_mut(ev.asset_no) };
                            let t = local.process_recv_order(ev.timestamp, wait_order_response)?;
                            self.evs.update_local_order(
                                ev.asset_no,
                                local.frontmost_recv_order_timestamp(),
                            );
                        }
                        EventType::ExchData => {
                            let exch = unsafe { self.exch.get_unchecked_mut(ev.asset_no) };
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
                            self.evs.update_local_order(
                                ev.asset_no,
                                exch.frontmost_send_order_timestamp(),
                            );
                        }
                        EventType::ExchOrder => {
                            let exch = unsafe { self.exch.get_unchecked_mut(ev.asset_no) };
                            let t = exch.process_recv_order(ev.timestamp, wait_order_response)?;
                            self.evs.update_exch_order(
                                ev.asset_no,
                                exch.frontmost_recv_order_timestamp(),
                            );
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

impl<Q, MD, Local, Exchange> Interface<Q, MD>
    for MultiAssetSingleExchangeBacktest<Q, MD, Local, Exchange>
where
    Q: Clone,
    MD: MarketDepth,
    Local: LocalProcessor<Q, MD>,
    Exchange: Processor,
{
    type Error = Error;

    fn current_timestamp(&self) -> i64 {
        self.cur_ts
    }

    fn position(&self, asset_no: usize) -> f64 {
        self.local.get(asset_no).unwrap().position()
    }

    fn state_values(&self, asset_no: usize) -> StateValues {
        self.local.get(asset_no).unwrap().state_values()
    }

    fn depth(&self, asset_no: usize) -> &MD {
        &self.local.get(asset_no).unwrap().depth()
    }

    fn trade(&self, asset_no: usize) -> &Vec<Event> {
        let local = self.local.get(asset_no).unwrap();
        local.trade()
    }

    fn clear_last_trades(&mut self, asset_no: Option<usize>) {
        match asset_no {
            Some(an) => {
                let local = self.local.get_mut(an).unwrap();
                local.clear_last_trades();
            }
            None => {
                for local in self.local.iter_mut() {
                    local.clear_last_trades();
                }
            }
        }
    }

    fn orders(&self, asset_no: usize) -> &HashMap<i64, Order<Q>> {
        &self.local.get(asset_no).unwrap().orders()
    }

    fn submit_buy_order(
        &mut self,
        asset_no: usize,
        order_id: i64,
        price: f32,
        qty: f32,
        time_in_force: TimeInForce,
        order_type: OrdType,
        wait: bool,
    ) -> Result<bool, Self::Error> {
        let local = self.local.get_mut(asset_no).unwrap();
        local.submit_order(
            order_id,
            Side::Buy,
            price,
            qty,
            order_type,
            time_in_force,
            self.cur_ts,
        )?;
        self.evs
            .update_exch_order(asset_no, local.frontmost_send_order_timestamp());

        if wait {
            return self.goto(UNTIL_END_OF_DATA, order_id);
        }
        Ok(true)
    }

    fn submit_sell_order(
        &mut self,
        asset_no: usize,
        order_id: i64,
        price: f32,
        qty: f32,
        time_in_force: TimeInForce,
        order_type: OrdType,
        wait: bool,
    ) -> Result<bool, Self::Error> {
        let local = self.local.get_mut(asset_no).unwrap();
        local.submit_order(
            order_id,
            Side::Sell,
            price,
            qty,
            order_type,
            time_in_force,
            self.cur_ts,
        )?;
        self.evs
            .update_exch_order(asset_no, local.frontmost_send_order_timestamp());

        if wait {
            return self.goto(UNTIL_END_OF_DATA, order_id);
        }
        Ok(true)
    }

    fn cancel(&mut self, asset_no: usize, order_id: i64, wait: bool) -> Result<bool, Self::Error> {
        let local = self.local.get_mut(asset_no).unwrap();
        local.cancel(order_id, self.cur_ts)?;
        self.evs
            .update_exch_order(asset_no, local.frontmost_send_order_timestamp());

        if wait {
            return self.goto(UNTIL_END_OF_DATA, order_id);
        }
        Ok(true)
    }

    fn clear_inactive_orders(&mut self, asset_no: Option<usize>) {
        match asset_no {
            Some(asset_no) => {
                self.local
                    .get_mut(asset_no)
                    .unwrap()
                    .clear_inactive_orders();
            }
            None => {
                for local in self.local.iter_mut() {
                    local.clear_inactive_orders();
                }
            }
        }
    }

    fn elapse(&mut self, duration: i64) -> Result<bool, Self::Error> {
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

    fn elapse_bt(&mut self, duration: i64) -> Result<bool, Self::Error> {
        self.elapse(duration)
    }

    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }
}
