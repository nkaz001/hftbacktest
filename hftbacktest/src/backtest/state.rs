use crate::{
    backtest::assettype::AssetType,
    types::{Order, StateValues},
};

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
                num_trades: 0,
                trading_volume: 0.0,
                trading_value: 0.0,
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
        self.state_values.num_trades += 1;
        self.state_values.trading_volume += order.exec_qty as f64;
        self.state_values.trading_value += amount;
    }

    #[inline]
    pub fn equity(&self, mid: f32) -> f64 {
        self.asset_type.equity(
            mid,
            self.state_values.balance,
            self.state_values.position,
            self.state_values.fee,
        )
    }

    #[inline]
    pub fn values(&self) -> &StateValues {
        &self.state_values
    }
}
