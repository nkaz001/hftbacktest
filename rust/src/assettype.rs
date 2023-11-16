pub trait AssetType {
    fn amount(&self, exec_price: f32, qty: f32) -> f64;
    fn equity(&self, price: f32, balance: f64, position: f64, fee: f64) -> f64;
}

pub struct LinearAsset {
    contract_size: f64
}

impl LinearAsset {
    pub fn new(contract_size: f64) -> Self {
        Self {
            contract_size
        }
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

pub struct InverseAsset {
    contract_size: f64
}

impl InverseAsset {
    pub fn new(contract_size: f64) -> Self {
        Self {
            contract_size
        }
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