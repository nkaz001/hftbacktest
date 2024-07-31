use crate::types::Order;

pub struct FeeArgs<'a> {
    pub order: &'a Order,
    pub amount: f64,
}

pub trait FeeModel {
    // Calculates the fee amount.
    fn amount(&self, args: FeeArgs) -> f64;
}

// fee based on a percentage of the order value, with the rate
// depending on whether the order is a maker or taker.
pub struct TradingValueFeeModel {
    pub maker_fee: f64,
    pub taker_fee: f64,
}
impl FeeModel for TradingValueFeeModel {
    fn amount(&self, args: FeeArgs) -> f64 {
        if args.order.maker {
            self.maker_fee * args.amount
        } else {
            self.taker_fee * args.amount
        }
    }
}
// fee per trading quantity
pub struct TradingQtyFeeModel {
    pub per_qty_fee: f64,
}
impl FeeModel for TradingQtyFeeModel {
    fn amount(&self, args: FeeArgs) -> f64 {
        self.per_qty_fee * args.order.qty
    }
}

// fee per trade
pub struct FlatPerTradeFeeModel {
    pub flat_fee: f64,
}
impl FeeModel for FlatPerTradeFeeModel {
    fn amount(&self, args: FeeArgs) -> f64 {
        self.flat_fee
    }
}

// different fees based on the direction.
pub struct DirectionalFeeModel {}
impl FeeModel for DirectionalFeeModel {
    fn amount(&self, args: FeeArgs) -> f64 {
        panic!("Not implemented");
    }
}
