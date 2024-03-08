use std::sync::mpsc::Sender;

use crate::ty::{Event, Order};

pub mod binancefutures;

pub trait Connector {
    fn add(
        &mut self,
        an: usize,
        symbol: String,
        tick_size: f32,
        lot_size: f32,
    ) -> Result<(), anyhow::Error>;

    fn run(&mut self, tx: Sender<Event>) -> Result<(), anyhow::Error>;

    fn submit(
        &self,
        an: usize,
        order: Order<()>,
        ev_tx: Sender<Event>,
    ) -> Result<(), anyhow::Error>;

    fn cancel(
        &self,
        an: usize,
        order: Order<()>,
        ev_tx: Sender<Event>,
    ) -> Result<(), anyhow::Error>;
}
