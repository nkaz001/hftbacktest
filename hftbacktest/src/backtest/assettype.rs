/// Calculates the value amount and the equity according to the asset type.
pub trait AssetType {
    /// Calculates the value amount.
    fn amount(&self, price: f64, qty: f64) -> f64;

    /// Calculates the equity.
    fn equity(&self, price: f64, balance: f64, position: f64, fee: f64) -> f64;
}

/// The common type of asset where the contract's notional value is linear to the quote currency.
#[derive(Clone)]
pub struct LinearAsset {
    contract_size: f64,
}

impl LinearAsset {
    /// Constructs an instance of `LinearAsset`.
    pub fn new(contract_size: f64) -> Self {
        Self { contract_size }
    }
}

impl AssetType for LinearAsset {
    fn amount(&self, exec_price: f64, qty: f64) -> f64 {
        self.contract_size * exec_price * qty
    }

    fn equity(&self, price: f64, balance: f64, position: f64, fee: f64) -> f64 {
        balance + self.contract_size * position * price - fee
    }
}

/// The contract’s notional value is denominated in the quote currency.
#[derive(Clone)]
pub struct InverseAsset {
    contract_size: f64,
}

impl InverseAsset {
    /// Constructs an instance of `InverseAsset`.
    pub fn new(contract_size: f64) -> Self {
        Self { contract_size }
    }
}

impl AssetType for InverseAsset {
    fn amount(&self, exec_price: f64, qty: f64) -> f64 {
        self.contract_size * qty / exec_price
    }

    fn equity(&self, price: f64, balance: f64, position: f64, fee: f64) -> f64 {
        -balance - self.contract_size * position / price - fee
    }
}
