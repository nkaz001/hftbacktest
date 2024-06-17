mod bot;
mod recorder;

pub use bot::{LiveBot, BotBuilder, BotError};
pub use recorder::LoggingRecorder;

/// Provides asset information for internal use.
#[derive(Clone)]
pub struct Asset {
    pub asset_no: usize,
    pub symbol: String,
    pub tick_size: f32,
    pub lot_size: f32,
}
