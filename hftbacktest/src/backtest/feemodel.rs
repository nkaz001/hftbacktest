use crate::types::Order;
#[derive(Clone)]
pub struct CommonFees {
    pub maker_fee: f64,
    pub taker_fee: f64,
}

pub trait FeeModel {
    // Calculates the fee amount.
    fn amount(&self, order: &Order, amount: f64) -> f64;
}

// fee based on a percentage of the order value, with the rate
// depending on whether the order is a maker or taker.
#[derive(Clone)]

pub struct TradingValueFeeModel<Fees> {
    pub common_fees: Fees,
}

impl TradingValueFeeModel<CommonFees> {
    pub fn new(common_fees: CommonFees) -> Self {
        Self { common_fees }
    }
}

impl FeeModel for TradingValueFeeModel<CommonFees> {
    fn amount(&self, order: &Order, amount: f64) -> f64 {
        if order.maker {
            self.common_fees.maker_fee * amount
        } else {
            self.common_fees.taker_fee * amount
        }
    }
}
// fee per trading quantity
#[derive(Clone)]

pub struct TradingQtyFeeModel<Fees> {
    pub common_fees: Fees,
}

impl TradingQtyFeeModel<CommonFees> {
    pub fn new(common_fees: CommonFees) -> Self {
        Self { common_fees }
    }
}
impl FeeModel for TradingQtyFeeModel<CommonFees> {
    fn amount(&self, order: &Order, amount: f64) -> f64 {
        if order.maker {
            self.common_fees.maker_fee * order.exec_qty
        } else {
            self.common_fees.taker_fee * order.exec_qty
        }
    }
}

// fee per trade
#[derive(Clone)]

pub struct FlatPerTradeFeeModel<Fees> {
    pub common_fees: Fees,
}
impl FlatPerTradeFeeModel<CommonFees> {
    pub fn new(common_fees: CommonFees) -> Self {
        Self { common_fees }
    }
}

impl FeeModel for FlatPerTradeFeeModel<CommonFees> {
    fn amount(&self, order: &Order, amount: f64) -> f64 {
        if order.maker {
            self.common_fees.maker_fee
        } else {
            self.common_fees.taker_fee
        }
    }
}

// different fees based on the direction.
#[derive(Clone)]

pub struct DirectionalFeeModel<Fees> {
    pub common_fees: Fees,
}
impl DirectionalFeeModel<CommonFees> {
    pub fn new(common_fees: CommonFees) -> Self {
        Self { common_fees }
    }
}

impl FeeModel for DirectionalFeeModel<CommonFees> {
    fn amount(&self, order: &Order, amount: f64) -> f64 {
        panic!("Not implemented");
    }
}
