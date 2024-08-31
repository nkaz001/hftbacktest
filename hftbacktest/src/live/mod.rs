use std::time::Duration;

pub use bot::{BotError, LiveBot, LiveBotBuilder};
pub use recorder::LoggingRecorder;

use crate::{prelude::Request, types::LiveEvent};

mod bot;
pub mod ipc;
mod recorder;

/// Provides asset information for internal use.
#[derive(Clone)]
pub struct Asset {
    pub asset_no: usize,
    pub symbol: String,
    pub tick_size: f64,
    pub lot_size: f64,
}

pub trait Channel {
    fn recv_timeout(&mut self, timeout: Duration) -> Result<LiveEvent, BotError>;

    fn send(&mut self, asset_no: usize, request: Request) -> Result<(), BotError>;
}
