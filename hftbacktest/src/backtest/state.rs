use crate::{backtest::assettype::AssetType, types::Order};
use crate::types::StateValues;

#[derive(Debug)]
pub struct State<AT>
where
    AT: AssetType,
{
    pub state_values: StateValues,
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
            state_values: StateValues {
                position: 0.0,
                balance: 0.0,
                fee: 0.0,
                trade_num: 0,
                trade_qty: 0.0,
                trade_amount: 0.0,
            },
            maker_fee,
            taker_fee,
            asset_type,
        }
    }

    #[inline]
    pub fn apply_fill(&mut self, order: &Order) {
        let fee = if order.maker {
            self.maker_fee
        } else {
            self.taker_fee
        };
        let amount = self.asset_type.amount(order.exec_price(), order.exec_qty);
        self.state_values.position += order.exec_qty as f64 * AsRef::<f64>::as_ref(&order.side);
        self.state_values.balance -= amount * AsRef::<f64>::as_ref(&order.side);
        self.state_values.fee += amount * fee;
        self.state_values.trade_num += 1;
        self.state_values.trade_qty += order.exec_qty as f64;
        self.state_values.trade_amount += amount;
    }

    #[inline]
    pub fn equity(&self, mid: f32) -> f64 {
        self.asset_type
            .equity(mid, self.state_values.balance, self.state_values.position, self.state_values.fee)
    }

    #[inline]
    pub fn values(&self) -> &StateValues {
        &self.state_values
    }
}
