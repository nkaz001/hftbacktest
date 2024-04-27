mod bot;

pub use bot::{Bot, BotBuilder, BotError};

/// Provides asset information.
#[derive(Clone)]
pub struct Asset {
    pub asset_no: usize,
    pub symbol: String,
    pub tick_size: f32,
    pub lot_size: f32,
}
