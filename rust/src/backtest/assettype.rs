/// Calculates the value and the equity according to the asset type.
pub trait AssetType {
    /// Calculates the value amount.
    fn amount(&self, price: f32, qty: f32) -> f64;

    /// Calculates the equity.
    fn equity(&self, price: f32, balance: f64, position: f64, fee: f64) -> f64;
}

/// The common type of asset.
#[derive(Clone)]
pub struct LinearAsset {
    contract_size: f64,
}

impl LinearAsset {
    /// Constructs [`LinearAsset`].
    pub fn new(contract_size: f64) -> Self {
        Self { contract_size }
    }
}

impl AssetType for LinearAsset {
    fn amount(&self, exec_price: f32, qty: f32) -> f64 {
        self.contract_size * exec_price as f64 * qty as f64
    }

    fn equity(&self, price: f32, balance: f64, position: f64, fee: f64) -> f64 {
        balance + self.contract_size * position * price as f64 - fee
    }
}

/// The contract’s notional value is denominated in the quote currency.
#[derive(Clone)]
pub struct InverseAsset {
    contract_size: f64,
}

impl InverseAsset {
    /// Constructs [`InverseAsset`].
    pub fn new(contract_size: f64) -> Self {
        Self { contract_size }
    }
}

impl AssetType for InverseAsset {
    fn amount(&self, exec_price: f32, qty: f32) -> f64 {
        self.contract_size * qty as f64 / exec_price as f64
    }

    fn equity(&self, price: f32, balance: f64, position: f64, fee: f64) -> f64 {
        -balance - self.contract_size * position / price as f64 - fee
    }
}
