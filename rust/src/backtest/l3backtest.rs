use std::{any::Any, collections::HashMap, marker::PhantomData};

use crate::{
    backtest::{
        evs::{EventSet, EventType},
        proc::{GenLocalProcessor, LocalProcessor, Processor},
        BacktestError,
    },
    depth::{L3MarketDepth, MarketDepth},
    prelude::{
        Bot,
        BotTypedDepth,
        BotTypedTrade,
        OrdType,
        Order,
        OrderRequest,
        Side,
        StateValues,
        TimeInForce,
        UNTIL_END_OF_DATA,
        WAIT_ORDER_RESPONSE_NONE,
    },
    types::L3Event,
};

pub struct L3MultiAssetSingleExchangeBacktest<MD, Local, Exchange>
where
    MD: L3MarketDepth,
    Local: LocalProcessor<MD, L3Event>,
    Exchange: Processor,
{
    cur_ts: i64,
    evs: EventSet,
    local: Vec<Local>,
    exch: Vec<Exchange>,
    _md_marker: PhantomData<MD>,
}

impl<MD, Local, Exchange> L3MultiAssetSingleExchangeBacktest<MD, Local, Exchange>
where
    MD: L3MarketDepth,
    Local: LocalProcessor<MD, L3Event>,
    Exchange: Processor,
{
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
            _md_marker: Default::default(),
        }
    }

    fn initialize_evs(&mut self) -> Result<(), BacktestError> {
        for (asset_no, local) in self.local.iter_mut().enumerate() {
            match local.initialize_data() {
                Ok(ts) => self.evs.update_local_data(asset_no, ts),
                Err(BacktestError::EndOfData) => {
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
                Err(BacktestError::EndOfData) => {
                    self.evs.invalidate_exch_data(asset_no);
                }
                Err(e) => {
                    return Err(e);
                }
            }
        }
        Ok(())
    }

    pub fn goto(
        &mut self,
        timestamp: i64,
        wait_order_response: (usize, i64),
    ) -> Result<bool, BacktestError> {
        let mut timestamp = timestamp;
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
                                Err(BacktestError::EndOfData) => {
                                    self.evs.invalidate_local_data(ev.asset_no);
                                }
                                Err(e) => {
                                    return Err(e);
                                }
                            }
                        }
                        EventType::LocalOrder => {
                            let local = unsafe { self.local.get_unchecked_mut(ev.asset_no) };
                            let wait_order_resp_id = if ev.asset_no == wait_order_response.0 {
                                wait_order_response.1
                            } else {
                                WAIT_ORDER_RESPONSE_NONE
                            };
                            if local.process_recv_order(ev.timestamp, wait_order_resp_id)? {
                                timestamp = ev.timestamp;
                            }
                            self.evs.update_local_order(
                                ev.asset_no,
                                local.earliest_recv_order_timestamp(),
                            );
                        }
                        EventType::ExchData => {
                            let exch = unsafe { self.exch.get_unchecked_mut(ev.asset_no) };
                            match exch.process_data() {
                                Ok((next_ts, _)) => {
                                    self.evs.update_exch_data(ev.asset_no, next_ts);
                                }
                                Err(BacktestError::EndOfData) => {
                                    self.evs.invalidate_exch_data(ev.asset_no);
                                }
                                Err(e) => {
                                    return Err(e);
                                }
                            }
                            self.evs.update_local_order(
                                ev.asset_no,
                                exch.earliest_send_order_timestamp(),
                            );
                        }
                        EventType::ExchOrder => {
                            let exch = unsafe { self.exch.get_unchecked_mut(ev.asset_no) };
                            let _ =
                                exch.process_recv_order(ev.timestamp, WAIT_ORDER_RESPONSE_NONE)?;
                            self.evs.update_exch_order(
                                ev.asset_no,
                                exch.earliest_recv_order_timestamp(),
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

impl<MD, Local, Exchange> Bot for L3MultiAssetSingleExchangeBacktest<MD, Local, Exchange>
where
    MD: L3MarketDepth,
    Local: LocalProcessor<MD, L3Event>,
    Exchange: Processor,
{
    type Error = BacktestError;

    #[inline]
    fn current_timestamp(&self) -> i64 {
        self.cur_ts
    }

    #[inline]
    fn num_assets(&self) -> usize {
        self.local.len()
    }

    #[inline]
    fn position(&self, asset_no: usize) -> f64 {
        self.local.get(asset_no).unwrap().position()
    }

    #[inline]
    fn state_values(&self, asset_no: usize) -> StateValues {
        self.local.get(asset_no).unwrap().state_values()
    }

    fn depth(&self, asset_no: usize) -> &dyn MarketDepth {
        self.local.get(asset_no).unwrap().depth()
    }

    fn trade(&self, asset_no: usize) -> Vec<&dyn Any> {
        self.local
            .get(asset_no)
            .unwrap()
            .trade()
            .iter()
            .map(|ev| ev as &dyn Any)
            .collect()
    }

    #[inline]
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

    #[inline]
    fn orders(&self, asset_no: usize) -> &HashMap<i64, Order> {
        &self.local.get(asset_no).unwrap().orders()
    }

    #[inline]
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
            .update_exch_order(asset_no, local.earliest_send_order_timestamp());

        if wait {
            return self.goto(UNTIL_END_OF_DATA, (asset_no, order_id));
        }
        Ok(true)
    }

    #[inline]
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
            .update_exch_order(asset_no, local.earliest_send_order_timestamp());

        if wait {
            return self.goto(UNTIL_END_OF_DATA, (asset_no, order_id));
        }
        Ok(true)
    }

    fn submit_order(
        &mut self,
        asset_no: usize,
        order: OrderRequest,
        wait: bool,
    ) -> Result<bool, Self::Error> {
        let local = self.local.get_mut(asset_no).unwrap();
        local.submit_order(
            order.order_id,
            Side::Sell,
            order.price,
            order.qty,
            order.order_type,
            order.time_in_force,
            self.cur_ts,
        )?;
        self.evs
            .update_exch_order(asset_no, local.earliest_send_order_timestamp());

        if wait {
            return self.goto(UNTIL_END_OF_DATA, (asset_no, order.order_id));
        }
        Ok(true)
    }

    #[inline]
    fn cancel(&mut self, asset_no: usize, order_id: i64, wait: bool) -> Result<bool, Self::Error> {
        let local = self.local.get_mut(asset_no).unwrap();
        local.cancel(order_id, self.cur_ts)?;
        self.evs
            .update_exch_order(asset_no, local.earliest_send_order_timestamp());

        if wait {
            return self.goto(UNTIL_END_OF_DATA, (asset_no, order_id));
        }
        Ok(true)
    }

    #[inline]
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

    #[inline]
    fn wait_order_response(
        &mut self,
        asset_no: usize,
        order_id: i64,
        timeout: i64,
    ) -> Result<bool, BacktestError> {
        self.goto(self.cur_ts + timeout, (asset_no, order_id))
    }

    fn wait_next_feed(
        &mut self,
        include_order_resp: bool,
        timeout: i64,
    ) -> Result<bool, Self::Error> {
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
        let mut timestamp = self.cur_ts + timeout;
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
                                Err(BacktestError::EndOfData) => {
                                    self.evs.invalidate_local_data(ev.asset_no);
                                }
                                Err(e) => {
                                    return Err(e);
                                }
                            }
                            timestamp = ev.timestamp;
                        }
                        EventType::LocalOrder => {
                            let local = unsafe { self.local.get_unchecked_mut(ev.asset_no) };
                            let _ =
                                local.process_recv_order(ev.timestamp, WAIT_ORDER_RESPONSE_NONE)?;
                            self.evs.update_local_order(
                                ev.asset_no,
                                local.earliest_recv_order_timestamp(),
                            );
                            if include_order_resp {
                                timestamp = ev.timestamp;
                            }
                        }
                        EventType::ExchData => {
                            let exch = unsafe { self.exch.get_unchecked_mut(ev.asset_no) };
                            match exch.process_data() {
                                Ok((next_ts, _)) => {
                                    self.evs.update_exch_data(ev.asset_no, next_ts);
                                }
                                Err(BacktestError::EndOfData) => {
                                    self.evs.invalidate_exch_data(ev.asset_no);
                                }
                                Err(e) => {
                                    return Err(e);
                                }
                            }
                            self.evs.update_local_order(
                                ev.asset_no,
                                exch.earliest_send_order_timestamp(),
                            );
                        }
                        EventType::ExchOrder => {
                            let exch = unsafe { self.exch.get_unchecked_mut(ev.asset_no) };
                            let _ =
                                exch.process_recv_order(ev.timestamp, WAIT_ORDER_RESPONSE_NONE)?;
                            self.evs.update_exch_order(
                                ev.asset_no,
                                exch.earliest_recv_order_timestamp(),
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

    #[inline]
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
        self.goto(self.cur_ts + duration, (0, WAIT_ORDER_RESPONSE_NONE))
    }

    #[inline]
    fn elapse_bt(&mut self, duration: i64) -> Result<bool, Self::Error> {
        self.elapse(duration)
    }

    #[inline]
    fn close(&mut self) -> Result<(), Self::Error> {
        Ok(())
    }

    #[inline]
    fn feed_latency(&self, asset_no: usize) -> Option<(i64, i64)> {
        self.local.get(asset_no).unwrap().feed_latency()
    }

    #[inline]
    fn order_latency(&self, asset_no: usize) -> Option<(i64, i64, i64)> {
        self.local.get(asset_no).unwrap().order_latency()
    }
}

impl<MD, Local, Exchange> BotTypedDepth<MD>
    for L3MultiAssetSingleExchangeBacktest<MD, Local, Exchange>
where
    MD: L3MarketDepth,
    Local: LocalProcessor<MD, L3Event>,
    Exchange: Processor,
{
    #[inline]
    fn depth_typed(&self, asset_no: usize) -> &MD {
        &self.local.get(asset_no).unwrap().depth()
    }
}

impl<MD, Local, Exchange> BotTypedTrade<L3Event>
    for L3MultiAssetSingleExchangeBacktest<MD, Local, Exchange>
where
    MD: L3MarketDepth,
    Local: LocalProcessor<MD, L3Event>,
    Exchange: Processor,
{
    #[inline]
    fn trade_typed(&self, asset_no: usize) -> &Vec<L3Event> {
        let local = self.local.get(asset_no).unwrap();
        local.trade()
    }
}

pub struct Backtest {
    local: Vec<Box<dyn GenLocalProcessor>>,
    exch: Vec<Box<dyn Processor>>,
}
