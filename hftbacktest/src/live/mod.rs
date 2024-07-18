mod bot;
mod recorder;

pub use bot::{BotError, LiveBot, LiveBotBuilder};
pub use recorder::LoggingRecorder;

/// Provides asset information for internal use.
#[derive(Clone)]
pub struct Asset {
    pub asset_no: usize,
    pub symbol: String,
    pub tick_size: f64,
    pub lot_size: f64,
}
