use crate::{
    backtest::{assettype::AssetType, models::FeeModel},
    types::{Order, StateValues},
};

#[derive(Debug)]
pub struct State<AT, FM>
where
    AT: AssetType,
    FM: FeeModel,
{
    pub state_values: StateValues,
    pub asset_type: AT,
    pub fee_model: FM,
}

impl<AT, FM> State<AT, FM>
where
    AT: AssetType,
    FM: FeeModel,
{
    pub fn new(asset_type: AT, fee_model: FM) -> Self {
        Self {
            state_values: StateValues {
                position: 0.0,
                balance: 0.0,
                fee: 0.0,
                num_trades: 0,
                trading_volume: 0.0,
                trading_value: 0.0,
            },
            fee_model,
            asset_type,
        }
    }

    #[inline]
    pub fn apply_fill(&mut self, order: &Order) {
        let amount = self.asset_type.amount(order.exec_price(), order.exec_qty);
        self.state_values.position += order.exec_qty * AsRef::<f64>::as_ref(&order.side);
        self.state_values.balance -= amount * AsRef::<f64>::as_ref(&order.side);
        self.state_values.fee += self.fee_model.amount(order, amount);
        self.state_values.num_trades += 1;
        self.state_values.trading_volume += order.exec_qty;
        self.state_values.trading_value += amount;
    }

    #[inline]
    pub fn equity(&self, mid: f64) -> f64 {
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
