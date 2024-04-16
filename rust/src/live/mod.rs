pub mod bot;

#[derive(Clone)]
pub struct AssetInfo {
    pub asset_no: usize,
    pub symbol: String,
    pub tick_size: f32,
    pub lot_size: f32,
}