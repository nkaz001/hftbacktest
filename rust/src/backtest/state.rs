use crate::{backtest::assettype::AssetType, types::Order};

#[derive(Debug)]
pub struct State<AT>
where
    AT: AssetType,
{
    pub position: f64,
    pub balance: f64,
    pub fee: f64,
    pub trade_num: i32,
    pub trade_qty: f64,
    pub trade_amount: f64,
    pub maker_fee: f64,
    pub taker_fee: f64,
    pub asset_type: AT,
}

impl<AT> State<AT>
where
    AT: AssetType,
{
    pub fn new(asset_type: AT, maker_fee: f64, taker_fee: f64) -> Self {
        Self {
            position: 0.0,
            balance: 0.0,
            fee: 0.0,
            trade_num: 0,
            trade_qty: 0.0,
            trade_amount: 0.0,
            maker_fee,
            taker_fee,
            asset_type,
        }
    }

    pub fn apply_fill<Q: Clone + Default>(&mut self, order: &Order<Q>) {
        let fee = if order.maker {
            self.maker_fee
        } else {
            self.taker_fee
        };
        let amount = self.asset_type.amount(order.exec_price(), order.exec_qty);
        self.position += order.exec_qty as f64 * order.side.as_f64();
        self.balance -= amount * order.side.as_f64();
        self.fee += amount * fee;
        self.trade_num += 1;
        self.trade_qty += order.exec_qty as f64;
        self.trade_amount += amount;
    }

    pub fn equity(&self, mid: f32) -> f64 {
        self.asset_type
            .equity(mid, self.balance, self.position, self.fee)
    }
}
